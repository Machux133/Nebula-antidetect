//! Starting a Chromium-family browser for one session, the fingerprint extension,
//! and the DevTools-protocol jobs (cookies, warm-up) that drive it.

use crate::devices::{self, Os};
use crate::model::{profile_dir, ProxyKind, Session, Settings};
use crate::proxy::{start_relay, Relay};
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub fn find_browser(custom: &str) -> Option<PathBuf> {
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        return p.is_file().then_some(p);
    }
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    // The patched Chromium that build.ps1 puts beside the manager comes first.
    let bundled = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("chromium").join("chrome.exe")));
    bundled.into_iter().chain([
        format!(r"{local}\Chromium\Application\chrome.exe"),
        format!(r"{pf}\Chromium\Application\chrome.exe"),
        format!(r"{pf}\Google\Chrome\Application\chrome.exe"),
        format!(r"{pf86}\Google\Chrome\Application\chrome.exe"),
        format!(r"{local}\Google\Chrome\Application\chrome.exe"),
        format!(r"{pf}\BraveSoftware\Brave-Browser\Application\brave.exe"),
        format!(r"{pf86}\Microsoft\Edge\Application\msedge.exe"),
        format!(r"{pf}\Microsoft\Edge\Application\msedge.exe"),
        "/usr/bin/chromium".into(),
        "/usr/bin/google-chrome".into(),
        "/Applications/Chromium.app/Contents/MacOS/Chromium".into(),
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
    ]
    .into_iter()
    .map(PathBuf::from))
    .find(|p| p.is_file())
}

/// Major version from the version-named folder Chromium-family installers put beside the exe.
pub fn browser_major(exe: &Path) -> Option<u32> {
    let dir = exe.parent()?;
    // build.ps1 writes the bundled Chromium's version here (its out folder has no version folder).
    if let Ok(v) = std::fs::read_to_string(dir.join("VERSION")) {
        return v.trim().split('.').next()?.parse().ok();
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (n.matches('.').count() == 3).then(|| n.split('.').next()?.parse::<u32>().ok()).flatten()
        })
        .max()
}

pub fn is_edge(exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().contains("edge"))
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(9333)
}

pub struct Running {
    pub child: Child,
    pub port: u16,
    pub started: Instant,
    _relay: Option<Relay>,
}

