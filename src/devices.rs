//! Real-device templates: every value of one template describes the same physical machine model,
//! so CPU cores, memory, screen, pixel ratio, GPU, fonts, audio rate and OS always agree.
//! Values are taken from the public specifications of each model (not captured from live machines).

use crate::model::{Fingerprint, FpMode, ProxyKind, Session};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub fn label(self) -> &'static str {
        match self {
            Os::Windows => "Windows",
            Os::Mac => "macOS",
            Os::Linux => "Linux",
        }
    }
    pub fn platform(self) -> &'static str {
        match self {
            Os::Windows => "Win32",
            Os::Mac => "MacIntel",
            Os::Linux => "Linux x86_64",
        }
    }
    pub fn ua_os(self) -> &'static str {
        match self {
            Os::Windows => "Windows NT 10.0; Win64; x64",
            Os::Mac => "Macintosh; Intel Mac OS X 10_15_7",
            Os::Linux => "X11; Linux x86_64",
        }
    }
    /// Space the taskbar / menu bar takes from the screen height.
    pub fn taskbar(self) -> u32 {
        match self {
            Os::Windows => 48,
            Os::Mac => 25,
            Os::Linux => 27,
        }
    }
    pub fn fonts(self) -> &'static [&'static str] {
        match self {
            Os::Windows => &[
                "Arial", "Arial Black", "Bahnschrift", "Calibri", "Cambria", "Cambria Math", "Candara", "Comic Sans MS",
                "Consolas", "Constantia", "Corbel", "Courier New", "Ebrima", "Franklin Gothic Medium", "Gabriola",
                "Gadugi", "Georgia", "Impact", "Ink Free", "Javanese Text", "Leelawadee UI", "Lucida Console",
                "Lucida Sans Unicode", "Malgun Gothic", "Marlett", "Microsoft Himalaya", "Microsoft JhengHei",
                "Microsoft New Tai Lue", "Microsoft PhagsPa", "Microsoft Sans Serif", "Microsoft Tai Le",
                "Microsoft YaHei", "Microsoft Yi Baiti", "MingLiU-ExtB", "Mongolian Baiti", "MS Gothic", "MV Boli",
                "Myanmar Text", "Nirmala UI", "Palatino Linotype", "Segoe MDL2 Assets", "Segoe Print", "Segoe Script",
                "Segoe UI", "Segoe UI Emoji", "Segoe UI Historic", "Segoe UI Symbol", "SimSun", "Sitka", "Sylfaen",
                "Symbol", "Tahoma", "Times New Roman", "Trebuchet MS", "Verdana", "Webdings", "Wingdings", "Yu Gothic",
                // Style and sibling names Chrome on Windows matches as families of their own; fingerprint scripts test
                // some of them (e.g. "Segoe UI Light", "MS UI Gothic"), and hiding them shows a Windows without its own fonts.
                "Calibri Light", "Leelawadee UI Semilight", "Malgun Gothic Semilight", "Microsoft JhengHei Light",
                "Microsoft JhengHei UI", "Microsoft JhengHei UI Light", "Microsoft YaHei Light", "Microsoft YaHei UI",
                "Microsoft YaHei UI Light", "MingLiU_HKSCS-ExtB", "MS PGothic", "MS UI Gothic", "Nirmala UI Semilight",
                "NSimSun", "PMingLiU-ExtB", "Segoe UI Black", "Segoe UI Light", "Segoe UI Semibold", "Segoe UI Semilight",
                "SimSun-ExtB", "Sitka Banner", "Sitka Display", "Sitka Heading", "Sitka Small", "Sitka Subheading",
                "Sitka Text", "Yu Gothic Light", "Yu Gothic Medium", "Yu Gothic UI", "Yu Gothic UI Light",
                "Yu Gothic UI Semibold", "Yu Gothic UI Semilight",
            ],
            Os::Mac => &[
                "American Typewriter", "Andale Mono", "Apple Chancery", "Apple Color Emoji", "Apple SD Gothic Neo",
                "Arial", "Arial Black", "Arial Narrow", "Arial Rounded MT Bold", "Arial Unicode MS", "Avenir",
                "Avenir Next", "Avenir Next Condensed", "Baskerville", "Big Caslon", "Bradley Hand", "Brush Script MT",
                "Chalkboard", "Chalkboard SE", "Chalkduster", "Charter", "Cochin", "Comic Sans MS", "Copperplate",
                "Courier", "Courier New", "Didot", "DIN Alternate", "DIN Condensed", "Futura", "Geneva", "Georgia",
                "Gill Sans", "Helvetica", "Helvetica Neue", "Herculanum", "Hoefler Text", "Impact", "Lucida Grande",
                "Luminari", "Marker Felt", "Menlo", "Monaco", "Noteworthy", "Optima", "Palatino", "Papyrus",
                "Phosphate", "Rockwell", "Savoye LET", "SignPainter", "Skia", "Snell Roundhand", "Tahoma", "Times",
                "Times New Roman", "Trattatello", "Trebuchet MS", "Verdana", "Zapfino",
            ],
            Os::Linux => &[
                "Cantarell", "DejaVu Sans", "DejaVu Sans Mono", "DejaVu Serif", "Droid Sans Fallback", "FreeMono",
                "FreeSans", "FreeSerif", "Liberation Mono", "Liberation Sans", "Liberation Sans Narrow",
                "Liberation Serif", "Noto Color Emoji", "Noto Mono", "Noto Sans", "Noto Sans Mono", "Noto Serif",
                "Ubuntu", "Ubuntu Condensed", "Ubuntu Mono",
            ],
        }
    }
    pub fn host() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Windows
        }
    }
}

