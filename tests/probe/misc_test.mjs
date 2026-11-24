// Misc switches of a session in the patched Chromium: node misc_test.mjs <chrome.exe> <profile dir made by the write_fixture test>
//   cd app && NEBULA_FIXTURE=<dir> cargo test write_fixture -- --ignored
// Checks: imported bookmarks show up, the preference files the manager wrote are read by Chromium (history, passwords,
// memory saver), and with --nebula-fp nolatin=1 a Cyrillic key press types nothing while a Latin one does (0021).
import { spawn } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [exe, fixture] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let ok = true;
const check = (name, pass, detail) => { ok &&= pass; console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}  ${detail ?? ''}`); };

const web = createServer((_, res) => { res.setHeader('content-type', 'text/html'); res.end('<title>NebulaHistoryProbe</title><p>x</p>'); });
await new Promise((r) => web.listen(0, '127.0.0.1', r));
const site = `http://127.0.0.1:${web.address().port}/`;

async function launch(port, historySaving) {
  const dir = mkdtempSync(join(tmpdir(), 'misc-'));
  cpSync(fixture, dir, { recursive: true });
  if (historySaving) { // the control: the same profile but with history left on
    const f = join(dir, 'Default', 'Preferences');
    const v = JSON.parse(readFileSync(f, 'utf8'));
    v.history.saving_disabled = false;
    writeFileSync(f, JSON.stringify(v));
  }
  const p = spawn(exe, [`--user-data-dir=${dir}`, `--remote-debugging-port=${port}`, '--remote-allow-origins=*', '--no-first-run', '--no-sandbox', '--nebula-fp=cores=8|nolatin=1', 'about:blank'], { stdio: 'ignore' });
  await sleep(3000);
  const tab = async (url) => {
    const t = await (await fetch(`http://127.0.0.1:${port}/json/new?${url}`, { method: 'PUT' })).json();
    const ws = new WebSocket(t.webSocketDebuggerUrl);
    await new Promise((r) => (ws.onopen = r));
    let id = 0; const waits = new Map();
    ws.onmessage = (m) => { const d = JSON.parse(m.data); waits.get(d.id)?.(d); };
    const send = (method, params = {}) => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
    const ev = async (expression) => { const d = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); return d.result?.result?.value ?? d.result?.exceptionDetails?.text; };
    return { send, ev, close: () => ws.close() };
  };
  const deepText = `(function t(r){let o='';for(const e of r.querySelectorAll('*')){if(e.shadowRoot)o+=t(e.shadowRoot);if(e.children.length===0)o+=(e.textContent||'')+' '}return o})(document)`;
  const historyHas = async () => {
    const v = await tab(site); await sleep(1500); v.close();
    const h = await tab('chrome://history/'); await sleep(3000);
    const text = await h.ev(deepText); h.close();
    return /NebulaHistoryProbe/.test(text);
  };
  return { p, tab, deepText, historyHas };
}

// Profile written by the manager: history off, passwords off, memory saver on, two bookmarks.
const a = await launch(9700, false);
const s = await a.tab('chrome://settings/');
await sleep(3000);
const pref = (k) => s.ev(`new Promise(r => chrome.settingsPrivate.getPref(${JSON.stringify(k)}, x => r(x && x.value)))`);
check('credentials_enable_service is off', (await pref('credentials_enable_service')) === false);
const ms = await pref('performance_tuning.high_efficiency_mode.state');
check('memory saver state is 2', ms === 2, String(ms));
s.close();
const b = await a.tab('chrome://bookmarks/');
await sleep(3000);
const bt = await b.ev(a.deepText);
check('bookmarks imported', /Nebula Example A/.test(bt) && /Nebula Example B/.test(bt));
b.close();
const t = await a.tab('data:text/html,<textarea id=t></textarea>');
await sleep(1500);
await t.ev('t.focus(); true');
const press = async (ch, code) => { await t.send('Input.dispatchKeyEvent', { type: 'keyDown', key: ch, text: ch, code, windowsVirtualKeyCode: ch.toUpperCase().charCodeAt(0) }); await t.send('Input.dispatchKeyEvent', { type: 'keyUp', key: ch, code }); };
await press('z', 'KeyZ'); await press('ж', 'Semicolon'); await press('ł', 'KeyL'); await press('7', 'Digit7');
await sleep(300);
const typed = await t.ev('t.value');
check('nolatin: Latin and digits typed, Cyrillic dropped', typed === 'zł7', JSON.stringify(typed));
t.close();
check('history off: visited page not recorded', !(await a.historyHas()));
a.p.kill();
await sleep(1500);

// Control: the same profile with history on must record the visit (otherwise the check above proves nothing).
const c = await launch(9701, true);
check('control, history on: visited page is recorded', await c.historyHas());
c.p.kill();
web.close();
console.log(ok ? 'ALL PASS' : 'FAILED');
