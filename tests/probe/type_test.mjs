// POST /sessions/type against the real manager and the bundled Chromium, in an isolated APPDATA:
//   node type_test.mjs <nebula-sessions.exe> <folder chromium>
// Starts a session with a debug port (only to load the test page and read the result), types into a textarea
// below the fold through the API and checks the text, trusted key events with keydown/keyup and the timing.
import { spawn } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [exeSrc, chromiumDir] = process.argv.slice(2);
const d = mkdtempSync(join(tmpdir(), 'nebula-type-'));
const exe = join(d, 'nebula-sessions.exe');
copyFileSync(exeSrc, exe);
symlinkSync(chromiumDir, join(d, 'chromium'), 'junction');
const appdata = mkdtempSync(join(tmpdir(), 'type-'));
mkdirSync(join(appdata, 'NebulaSessions'));
writeFileSync(join(appdata, 'NebulaSessions', 'store.json'), JSON.stringify({ settings: { api_port: 40081 } }));
const p = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, APPDATA: appdata } });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const call = (method, path, body) => new Promise((resolve, reject) => {
  const data = body === undefined ? '' : JSON.stringify(body);
  const req = request({ host: '127.0.0.1', port: 40081, method, path, headers: { 'content-type': 'application/json', 'content-length': Buffer.byteLength(data) } }, (res) => {
    let t = ''; res.on('data', (x) => (t += x)); res.on('end', () => { let j; try { j = JSON.parse(t); } catch { j = t; } resolve([res.statusCode, j]); });
  });
  req.on('error', reject); req.end(data);
});
let ok = true;
const check = (name, pass, detail) => { ok &&= pass; console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${pass ? '' : '  ' + JSON.stringify(detail)}`); };
for (let i = 0; i < 40; i++) { try { await call('GET', '/sessions'); break; } catch { await sleep(500); } }

let uuid;
try {
  let [c, r] = await call('POST', '/sessions/create_quick', { count: 1 });
  uuid = r[0].uuid;
  [c, r] = await call('POST', '/sessions/type', { uuid, text: 'x' });
  check('type on a stopped session is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/start', { uuid, debug_port: 9334 });
  check('start with debug port', c === 200, [c, r]);
  let tabs = [];
  for (let i = 0; i < 40 && !tabs.some((t) => t.type === 'page'); i++) { try { tabs = await (await fetch('http://127.0.0.1:9334/json')).json(); } catch {} await sleep(500); }
  const ws = new WebSocket(tabs.find((t) => t.type === 'page').webSocketDebuggerUrl);
  await new Promise((res) => (ws.onopen = res));
  let id = 0; const waits = new Map();
  ws.onmessage = (m) => { const x = JSON.parse(m.data); waits.get(x.id)?.(x); };
  const send = (method, params = {}) => new Promise((res) => { const i = ++id; waits.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (e) => (await send('Runtime.evaluate', { expression: e, returnByValue: true })).result.result.value;
  await send('Page.navigate', { url: 'data:text/html,<div style="height:1600px">top</div><textarea id=t rows=4 cols=60></textarea><script>window.k={down:0,up:0,untrusted:0,shift:0,clicks:0,t:[]};for(const n of ["keydown","keyup","click"])t.addEventListener(n,e=>{if(!e.isTrusted)k.untrusted++;if(n=="keydown"){k.down++;k.t.push(performance.now());if(e.shiftKey)k.shift++}if(n=="keyup")k.up++;if(n=="click")k.clicks++})</script>' });
  await sleep(1000);

  const text = 'Hello Zażółć! line\nTwo';
  [c, r] = await call('POST', '/sessions/type', { uuid, text: 'abcd', selector: '#nope' });
  check('type answers at once with the length', c === 200 && r.chars === 4, [c, r]);
  for (let i = 0; i < 40; i++) { [, r] = await call('GET', `/sessions/${uuid}`); if (!r.busy) break; await sleep(250); }
  [c, r] = await call('POST', '/sessions/type', { uuid, text: 'x', min_delay_ms: 500, max_delay_ms: 100 });
  check('bad delay range is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/type', { uuid, text, selector: '#t' });
  check('type with selector accepted', c === 200 && r.chars === [...text].length, [c, r]);
  [, r] = await call('GET', `/sessions/${uuid}`);
  check('session shows busy "typing"', r.busy === 'typing', r.busy);
  for (let i = 0; i < 120; i++) { [, r] = await call('GET', `/sessions/${uuid}`); if (!r.busy) break; await sleep(250); }
  const res = JSON.parse(await ev('JSON.stringify({value:t.value,scrolled:scrollY,...k,span:k.t.length?k.t.at(-1)-k.t[0]:0})'));
  console.log(JSON.stringify({ ...res, t: undefined }));
  check('text arrived exactly (Polish letters, new line as Enter)', res.value === text, res.value);
  check('field reached by wheel and clicked with the mouse', res.scrolled > 500 && res.clicks === 1, res);
  check('every key has keydown and keyup, all trusted', res.down === [...text].length && res.up === res.down && res.untrusted === 0, res);
  check('Shift on H, Z, T and "!"', res.shift === 4, res.shift);
  check('typed over time, not at once', res.span > [...text].length * 50, res.span);
  ws.close();
} finally {
  if (uuid) await call('POST', '/sessions/stop', { uuids: [uuid] }).catch(() => {});
  await sleep(1000);
  p.kill();
}
console.log(ok ? 'ALL PASS' : 'SOME FAILED');
process.exit(ok ? 0 : 1);