pub struct Device {
    pub id: &'static str,
    pub label: &'static str,
    pub os: Os,
    pub cores: u32,
    /// navigator.deviceMemory: Chrome reports 2, 4 or 8 at most.
    pub ram: u32,
    /// CSS-pixel screen sizes this model ships with.
    pub screens: &'static [(u32, u32)],
    pub dpr: f32,
    pub gpu_vendor: &'static str,
    pub gpu_renderer: &'static str,
    pub sample_rate: u32,
    pub arch: &'static str,
    pub platform_version: &'static str,
    pub touch: u32,
    /// Colour gamut of the built-in/typical display: "p3" or "srgb" (the `(color-gamut)` media feature).
    pub gamut: &'static str,
    /// Has a battery: laptops report a level and a charging state, desktops "full and plugged in".
    pub laptop: bool,
}

pub static DEVICES: &[Device] = &[
    Device {
        id: "mba-m2", label: "MacBook Air 13\" (Apple M2)", os: Os::Mac, cores: 8, ram: 8,
        screens: &[(1470, 956), (1280, 832)], dpr: 2.0,
        gpu_vendor: "Google Inc. (Apple)", gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)",
        sample_rate: 48000, arch: "arm", platform_version: "14.5.0", touch: 0, gamut: "p3", laptop: true,
    },
    Device {
        id: "mbp14-m3pro", label: "MacBook Pro 14\" (Apple M3 Pro)", os: Os::Mac, cores: 12, ram: 8,
        screens: &[(1512, 982), (1352, 878)], dpr: 2.0,
        gpu_vendor: "Google Inc. (Apple)", gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3 Pro, Unspecified Version)",
        sample_rate: 48000, arch: "arm", platform_version: "15.1.0", touch: 0, gamut: "p3", laptop: true,
    },
    Device {
        id: "imac24-m1", label: "iMac 24\" (Apple M1)", os: Os::Mac, cores: 8, ram: 8,
        screens: &[(2240, 1260), (2048, 1152)], dpr: 2.0,
        gpu_vendor: "Google Inc. (Apple)", gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M1, Unspecified Version)",
        sample_rate: 44100, arch: "arm", platform_version: "14.6.1", touch: 0, gamut: "p3", laptop: false,
    },
    Device {
        id: "win-iris-xe", label: "Windows laptop (Core i5-1235U, Iris Xe)", os: Os::Windows, cores: 12, ram: 8,
        screens: &[(1536, 864), (1920, 1080)], dpr: 1.25,
        gpu_vendor: "Google Inc. (Intel)", gpu_renderer: "ANGLE (Intel, Intel(R) Iris(R) Xe Graphics (0x000046A8) Direct3D11 vs_5_0 ps_5_0, D3D11)",
        sample_rate: 48000, arch: "x86", platform_version: "15.0.0", touch: 0, gamut: "srgb", laptop: true,
    },
    Device {
        id: "win-rtx3060", label: "Windows desktop (Ryzen 5 5600X, RTX 3060)", os: Os::Windows, cores: 12, ram: 8,
        screens: &[(1920, 1080), (2560, 1440)], dpr: 1.0,
        gpu_vendor: "Google Inc. (NVIDIA)", gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 vs_5_0 ps_5_0, D3D11)",
        sample_rate: 48000, arch: "x86", platform_version: "15.0.0", touch: 0, gamut: "srgb", laptop: false,
    },
    Device {
        id: "win-gtx1660s", label: "Windows desktop (Core i7-12700, GTX 1660 SUPER)", os: Os::Windows, cores: 20, ram: 8,
        screens: &[(2560, 1440), (1920, 1080)], dpr: 1.0,
        gpu_vendor: "Google Inc. (NVIDIA)", gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 SUPER (0x000021C4) Direct3D11 vs_5_0 ps_5_0, D3D11)",
        sample_rate: 48000, arch: "x86", platform_version: "15.0.0", touch: 0, gamut: "srgb", laptop: false,
    },
    Device {
        id: "win-radeon-5700u", label: "Windows laptop (Ryzen 7 5700U, Radeon)", os: Os::Windows, cores: 16, ram: 8,
        screens: &[(1536, 864), (1280, 720)], dpr: 1.25,
        gpu_vendor: "Google Inc. (AMD)", gpu_renderer: "ANGLE (AMD, AMD Radeon(TM) Graphics (0x00001638) Direct3D11 vs_5_0 ps_5_0, D3D11)",
        sample_rate: 48000, arch: "x86", platform_version: "15.0.0", touch: 0, gamut: "srgb", laptop: true,
    },
    Device {
        id: "linux-uhd620", label: "Linux laptop (Core i5-8250U, UHD 620)", os: Os::Linux, cores: 8, ram: 8,
        screens: &[(1920, 1080), (1366, 768)], dpr: 1.0,
        gpu_vendor: "Google Inc. (Intel)", gpu_renderer: "ANGLE (Intel, Mesa Intel(R) UHD Graphics 620 (KBL GT2), OpenGL 4.6)",
        sample_rate: 44100, arch: "x86", platform_version: "6.5.0", touch: 0, gamut: "srgb", laptop: true,
    },
];

