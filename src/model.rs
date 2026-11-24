use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn ago(ts: u64) -> String {
    if ts == 0 {
        return "never".into();
    }
    let s = now().saturating_sub(ts);
    let (n, unit) = match s {
        0..=59 => return "just now".into(),
        60..=3599 => (s / 60, "minute"),
        3600..=86_399 => (s / 3600, "hour"),
        86_400..=2_591_999 => (s / 86_400, "day"),
        2_592_000..=31_535_999 => (s / 2_592_000, "month"),
        _ => (s / 31_536_000, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

pub fn duration(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("NebulaSessions")
}

/// Tags from a comma-separated string: trimmed, at most 20 characters, no empty ones, no duplicates.
pub fn clean_tags(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in raw.split(',').map(|t| t.trim().chars().take(20).collect::<String>()) {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

pub fn profile_dir(uuid: &str) -> PathBuf {
    data_dir().join("profiles").join(uuid)
}

/// Connection types the original's "Set connection" API accepts that this rebuild implements.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum ProxyKind {
    #[default]
    Direct,
    Http,
    Socks5,
    Tor,
    /// A proxy already running on this computer; only the port is needed, SOCKS5 or HTTP is detected.
    Localhost,
    /// SSH tunnel (`ssh -D`); needs the system's OpenSSH client.
    Ssh,
}

impl ProxyKind {
    pub const ALL: [ProxyKind; 6] = [ProxyKind::Direct, ProxyKind::Http, ProxyKind::Socks5, ProxyKind::Ssh, ProxyKind::Localhost, ProxyKind::Tor];
    pub fn label(self) -> &'static str {
        match self {
            ProxyKind::Direct => "DIRECT",
            ProxyKind::Http => "HTTP",
            ProxyKind::Socks5 => "SOCKS5",
            ProxyKind::Tor => "TOR",
            ProxyKind::Localhost => "LOCALHOST",
            ProxyKind::Ssh => "SSH",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub login: String,
    pub password: String,
    pub change_ip_url: String,
    /// Custom DNS servers (`1.1.1.1;8.8.8.8`); empty = whatever the proxy or system resolves.
    pub dns: String,
}

impl Proxy {
    /// Accepts `host:port`, `host:port:user:pass`, `user:pass@host:port` and `scheme://user:pass@host:port`.
    pub fn parse(line: &str, default_kind: ProxyKind) -> Option<Proxy> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let (kind, rest) = match line.split_once("://") {
            Some((s, r)) => (
                match s.to_ascii_lowercase().as_str() {
                    "http" | "https" => ProxyKind::Http,
                    "socks5" | "socks5h" | "socks" => ProxyKind::Socks5,
                    _ => return None,
                },
                r,
            ),
            None => (if default_kind == ProxyKind::Direct { ProxyKind::Http } else { default_kind }, line),
        };
        let rest = rest.trim_end_matches('/');
        let (creds, hostport) = match rest.rsplit_once('@') {
            Some((c, h)) => (Some(c), h),
            None => (None, rest),
        };
        let parts: Vec<&str> = hostport.split(':').collect();
        let (host, port, mut login, mut password) = match parts.as_slice() {
            [h, p] => (*h, p.parse().ok()?, String::new(), String::new()),
            [h, p, u, pw] if creds.is_none() => (*h, p.parse().ok()?, u.to_string(), pw.to_string()),
            _ => return None,
        };
        if let Some(c) = creds {
            let (u, p) = c.split_once(':').unwrap_or((c, ""));
            login = u.to_string();
            password = p.to_string();
        }
        if host.is_empty() {
            return None;
        }
        Some(Proxy { kind, host: host.into(), port, login, password, ..Default::default() })
    }

    pub fn endpoint(&self) -> (String, u16) {
        if self.kind == ProxyKind::Localhost {
            return ("127.0.0.1".into(), self.port);
        }
        if self.kind == ProxyKind::Tor && self.host.is_empty() {
            return ("127.0.0.1".into(), if self.port == 0 { 9050 } else { self.port });
        }
        (self.host.clone(), self.port)
    }

    /// Proxy URL for the HTTP client used by the proxy checker.
    pub fn url(&self) -> Option<String> {
        let scheme = match self.kind {
            ProxyKind::Direct | ProxyKind::Ssh => return None, // SSH needs its tunnel first, see proxy::check
            ProxyKind::Http => "http",
            ProxyKind::Localhost => if crate::proxy::local_is_socks(self.port) { "socks5" } else { "http" },
            ProxyKind::Socks5 | ProxyKind::Tor => "socks5",
        };
        let (h, p) = self.endpoint();
        let creds = if self.login.is_empty() { String::new() } else { format!("{}:{}@", self.login, self.password) };
        Some(format!("{scheme}://{creds}{h}:{p}"))
    }

    pub fn short(&self) -> String {
        match self.kind {
            ProxyKind::Direct => "direct".into(),
            _ => {
                let (h, p) = self.endpoint();
                format!("{h}:{p}")
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct GeoInfo {
    pub ok: bool,
    pub error: String,
    pub ip: String,
    pub country_code: String,
    pub country: String,
    pub city: String,
    pub zip: String,
    pub timezone: String,
    pub isp: String,
    pub latency_ms: u64,
    pub fraud_score: Option<i64>,
    pub checked: u64,
}

impl GeoInfo {
    pub fn place(&self) -> String {
        if !self.ok {
            return self.error.clone();
        }
        let mut s = format!("{} ({})", self.country_code, self.country);
        if !self.city.is_empty() {
            s += &format!(" {}", self.city);
        }
        if !self.zip.is_empty() {
            s += &format!(", {}", self.zip);
        }
        s
    }
}

/// How a noisy fingerprint surface (canvas, audio, client rects) is treated.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FpMode {
    /// Stable per device model: every session on the same model shares the result, like real machines of one model.
    #[default]
    Real,
    /// Unique per session (classic noise).
    Noise,
    /// This computer's own value.
    Off,
}

impl FpMode {
    pub fn label(self) -> &'static str {
        match self {
            FpMode::Real => "Real device",
            FpMode::Noise => "Noise",
            FpMode::Off => "Off (this PC)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Fingerprint {
    pub device: String,
    pub screen: (u32, u32),
    pub seed: u32,
    /// Empty: built from the device and the browser's own version.
    pub user_agent: String,
    pub language: String,
    pub timezone: String,
    pub geo_from_proxy: bool,
    pub canvas: FpMode,
    pub webgl: FpMode,
    pub audio: FpMode,
    pub client_rects: FpMode,
    pub fonts: bool,
    pub hide_webgpu: bool,
    pub fake_media_devices: bool,
    pub webrtc: WebRtc,
}

/// How WebRTC is handled. Fake and N/A cannot rewrite candidates without a native patch, so they block leaks instead.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum WebRtc {
    /// Leak protection only when a proxy is used.
    #[default]
    Adaptive,
    Fake,
    Direct,
    Na,
    Off,
}

impl WebRtc {
    pub const ALL: [WebRtc; 5] = [WebRtc::Adaptive, WebRtc::Fake, WebRtc::Direct, WebRtc::Na, WebRtc::Off];
    pub fn label(self) -> &'static str {
        match self {
            WebRtc::Adaptive => "Adaptive",
            WebRtc::Fake => "Fake",
            WebRtc::Direct => "Direct",
            WebRtc::Na => "N/A",
            WebRtc::Off => "Off",
        }
    }
}

impl Default for Fingerprint {
    fn default() -> Self {
        Fingerprint {
            device: String::new(),
            screen: (1920, 1080),
            seed: fastrand::u32(..),
            user_agent: String::new(),
            language: "en-US".into(),
            timezone: String::new(),
            geo_from_proxy: true,
            canvas: FpMode::Real,
            webgl: FpMode::Real,
            audio: FpMode::Real,
            client_rects: FpMode::Real,
            fonts: true,
            hide_webgpu: false,
            fake_media_devices: true,
            webrtc: WebRtc::Adaptive,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Misc {
    pub restore_open_tabs: bool,
    pub hardware_acceleration: bool,
    pub disable_images: bool,
    pub enable_quic: bool,
    pub do_not_track: bool,
    pub start_url: String,
}

impl Default for Misc {
    fn default() -> Self {
        Misc {
            restore_open_tabs: true,
            hardware_acceleration: true,
            disable_images: false,
            enable_quic: false,
            do_not_track: false,
            start_url: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Session {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub desktop: String,
    pub created: u64,
    pub used: u64,
    pub modified: u64,
    pub run_secs: u64,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub trashed: bool,
    /// When it went to the trash (0 = unknown); trashed sessions are deleted 72 hours later, like the original.
    pub trashed_at: u64,
    pub warmed: bool,
    pub proxy: Proxy,
    pub geo: Option<GeoInfo>,
    pub fp: Fingerprint,
    pub misc: Misc,
    pub log: Vec<(u64, String)>,
}

impl Session {
    pub fn new(name: String, desktop: &str, preset: crate::devices::Preset) -> Session {
        let t = now();
        Session {
            uuid: uuid::Uuid::new_v4().to_string(),
            name,
            desktop: desktop.into(),
            created: t,
            modified: t,
            fp: crate::devices::random_fingerprint(preset),
            ..Default::default()
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.log.push((now(), msg.into()));
        if self.log.len() > 300 {
            self.log.remove(0);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Desktop {
    pub uuid: String,
    pub name: String,
}

/// Remote-desktop ports pages may not probe; the original's defaults (0x140591ed0, table at 0x14276fd00).
pub const DEFAULT_CLOSED_PORTS: [u16; 17] =
    [2112, 3389, 5900, 5901, 5902, 5903, 5279, 5939, 5931, 5944, 5950, 6039, 6040, 7070, 63333, 9602, 6565];

pub const DEFAULT_WARMUP_URLS: &str = "https://www.wikipedia.org\nhttps://www.youtube.com\nhttps://www.amazon.com\nhttps://www.reddit.com\nhttps://www.bbc.com\nhttps://www.cnn.com\nhttps://www.ebay.com\nhttps://www.imdb.com\nhttps://www.booking.com\nhttps://www.weather.com\nhttps://www.nytimes.com\nhttps://www.theguardian.com\nhttps://stackoverflow.com\nhttps://www.etsy.com\nhttps://www.spotify.com";

/// Popular sites by country (ISO code), for a warm-up list that looks like a local user's. Estimated from
/// public "most visited" rankings, not measured; sites that refuse automated visits simply yield fewer cookies.
const REGIONAL_SITES: &[(&str, &[&str])] = &[
    ("PL", &["allegro.pl", "onet.pl", "wp.pl", "interia.pl", "olx.pl", "gazeta.pl", "filmweb.pl", "ceneo.pl", "otomoto.pl", "tvn24.pl", "pudelek.pl", "wykop.pl"]),
    ("DE", &["spiegel.de", "bild.de", "ebay.de", "otto.de", "web.de", "gmx.net", "t-online.de", "chefkoch.de", "zalando.de", "kicker.de"]),
    ("FR", &["lemonde.fr", "leboncoin.fr", "lefigaro.fr", "orange.fr", "marmiton.org", "fnac.com", "cdiscount.com", "20minutes.fr", "allocine.fr"]),
    ("GB", &["bbc.co.uk", "theguardian.com", "dailymail.co.uk", "argos.co.uk", "rightmove.co.uk", "gumtree.com", "sky.com", "telegraph.co.uk"]),
    ("ES", &["elpais.com", "marca.com", "as.com", "elmundo.es", "idealista.com", "milanuncios.com", "20minutos.es"]),
    ("IT", &["repubblica.it", "corriere.it", "subito.it", "gazzetta.it", "ansa.it", "libero.it", "ilsole24ore.com"]),
    ("NL", &["marktplaats.nl", "nu.nl", "bol.com", "tweakers.net", "telegraaf.nl", "ad.nl"]),
    ("BR", &["globo.com", "uol.com.br", "mercadolivre.com.br", "magazineluiza.com.br", "olx.com.br", "terra.com.br"]),
    ("CZ", &["seznam.cz", "novinky.cz", "aktualne.cz", "alza.cz", "sreality.cz"]),
    ("US", &["nytimes.com", "cnn.com", "weather.com", "etsy.com", "imdb.com", "reddit.com"]),
];

/// Sites for the warm-up list: the country's own popular sites plus a few global ones, one per line.
pub fn regional_sites(country_code: &str) -> String {
    let cc = country_code.to_uppercase();
    let mut v: Vec<&str> = REGIONAL_SITES.iter().find(|(c, _)| *c == cc).map(|(_, s)| s.to_vec()).unwrap_or_default();
    v.extend(["wikipedia.org", "youtube.com", "amazon.com"]);
    if v.len() == 3 {
        return DEFAULT_WARMUP_URLS.into(); // no local list for this country: the global default
    }
    v.iter().map(|d| format!("https://www.{d}")).collect::<Vec<_>>().join("
")
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub closed_ports: Vec<u16>,
    pub enable_ip_stats: bool,
    pub ipqs_key: String,
    pub warmup_accept_cookies: bool,
    pub warmup_urls: String,
    pub prolongate_cookies: bool,
    /// Local API port; 0 = off.
    pub api_port: u16,
    /// Give sessions the Widevine CDM copied from the user's own Edge/Chrome (DRM video).
    pub widevine: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            closed_ports: DEFAULT_CLOSED_PORTS.to_vec(),
            enable_ip_stats: true,
            ipqs_key: String::new(),
            warmup_accept_cookies: true,
            warmup_urls: DEFAULT_WARMUP_URLS.into(),
            prolongate_cookies: true,
            api_port: 0,
            widevine: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Store {
    pub sessions: Vec<Session>,
    pub desktops: Vec<Desktop>,
    pub current_desktop: String,
    pub settings: Settings,
    pub banner_closed: bool,
    pub notifications: Vec<(u64, String)>,
    /// Proxy checker text, kept between runs.
    pub proxy_list: String,
    pub presets: Vec<UserPreset>,
    /// Name of the selected user preset; None = the built-in OS preset.
    pub active_preset: Option<String>,
}

/// A saved session setup: new sessions copy its connection, noise modes, WebRTC, language and browser options,
/// but get their own device, screen and seed.
#[derive(Serialize, Deserialize, Clone)]
pub struct UserPreset {
    pub name: String,
    pub base: crate::devices::Preset,
    pub template: Session,
}

impl Store {
    pub fn user_preset(&self) -> Option<&UserPreset> {
        let n = self.active_preset.as_ref()?;
        self.presets.iter().find(|p| &p.name == n)
    }

    pub fn preset_label(&self, os: crate::devices::Preset) -> String {
        self.user_preset().map(|p| p.name.clone()).unwrap_or_else(|| os.label().into())
    }

    /// A new session from the active preset (or the built-in `os` preset).
    pub fn session_from_preset(&self, name: String, desktop: &str, os: crate::devices::Preset) -> Session {
        let Some(u) = self.user_preset() else { return Session::new(name, desktop, os) };
        let mut s = Session::new(name, desktop, u.base);
        let t = &u.template;
        s.proxy = t.proxy.clone();
        s.misc = t.misc.clone();
        s.tags = t.tags.clone();
        s.description = t.description.clone();
        let (f, g) = (&mut s.fp, &t.fp);
        (f.canvas, f.webgl, f.audio, f.client_rects) = (g.canvas, g.webgl, g.audio, g.client_rects);
        (f.fonts, f.hide_webgpu, f.fake_media_devices, f.webrtc) = (g.fonts, g.hide_webgpu, g.fake_media_devices, g.webrtc);
        (f.language, f.timezone, f.geo_from_proxy) = (g.language.clone(), g.timezone.clone(), g.geo_from_proxy);
        s
    }
}

impl Store {
    pub fn load() -> Store {
        let mut s: Store = std::fs::read_to_string(data_dir().join("store.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(Store::fresh);
        s.ensure_desktop();
        s.purge_trash();
        s
    }

    /// First start: one sample session, like the original's "Desktop preset 1".
    pub fn fresh() -> Store {
        let mut st = Store::default();
        st.ensure_desktop();
        let mut s = Session::new("Desktop preset 1".into(), &st.current_desktop, crate::devices::Preset::Desktop);
        s.log("Session created");
        st.sessions.push(s);
        st
    }

    pub fn find(&mut self, uuid: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.uuid == uuid)
    }

    /// Deletes sessions (and their profile folders) that have been in the trash for over 72 hours.
    pub fn purge_trash(&mut self) {
        let limit = now().saturating_sub(72 * 3600);
        let old: Vec<String> = self.sessions.iter().filter(|s| s.trashed && s.trashed_at > 0 && s.trashed_at < limit).map(|s| s.uuid.clone()).collect();
        for id in &old {
            let _ = std::fs::remove_dir_all(profile_dir(id));
        }
        self.sessions.retain(|s| !old.contains(&s.uuid));
    }

    pub fn ensure_desktop(&mut self) {
        if self.desktops.is_empty() {
            self.desktops.push(Desktop { uuid: uuid::Uuid::new_v4().to_string(), name: "Desktop".into() });
        }
        if !self.desktops.iter().any(|d| d.uuid == self.current_desktop) {
            self.current_desktop = self.desktops[0].uuid.clone();
        }
        let first = self.desktops[0].uuid.clone();
        for s in &mut self.sessions {
            if !self.desktops.iter().any(|d| d.uuid == s.desktop) {
                s.desktop = first.clone();
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("store.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("store.json"))
    }

    pub fn desktop_name(&self) -> &str {
        self.desktops.iter().find(|d| d.uuid == self.current_desktop).map(|d| d.name.as_str()).unwrap_or("Desktop")
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notifications.push((now(), msg.into()));
        if self.notifications.len() > 100 {
            self.notifications.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proxy_lines() {
        let p = Proxy::parse("1.2.3.4:8080", ProxyKind::Http).unwrap();
        assert_eq!((p.host.as_str(), p.port, p.kind), ("1.2.3.4", 8080, ProxyKind::Http));
        let p = Proxy::parse("1.2.3.4:1080:bob:pw", ProxyKind::Socks5).unwrap();
        assert_eq!((p.login.as_str(), p.password.as_str(), p.kind), ("bob", "pw", ProxyKind::Socks5));
        let p = Proxy::parse("socks5://u:p@host.net:1080", ProxyKind::Http).unwrap();
        assert_eq!((p.kind, p.host.as_str(), p.login.as_str()), (ProxyKind::Socks5, "host.net", "u"));
        assert_eq!(p.url().unwrap(), "socks5://u:p@host.net:1080");
        assert!(Proxy::parse("nonsense", ProxyKind::Http).is_none());
        assert!(Proxy::parse("ftp://a:1", ProxyKind::Http).is_none());
    }
}

#[cfg(test)]
mod trash_tests {
    use super::*;

    #[test]
    fn tags_are_limited_and_unique() {
        assert_eq!(clean_tags(" a, b ,a,,abcdefghijklmnopqrstuvwxyz"), vec!["a", "b", "abcdefghijklmnopqrst"]);
    }

    #[test]
    fn only_old_trash_is_purged() {
        let mut st = Store::default();
        let t = now();
        let mk = |trashed, at| Session { uuid: uuid::Uuid::new_v4().to_string(), trashed, trashed_at: at, ..Default::default() };
        st.sessions = vec![mk(true, t - 73 * 3600), mk(true, t - 3600), mk(true, 0), mk(false, 0)];
        st.purge_trash();
        assert_eq!(st.sessions.len(), 3);
    }
}