impl Running {
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    pub fn stop(&mut self) {
        let _ = cdp_browser(self.port).and_then(|mut c| c.call("Browser.close", json!({})));
        for _ in 0..20 {
            if !self.alive() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct LaunchInfo {
    pub args: Vec<String>,
}

/// Command-line switches for one session. Pure, so it can be checked without starting a browser.
pub fn build_args(s: &Session, relay_port: Option<u16>, debug_port: u16, ext: &Path, ua: &str, headless: bool) -> LaunchInfo {
    let d = devices::get(&s.fp.device);
    let data = profile_dir(&s.uuid).join("data");
    let w = s.fp.screen.0.saturating_sub(40).max(800);
    let h = s.fp.screen.1.saturating_sub(d.os.taskbar() + 40).max(600);
    let mut a = vec![
        format!("--user-data-dir={}", data.display()),
        format!("--remote-debugging-port={debug_port}"),
        "--remote-allow-origins=*".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-sync".into(),
        format!("--load-extension={}", ext.display()),
        format!("--user-agent={ua}"),
        format!("--window-size={w},{h}"),
        // Real pixel ratio of the modelled screen, so layouts and canvas sizes agree with it.
        format!("--force-device-scale-factor={}", d.dpr),
        // Read natively by the bundled patched Chromium (workers and client-hint headers too); other browsers ignore it.
        format!("--nebula-fp={}", nebula_fp(&s.fp, timezone(s).as_deref())),
    ];
    if !s.fp.language.is_empty() {
        a.push(format!("--lang={}", s.fp.language));
        a.push(format!("--accept-lang={},{}", s.fp.language, s.fp.language.split('-').next().unwrap_or("en")));
    }
    if let Some(p) = relay_port {
        a.push(format!("--proxy-server=socks5://127.0.0.1:{p}"));
        a.push("--proxy-bypass-list=<-loopback>".into());
    }
    if s.fp.webrtc_protect && s.proxy.kind != ProxyKind::Direct {
        a.push("--force-webrtc-ip-handling-policy=disable_non_proxied_udp".into());
    }
    let m = &s.misc;
    if m.restore_open_tabs && !headless {
        a.push("--restore-last-session".into());
    }
    if !m.hardware_acceleration {
        a.push("--disable-gpu".into());
    }
    if m.disable_images {
        a.push("--blink-settings=imagesEnabled=false".into());
    }
    a.push(if m.enable_quic { "--enable-quic" } else { "--disable-quic" }.into());
    if headless {
        a.push("--headless=new".into());
    }
    if !m.start_url.trim().is_empty() && !headless {
        a.push(m.start_url.trim().into());
    }
    LaunchInfo { args: a }
}

/// `key=value|...` for the patched Chromium (chromium/patches/0001-nebula-device-values.patch).
pub fn nebula_fp(fp: &crate::model::Fingerprint, tz: Option<&str>) -> String {
    let d = devices::get(&fp.device);
    let mut v = format!("cores={}|ram={}|plat={}|pver={}|arch={}", d.cores, d.ram, d.os.label(), d.platform_version, d.arch);
    if fp.webgl != crate::model::FpMode::Off {
        v += &format!("|gpuv={}|gpur={}", d.gpu_vendor, d.gpu_renderer);
    }
    // Timezone and font allow-list, also native (0003-nebula-timezone.patch, 0004-nebula-fonts.patch).
    if let Some(tz) = tz {
        v += &format!("|tz={tz}");
    }
    if fp.fonts {
        v += &format!("|fonts={}", d.os.fonts().join(","));
    }
    // Seeded noise done natively (chromium/patches/0002-nebula-noise.patch); an absent key means the real value.
    for (key, mode) in [("cn", fp.canvas), ("an", fp.audio), ("rn", fp.client_rects)] {
        if let Some(seed) = devices::seed_for(mode, fp) {
            v += &format!("|{key}={seed}");
        }
    }
    v
}

pub fn launch(settings: &Settings, s: &Session, headless: bool) -> Result<Running, String> {
    let exe = find_browser(&settings.browser_path).ok_or("No Chromium, Chrome, Brave or Edge found. Set its path in Preferences.")?;
    let major = browser_major(&exe).unwrap_or(140);
    let ua = devices::user_agent(&s.fp, major, is_edge(&exe));
    let ext = write_extension(s, settings, major).map_err(|e| format!("Could not write the fingerprint extension: {e}"))?;
    let relay = match s.proxy.kind {
        ProxyKind::Direct => None,
        _ => Some(start_relay(s.proxy.clone()).map_err(|e| format!("Could not start the proxy relay: {e}"))?),
    };
    let port = free_port();
    let info = build_args(s, relay.as_ref().map(|r| r.port), port, &ext, &ua, headless);
    let mut cmd = Command::new(&exe);
    cmd.args(&info.args);
    if let Some(tz) = timezone(s) {
        cmd.env("TZ", tz);
    }
    let child = cmd.spawn().map_err(|e| format!("Could not start {}: {e}", exe.display()))?;
    Ok(Running { child, port, started: Instant::now(), _relay: relay })
}

pub fn timezone(s: &Session) -> Option<String> {
    if s.fp.geo_from_proxy {
        if let Some(g) = s.geo.as_ref().filter(|g| g.ok && !g.timezone.is_empty()) {
            return Some(g.timezone.clone());
        }
    }
    (!s.fp.timezone.is_empty()).then(|| s.fp.timezone.clone())
}

/// The per-session extension: one config object, all taken from the same device template.
pub fn fingerprint_config(s: &Session, settings: &Settings, major: u32) -> Value {
    let fp = &s.fp;
    let d = devices::get(&fp.device);
    json!({
        "seed": fp.seed,
        "cores": d.cores,
        "ram": d.ram,
        "platform": d.os.platform(),
        "uaPlatform": d.os.label(),
        "platformVersion": d.platform_version,
        "arch": d.arch,
        "major": major,
        "touch": d.touch,
        "screen": [fp.screen.0, fp.screen.1],
        "taskbar": d.os.taskbar(),
        "depth": if d.os == Os::Mac { 30 } else { 24 },
        "lang": fp.language,
        "tz": timezone(s),
        "dnt": s.misc.do_not_track,
        "webgl": fp.webgl != crate::model::FpMode::Off,
        "gpuVendor": d.gpu_vendor,
        "gpuRenderer": d.gpu_renderer,
        "sampleRate": d.sample_rate,
        "canvasSeed": devices::seed_for(fp.canvas, fp),
        "audioSeed": devices::seed_for(fp.audio, fp),
        "rectsSeed": devices::seed_for(fp.client_rects, fp),
        "fonts": if fp.fonts { d.os.fonts().to_vec() } else { vec![] },
        "hideWebgpu": fp.hide_webgpu,
        "fakeMedia": fp.fake_media_devices,
        "ports": settings.closed_ports,
    })
}

fn write_extension(s: &Session, settings: &Settings, major: u32) -> std::io::Result<PathBuf> {
    let dir = profile_dir(&s.uuid).join("shield");
    std::fs::create_dir_all(&dir)?;
    let manifest = json!({
        "manifest_version": 3,
        "name": "Session shield",
        "version": "1.0",
        "content_scripts": [{
            "matches": ["<all_urls>"],
            "js": ["fp.js"],
            "run_at": "document_start",
            "all_frames": true,
            "match_about_blank": true,
            "world": "MAIN"
        }]
    });
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
    let cfg = fingerprint_config(s, settings, major).to_string();
    std::fs::write(dir.join("fp.js"), FP_JS.replace("__CFG__", &cfg))?;
    Ok(dir)
}

const FP_JS: &str = include_str!("fp.js");

// ---------------------------------------------------------------- DevTools protocol

pub struct Cdp {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    id: u64,
}

impl Cdp {
    pub fn connect(url: &str) -> Result<Cdp, String> {
        let (ws, _) = tungstenite::connect(url).map_err(|e| e.to_string())?;
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
        }
        Ok(Cdp { ws, id: 0 })
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        self.ws
            .send(Message::text(json!({"id": id, "method": method, "params": params}).to_string()))
            .map_err(|e| e.to_string())?;
        loop {
            let m = self.ws.read().map_err(|e| e.to_string())?;
            if let Message::Text(t) = m {
                let v: Value = serde_json::from_str(t.as_str()).unwrap_or(Value::Null);
                if v["id"].as_u64() == Some(id) {
                    if let Some(e) = v.get("error") {
                        return Err(e["message"].as_str().unwrap_or("error").to_string());
                    }
                    return Ok(v["result"].clone());
                }
            }
        }
    }

    pub fn eval(&mut self, js: &str) -> Result<Value, String> {
        let r = self.call("Runtime.evaluate", json!({"expression": js, "returnByValue": true}))?;
        Ok(r["result"]["value"].clone())
    }
}

fn http(port: u16) -> ureq::Agent {
    let _ = port;
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
}

pub fn wait_ready(port: u16, secs: u64) -> Result<(), String> {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        if http(port).get(&format!("http://127.0.0.1:{port}/json/version")).call().is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("The browser did not open its control port in time".into())
}

pub fn cdp_browser(port: u16) -> Result<Cdp, String> {
    let v: Value = http(port)
        .get(&format!("http://127.0.0.1:{port}/json/version"))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    Cdp::connect(v["webSocketDebuggerUrl"].as_str().ok_or("no browser socket")?)
}

// ---------------------------------------------------------------- cookies

/// Accepts a JSON array (browser-extension or DevTools export) or a Netscape cookies.txt file.
pub fn parse_cookies(text: &str) -> Result<Vec<Value>, String> {
    let t = text.trim_start_matches('\u{feff}').trim();
    let raw: Vec<Value> = if t.starts_with('[') {
        serde_json::from_str(t).map_err(|e| format!("Not a cookie list: {e}"))?
    } else if t.starts_with('{') {
        let v: Value = serde_json::from_str(t).map_err(|e| format!("Not a cookie list: {e}"))?;
        v["cookies"].as_array().cloned().ok_or("No \"cookies\" list in the file")?
    } else {
        t.lines()
            .filter_map(|l| {
                let (http_only, l) = match l.strip_prefix("#HttpOnly_") {
                    Some(r) => (true, r),
                    None => (false, l),
                };
                if l.starts_with('#') || l.trim().is_empty() {
                    return None;
                }
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() >= 7).then(|| {
                    json!({"domain": f[0], "path": f[2], "secure": f[3].eq_ignore_ascii_case("TRUE"),
                           "expires": f[4].parse::<f64>().unwrap_or(-1.0), "name": f[5], "value": f[6], "httpOnly": http_only})
                })
            })
            .collect()
    };
    let out: Vec<Value> = raw
        .iter()
        .filter_map(|c| {
            let name = c["name"].as_str()?;
            let mut o = json!({
                "name": name,
                "value": c["value"].as_str().unwrap_or(""),
                "domain": c["domain"].as_str().or(c["host"].as_str())?,
                "path": c["path"].as_str().unwrap_or("/"),
                "secure": c["secure"].as_bool().unwrap_or(false),
                "httpOnly": c["httpOnly"].as_bool().unwrap_or(false),
            });
            let exp = c["expires"].as_f64().or(c["expirationDate"].as_f64());
            if let Some(e) = exp.filter(|e| *e > 0.0) {
                o["expires"] = json!(e);
            }
            let ss = c["sameSite"].as_str().unwrap_or("").to_ascii_lowercase();
            let ss = match ss.as_str() {
                "strict" => Some("Strict"),
                "lax" => Some("Lax"),
                "none" | "no_restriction" => Some("None"),
                _ => None,
            };
            if let Some(ss) = ss {
                o["sameSite"] = json!(ss);
            }
            Some(o)
        })
        .collect();
    if out.is_empty() {
        return Err("No cookies found in the file".into());
    }
    Ok(out)
}

pub fn import_cookies(port: u16, cookies: &[Value]) -> Result<usize, String> {
    let mut c = cdp_browser(port)?;
    c.call("Storage.setCookies", json!({ "cookies": cookies }))?;
    Ok(cookies.len())
}

pub fn export_cookies(port: u16) -> Result<Vec<Value>, String> {
    let mut c = cdp_browser(port)?;
    Ok(c.call("Storage.getCookies", json!({}))?["cookies"].as_array().cloned().unwrap_or_default())
}

// ---------------------------------------------------------------- warm-up (cookie bot)

#[derive(Clone)]
pub struct Warmup {
    pub urls: Vec<String>,
    pub url_count: u32,
    pub time_per_url: u32,
    pub view_depth: u32,
    pub accept_cookies: bool,
}

/// Limits from the original's "Start warmup" API schema (string at 0x14279ce20).
pub const URL_COUNT: (u32, u32) = (1, 50);
pub const TIME_PER_URL: (u32, u32) = (0, 99);
pub const VIEW_DEPTH: (u32, u32) = (1, 10);
pub const MAX_URLS: usize = 50;
const LOAD_WAIT_SECS: u64 = 3; // estimated: minimum wait for a page to load before acting on it

const ACCEPT_JS: &str = r#"(()=>{const re=/^(accept( all)?( cookies)?|allow( all)?( cookies)?|i agree|agree( and continue)?|got it|ok|consent|zaakceptuj( wszystko| wszystkie)?|akceptuj(ę)?( wszystko| wszystkie)?|zgadzam się|alle akzeptieren|akzeptieren|tout accepter|accepter|aceptar( todo)?|accetta( tutto)?|aceitar( tudo)?)$/i;
for(const id of ['onetrust-accept-btn-handler','L2AGLb','didomi-notice-agree-button','CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll','truste-consent-button']){const e=document.getElementById(id);if(e){e.click();return id}}
for(const e of document.querySelectorAll('button,a,[role=button],input[type=button],input[type=submit]')){const t=(e.innerText||e.value||'').trim().replace(/\s+/g,' ');if(t&&t.length<40&&re.test(t)){e.click();return t}}return ''})()"#;
const SCROLL_JS: &str = "window.scrollBy({top: innerHeight*(0.4+Math.random()), behavior:'smooth'}); true";
const CLICK_JS: &str = r#"(()=>{const l=[...document.links].filter(a=>a.host===location.host&&a.href.split('#')[0]!==location.href.split('#')[0]&&a.offsetParent);if(!l.length)return false;location.href=l[Math.floor(Math.random()*l.length)].href;return true})()"#;

