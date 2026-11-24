// Local REST API against the real manager, in an isolated APPDATA (your sessions are not touched):
//   node api_test.mjs <nebula-sessions.exe>
// Starts the manager with api_port 40080 and calls the endpoints that do not need a browser; with the bundled
// Chromium beside the exe it also starts and stops a session.
import { spawn } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

const exeSrc = process.argv[2];
const chromiumDir = process.argv[3]; // optional: folder with chrome.exe (dist/Nebula/chromium) for the browser-backed checks
let exe = exeSrc;
if (chromiumDir) {
  const d = mkdtempSync(join(tmpdir(), 'nebula-exe-'));
  exe = join(d, 'nebula-sessions.exe');
  copyFileSync(exeSrc, exe);
  symlinkSync(chromiumDir, join(d, 'chromium'), 'junction');
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const appdata = mkdtempSync(join(tmpdir(), 'api-'));
mkdirSync(join(appdata, 'NebulaSessions'));
writeFileSync(join(appdata, 'NebulaSessions', 'store.json'), JSON.stringify({ settings: { api_port: 40080 } }));
const p = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, APPDATA: appdata } });
const call = (method, path, body) => new Promise((resolve, reject) => {
  const data = body === undefined ? '' : JSON.stringify(body);
  const req = request({ host: '127.0.0.1', port: 40080, method, path, headers: { 'content-type': 'application/json', 'content-length': Buffer.byteLength(data) } }, (res) => {
    let t = ''; res.on('data', (d) => (t += d)); res.on('end', () => { let j; try { j = JSON.parse(t); } catch { j = t; } resolve([res.statusCode, j]); });
  });
  req.on('error', reject); req.end(data);
});
let ok = true;
const check = (name, pass, detail) => { ok &&= pass; console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${pass ? '' : '  ' + JSON.stringify(detail)}`); };
for (let i = 0; i < 40; i++) { try { await call('GET', '/sessions'); break; } catch { await sleep(500); } }

try {
  // sessions: create, list, info, rename
  let [c, r] = await call('POST', '/sessions/create_quick', { count: 2 });
  check('create_quick count=2 returns an array', c === 200 && Array.isArray(r) && r.length === 2 && r[0].uuid && r[0].name, [c, r]);
  const [a, b] = r.map((x) => x.uuid);
  [c, r] = await call('GET', '/sessions', { proxy_info: true });
  check('GET /sessions: status + proxy', c === 200 && r.length === 2 && r[0].status === 'stopped' && r[0].proxy?.protocol, [c, r]);
  [c, r] = await call('GET', '/sessions', { status: 'running' });
  check('GET /sessions status filter', c === 200 && r.length === 0, [c, r]);
  [c, r] = await call('GET', '/sessions', { status: 'bogus' });
  check('GET /sessions bad status is 400', c === 400, [c, r]);
  [c, r] = await call('GET', `/sessions/${a}`);
  check('GET /sessions/:uuid', c === 200 && r.device?.cpu > 0 && r.device?.noises && r.proxy?.connection && r.webrtc?.state && r.status === 'stopped', [c, r]);
  [c, r] = await call('GET', '/sessions/nope');
  check('GET /sessions/:uuid unknown is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/set_name', { uuid: a, name: 'Alpha' });
  check('set_name', c === 200, [c, r]);
  [c, r] = await call('POST', '/sessions/set_name', { uuid: a, name: '' });
  check('set_name empty is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/set_description', { uuid: a, description: 'about **alpha**' });
  check('set_description', c === 200, [c, r]);

  // misc, connection, geo, user agent, fingerprints
  [c, r] = await call('POST', '/sessions/set_misc', { uuid: a, memory_saver: true, browsing_history: false, disable_popups: false, always_focus_window: true });
  check('set_misc (+ ignored unsupported flag)', c === 200 && r.ignored?.includes('always_focus_window'), [c, r]);
  [c, r] = await call('POST', '/sessions/set_misc', { uuid: a });
  check('set_misc without flags is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/connection', { uuid: a, type: 'socks5', ip: '10.1.2.3', port: 1080, login: 'u', password: 'p', customDns: '1.1.1.1' });
  check('connection socks5', c === 200 && r.uuid === a, [c, r]);
  [c, r] = await call('POST', '/sessions/connection', { uuid: a, type: 'socks5', ip: '10.1.2.3' });
  check('connection without port is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/connection', { uuid: a, type: 'freeProxy' });
  check('connection freeProxy is refused', c === 400, [c, r]);
  [c, r] = await call('GET', '/sessions', { proxy_info: true });
  const sa = r.find((x) => x.uuid === a);
  check('proxy shows in the list', sa.name === 'Alpha' && sa.proxy.protocol === 'socks5' && sa.proxy.ip === '10.1.2.3' && sa.proxy.port === 1080, sa);
  [c, r] = await call('POST', '/sessions/geo', { uuid: a, timezone: 'Europe/Berlin', language: 'de,en-US;q=0.9' });
  check('geo', c === 200, [c, r]);
  [c, r] = await call('POST', '/sessions/geo', { uuid: a, latitude: 1, longitude: 2 });
  check('geo latitude is refused (not modelled)', c === 400, [c, r]);
  [c, r] = await call('GET', `/sessions/${a}`);
  check('info reflects geo + noises', r.proxy.timezone === 'Europe/Berlin' && r.proxy.language === 'de' && r.proxy.connection.protocol === 'socks5', r);
  [c, r] = await call('POST', '/sessions/set_useragent', { uuid: a, useragent: 'Mozilla/5.0 TestAgent' });
  check('set_useragent', c === 200, [c, r]);
  [c, r] = await call('POST', '/sessions/set_fingerprints', { uuid: a, canvas: 'fake', audio: 'direct', media_devices: 'fake' });
  check('set_fingerprints noises', c === 200, [c, r]);
  [c, r] = await call('GET', `/sessions/${a}`);
  check('noise reflected', r.device.noises.canvas === 'fake' && r.device.noises.audio === 'direct', r.device.noises);
  [c, r] = await call('POST', '/sessions/set_fingerprints', { uuid: a, canvas: 'bogus' });
  check('set_fingerprints bad value is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/set_fingerprints', { uuid: a, browser: 'opera' });
  check('set_fingerprints opera is refused', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/set_fingerprints', { uuid: a, gpu: 'no such gpu' });
  check('set_fingerprints unknown gpu is 400 with the list', c === 400 && /Available/.test(r.error), [c, r]);

  // action log
  [c, r] = await call('GET', '/sessions/actionlog', { uuid: a, limit: 5 });
  check('actionlog', c === 200 && r.session_uuid === a && r.actions.length > 0 && r.total_count >= r.returned_count && r.actions[0].timestamp >= r.actions.at(-1).timestamp, [c, r]);
  [c, r] = await call('GET', '/sessions/actionlog', { uuid: a, action_types: '3' });
  check('actionlog filter by action type', c === 200 && r.actions.every((x) => x.action === 3) && r.actions.length >= 1, [c, r]);

  // cookies
  [c, r] = await call('POST', '/sessions/import_cookies', { uuid: a, json: [{ domain: '.example.com', name: 'k', value: 'v', path: '/', secure: true, httpOnly: false, expirationDate: 4102444800 }] });
  check('import_cookies json (applied at next start)', c === 200 && r.imported === 1, [c, r]);
  [c, r] = await call('POST', '/sessions/import_cookies', { uuid: a, file_path: 'C:/definitely/missing.json' });
  check('import_cookies missing file is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/export_cookies', { uuids: [a], folder_path: join(appdata, 'out') });
  check('export_cookies needs a running session', c === 400, [c, r]);

  // clone, stop, delete (many), check_proxy
  [c, r] = await call('POST', '/sessions/check_proxy', { uuids: [a, b] });
  check('check_proxy many', c === 200 && r.result === 'Success' && r.uuids?.length === 2, [c, r]);
  [c, r] = await call('POST', '/sessions/stop', { uuids: [a, b] });
  check('stop many (not running is fine)', c === 200 && r.uuids?.length === 2, [c, r]);
  [c, r] = await call('POST', '/sessions/stop', { uuids: [a, 'nope'] });
  check('stop with an unknown id changes nothing: 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/delete', { uuids: [b] });
  check('delete (to trash)', c === 200, [c, r]);
  [c, r] = await call('GET', '/sessions');
  check('deleted session is gone from the list', r.length === 1 && r[0].uuid === a, r);
  [c, r] = await call('DELETE', '/sessions', { uuid: a });
  check('DELETE /sessions', c === 200 && r.uuid === a, [c, r]);

  // presets/providers, desktops
  [c, r] = await call('GET', '/providers');
  check('GET /providers', c === 200 && Array.isArray(r) && r.length >= 4 && r[0].name && r[0].uuid, [c, r]);
  [c, r] = await call('POST', '/providers', { uuid: r[1].uuid });
  check('POST /providers sets the active one', c === 200, [c, r]);
  [c, r] = await call('DELETE', '/providers', { uuid: 'Windows' });
  check('DELETE /providers refuses built-ins', c === 400, [c, r]);
  [c, r] = await call('GET', '/desktops');
  check('GET /desktops', c === 200 && r.length >= 1 && r[0].uuid, [c, r]);
  const first = r[0].uuid;
  [c, r] = await call('PUT', '/desktops', { name: 'Second' });
  check('PUT /desktops', c === 200 && r.uuid, [c, r]);
  const second = r.uuid;
  [c, r] = await call('POST', '/desktops', { uuid: second });
  check('POST /desktops sets the active one', c === 200, [c, r]);
  [c, r] = await call('DELETE', '/desktops', { uuid: second });
  check('DELETE active desktop is refused', c === 400, [c, r]);
  [c, r] = await call('POST', '/desktops', { uuid: first });
  [c, r] = await call('DELETE', '/desktops', { uuid: second });
  check('DELETE an empty, inactive desktop', c === 200, [c, r]);
  [c, r] = await call('POST', '/desktops/unlock_stopped_sessions');
  check('unlock_stopped_sessions', c === 200 && Array.isArray(r), [c, r]);

  // app
  [c, r] = await call('POST', '/app/optimize_storage');
  check('optimize_storage', c === 200 && 'freed_bytes' in r, [c, r]);
  [c, r] = await call('POST', '/app/api_threads_count/4');
  check('api_threads_count', c === 200, [c, r]);
  [c, r] = await call('POST', '/app/api_threads_count/1');
  check('api_threads_count out of range is 400', c === 400, [c, r]);
  [c, r] = await call('POST', '/sessions/start_warmup', { uuid: a, url_count: 999 });
  check('start_warmup validates url_count', c === 400, [c, r]);
  [c, r] = await call('POST', '/nothing');
  check('unknown endpoint is 404', c === 404, [c, r]);

  if (chromiumDir) {
    const v = async (port) => { try { return (await (await fetch(`http://127.0.0.1:${port}/json/version`)).json()).Browser; } catch { return null; } };
    [c, r] = await call('POST', '/sessions/create_quick', {});
    const w = r[0].uuid;
    [c, r] = await call('POST', '/sessions/start', { uuid: w, headless: true, debug_port: 9811, disable_images: true, chromium_args: '--mute-audio', referrer_values: [{ url: 'https://a.example', replace: 'https://b.example' }] });
    check('start returns the debug port and flags ignored referrer_values', c === 200 && r.debug_port === 9811 && r.ignored?.[0] === 'referrer_values', [c, r]);
    await sleep(2500);
    check('the browser answers on that port', /Chrome/.test((await v(9811)) ?? ''), await v(9811));
    [c, r] = await call('GET', '/sessions');
    check('status is automationRunning', r.find((x) => x.uuid === w)?.status === 'automationRunning', r);
    [c, r] = await call('POST', '/sessions/import_cookies', { uuid: w, json: [{ domain: '.example.com', name: 'k', value: 'v', path: '/', secure: true, expirationDate: 4102444800 }] });
    await sleep(1500);
    const out = join(appdata, 'cookies-out');
    [c, r] = await call('POST', '/sessions/export_cookies', { uuids: [w], folder_path: out });
    check('export_cookies writes a file', c === 200 && r[0]?.cookie_path && existsSync(r[0].cookie_path), [c, r]);
    if (c === 200 && r[0]?.cookie_path) check('exported file has the imported cookie', /"k"/.test(readFileSync(r[0].cookie_path, 'utf8')), r);
    [c, r] = await call('POST', '/sessions/force_stop', { uuid: w });
    check('force_stop', c === 200, [c, r]);
    await sleep(1000);
    check('the browser is gone', (await v(9811)) === null);

    [c, r] = await call('POST', '/sessions/run_one_time', { count: 1, headless: true });
    const ids = Object.keys(r);
    check('run_one_time returns {uuid: port}', c === 200 && ids.length === 1 && Number.isInteger(r[ids[0]]), [c, r]);
    await sleep(2500);
    check('one-time browser answers', /Chrome/.test((await v(r[ids[0]])) ?? ''));
    await call('POST', '/sessions/stop', { uuid: ids[0] });
    await sleep(1000);
    [c, r] = await call('GET', '/sessions');
    check('one-time session is deleted after stop', !r.some((x) => x.uuid === ids[0]), r);
  }
} finally {
  p.kill();
}
console.log(ok ? 'ALL PASS' : 'FAILED');
process.exit(ok ? 0 : 1);