pub fn get(id: &str) -> &'static Device {
    DEVICES.iter().find(|d| d.id == id).unwrap_or(&DEVICES[3])
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Preset {
    Desktop,
    Windows,
    Mac,
    Linux,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Desktop, Preset::Windows, Preset::Mac, Preset::Linux];
    pub fn label(self) -> &'static str {
        match self {
            Preset::Desktop => "Desktop preset",
            Preset::Windows => "Windows preset",
            Preset::Mac => "macOS preset",
            Preset::Linux => "Linux preset",
        }
    }
}

/// A fresh, internally consistent fingerprint drawn from one device model.
pub fn random_fingerprint(preset: Preset) -> Fingerprint {
    let pool: Vec<&Device> = DEVICES
        .iter()
        .filter(|d| match preset {
            Preset::Desktop => d.os == Os::host(),
            Preset::Windows => d.os == Os::Windows,
            Preset::Mac => d.os == Os::Mac,
            Preset::Linux => d.os == Os::Linux,
        })
        .collect();
    let pool = if pool.is_empty() { DEVICES.iter().collect() } else { pool };
    let d = pool[fastrand::usize(..pool.len())];
    Fingerprint { device: d.id.into(), screen: d.screens[fastrand::usize(..d.screens.len())], ..Fingerprint::default() }
}