pub fn pick_urls(w: &Warmup) -> Vec<String> {
    let mut urls: Vec<String> = w.urls.iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).take(MAX_URLS).collect();
    fastrand::shuffle(&mut urls);
    urls.truncate(w.url_count.clamp(URL_COUNT.0, URL_COUNT.1) as usize);
    urls.into_iter().map(|u| if u.contains("://") { u } else { format!("https://{u}") }).collect()
}

pub fn run_warmup(port: u16, w: &Warmup, progress: &dyn Fn(String)) -> Result<usize, String> {
    let urls = pick_urls(w);
    if urls.is_empty() {
        return Err("The warm-up list has no addresses".into());
    }
    let dwell = Duration::from_secs(w.time_per_url.clamp(TIME_PER_URL.0, TIME_PER_URL.1) as u64);
    let depth = w.view_depth.clamp(VIEW_DEPTH.0, VIEW_DEPTH.1);
    let load = Duration::from_secs(LOAD_WAIT_SECS);
    let mut done = 0;
    for (i, url) in urls.iter().enumerate() {
        progress(format!("Warm-up {}/{}: {url}", i + 1, urls.len()));
        let new = format!("http://127.0.0.1:{port}/json/new?{url}");
        let resp = http(port).put(&new).call().or_else(|_| http(port).get(&new).call()).map_err(|e| e.to_string())?;
        let t: Value = resp.into_json().map_err(|e| e.to_string())?;
        let id = t["id"].as_str().unwrap_or("").to_string();
        if let Some(ws) = t["webSocketDebuggerUrl"].as_str() {
            if let Ok(mut c) = Cdp::connect(ws) {
                for d in 0..depth {
                    std::thread::sleep(load);
                    if w.accept_cookies {
                        if let Ok(Value::String(b)) = c.eval(ACCEPT_JS) {
                            if !b.is_empty() {
                                progress(format!("Accepted cookies on {url} ({b})"));
                            }
                        }
                    }
                    let _ = c.eval(SCROLL_JS);
                    std::thread::sleep(dwell);
                    if d + 1 < depth && !matches!(c.eval(CLICK_JS), Ok(Value::Bool(true))) {
                        break;
                    }
                }
            }
        }
        let _ = http(port).get(&format!("http://127.0.0.1:{port}/json/close/{id}")).call();
        done += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_formats() {
        let j = r#"[{"name":"a","value":"1","domain":".x.com","expirationDate":1999999999,"sameSite":"no_restriction"}]"#;
        let c = parse_cookies(j).unwrap();
        assert_eq!(c[0]["sameSite"], "None");
        assert_eq!(c[0]["path"], "/");
        let n = "# Netscape HTTP Cookie File\n#HttpOnly_.y.com\tTRUE\t/\tTRUE\t0\tsid\tabc\n";
        let c = parse_cookies(n).unwrap();
        assert_eq!((c[0]["name"].as_str(), c[0]["httpOnly"].as_bool()), (Some("sid"), Some(true)));
        assert!(parse_cookies("[]").is_err());
    }

    #[test]
    fn warmup_respects_limits() {
        let w = Warmup { urls: (0..80).map(|i| format!("site{i}.com")).collect(), url_count: 500, time_per_url: 0, view_depth: 1, accept_cookies: true };
        let p = pick_urls(&w);
        assert_eq!(p.len(), 50);
        assert!(p[0].starts_with("https://"));
    }

    #[test]
    fn args_follow_device() {
        let mut s = Session::new("t".into(), "d", devices::Preset::Mac);
        s.proxy.kind = ProxyKind::Socks5;
        let a = build_args(&s, Some(1234), 9000, Path::new("ext"), "UA", false).args;
        assert!(a.iter().any(|x| x == "--force-device-scale-factor=2"));
        assert!(a.iter().any(|x| x == "--proxy-server=socks5://127.0.0.1:1234"));
        assert!(a.iter().any(|x| x.starts_with("--force-webrtc-ip-handling-policy")));
        let fp = a.iter().find(|x| x.starts_with("--nebula-fp=")).unwrap();
        assert!(fp.contains("plat=macOS") && fp.contains("arch=arm") && fp.contains("gpur=ANGLE (Apple") && fp.contains("|cn=") && fp.contains("|fonts=") && !fp.contains("|tz="));
        for d in devices::DEVICES {
            assert!(!format!("{}{}{}", d.gpu_vendor, d.gpu_renderer, d.platform_version).contains('|'), "| separates --nebula-fp values");
        }
    }
}
