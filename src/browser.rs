//! Starting a Chromium-family browser for one session, the fingerprint extension,
//! and the DevTools-protocol cookie jobs (the warm-up is in warmup.rs).

use crate::devices::{self, Os};
use crate::model::{profile_dir, Proxy, ProxyKind, Session, Settings, WebRtc};
use crate::proxy::{open_ssh, start_relay, Relay, SshTunnel};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub use crate::cdp::{wait_ready, Conn};
use crate::cdp::{self, Cdp};

/// The Nebula Chromium that build.ps1 puts in a `chromium` folder beside the manager: the only browser sessions run in.
pub fn find_browser() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.parent()?.join("chromium").join("chrome.exe");
    exe.is_file().then_some(exe)
}

/// Major version from the VERSION file build.ps1 writes next to chrome.exe.
pub fn browser_major(exe: &Path) -> Option<u32> {
    std::fs::read_to_string(exe.parent()?.join("VERSION")).ok()?.trim().split('.').next()?.parse().ok()
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(9333)
}

pub struct Running {
    pub child: Child,
    /// The debug port, only when the local API asked for one (outside automation); otherwise the browser is
    /// reached through a private pipe and nothing listens.
    pub port: Option<u16>,
    pub conn: Conn,
    pub started: Instant,
    _relay: Option<Relay>,
    _ssh: Option<SshTunnel>,
}

