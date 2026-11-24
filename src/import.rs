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

/// Sets `value` at a nested `path`, making the objects on the way.
fn set_path(v: &mut serde_json::Value, path: &[&str], value: serde_json::Value) {
    let mut cur = v;
    for k in &path[..path.len() - 1] {
        if !cur[*k].is_object() {
            cur[*k] = serde_json::json!({});
        }
        cur = &mut cur[*k];
    }
    cur[path[path.len() - 1]] = value;
}

/// Reads a JSON object file (empty when missing or damaged), sets the values and writes it back.
fn merge_json(p: &std::path::Path, values: &[(&[&str], serde_json::Value)]) {
    let mut v = std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}));
    for (path, value) in values {
        set_path(&mut v, path, value.clone());
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_vec(&v).unwrap_or_default());
}

/// The session's Misc switches that Chromium keeps in its own preference files, written before launch. None of these prefs
/// is integrity-protected. History and passwords are profile prefs (`Default/Preferences`); Memory Saver is a local-state
/// pref (`Local State`, state 2 = on).
pub fn write_profile_prefs(data: &std::path::Path, m: &crate::model::Misc) {
    merge_json(
        &data.join("Default").join("Preferences"),
        &[
            (&["history", "saving_disabled"], (!m.save_history).into()),
            (&["credentials_enable_service"], m.save_passwords.into()),
            (&["credentials_enable_autosignin"], m.save_passwords.into()),
        ],
    );
    merge_json(&data.join("Local State"), &[(&["performance_tuning", "high_efficiency_mode", "state"], (if m.memory_saving { 2 } else { 0 }).into())]);
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// (name, url) pairs from a Netscape bookmarks file (`<A HREF="...">name</A>`, folders are flattened) or, when the text
/// has no such tags, a list of links, one per line with an optional name before the URL.
pub fn parse_bookmarks(text: &str) -> Vec<(String, String)> {
    let low = text.to_ascii_lowercase(); // same byte offsets as `text`
    let mut out = vec![];
    let mut at = 0;
    while let Some(i) = low[at..].find("<a ") {
        let tag = at + i;
        let Some(end) = low[tag..].find('>') else { break };
        let head = &text[tag..tag + end];
        let url = low[tag..tag + end].find("href=\"").and_then(|h| {
            let from = h + 6;
            head[from..].find('"').map(|e| unescape_html(&head[from..from + e]))
        });
        let body = tag + end + 1;
        let close = low[body..].find("</a>").map_or(text.len(), |c| body + c);
        if let Some(u) = url.filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
            let name = unescape_html(text[body..close].trim());
            out.push((if name.is_empty() { u.clone() } else { name }, u));
        }
        at = close;
    }
    if out.is_empty() && !low.contains("<a ") {
        for line in text.lines() {
            if let Some(i) = line.find("http://").or_else(|| line.find("https://")) {
                let url = line[i..].split_whitespace().next().unwrap_or("").to_string();
                let name = line[..i].trim().trim_end_matches(['-', ':', ',', '|', '\t']).trim();
                out.push((if name.is_empty() { url.clone() } else { name.to_string() }, url));
            }
        }
    }
    out
}

