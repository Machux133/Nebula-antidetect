//! Bulk import: cookie files (one session each) and the universal XLSX/CSV table
//! (same columns as Linken Sphere's import template).

use crate::devices::{Os, Preset, DEVICES};
use crate::model::{profile_dir, FpMode, Proxy, ProxyKind, Session, WebRtc};
use std::path::Path;

pub const TEMPLATE_CSV: &str = "Session name,Description,Connection protocol,Ip,Port,Login,Password,Webrtc,Change ip url,Custom DNS,System,Browser,Cpu,Ram,Screen,System version,Video adapter,Canvas,Webgl,Client rects,Audio,Webgpu,Media devices,Import cookies,Import passwords\n\
Session 1,test 1,direct,,,,,fake,,,windows,chrome,4,8,1920x1080,11,,fake,direct,fake,direct,,direct,,\n";

/// Cookies wait here until the session is first started (they can only be set through a running browser).
pub fn pending_cookies_path(uuid: &str) -> std::path::PathBuf {
    profile_dir(uuid).join("pending_cookies.json")
}

pub fn read_table(path: &Path) -> Result<Vec<Vec<String>>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "csv" {
        let mut r = csv::ReaderBuilder::new().has_headers(false).flexible(true).from_path(path).map_err(|e| e.to_string())?;
        return r.records().map(|x| x.map(|r| r.iter().map(|c| c.trim_start_matches('\u{feff}').to_string()).collect()).map_err(|e| e.to_string())).collect();
    }
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let name = wb.sheet_names().first().cloned().ok_or("The workbook has no sheets")?;
    let range = wb.worksheet_range(&name).map_err(|e| e.to_string())?;
    Ok(range.rows().map(|r| r.iter().map(|c| c.to_string()).collect()).collect())
}

fn mode(v: &str) -> Option<FpMode> {
    match v.trim().to_ascii_lowercase().as_str() {
        "fake" => Some(FpMode::Noise),
        "direct" => Some(FpMode::Off),
        _ => None,
    }
}

fn num(v: &str) -> Option<u32> {
    v.trim().parse::<f64>().ok().filter(|n| *n >= 0.0).map(|n| n as u32)
}