impl Running {
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    pub fn stop(&mut self) {
        let _ = cdp::call(&self.conn, None, "Browser.close", json!({}));
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
/// How the manager controls the browser.
pub enum Debug {
    /// `--remote-debugging-port`: open to any local program, so only for outside automation that asked for it.
    Port(u16),
    /// `--remote-debugging-pipe` with the inherited pipe handles (the value is the `--remote-debugging-io-pipes` switch).
    Pipe(String),
}

/// Chromium's fake capture only reads Y4M/MJPEG video and WAV audio; there is no conversion.
pub fn media_file_error(path: &str, video: bool) -> Option<String> {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let (ok, want) = if video { (matches!(ext.as_str(), "y4m" | "mjpeg"), "Y4M or MJPEG") } else { (ext == "wav", "WAV") };
    (!ok).then(|| format!("the {} file must be {want} (no conversion is done), got .{ext}", if video { "camera" } else { "microphone" }))
}

fn media_file_ok(path: &str, video: bool) -> bool {
    !path.is_empty() && media_file_error(path, video).is_none() && Path::new(path).is_file()
}

pub fn build_args(s: &Session, relay_port: Option<u16>, debug: &Debug, closed_ports: &[u16], search: &str, ua: &str, headless: bool) -> LaunchInfo {
    let d = devices::get(&s.fp.device);
    let data = profile_dir(&s.uuid).join("data");
    let (x, y, w, h) = window_rect(s.fp.screen, d.os);
    let mut a = vec![
        format!("--user-data-dir={}", data.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-sync".into(),
        format!("--user-agent={ua}"),
        format!("--window-size={w},{h}"),
        format!("--window-position={x},{y}"),
        // Real pixel ratio of the modelled screen, so layouts and canvas sizes agree with it.
        format!("--force-device-scale-factor={}", d.dpr),
        // Read natively by the bundled patched Chromium (workers and client-hint headers too); other browsers ignore it.
        format!("--nebula-fp={}{}{}", nebula_fp(&s.fp, timezone(s).as_deref(), geolocation(s), s.misc.do_not_track, closed_ports), search_key(search), if s.misc.block_non_latin { "|nolatin=1" } else { "" }),
    ];
    if !s.fp.language.is_empty() {
        a.push(format!("--lang={}", s.fp.language));
        a.push(format!("--accept-lang={}", languages(&s.fp.language)));
    }
    if let Some(p) = relay_port {
        a.push(format!("--proxy-server=socks5://127.0.0.1:{p}"));
        a.push("--proxy-bypass-list=<-loopback>".into());
    }
    let block = match s.fp.webrtc {
        WebRtc::Adaptive => s.proxy.kind != ProxyKind::Direct,
        WebRtc::Direct => false,
        _ => true,
    };
    if block {
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
    if !m.block_popups {
        a.push("--disable-popup-blocking".into());
    }
    match debug {
        Debug::Port(p) => {
            a.push(format!("--remote-debugging-port={p}"));
            a.push("--remote-allow-origins=*".into());
        }
        Debug::Pipe(io) => {
            a.push("--remote-debugging-pipe".into());
            a.push(io.clone());
        }
    }
    a.push(if m.enable_quic { "--enable-quic" } else { "--disable-quic" }.into());
    // TLS hello of Chrome 158 stable. This Chromium tag turns on three things the stable build does not send yet
    // (trust-anchor-IDs 0xCA34, server padding 4832); with them on, JA4 is t13d1518h2_* instead of Chrome's
    // t13d1516h2_*. Measured against Edge 158 (tls.peet.ws): JA4, peetprint and the HTTP/2 fingerprint then match.
    a.push("--disable-features=TLSTrustAnchorIDs,NonMtcTrustAnchorIDs,AddTLSServerHandshakePadding".into());
    let (cam, mic) = (media_file_ok(&m.camera_file, true), media_file_ok(&m.mic_file, false));
    if cam || mic {
        // No --use-fake-ui-for-media-stream: it would auto-grant the permission prompt, which pages can tell apart.
        a.push("--use-fake-device-for-media-stream".into());
    }
    if cam {
        a.push(format!("--use-file-for-fake-video-capture={}", m.camera_file));
    }
    if mic {
        a.push(format!("--use-file-for-fake-audio-capture={}", m.mic_file));
    }
    if headless {
        a.push("--headless=new".into());
    }
    if !m.start_url.trim().is_empty() && !headless {
        a.push(m.start_url.trim().into());
    }
    LaunchInfo { args: a }
}

/// `navigator.languages` / Accept-Language of a locale: "en-US" gives "en-US,en", "de" gives "de".
fn languages(lang: &str) -> String {
    match lang.split('-').next() {
        Some(p) if p != lang => format!("{lang},{p}"),
        _ => lang.to_string(),
    }
}

/// `|search=<engine>` for a non-default search engine (0020-nebula-default-search.patch); Google is Chromium's own default.
fn search_key(search: &str) -> String {
    match search {
        "bing" | "duckduckgo" | "yandex" => format!("|search={search}"),
        _ => String::new(),
    }
}

/// `key=value|...` for the patched Chromium (chromium/patches/0001-nebula-device-values.patch).
pub fn nebula_fp(fp: &crate::model::Fingerprint, tz: Option<&str>, geo: Option<(f64, f64, u32)>, dnt: bool, closed_ports: &[u16]) -> String {
    let d = devices::get(&fp.device);
    let mut v = format!("cores={}|ram={}|plat={}|pver={}|arch={}", d.cores, d.ram, d.os.label(), d.platform_version, d.arch);
    // Navigator and screen, native in window, iframes and every worker (0005-nebula-navigator-screen.patch).
    let (w, h) = fp.screen;
    let depth = if d.os == Os::Mac { 30 } else { 24 };
    v += &format!("|np={}|touch={}|sw={w}|sh={h}|ah={}|cd={depth}", d.os.platform(), d.touch, h.saturating_sub(d.os.taskbar()));
    if !fp.language.is_empty() {
        v += &format!("|langs={}", languages(&fp.language));
    }
    if dnt {
        v += "|dnt=1";
    }
    // speechSynthesis voices of the modelled machine (0016-nebula-voices.patch); the host's own would show its OS and language.
    let lang = if fp.language.is_empty() { "en-US" } else { fp.language.as_str() };
    v += &format!("|voices={}", devices::speech_voices(d.os, lang));
    // Replacements for what the old extension did in JS (0017-nebula-audio-media-websocket.patch): the modelled
    // output sample rate, the device list a page sees before any permission, and local ports that look closed.
    v += &format!("|asr={}", d.sample_rate);
    // The modelled screen, battery and network as pages see them (0018-nebula-media-features-battery-connection.patch).
    v += &format!("|gamut={}|mqdef=1", d.gamut);
    let r = fp.seed.wrapping_mul(2_654_435_761); // a per-session spread, stable for the session
    if d.laptop {
        let charging = (r >> 28) < 9; // about 56 % of the time plugged in
        let percent = if charging && (r >> 20) % 3 == 0 { 100 } else { 30 + (r >> 8) % 70 };
        v += &format!("|batt={percent},{}", u8::from(charging));
    } else {
        v += "|batt=100,1"; // what Chrome reports without a battery
    }
    let rtt = [50, 75, 100][((r >> 4) % 3) as usize];
    let downlink = [10.0, 10.0, 7.7, 5.45][((r >> 12) % 4) as usize];
    v += &format!("|conn=4g,{rtt},{downlink}");
    // Storage quota of the modelled disk, V8 heap limit of the modelled RAM and the US keyboard map
    // (0024-nebula-quota-heap-keyboard.patch). Other languages keep the host's layout for now.
    v += &format!("|quota={}|heap={}", storage_quota(fp.seed), if d.ram >= 8 { 4_294_705_152u64 } else { 2_172_649_472 });
    if !fp.language.is_empty() && fp.language.split('-').next() == Some("en") {
        v += "|kbd=us";
    }
    // The window as pages see it, on the modelled screen rather than the host monitor (0023-nebula-window-screen.patch).
    let (x, y, ow, oh) = window_rect(fp.screen, d.os);
    v += &format!("|dpr={}|wx={x}|wy={y}|ow={ow}|oh={oh}|orient=landscape-primary", d.dpr);
    if fp.fake_media_devices {
        v += &format!("|mdev=1,1,1,{:x}", fp.seed);
    }
    if !closed_ports.is_empty() {
        v += &format!("|wsblock={}", closed_ports.iter().map(u16::to_string).collect::<Vec<_>>().join(","));
    }
    if fp.webgl != crate::model::FpMode::Off {
        v += &format!("|gpuv={}|gpur={}", d.gpu_vendor, d.gpu_renderer);
        v += &gl_profile(d.id);
    }
    // Timezone and font allow-list, also native (0003-nebula-timezone.patch, 0004-nebula-fonts.patch).
    if let Some(tz) = tz {
        v += &format!("|tz={tz}");
    }
    // navigator.geolocation answers with this instead of the host's position (0022-nebula-geolocation.patch).
    if let Some((lat, lon, acc)) = geo {
        v += &format!("|geo={lat:.6},{lon:.6},{acc}");
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

/// What `navigator.storage.estimate().quota` shows for a seeded disk of 256/512/1000/2000 GB: Chrome's pool is 80 % of
/// the disk and one origin may use 75 % of the pool (storage/browser/quota/quota_settings.cc). The OS reports a bit
/// less than the marketed size, hence the 0.93.
pub fn storage_quota(seed: u32) -> u64 {
    let gb = [256u64, 512, 1000, 2000][(seed.wrapping_mul(2_654_435_761) >> 16) as usize % 4];
    gb * 1_000_000_000 * 93 / 100 * 80 / 100 * 75 / 100
}

/// The session window: maximised on the modelled screen, so it fills the area the taskbar / menu bar leaves.
/// (x, y, width, height) in CSS pixels; the menu bar (macOS) and GNOME's top bar sit above it, the Windows taskbar below.
fn window_rect((w, h): (u32, u32), os: Os) -> (u32, u32, u32, u32) {
    let top = if os == Os::Windows { 0 } else { os.taskbar() };
    (0, top, w, h.saturating_sub(os.taskbar()))
}

/// `|glmax=..|glext=..|...` for one device model from chromium/gl_profiles.json
/// (0008-nebula-webgl.patch). Empty when the model has no profile: the real values stay.
fn gl_profile(id: &str) -> String {
    const GL: &str = include_str!("../chromium/gl_profiles.json");
    let Ok(j) = serde_json::from_str::<serde_json::Value>(GL) else { return String::new() };
    let Some(p) = j["profiles"].get(id) else { return String::new() };
    let mut out = String::new();
    for k in ["glmax", "glprec", "glext", "wgv", "wga", "wgsmin", "wgsmax"] {
        let val = p[k].as_str().map(String::from).or_else(|| p[k].as_u64().map(|n| n.to_string()));
        if let Some(val) = val {
            out += &format!("|{k}={val}");
        }
    }
    out
}

/// The bundled Chromium's sandbox runs the network service in an AppContainer, which can only start when
/// "ALL APPLICATION PACKAGES" (and the LPAC group) may read chrome.exe and its files. Chrome in Program Files
/// has that from the installer; a folder we unpack or build does not, and the browser then dies with
/// "Sandbox cannot access executable". Granting it needs no elevation for a folder the user owns. Only the
/// folder that build.ps1 marked with a VERSION file is touched, once (a marker file records it; a new package
/// has no marker, so it is granted again).
#[cfg(windows)]
fn ensure_appcontainer_access(exe: &Path) {
    let Some(dir) = exe.parent() else { return };
    let marker = dir.join(".appcontainer-acl");
    if marker.exists() || !dir.join("VERSION").exists() {
        return;
    }
    let mut cmd = Command::new("icacls");
    cmd.arg(dir).args(["/grant", "*S-1-15-2-1:(OI)(CI)(RX)", "*S-1-15-2-2:(OI)(CI)(RX)", "/T", "/C", "/Q"]);
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // no console window
    if cmd.status().is_ok_and(|s| s.success()) {
        let _ = std::fs::write(marker, "granted ALL APPLICATION PACKAGES read/execute");
    }
}

#[cfg(not(windows))]
fn ensure_appcontainer_access(_exe: &Path) {}

pub fn launch(settings: &Settings, s: &Session, headless: bool, debug_port: Option<u16>, extra_args: &[String]) -> Result<Running, String> {
    let exe = find_browser().ok_or("Nebula Chromium not found: put the chromium folder built by build.ps1 beside nebula-sessions.exe.")?;
    ensure_appcontainer_access(&exe);
    if settings.widevine {
        // Not fatal: without the CDM only DRM-protected video is missing.
        let _ = crate::widevine::prepare(&crate::model::data_dir().join("widevine"), &profile_dir(&s.uuid).join("data"));
    }
    crate::import::write_profile_prefs(&profile_dir(&s.uuid).join("data"), &s.misc);
    let major = browser_major(&exe).unwrap_or(140);
    let ua = devices::user_agent(&s.fp, major);
    let (ssh, upstream) = if s.proxy.kind == ProxyKind::Ssh {
        let t = open_ssh(&s.proxy)?;
        let p = Proxy { kind: ProxyKind::Socks5, host: "127.0.0.1".into(), port: t.port, dns: s.proxy.dns.clone(), ..Default::default() };
        (Some(t), p)
    } else {
        (None, s.proxy.clone())
    };
    let relay = match upstream.kind {
        ProxyKind::Direct if upstream.dns.trim().is_empty() => None,
        _ => Some(start_relay(upstream).map_err(|e| format!("Could not start the proxy relay: {e}"))?),
    };
    // A debug port only for outside automation that asked for one; otherwise a private pipe (no listening port).
    let (debug, pipes) = match debug_port {
        Some(p) => (Debug::Port(p), None),
        None => match cdp::winpipe::create() {
            Ok((write, read, child_ends)) => (Debug::Pipe(child_ends.switch()), Some((write, read, child_ends))),
            Err(_) => (Debug::Port(free_port()), None),
        },
    };
    let info = build_args(s, relay.as_ref().map(|r| r.port), &debug, &settings.closed_ports, &settings.search_engine, &ua, headless);
    let mut cmd = Command::new(&exe);
    cmd.args(&info.args);
    cmd.args(extra_args);
    if let Some(tz) = timezone(s) {
        cmd.env("TZ", tz);
    }
    let child = cmd.spawn().map_err(|e| format!("Could not start {}: {e}", exe.display()))?;
    let (port, conn) = match (debug, pipes) {
        (Debug::Port(p), _) => (Some(p).filter(|_| debug_port.is_some()), Cdp::port(p)),
        (Debug::Pipe(_), Some((write, read, child_ends))) => {
            drop(child_ends); // the browser has its own copies now; ours must go so its exit ends our reads
            (None, Cdp::pipe(write, read))
        }
        (Debug::Pipe(_), None) => unreachable!("a pipe switch always has its pipes"),
    };
    Ok(Running { child, port, conn, started: Instant::now(), _relay: relay, _ssh: ssh })
}

pub fn timezone(s: &Session) -> Option<String> {
    if s.fp.geo_from_proxy {
        if let Some(g) = s.geo.as_ref().filter(|g| g.ok && !g.timezone.is_empty()) {
            return Some(g.timezone.clone());
        }
    }
    (!s.fp.timezone.is_empty()).then(|| s.fp.timezone.clone())
}

/// Latitude, longitude and accuracy in metres for `navigator.geolocation`: the proxy's place, or the one set through the API.
/// None keeps the browser's real behaviour.
pub fn geolocation(s: &Session) -> Option<(f64, f64, u32)> {
    let r = s.fp.seed.wrapping_mul(2_246_822_519); // stable per session, independent of the spread in nebula_fp
    let accuracy = 20 + r % 131;
    if s.fp.geo_from_proxy {
        if let Some((lat, lon)) = s.geo.as_ref().filter(|g| g.ok).and_then(|g| g.lat.zip(g.lon)) {
            // ip-api gives the city centre, which every session in that city would share: move it up to ~3 km.
            let unit = |bits: u32| f64::from(bits & 0x7ff) / 1023.5 - 1.0;
            let lat2 = (lat + unit(r >> 21) * 0.027).clamp(-90.0, 90.0);
            let lon2 = (lon + unit(r >> 10) * 0.027 / lat.to_radians().cos().max(0.1) + 540.0) % 360.0 - 180.0;
            return Some((lat2, lon2, accuracy));
        }
    }
    s.fp.geo_pos.map(|(lat, lon)| (lat, lon, accuracy))
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
    let mut seen = std::collections::HashSet::new();
    let out: Vec<Value> = raw
        .iter()
        .filter_map(|c| {
            let name = c["name"].as_str().filter(|n| !n.is_empty())?;
            c["domain"].as_str().or(c["host"].as_str()).filter(|d| !d.is_empty())?;
            // duplicates (same name, domain and path) are dropped, the first one wins
            let key = (name.to_string(), c["domain"].as_str().or(c["host"].as_str())?.to_string(), c["path"].as_str().unwrap_or("/").to_string());
            if !seen.insert(key) {
                return None;
            }
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

pub fn import_cookies(conn: &Conn, cookies: &[Value]) -> Result<usize, String> {
    cdp::call(conn, None, "Storage.setCookies", json!({ "cookies": cookies }))?;
    Ok(cookies.len())
}

pub fn export_cookies(conn: &Conn) -> Result<Vec<Value>, String> {
    Ok(cdp::call(conn, None, "Storage.getCookies", json!({}))?["cookies"].as_array().cloned().unwrap_or_default())
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
    fn quota_follows_chrome_formula() {
        let all: Vec<u64> = (0..64).map(storage_quota).collect();
        assert!(all.contains(&285_696_000_000)); // 512 GB: 512e9 * 0.93 * 0.8 * 0.75
        assert!(all.iter().all(|q| [142_848_000_000, 285_696_000_000, 558_000_000_000, 1_116_000_000_000].contains(q)));
    }

    #[test]
    fn regional_lists() {
        let pl = crate::model::regional_sites("pl");
        assert!(pl.contains("https://www.allegro.pl") && pl.contains("wikipedia.org"));
        assert_eq!(crate::model::regional_sites("ZZ"), crate::model::DEFAULT_WARMUP_URLS);
        assert_eq!(crate::model::regional_sites(""), crate::model::DEFAULT_WARMUP_URLS);
    }

    #[test]
    fn media_file_formats() {
        assert!(media_file_error("a.Y4M", true).is_none() && media_file_error("a.mjpeg", true).is_none());
        assert!(media_file_error("a.mp4", true).is_some() && media_file_error("a.mp3", false).is_some() && media_file_error("a.wav", false).is_none());
        assert!(!media_file_ok("missing.wav", false));
    }

    #[test]
    fn window_fills_avail_area() {
        assert_eq!(window_rect((1920, 1080), Os::Windows), (0, 0, 1920, 1032));
        assert_eq!(window_rect((1470, 956), Os::Mac), (0, 25, 1470, 931));
    }

    #[test]
    fn args_follow_device() {
        let mut s = Session::new("t".into(), "d", devices::Preset::Mac);
        s.proxy.kind = ProxyKind::Socks5;
        s.fp.screen = (1470, 956);
        let a = build_args(&s, Some(1234), &Debug::Port(9000), &[3389, 5900], "", "UA", false).args;
        assert!(a.iter().any(|x| x == "--force-device-scale-factor=2"));
        assert!(a.iter().any(|x| x == "--window-size=1470,931") && a.iter().any(|x| x == "--window-position=0,25"));
        assert!(a.iter().any(|x| x == "--proxy-server=socks5://127.0.0.1:1234"));
        assert!(a.iter().any(|x| x.starts_with("--force-webrtc-ip-handling-policy")));
        let fp = a.iter().find(|x| x.starts_with("--nebula-fp=")).unwrap();
        assert!(fp.contains("|np=MacIntel|touch=0|sw=1470|sh=956|ah=931|cd=30|langs=en-US,en") && !fp.contains("|dnt=") && fp.contains("|voices=Alex~en-US~1~1;") && fp.contains(&format!("|asr={}|", devices::get(&s.fp.device).sample_rate)) && fp.contains("|mdev=1,1,1,") && fp.contains("|wsblock=3389,5900") && fp.contains("|mqdef=1|batt=") && fp.contains("|conn=4g,") && fp.contains("|dpr=2|wx=0|wy=25|ow=1470|oh=931|orient=landscape-primary"));
        assert!(a.iter().any(|x| x == "--accept-lang=en-US,en"));
        assert!(a.iter().any(|x| x == "--remote-debugging-port=9000") && a.iter().any(|x| x == "--remote-allow-origins=*"));
        // The default: a private pipe, no listening port and no open WebSocket origins.
        let p = build_args(&s, None, &Debug::Pipe("--remote-debugging-io-pipes=7,9".into()), &[], "", "UA", false).args;
        assert!(p.iter().any(|x| x == "--remote-debugging-pipe") && p.iter().any(|x| x == "--remote-debugging-io-pipes=7,9"));
        assert!(!p.iter().any(|x| x.starts_with("--remote-debugging-port") || x.starts_with("--remote-allow-origins")));
        assert!(!a.iter().any(|x| x.starts_with("--load-extension")), "no JS extension any more");
        assert!(!a.iter().any(|x| x == "--disable-popup-blocking"), "pop-ups blocked by default");
        let mut open = s.clone();
        open.misc.block_popups = false;
        assert!(build_args(&open, None, &Debug::Port(9000), &[], "", "UA", false).args.iter().any(|x| x == "--disable-popup-blocking"));
        let ddg = build_args(&s, None, &Debug::Port(9000), &[], "duckduckgo", "UA", false).args;
        assert!(ddg.iter().any(|x| x.starts_with("--nebula-fp=") && x.ends_with("|search=duckduckgo")) && !fp.contains("|search="), "only a non-Google engine is passed");
        assert!(!fp.contains("|nolatin="));
        let mut nl = s.clone();
        nl.misc.block_non_latin = true;
        assert!(build_args(&nl, None, &Debug::Port(9000), &[], "", "UA", false).args.iter().any(|x| x.starts_with("--nebula-fp=") && x.ends_with("|nolatin=1")));
        assert!(search_key("bogus").is_empty() && search_key("google").is_empty());
        assert!(geolocation(&s).is_none() && !fp.contains("|geo="), "no known position: the real behaviour");
        let mut g = s.clone();
        g.geo = Some(crate::model::GeoInfo { ok: true, lat: Some(52.23), lon: Some(21.01), ..Default::default() });
        let (lat, lon, acc) = geolocation(&g).unwrap();
        assert!((lat - 52.23).abs() <= 0.027 && (lon - 21.01).abs() <= 0.045 && (20..=150).contains(&acc), "{lat},{lon},{acc}");
        assert_eq!(geolocation(&g), geolocation(&g.clone()), "stable per session");
        assert!(build_args(&g, None, &Debug::Port(9000), &[], "", "UA", false).args.iter().any(|x| x.contains(&format!("|geo={lat:.6},{lon:.6},{acc}"))));
        g.fp.geo_from_proxy = false;
        g.fp.geo_pos = Some((-33.9, 151.2));
        assert_eq!(geolocation(&g).map(|p| (p.0, p.1)), Some((-33.9, 151.2)), "a position set through the API is used as given");
        assert!(a.iter().filter(|x| x.starts_with("--disable-features=")).count() == 1 && a.iter().any(|x| x.contains("AddTLSServerHandshakePadding")), "one --disable-features switch, with the TLS ones");
        assert!(fp.contains("plat=macOS") && fp.contains("arch=arm") && fp.contains("gpur=ANGLE (Apple") && fp.contains("|cn=") && fp.contains("|fonts=") && !fp.contains("|tz="));
        for d in devices::DEVICES {
            let mut fp = crate::model::Fingerprint::default();
            fp.device = d.id.to_string();
            let v = nebula_fp(&fp, None, None, false, &[]);
            assert!(v.split('|').all(|kv| kv.contains('=')), "{}: a value contains '|': {v}", d.id);
            assert!(v.contains("|glext="), "{}: no WebGL profile in gl_profiles.json", d.id);
            assert!(v.contains(&format!("|gamut={}|mqdef=1|", d.gamut)), "{}: gamut", d.id);
            if !d.laptop {
                assert!(v.contains("|batt=100,1|"), "{}: a desktop has no battery", d.id);
            }
            assert!(!format!("{}{}{}", d.gpu_vendor, d.gpu_renderer, d.platform_version).contains('|'), "| separates --nebula-fp values");
        }
    }

    #[test]
    fn languages_match_chrome_list() {
        // Chromium turns this list into "Accept-Language: pl-PL,pl;q=0.9,en-US;q=0.8,en;q=0.7" itself (q steps of 0.1).
        assert_eq!(languages("pl-PL"), "pl-PL,pl");
        assert_eq!(languages("en-US"), "en-US,en");
        assert_eq!(languages("de"), "de");
    }
}
