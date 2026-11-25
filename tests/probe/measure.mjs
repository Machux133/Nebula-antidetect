// Automatic measurement on public detection sites, saved per run so each patch or profile change can be compared:
// MEASURE_SITES=creepjs,fingerprint limits the run to those sites.
//   node tests/probe/measure.mjs <nebula-sessions.exe> [win|mac|linux|<device id>|plain|ref=<chrome.exe> ...]   (default: win)
// plain: the bundled Chromium with no session switches; ref=<exe>: another browser (e.g. a real Chrome) as the baseline.
// For every preset it takes the exact switches of a fresh session (--print-args), opens each site in a fresh
// profile, reads the verdict from the page text and writes tests/probe/runs/measure-<preset>-<time>.json, then
// prints what changed against the previous run of that preset. Uses Runtime.evaluate only (no Runtime.enable).
// The sites change their pages; a value of null means the pattern was not found (look at the saved `text`).
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const runs = join(here, 'runs');
mkdirSync(runs, { recursive: true });
const manager = process.argv[2];
const presets = process.argv.slice(3).length ? process.argv.slice(3) : ['win'];
const label = (p) => (p.startsWith('ref=') ? 'ref' : p);
const chrome = join(dirname(manager), 'chromium', 'chrome.exe');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// build.ps1 recreates dist, which drops the read access the sandboxed network process (AppContainer) needs; the
// manager grants it on a session's first start (browser::ensure_appcontainer_access), these runs start chrome.exe
// directly, so they grant it the same way.
const acl = join(dirname(chrome), '.appcontainer-acl');
if (!existsSync(acl)) {
  execFileSync('icacls', [dirname(chrome), '/grant', '*S-1-15-2-1:(OI)(CI)(RX)', '*S-1-15-2-2:(OI)(CI)(RX)', '/T', '/C', '/Q'], { stdio: 'ignore' });
  writeFileSync(acl, 'granted by tests/probe/measure.mjs');
}
const num = (re) => (t) => { const m = t.match(re); return m ? Number(m[1]) : null; };
const str = (re) => (t) => { const m = t.match(re); return m ? m[1].trim() : null; };

// What each site says, read from document.body.innerText; `wait` is how long its tests take.
const SITES = [
  { name: 'sannysoft', url: 'https://bot.sannysoft.com/', wait: 8000,
    js: `JSON.stringify({failed:document.querySelectorAll('td.failed,.failed').length, warn:document.querySelectorAll('td.warn,.warn').length})`,
    read: {} },
  { name: 'creepjs', url: 'https://abrahamjuliot.github.io/creepjs/', wait: 25000,
    read: { headless: num(/(\d+)%\s*headless/i), like_headless: num(/(\d+)%\s*like headless/i), stealth: num(/(\d+)%\s*stealth/i),
      lies: num(/lies\s*\(?(\d+)\)?/i), fp_id: str(/FP ID:\s*([0-9a-f]{16,})/i) } },
  { name: 'browserscan', url: 'https://www.browserscan.net/bot-detection', wait: 12000,
    read: { verdict: str(/Test Results?:?\s*(Normal|Robot|Suspicious)/i), robot_rows: (t) => (t.match(/\bRobot\b/g) || []).length } },
  { name: 'pixelscan', url: 'https://pixelscan.net/fingerprint-check', wait: 20000,
    read: { consistent: (t) => (/inconsistent/i.test(t) ? false : /consistent/i.test(t) ? true : null), masking: str(/(No masking detected|Masking detected)/i),
      automation: str(/(No automation framework detected|Automation framework detected)/i) } },
  { name: 'fingerprint', url: 'https://demo.fingerprint.com/playground', wait: 12000,
    read: { browser: str(/\nBrowser\s*\n\s*([^\n]+)/), bot: str(/\nBot\s*\n\s*([^\n]+)/), vpn: str(/\nVPN\s*\n\s*([^\n]+)/),
      tampering: str(/Browser Tampering\s*\n\s*(Yes|No|Not detected)/), suspect_score: num(/Suspect Score\s*\n\s*(\d+)/),
      anti_detect: str(/anti_detect_browser"?:\s*(true|false)/), anomaly_score: num(/anomaly_score"?:\s*([\d.]+)/) } },
];

