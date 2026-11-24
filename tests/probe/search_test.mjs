// Default search engine of the patched Chromium (0020): node search_test.mjs <chrome.exe>
// Opens chrome://newtab with --nebula-fp search=<engine> in a fresh profile: the new tab page URL is the default search engine's.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const exe = process.argv[2];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Yandex has no new-tab URL of its own: any third-party NTP means "not Google" (indirect evidence).
const cases = [['', /google\.com|chrome:\/\/(new-tab-page|newtab)/], ['bing', /bing\.com/], ['duckduckgo', /duckduckgo\.com/], ['yandex', /yandex\.|new-tab-page-third-party/]];
let ok = true, n = 0;
for (const [key, want] of cases) {
  const port = 9400 + n++;
  const fp = key ? `cores=8|search=${key}` : 'cores=8';
  const p = spawn(exe, [`--user-data-dir=${mkdtempSync(join(tmpdir(), 'srch-'))}`, `--remote-debugging-port=${port}`, '--remote-allow-origins=*', '--no-first-run', '--no-sandbox', `--nebula-fp=${fp}`, 'about:blank'], { stdio: 'ignore' });
  await sleep(3000);
  let url = '';
  const t = await (await fetch(`http://127.0.0.1:${port}/json/new?chrome://newtab`, { method: 'PUT' })).json();
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const waits = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); waits.get(d.id)?.(d); };
  const here = () => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method: 'Runtime.evaluate', params: { expression: 'location.href', returnByValue: true } })); }).then((d) => d.result?.result?.value ?? '');
  for (let i = 0; i < 20; i++) {
    await sleep(500);
    url = await here();
    if (i > 3 && url && !url.startsWith('chrome://newtab')) break;
  }
  ws.close();
  const pass = want.test(url);
  ok &&= pass;
  console.log(`${key || '(none)'} -> ${url}  ${pass ? 'PASS' : 'FAIL'}`);
  p.kill();
  await sleep(800);
}
console.log(ok ? 'ALL PASS' : 'FAILED');