pub fn user_agent(fp: &Fingerprint, major: u32) -> String {
    if !fp.user_agent.trim().is_empty() {
        return fp.user_agent.trim().to_string();
    }
    let d = get(&fp.device);
    format!(
        "Mozilla/5.0 ({}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36",
        d.os.ua_os()
    )
}

/// Stable seed for one device model, so every session on that model gives the same canvas/audio result.
pub fn model_seed(id: &str) -> u32 {
    id.bytes().fold(2_166_136_261u32, |h, b| (h ^ b as u32).wrapping_mul(16_777_619))
}

pub fn seed_for(mode: FpMode, fp: &Fingerprint) -> Option<u32> {
    match mode {
        FpMode::Real => Some(model_seed(&fp.device)),
        FpMode::Noise => Some(fp.seed),
        FpMode::Off => None,
    }
}

// ------------------------------------------------------------------ speech voices
// Estimated from memory of what Chrome on each OS lists (names and languages), not captured from real machines.

/// OS voices of Windows by language: (language, names). The names follow Windows' "Microsoft <Name> - <Language> (<Country>)".
const WIN_VOICES: &[(&str, &[&str])] = &[
    ("en-US", &["Microsoft David - English (United States)", "Microsoft Mark - English (United States)", "Microsoft Zira - English (United States)"]),
    ("en-GB", &["Microsoft George - English (United Kingdom)", "Microsoft Hazel - English (United Kingdom)", "Microsoft Susan - English (United Kingdom)"]),
    ("pl-PL", &["Microsoft Adam - Polish (Poland)", "Microsoft Paulina - Polish (Poland)"]),
    ("de-DE", &["Microsoft Hedda - German (Germany)", "Microsoft Stefan - German (Germany)", "Microsoft Katja - German (Germany)"]),
    ("fr-FR", &["Microsoft Hortense - French (France)", "Microsoft Paul - French (France)", "Microsoft Julie - French (France)"]),
    ("es-ES", &["Microsoft Helena - Spanish (Spain)", "Microsoft Pablo - Spanish (Spain)", "Microsoft Laura - Spanish (Spain)"]),
    ("it-IT", &["Microsoft Elsa - Italian (Italy)", "Microsoft Cosimo - Italian (Italy)"]),
    ("pt-BR", &["Microsoft Maria - Portuguese (Brazil)", "Microsoft Daniel - Portuguese (Brazil)"]),
    ("ru-RU", &["Microsoft Irina - Russian (Russia)", "Microsoft Pavel - Russian (Russia)"]),
    ("nl-NL", &["Microsoft Frank - Dutch (Netherlands)"]),
    ("cs-CZ", &["Microsoft Jakub - Czech (Czech Republic)"]),
    ("ja-JP", &["Microsoft Haruka - Japanese (Japan)", "Microsoft Ichiro - Japanese (Japan)"]),
    ("zh-CN", &["Microsoft Huihui - Chinese (Simplified, PRC)", "Microsoft Kangkang - Chinese (Simplified, PRC)", "Microsoft Yaoyao - Chinese (Simplified, PRC)"]),
];