/// Adds the links to the "Bookmarks bar" of a stopped session's profile (`<data>/Default/Bookmarks`), skipping URLs that
/// are already anywhere in the file. Returns how many were added. The `checksum` is dropped so Chromium recomputes it.
pub fn add_bookmarks(data: &std::path::Path, items: &[(String, String)]) -> Result<usize, String> {
    use serde_json::{json, Value};
    let p = data.join("Default").join("Bookmarks");
    let mut v: Value = match std::fs::read(&p) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("The profile's Bookmarks file is damaged: {e}"))?,
        Err(_) => json!({"roots": {
            "bookmark_bar": {"children": [], "date_added": "0", "date_modified": "0", "guid": "0bc5d13f-2cba-5d74-951f-3f233fe6c908", "id": "1", "name": "Bookmarks bar", "type": "folder"},
            "other": {"children": [], "date_added": "0", "date_modified": "0", "guid": "82b081ec-3dd3-529c-8475-ab6c344590dd", "id": "2", "name": "Other bookmarks", "type": "folder"},
            "synced": {"children": [], "date_added": "0", "date_modified": "0", "guid": "4cf2e351-0e85-532b-bb37-df045d8f8d0f", "id": "3", "name": "Mobile bookmarks", "type": "folder"}},
            "version": 1}),
    };
    fn walk(n: &Value, ids: &mut u64, urls: &mut std::collections::HashSet<String>) {
        if let Some(i) = n["id"].as_str().and_then(|s| s.parse::<u64>().ok()) {
            *ids = (*ids).max(i);
        }
        if let Some(u) = n["url"].as_str() {
            urls.insert(u.to_string());
        }
        for c in n["children"].as_array().into_iter().flatten() {
            walk(c, ids, urls);
        }
    }
    let (mut max_id, mut have) = (3, std::collections::HashSet::new());
    for (_, root) in v["roots"].as_object().into_iter().flatten() {
        walk(root, &mut max_id, &mut have);
    }
    // Chromium time: microseconds since 1601-01-01.
    let stamp = ((crate::model::now() as u128 * 1000 + 11_644_473_600_000) * 1000).to_string();
    let bar = v["roots"]["bookmark_bar"]["children"].as_array_mut().ok_or("The profile's Bookmarks file has no bookmarks bar")?;
    let mut added = 0;
    for (name, url) in items {
        if !have.insert(url.clone()) {
            continue;
        }
        max_id += 1;
        bar.push(json!({"date_added": stamp, "guid": uuid::Uuid::new_v4().to_string(), "id": max_id.to_string(), "name": name, "type": "url", "url": url}));
        added += 1;
    }
    if let Some(o) = v.as_object_mut() {
        o.remove("checksum");
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_vec(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(added)
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

    #[test]
    fn bookmarks_netscape_and_list() {
        let html = r#"<DL><p><DT><H3>Work</H3><DL><DT><A HREF="https://a.example/?x=1&amp;y=2" ADD_DATE="1">Site &amp; A</A><DT><a href="http://b.example/">B</a></DL><DT><A HREF="javascript:alert(1)">bad</A>"#;
        assert_eq!(parse_bookmarks(html), vec![("Site & A".to_string(), "https://a.example/?x=1&y=2".to_string()), ("B".to_string(), "http://b.example/".to_string())]);
        assert_eq!(parse_bookmarks("https://c.example\nMy D - https://d.example/p  \nnot a link\n"), vec![("https://c.example".to_string(), "https://c.example".to_string()), ("My D".to_string(), "https://d.example/p".to_string())]);
        assert!(parse_bookmarks("").is_empty());
    }

    #[test]
    fn bookmarks_merge_into_profile_without_duplicates() {
        let d = std::env::temp_dir().join(format!("nebula-bm-{}", uuid::Uuid::new_v4()));
        let items = vec![("A".to_string(), "https://a.example/".to_string()), ("A again".to_string(), "https://a.example/".to_string()), ("B".to_string(), "https://b.example/".to_string())];
        assert_eq!(add_bookmarks(&d, &items).unwrap(), 2, "a repeated URL is added once");
        assert_eq!(add_bookmarks(&d, &items).unwrap(), 0, "a second import adds nothing");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("Default").join("Bookmarks")).unwrap()).unwrap();
        let kids = v["roots"]["bookmark_bar"]["children"].as_array().unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!((kids[0]["id"].as_str(), kids[1]["id"].as_str()), (Some("4"), Some("5")), "ids continue after the roots");
        assert!(v.get("checksum").is_none());
        std::fs::write(d.join("Default").join("Bookmarks"), b"{not json").unwrap();
        assert!(add_bookmarks(&d, &items).is_err(), "a damaged file is reported, not overwritten");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Writes a profile the way the manager would, for tests/probe/misc_test.mjs:
    /// `NEBULA_FIXTURE=<dir> cargo test write_fixture -- --ignored`.
    #[test]
    #[ignore]
    fn write_fixture() {
        let d = std::path::PathBuf::from(std::env::var("NEBULA_FIXTURE").expect("NEBULA_FIXTURE"));
        let items = parse_bookmarks(r#"<DT><A HREF="https://example.com/a">Nebula Example A</A><DT><A HREF="https://example.org/b">Nebula Example B</A>"#);
        add_bookmarks(&d, &items).unwrap();
        let mut m = crate::model::Misc::default();
        m.save_history = false;
        m.save_passwords = false;
        m.memory_saving = true;
        write_profile_prefs(&d, &m);
    }

    #[test]
    fn profile_prefs_keep_other_keys() {
        let d = std::env::temp_dir().join(format!("nebula-pf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("Default")).unwrap();
        std::fs::write(d.join("Default").join("Preferences"), br#"{"history":{"other":1},"keep":true}"#).unwrap();
        let mut m = crate::model::Misc::default();
        m.save_history = false;
        m.save_passwords = false;
        m.memory_saving = true;
        write_profile_prefs(&d, &m);
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("Default").join("Preferences")).unwrap()).unwrap();
        assert_eq!((v["history"]["saving_disabled"].as_bool(), v["history"]["other"].as_i64(), v["keep"].as_bool()), (Some(true), Some(1), Some(true)));
        let ls: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("Local State")).unwrap()).unwrap();
        assert_eq!((v["credentials_enable_service"].as_bool(), ls["performance_tuning"]["high_efficiency_mode"]["state"].as_i64()), (Some(false), Some(2)));
        let _ = std::fs::remove_dir_all(&d);
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
