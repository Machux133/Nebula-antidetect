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
}

impl ProxyKind {
    pub const ALL: [ProxyKind; 4] = [ProxyKind::Direct, ProxyKind::Http, ProxyKind::Socks5, ProxyKind::Tor];
    pub fn label(self) -> &'static str {
        match self {
            ProxyKind::Direct => "DIRECT",
            ProxyKind::Http => "HTTP",
            ProxyKind::Socks5 => "SOCKS5",
            ProxyKind::Tor => "TOR",
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
        Some(Proxy { kind, host: host.into(), port, login, password, change_ip_url: String::new() })
    }

    pub fn endpoint(&self) -> (String, u16) {
        if self.kind == ProxyKind::Tor && self.host.is_empty() {
            return ("127.0.0.1".into(), if self.port == 0 { 9050 } else { self.port });
        }
        (self.host.clone(), self.port)
    }

    /// Proxy URL for the HTTP client used by the proxy checker.
    pub fn url(&self) -> Option<String> {
        let scheme = match self.kind {
            ProxyKind::Direct => return None,
            ProxyKind::Http => "http",
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
    pub const ALL: [FpMode; 3] = [FpMode::Real, FpMode::Noise, FpMode::Off];
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
    pub webrtc_protect: bool,
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
            webrtc_protect: true,
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub browser_path: String,
    pub closed_ports: Vec<u16>,
    pub enable_ip_stats: bool,
    pub ipqs_key: String,
    pub warmup_accept_cookies: bool,
    pub warmup_urls: String,
    pub prolongate_cookies: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            browser_path: String::new(),
            closed_ports: DEFAULT_CLOSED_PORTS.to_vec(),
            enable_ip_stats: true,
            ipqs_key: String::new(),
            warmup_accept_cookies: true,
            warmup_urls: DEFAULT_WARMUP_URLS.into(),
            prolongate_cookies: true,
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
}

impl Store {
    pub fn load() -> Store {
        let mut s: Store = std::fs::read_to_string(data_dir().join("store.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(Store::fresh);
        s.ensure_desktop();
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