/// macOS voices by language (Apple names), plus the English novelty voices every Mac has.
const MAC_VOICES: &[(&str, &[&str])] = &[
    ("en-US", &["Alex", "Fred", "Samantha", "Victoria", "Junior", "Kathy", "Ralph", "Albert", "Bad News", "Bahh", "Bells", "Boing", "Bubbles", "Cellos", "Good News", "Jester", "Organ", "Superstar", "Trinoids", "Whisper", "Wobble", "Zarvox"]),
    ("en-GB", &["Daniel"]),
    ("en-AU", &["Karen"]),
    ("en-IE", &["Moira"]),
    ("pl-PL", &["Zosia"]),
    ("de-DE", &["Anna"]),
    ("fr-FR", &["Thomas", "Amelie"]),
    ("es-ES", &["Monica", "Jorge"]),
    ("it-IT", &["Alice", "Luca"]),
    ("pt-BR", &["Luciana", "Felipe"]),
    ("ru-RU", &["Milena", "Yuri"]),
    ("nl-NL", &["Xander", "Ellen"]),
    ("cs-CZ", &["Zuzana"]),
    ("ja-JP", &["Kyoko", "Otoya"]),
    ("zh-CN", &["Ting-Ting"]),
];

/// The remote voices Google Chrome adds on every desktop OS (not local).
const GOOGLE_VOICES: &[(&str, &str)] = &[
    ("Google Deutsch", "de-DE"),
    ("Google US English", "en-US"),
    ("Google UK English Female", "en-GB"),
    ("Google UK English Male", "en-GB"),
    ("Google espa\u{f1}ol", "es-ES"),
    ("Google espa\u{f1}ol de Estados Unidos", "es-US"),
    ("Google fran\u{e7}ais", "fr-FR"),
    ("Google \u{939}\u{93f}\u{928}\u{94d}\u{926}\u{940}", "hi-IN"),
    ("Google Bahasa Indonesia", "id-ID"),
    ("Google italiano", "it-IT"),
    ("Google \u{65e5}\u{672c}\u{8a9e}", "ja-JP"),
    ("Google \u{d55c}\u{ad6d}\u{c758}", "ko-KR"),
    ("Google Nederlands", "nl-NL"),
    ("Google polski", "pl-PL"),
    ("Google portugu\u{ea}s do Brasil", "pt-BR"),
    ("Google \u{440}\u{443}\u{441}\u{441}\u{43a}\u{438}\u{439}", "ru-RU"),
    ("Google \u{666e}\u{901a}\u{8bdd}\u{ff08}\u{4e2d}\u{56fd}\u{5927}\u{9646}\u{ff09}", "zh-CN"),
    ("Google \u{7cb5}\u{8a9e}\u{ff08}\u{9999}\u{6e2f}\u{ff09}", "zh-HK"),
    ("Google \u{570b}\u{8a9e}\u{ff08}\u{81fa}\u{7058}\u{ff09}", "zh-TW"),
];