async function measure(preset) {
  const exe = preset.startsWith('ref=') ? preset.slice(4) : chrome;
  let args = [];
  if (preset !== 'plain' && !preset.startsWith('ref=')) {
  const argsFile = join(mkdtempSync(join(tmpdir(), 'measure-')), 'args.txt');
  const os = ['win', 'mac', 'linux'].includes(preset) ? preset : /^(mba|mbp|imac)/.test(preset) ? 'mac' : preset.startsWith('linux') ? 'linux' : 'win';
  execFileSync(manager, ['--print-args', os, argsFile, ...(os === preset ? [] : [preset])]);
  args = readFileSync(argsFile, 'utf8').split('\n').map((s) => s.trim()).filter(Boolean);
  // MEASURE_DROP_KEYS=glmax,glext removes those keys from --nebula-fp, to find which one a site reacts to.
  const drop = (process.env.MEASURE_DROP_KEYS || '').split(',').filter(Boolean);
  if (drop.length) args = args.map((a) => (a.startsWith('--nebula-fp=') ? '--nebula-fp=' + a.slice(12).split('|').filter((kv) => !drop.includes(kv.split('=')[0])).join('|') : a));
  }
  const out = { preset: label(preset), time: new Date().toISOString(), sites: {} };
  for (const site of SITES.filter((x) => !process.env.MEASURE_SITES || process.env.MEASURE_SITES.split(',').includes(x.name))) {
    const port = 9500 + Math.floor(Math.random() * 400);
    const dir = mkdtempSync(join(tmpdir(), 'measure-prof-'));
    const p = spawn(exe, [...args, `--user-data-dir=${dir}`, `--remote-debugging-port=${port}`, '--remote-allow-origins=*', 'about:blank'], { stdio: 'ignore' });
    try {
      let tabs = [];
      for (let i = 0; i < 60 && !tabs.some((t) => t.type === 'page'); i++) { try { tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); } catch {} await sleep(500); }
      const ws = new WebSocket(tabs.find((t) => t.type === 'page').webSocketDebuggerUrl);
      await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
      let id = 0; const waits = new Map();
      ws.onmessage = (m) => { const d = JSON.parse(m.data); waits.get(d.id)?.(d); };
      const send = (method, params = {}) => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
      const ev = async (e) => (await send('Runtime.evaluate', { expression: e, returnByValue: true })).result?.result?.value;
      await send('Page.navigate', { url: site.url });
      await sleep(site.wait);
      const text = (await ev('document.body ? document.body.innerText : ""')) || '';
      const r = {};
      for (const [k, f] of Object.entries(site.read)) r[k] = f(text);
      if (site.js) Object.assign(r, JSON.parse((await ev(site.js)) || '{}'));
      r.text = text.slice(0, 30000); // to fix a pattern when the site changes
      out.sites[site.name] = r;
      ws.close();
    } catch (e) {
      out.sites[site.name] = { error: String(e) };
    } finally {
      p.kill();
      await sleep(800);
    }
    const shown = Object.fromEntries(Object.entries(out.sites[site.name]).filter(([k]) => k !== 'text'));
    console.log(`${label(preset)}  ${site.name.padEnd(12)} ${JSON.stringify(shown)}`);
  }
  return out;
}

for (const preset of presets) {
  const prevName = readdirSync(runs).filter((f) => new RegExp(`^measure-${label(preset)}-\\d{4}-`).test(f)).sort().at(-1);
  const run = await measure(preset);
  const file = join(runs, `measure-${label(preset)}-${run.time.replace(/[:.]/g, '-')}.json`);
  writeFileSync(file, JSON.stringify(run, null, 1));
  console.log(`saved ${file}`);
  if (prevName) {
    const prev = JSON.parse(readFileSync(join(runs, prevName), 'utf8'));
    const changes = [];
    for (const [site, r] of Object.entries(run.sites)) {
      for (const [k, v] of Object.entries(r)) {
        if (k === 'text') continue;
        const old = prev.sites?.[site]?.[k];
        if (JSON.stringify(old) !== JSON.stringify(v)) changes.push(`  ${site}.${k}: ${JSON.stringify(old)} -> ${JSON.stringify(v)}`);
      }
    }
    console.log(changes.length ? `changes against ${prevName}:\n${changes.join('\n')}` : `no change against ${prevName}`);
  }
}
