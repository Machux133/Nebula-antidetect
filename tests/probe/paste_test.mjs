// Tests Ctrl+Shift+V "paste like a human" in the patched Chromium: text must arrive gradually, with no 'paste' event.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const exe = process.argv[2];
const dir = mkdtempSync(join(tmpdir(), 'paste-'));
const port = 9333;
const p = spawn(exe, [`--user-data-dir=${dir}`, `--remote-debugging-port=${port}`, '--remote-allow-origins=*', '--no-first-run', '--no-sandbox', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let tabs;
for (let i = 0; i < 40; i++) {
  try { tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (tabs.length) break; } catch {}
  await sleep(500);
}
const ws = new WebSocket(tabs.find((t) => t.type === 'page').webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const waits = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); waits.get(d.id)?.(d); };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expr) => (await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })).result.result.value;

const text = 'Hello, Zażółć gęślą jaźń 😀 line2';
await send('Page.navigate', { url: 'data:text/html,<textarea id=t rows=4 cols=60></textarea><script>window.pasteEvents=0;window.inputs=[];t.addEventListener("paste",()=>pasteEvents++);t.addEventListener("input",e=>inputs.push(performance.now()))</script>' });
await sleep(800);
await send('Browser.grantPermissions', { permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] });
await ev(`t.focus(); true`);
await send('Page.bringToFront'); await send('Emulation.setFocusEmulationEnabled',{enabled:true});
await ev(`t.value=${JSON.stringify(text)}; t.focus(); t.select(); true`);
await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', modifiers: 2, windowsVirtualKeyCode: 67, code: 'KeyC', key: 'c', commands: ['copy'] });
await send('Input.dispatchKeyEvent', { type: 'keyUp', modifiers: 2, windowsVirtualKeyCode: 67, code: 'KeyC', key: 'c' });
await ev(`t.value=''; window.inputs.length=0; true`);
const key = (type) => send('Input.dispatchKeyEvent', { type, modifiers: 10, windowsVirtualKeyCode: 86, code: 'KeyV', key: 'V', commands: [] });
await key('rawKeyDown'); await key('keyUp');
await sleep(300);
const early = await ev(`t.value`);
await sleep(text.length * 200 + 1500);
const r = JSON.parse(await ev(`JSON.stringify({value:t.value, paste:pasteEvents, n:inputs.length, span:inputs.length?inputs.at(-1)-inputs[0]:0})`));
console.log(JSON.stringify({ expected: text, early, ...r }));
console.log(r.value === text && early.length < text.length && r.paste === 0 && r.span > 500 ? 'PASS' : 'FAIL');
ws.close(); p.kill();