/// Percent-encodes everything but [A-Za-z0-9-_.], so the voice list survives the command line as ASCII.
fn pct(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The modelled machine's `speechSynthesis` voices as `name~lang~local~default;...` (fields percent-encoded) for
/// `--nebula-fp voices=`: the OS voices for the session's language, then Google Chrome's remote voices. The first OS
/// voice is the default. Linux has no OS voices (speech-dispatcher is rarely installed), only the remote ones.
pub fn speech_voices(os: Os, language: &str) -> String {
    let table: &[(&str, &[&str])] = match os {
        Os::Windows => WIN_VOICES,
        Os::Mac => MAC_VOICES,
        Os::Linux => &[],
    };
    // Exact language tag, else the same primary language ("pl" -> pl-PL), else English (US).
    let primary = language.split('-').next().unwrap_or("en");
    let pick = table
        .iter()
        .find(|(l, _)| l.eq_ignore_ascii_case(language))
        .or_else(|| table.iter().find(|(l, _)| l.split('-').next() == Some(primary)))
        .or_else(|| table.iter().find(|(l, _)| *l == "en-US"));
    let mut out: Vec<String> = vec![];
    if let Some((lang, names)) = pick {
        for (i, n) in names.iter().enumerate() {
            out.push(format!("{}~{}~1~{}", pct(n), lang, u8::from(i == 0)));
        }
    }
    for (n, lang) in GOOGLE_VOICES {
        out.push(format!("{}~{}~0~0", pct(n), lang));
    }
    out.join(";")
}

/// Signals that would not match one physical machine. `true` = serious mismatch.
pub fn consistency(s: &Session) -> Vec<(bool, String)> {
    let fp = &s.fp;
    let d = get(&fp.device);
    let mut out = Vec::new();
    let ua = fp.user_agent.trim();
    if !ua.is_empty() && !ua.contains(d.os.ua_os().split(';').next().unwrap_or("")) {
        out.push((true, format!("The user agent does not name {} like the {} does.", d.os.label(), d.label)));
    }
    if !d.screens.contains(&fp.screen) {
        out.push((true, format!("Screen {}x{} is not one the {} ships with.", fp.screen.0, fp.screen.1, d.label)));
    }
    if fp.webgl == FpMode::Off && d.os != Os::host() {
        out.push((true, "WebGL shows this computer's graphics card, not the profile's.".into()));
    }
    if fp.canvas == FpMode::Off && fp.webgl != FpMode::Off {
        out.push((false, "Canvas shows this computer's rendering while WebGL claims another card.".into()));
    }
    if fp.canvas == FpMode::Noise || fp.audio == FpMode::Noise {
        out.push((false, "Noise makes this session unique; no real machine of this model has its canvas/audio hash.".into()));
    }
    if !fp.fonts && d.os != Os::host() {
        out.push((true, format!("All fonts of this {} computer are visible.", Os::host().label())));
    }
    if d.os != Os::host() {
        out.push((false, format!(
            "Fonts and rendering come from this {} computer: Mac/Linux-only fonts can be hidden from the list but not added.",
            Os::host().label()
        )));
    }
    if d.os != Os::host() && s.proxy.kind == ProxyKind::Direct {
        out.push((true, format!(
            "Direct connection: sites can read this {} computer's TCP/IP traits (TTL, window), which contradict the {} profile. With a proxy they see the proxy's stack instead.",
            Os::host().label(),
            d.os.label()
        )));
    }
    if s.proxy.kind != ProxyKind::Direct && fp.timezone.is_empty() && !fp.geo_from_proxy {
        out.push((true, "The time zone does not follow the proxy's location.".into()));
    }
    if s.proxy.kind != ProxyKind::Direct && fp.webrtc == crate::model::WebRtc::Direct {
        out.push((true, "WebRTC can reveal the real address behind the proxy.".into()));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn speech_voices_format() {
        let win_pl = speech_voices(Os::Windows, "pl-PL");
        assert!(win_pl.starts_with("Microsoft%20Adam%20-%20Polish%20%28Poland%29~pl-PL~1~1;Microsoft%20Paulina"));
        assert!(speech_voices(Os::Windows, "pl").contains("~pl-PL~1~")); // primary language only
        assert!(speech_voices(Os::Windows, "xx-YY").starts_with("Microsoft%20David")); // unknown: English (US)
        assert!(speech_voices(Os::Mac, "en-US").starts_with("Alex~en-US~1~1;Fred~"));
        // Linux has no OS voices: every entry is a remote one
        assert!(speech_voices(Os::Linux, "en-US").split(';').all(|v| v.ends_with("~0~0")));
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let v = speech_voices(os, "en-US");
            assert!(v.is_ascii() && !v.contains('|') && !v.contains(' '), "must survive the command line");
            assert!(v.split(';').all(|e| e.split('~').count() == 4));
        }
    }

    use super::*;

    #[test]
    fn templates_are_coherent() {
        for d in DEVICES {
            assert!([2, 4, 8].contains(&d.ram), "{}", d.id);
            assert!(!d.screens.is_empty());
            let apple = d.gpu_renderer.contains("Apple");
            assert_eq!(apple, d.os == Os::Mac, "{}", d.id);
            assert_eq!(d.arch == "arm", apple, "{}", d.id);
        }
        let fp = random_fingerprint(Preset::Mac);
        assert_eq!(get(&fp.device).os, Os::Mac);
        assert!(user_agent(&fp, 140).contains("Macintosh"));
    }
}