/// Sessions from table rows; the second value lists what could not be applied.
pub fn sessions_from_table(rows: &[Vec<String>], preset: Preset, desktop: &str) -> Result<(Vec<Session>, Vec<String>), String> {
    let hi = rows
        .iter()
        .position(|r| r.iter().any(|c| c.trim().eq_ignore_ascii_case("session name") || c.trim().eq_ignore_ascii_case("connection protocol")))
        .ok_or("No header row (\"Session name\", \"Connection protocol\", …). Use the template.")?;
    let head: Vec<String> = rows[hi].iter().map(|c| c.trim().to_ascii_lowercase()).collect();
    let mut out = vec![];
    let mut warn = vec![];
    for (i, row) in rows[hi + 1..].iter().enumerate() {
        let get = |k: &str| head.iter().position(|h| h == k).and_then(|j| row.get(j)).map(|c| c.trim().to_string()).unwrap_or_default();
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let line = hi + i + 2;
        let mut name = get("session name");
        if name.is_empty() {
            name = format!("{} {}", preset.label(), out.len() + 1);
        }
        let mut s = Session::new(name.chars().take(50).collect(), desktop, preset);
        s.description = get("description");
        let (ip, port, login, password) = (get("ip"), num(&get("port")).unwrap_or(0) as u16, get("login"), get("password"));
        let kind = get("connection protocol").to_ascii_lowercase();
        let kind = match kind.as_str() {
            "" | "not set" | "direct" => ProxyKind::Direct,
            "tor" => ProxyKind::Tor,
            "socks5" | "socks" => ProxyKind::Socks5,
            "localhost" => ProxyKind::Localhost,
            "ssh" => ProxyKind::Ssh,
            "http" | "https" => ProxyKind::Http,
            other => {
                warn.push(format!("Row {line}: connection \"{other}\" is not supported here, using DIRECT"));
                ProxyKind::Direct
            }
        };
        if kind != ProxyKind::Direct && kind != ProxyKind::Tor {
            let host = if kind == ProxyKind::Localhost { "127.0.0.1".to_string() } else { ip };
            if host.is_empty() || port == 0 {
                warn.push(format!("Row {line}: missing ip/port, using DIRECT"));
            } else {
                s.proxy = Proxy { kind, host, port, login, password, change_ip_url: get("change ip url"), ..Default::default() };
            }
        } else {
            s.proxy.kind = kind;
        }
        match get("webrtc").to_ascii_lowercase().as_str() {
            "direct" => s.fp.webrtc = WebRtc::Direct,
            "fake" => s.fp.webrtc = WebRtc::Fake,
            "n/a" | "n a" => s.fp.webrtc = WebRtc::Na,
            "off" => s.fp.webrtc = WebRtc::Off,
            _ => {}
        }
        let dns = get("custom dns");
        if !dns.is_empty() && dns != "---" && !dns.eq_ignore_ascii_case("auto") && !dns.eq_ignore_ascii_case("ip") {
            s.proxy.dns = dns;
        }
        let sys = get("system").to_ascii_lowercase();
        let os = if sys.starts_with("win") { Some(Os::Windows) } else if sys.contains("mac") || sys.contains("arm") || sys.contains("intel") || sys.contains("apple") { Some(Os::Mac) } else if sys.contains("linux") { Some(Os::Linux) } else { None };
        if let Some(os) = os {
            let (cpu, gpu) = (num(&get("cpu")), get("video adapter").to_ascii_lowercase());
            let pool: Vec<_> = DEVICES.iter().filter(|d| d.os == os).collect();
            let best = [
                pool.iter().filter(|d| cpu == Some(d.cores) && !gpu.is_empty() && d.gpu_renderer.to_ascii_lowercase().contains(&gpu.replace(", ", " "))).collect::<Vec<_>>(),
                pool.iter().filter(|d| cpu == Some(d.cores)).collect(),
                pool.iter().collect(),
            ]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or_default();
            if let Some(d) = best.get(fastrand::usize(..best.len().max(1))) {
                s.fp.device = d.id.into();
                s.fp.screen = d.screens[0];
            }
        }
        if let Some((w, h)) = get("screen").to_ascii_lowercase().split_once('x').and_then(|(w, h)| Some((num(w)?, num(h)?))) {
            s.fp.screen = (w, h);
        }
        for (col, slot) in [("canvas", &mut s.fp.canvas), ("webgl", &mut s.fp.webgl), ("client rects", &mut s.fp.client_rects), ("audio", &mut s.fp.audio)] {
            if let Some(m) = mode(&get(col)) {
                *slot = m;
            }
        }
        if let Some(m) = mode(&get("media devices")) {
            s.fp.fake_media_devices = m == FpMode::Noise;
        }
        let cookies = get("import cookies");
        if !cookies.is_empty() {
            match crate::browser::parse_cookies(&cookies) {
                Ok(c) => save_pending(&s.uuid, &c),
                Err(e) => warn.push(format!("Row {line}: cookies skipped ({e})")),
            }
        }
        if !get("import passwords").is_empty() {
            warn.push(format!("Row {line}: passwords are not imported (no autofill here)"));
        }
        s.log("Imported from table");
        out.push(s);
    }
    if out.is_empty() {
        return Err("The table has no session rows".into());
    }
    Ok((out, warn))
}

pub fn save_pending(uuid: &str, cookies: &[serde_json::Value]) {
    let p = pending_cookies_path(uuid);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_vec(cookies).unwrap_or_default());
}

pub fn load_pending(uuid: &str) -> Option<Vec<serde_json::Value>> {
    serde_json::from_slice(&std::fs::read(pending_cookies_path(uuid)).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_row_becomes_session() {
        let rows: Vec<Vec<String>> = TEMPLATE_CSV.lines().map(|l| l.split(',').map(String::from).collect()).collect();
        let (s, w) = sessions_from_table(&rows, Preset::Desktop, "d").unwrap();
        assert_eq!(s.len(), 1);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(s[0].name, "Session 1");
        assert_eq!(s[0].fp.canvas, FpMode::Noise);
        assert_eq!(crate::devices::get(&s[0].fp.device).os, Os::Windows);
    }
}

#[cfg(test)]
mod xlsx_tests {
    #[test]
    fn real_template() {
        let p = std::path::Path::new("C:/Users/Mateusz/Downloads/template.xlsx");
        if !p.exists() { return; }
        let r = super::read_table(p).unwrap();
        let (s, w) = super::sessions_from_table(&r, crate::devices::Preset::Desktop, "d").unwrap();
        println!("{} {:?} {:?}", s.len(), w, s[0].fp.device);
        assert_eq!(s[0].name, "Session 1");
    }
}
