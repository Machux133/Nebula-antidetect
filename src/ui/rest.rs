//! Handlers of the local REST API (see `crate::api` for the HTTP side), shaped like Linken Sphere's documented API
//! (Postman collection "Linken Sphere API", LS2 2.23.9). Everything that needs the producer's server (sign-in, sharing,
//! teams, free proxies) is left out. Requests act on the sessions of this computer; `uuid` or `uuids` select them.

use super::*;
use serde_json::{json, Value};

type Out = (u16, Value);

fn err(m: impl Into<String>) -> Out {
    (400, json!({"error": m.into()}))
}

/// `uuid` and `uuids` of a request body, in one list.
fn ids(b: &Value) -> Vec<String> {
    let mut v: Vec<String> = b["uuids"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    if let Some(u) = b["uuid"].as_str() {
        v.push(u.into());
    }
    v
}

/// The reply for one or many sessions: `{uuid}` for a single `uuid` request, `{uuids}` otherwise.
fn reply_ids(b: &Value, v: &[String]) -> Out {
    if b["uuids"].is_array() {
        (200, json!({"uuids": v}))
    } else {
        (200, json!({"uuid": v.first().cloned().unwrap_or_default()}))
    }
}

/// Our log lines mapped to the original's action ids (0 Start, 1 Stop, 2 Edit, 3 ChangeProxy, 5 ImportCookies,
/// 6 ExportCookies, 7 RemoveAllData, 8 Warmup, 9 Clone, 10 MoveToTrash, 11 Restore, 12 ChangeDescription, 15 Pin,
/// 16 Unpin, 17 TagsChanged). Best effort from the text; -1 when no id fits.
fn action_of(msg: &str) -> (i64, &'static str) {
    const TABLE: &[(&str, i64, &str)] = &[
        ("backup created", 18, "Backup"),
        ("restored from backup", 19, "RestoredFromBackup"),
        ("browser started", 0, "Start"),
        ("browser stopped", 1, "Stop"),
        ("browser closed", 1, "Stop"),
        ("proxy set", 3, "ChangeProxy"),
        ("connection set", 3, "ChangeProxy"),
        ("cookies", 5, "ImportCookies"),
        ("warm", 8, "Warmup"),
        ("clone", 9, "Clone"),
        ("trash", 10, "MoveToTrash"),
        ("restored", 11, "Restore"),
        ("description", 12, "ChangeDescription"),
        ("pinned", 15, "Pin"),
        ("tags", 17, "TagsChanged"),
        ("edited", 2, "Edit"),
    ];
    let l = msg.to_lowercase();
    TABLE.iter().find(|(k, _, _)| l.contains(k)).map_or((-1, "Other"), |(_, a, n)| (*a, *n))
}

fn proxy_json(p: &Proxy) -> Value {
    if p.kind == ProxyKind::Direct {
        return json!({"protocol": "direct"});
    }
    let (ip, port) = p.endpoint();
    json!({"protocol": p.kind.label().to_lowercase(), "ip": ip, "port": port})
}

fn mode_of(v: &Value, what: &str) -> Result<Option<FpMode>, Out> {
    match v.as_str() {
        None => Ok(None),
        Some("direct") => Ok(Some(FpMode::Real)),
        Some("fake") => Ok(Some(FpMode::Noise)),
        Some(o) => Err(err(format!("{what}: \"{o}\" is not \"direct\" or \"fake\""))),
    }
}

fn number_str(v: &Value) -> Option<u32> {
    v.as_u64().map(|n| n as u32).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn dir_size(p: &std::path::Path) -> u64 {
    std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.metadata().map_or(0, |m| if m.is_dir() { dir_size(&e.path()) } else { m.len() })).sum()
}

impl App {
    fn api_status(&self, id: &str) -> &'static str {
        match self.running.get(id) {
            Some(r) if r.port.is_some() => "automationRunning",
            Some(_) => "running",
            None if self.busy.get(id).is_some_and(|b| b.contains("warm")) => "warmup",
            None => "stopped",
        }
    }

    fn find_session(&self, id: &str) -> Result<&Session, Out> {
        self.store.sessions.iter().find(|s| s.uuid == id && !s.trashed).ok_or_else(|| err("No such session"))
    }

    /// Every id must name a session before anything is done, so a bad request changes nothing.
    fn existing_ids(&self, b: &Value) -> Result<Vec<String>, Out> {
        let v = ids(b);
        if v.is_empty() {
            return Err(err("uuid or uuids is required"));
        }
        for i in &v {
            self.find_session(i)?;
        }
        Ok(v)
    }

    fn session_info(&self, s: &Session) -> Value {
        let d = devices::get(&s.fp.device);
        let mode = |m: FpMode| if m == FpMode::Noise { "fake" } else { "direct" };
        let tz = if s.fp.timezone.is_empty() { s.geo.as_ref().map(|g| g.timezone.clone()).unwrap_or_default() } else { s.fp.timezone.clone() };
        let ver = browser::find_browser().and_then(|p| browser::browser_major(&p)).map(|m| m.to_string()).unwrap_or_default();
        json!({
            "name": s.name,
            "status": self.api_status(&s.uuid),
            "device": {
                "browser_version": ver, "cpu": d.cores, "ram": d.ram, "mode": "normal", "type": "desktop",
                "os": d.os.label(), "os_version": d.platform_version, "screen": format!("{}x{}", s.fp.screen.0, s.fp.screen.1),
                "webgl": d.gpu_renderer,
                "noises": {
                    "canvas": mode(s.fp.canvas), "audio": mode(s.fp.audio), "clientRects": mode(s.fp.client_rects), "webgl": mode(s.fp.webgl),
                    "webglNoise": mode(s.fp.webgl), "mediaDevices": if s.fp.fake_media_devices { "fake" } else { "direct" },
                    "webgpu": "direct", "doNotTrack": s.misc.do_not_track,
                },
            },
            "proxy": {
                "connection": {"protocol": s.proxy.kind.label().to_lowercase()},
                "geolocation": s.geo.as_ref().map(|g| g.place()).unwrap_or_default(),
                "language": s.fp.language, "publicIp": s.geo.as_ref().map(|g| g.ip.clone()).unwrap_or_default(), "timezone": tz,
            },
            "webrtc": {"state": s.fp.webrtc.label(), "value": ""},
            "uniqueness": {"level": uniqueness_level(self.store.fp_twins(s)), "shared_with": self.store.fp_twins(s)},
        })
    }

    /// A one-time session is deleted, profile and all, when its browser has ended.
    pub(super) fn forget_one_time(&mut self, id: &str) {
        if self.one_time.remove(id) {
            let _ = std::fs::remove_dir_all(profile_dir(id));
            self.store.sessions.retain(|s| s.uuid != id);
            self.dirty = true;
        }
    }

    fn set_active_preset(&mut self, n: &str) -> Out {
        if let Some(p) = Preset::ALL.iter().find(|p| p.label() == n) {
            self.preset = *p;
            self.store.active_preset = None;
            (200, json!({"active": n, "uuid": n}))
        } else if self.store.presets.iter().any(|p| p.name == n) {
            self.store.active_preset = Some(n.to_string());
            (200, json!({"active": n, "uuid": n}))
        } else {
            err("no such preset")
        }
    }

    pub(super) fn handle_api(&mut self, ctx: &egui::Context, req: crate::api::Req) {
        let out = self.route(ctx, &req);
        let _ = req.reply.send(out);
    }

    fn route(&mut self, ctx: &egui::Context, req: &crate::api::Req) -> Out {
        let b = &req.body;
        let uuid = b["uuid"].as_str().unwrap_or("").to_string();
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/sessions") => {
                let (filter, info) = (b["status"].as_str(), b["proxy_info"].as_bool().unwrap_or(false));
                if let Some(f) = filter.filter(|f| !["running", "stopped", "imported", "warmup", "automationRunning"].contains(f)) {
                    return err(format!("unknown status \"{f}\""));
                }
                let v: Vec<Value> = self
                    .store
                    .sessions
                    .iter()
                    .filter(|s| !s.trashed && s.desktop == self.store.current_desktop)
                    .filter(|s| filter.is_none_or(|f| f == self.api_status(&s.uuid)))
                    .map(|s| {
                        let mut o = json!({"uuid": s.uuid, "name": s.name, "status": self.api_status(&s.uuid), "debug_port": self.running.get(&s.uuid).and_then(|r| r.port)});
                        if info {
                            o["proxy"] = proxy_json(&s.proxy);
                        }
                        o
                    })
                    .collect();
                (200, json!(v))
            }
            ("GET", "/sessions/actionlog") => {
                let s = match self.find_session(&uuid) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let types: Option<Vec<i64>> = b["action_types"].as_str().map(|t| t.split(',').filter_map(|x| x.trim().parse().ok()).collect());
                let (from, to) = (b["from_time"].as_u64(), b["to_time"].as_u64());
                let limit = b["limit"].as_u64().unwrap_or(50).min(100) as usize;
                let offset = b["offset"].as_u64().unwrap_or(0) as usize;
                let keep = s.log.len().saturating_sub(100); // the original keeps the last 100 actions
                let all: Vec<(usize, &(u64, String))> = s
                    .log
                    .iter()
                    .enumerate()
                    .skip(keep)
                    .rev()
                    .filter(|(_, (t, m))| from.is_none_or(|f| *t >= f) && to.is_none_or(|x| *t <= x) && types.as_ref().is_none_or(|ty| ty.contains(&action_of(m).0)))
                    .collect();
                let page: Vec<Value> = all
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|(i, (t, m))| {
                        let (a, n) = action_of(m);
                        json!({"action": a, "action_name": n, "data": m, "id": i, "timestamp": t, "user_email": ""})
                    })
                    .collect();
                (200, json!({"actions": page, "offset": offset, "returned_count": page.len(), "session_name": s.name, "session_uuid": s.uuid, "total_count": all.len()}))
            }
            ("GET", p) if p.starts_with("/sessions/") && p != "/sessions/backups" => match self.find_session(&p["/sessions/".len()..]) {
                Ok(s) => (200, self.session_info(s)),
                Err(e) => e,
            },
            ("POST", "/sessions/create_quick") => {
                let n = b["count"].as_u64().unwrap_or(1);
                if !(1..=100).contains(&n) {
                    return err("count must be 1 to 100");
                }
                let mut out = vec![];
                for _ in 0..n {
                    let mut s = self.next_session();
                    s.log("Session created through the API");
                    out.push(json!({"name": s.name, "uuid": s.uuid}));
                    self.store.sessions.push(s);
                }
                self.dirty = true;
                (200, json!(out))
            }
            ("POST", "/sessions/start") => {
                let port = b["debug_port"].as_u64().filter(|p| (1..=65535).contains(p)).map(|p| p as u16);
                let args: Vec<String> = b["chromium_args"].as_str().unwrap_or("").split_whitespace().map(String::from).collect();
                match self.start_session_with(ctx, &uuid, b["headless"].as_bool().unwrap_or(false), port, b["disable_images"].as_bool().unwrap_or(false), args) {
                    Ok(p) => {
                        let mut o = json!({"debug_port": p, "uuid": uuid});
                        if b["referrer_values"].is_array() {
                            o["ignored"] = json!(["referrer_values"]); // per-site Referer replacement is not implemented
                        }
                        (200, o)
                    }
                    Err(e) => err(e),
                }
            }
            ("POST", "/sessions/run_one_time") => {
                let n = b["count"].as_u64().unwrap_or(1);
                if !(1..=20).contains(&n) {
                    return err("count must be 1 to 20");
                }
                let args: Vec<String> = b["chromium_args"].as_str().unwrap_or("").split_whitespace().map(String::from).collect();
                let mut out = serde_json::Map::new();
                for _ in 0..n {
                    let mut s = self.next_session();
                    s.log("One-time session created through the API");
                    let id = s.uuid.clone();
                    self.store.sessions.push(s);
                    self.one_time.insert(id.clone());
                    let port = browser::free_port();
                    match self.start_session_with(ctx, &id, b["headless"].as_bool().unwrap_or(false), Some(port), b["disable_images"].as_bool().unwrap_or(false), args.clone()) {
                        Ok(p) => {
                            out.insert(id, json!(p));
                        }
                        Err(e) => {
                            self.forget_one_time(&id);
                            return err(e);
                        }
                    }
                }
                self.dirty = true;
                (200, Value::Object(out))
            }
            ("POST", "/sessions/set_name") => {
                let n = b["name"].as_str().unwrap_or("").trim().to_string();
                if n.is_empty() || n.chars().count() > 50 {
                    return err("name needs 1 to 50 characters");
                }
                match self.sess(&uuid) {
                    Some(s) => {
                        s.name = n;
                        s.modified = now();
                        self.dirty = true;
                        (200, json!({}))
                    }
                    None => err("No such session"),
                }
            }
            ("POST", "/sessions/set_description") => match (self.sess(&uuid), b["description"].as_str()) {
                (Some(s), Some(d)) => {
                    s.description = d.to_string();
                    s.modified = now();
                    self.dirty = true;
                    (200, json!({}))
                }
                (None, _) => err("No such session"),
                (_, None) => err("description is required"),
            },
            ("POST", "/sessions/set_fingerprints") => {
                let mut fp = match self.find_session(&uuid) {
                    Ok(s) => s.fp.clone(),
                    Err(e) => return e,
                };
                let mut ignored = vec![];
                let mut any = false;
                if let Some(br) = b["browser"].as_str() {
                    if br != "chrome" {
                        return err("only \"chrome\" is built; the bundled Chromium reports itself as Chrome");
                    }
                    any = true;
                }
                // GPU, RAM and cores come from the device model: pick the model of the same OS that has them.
                let (gpu, ram, cores) = (b["gpu"].as_str().map(|g| g.to_lowercase()), number_str(&b["ram"]), number_str(&b["cpu_cores"]));
                if gpu.is_some() || ram.is_some() || cores.is_some() {
                    any = true;
                    let fits = |d: &devices::Device| {
                        gpu.as_ref().is_none_or(|g| d.gpu_renderer.to_lowercase().contains(g.as_str())) && ram.is_none_or(|r| r == d.ram) && cores.is_none_or(|c| c == d.cores)
                    };
                    let cur = devices::get(&fp.device);
                    if !fits(cur) {
                        match devices::DEVICES.iter().find(|d| d.os == cur.os && fits(d)) {
                            Some(d) => {
                                fp.device = d.id.into();
                                fp.screen = d.screens[0];
                            }
                            None => {
                                let have: Vec<String> = devices::DEVICES.iter().filter(|d| d.os == cur.os).map(|d| format!("{} ({} cores, {} GB)", d.gpu_renderer, d.cores, d.ram)).collect();
                                return err(format!("No device model has that GPU, RAM and cores together. Available: {}", have.join("; ")));
                            }
                        }
                    }
                }
                for (key, slot) in [("canvas", &mut fp.canvas), ("client_rects", &mut fp.client_rects), ("audio", &mut fp.audio), ("webgl", &mut fp.webgl)] {
                    match mode_of(&b[key], key) {
                        Ok(Some(m)) => {
                            *slot = m;
                            any = true;
                        }
                        Ok(None) => {}
                        Err(e) => return e,
                    }
                }
                match mode_of(&b["media_devices"], "media_devices") {
                    Ok(Some(m)) => {
                        fp.fake_media_devices = m == FpMode::Noise;
                        any = true;
                    }
                    Ok(None) => {}
                    Err(e) => return e,
                }
                if b["webgpu"].is_string() {
                    ignored.push("webgpu"); // navigator.gpu stays as the real browser has it
                    any = true;
                }
                if !any {
                    return err("give at least one of browser, gpu, ram, cpu_cores, canvas, client_rects, audio, webgl, webgpu, media_devices");
                }
                let s = self.sess(&uuid).expect("checked above");
                s.fp = fp;
                s.modified = now();
                s.log("Fingerprint changed through the API");
                self.dirty = true;
                if ignored.is_empty() {
                    (200, json!({}))
                } else {
                    (200, json!({"ignored": ignored}))
                }
            }
            ("POST", "/sessions/connection") => {
                let t = b["type"].as_str().unwrap_or("");
                let kind = match t {
                    "notSet" | "direct" => ProxyKind::Direct,
                    "socks5" | "liveSocks5" => ProxyKind::Socks5,
                    "http" | "liveHttp" => ProxyKind::Http,
                    "ssh" => ProxyKind::Ssh,
                    "tor" => ProxyKind::Tor,
                    "localhost" => ProxyKind::Localhost,
                    "freeProxy" => return err("freeProxy comes from the producer's server and is not available here"),
                    _ => return err("type must be notSet, socks5, http, ssh, liveSocks5, liveHttp, direct, tor or localhost"),
                };
                let host = b["ip"].as_str().unwrap_or("").trim().to_string();
                let port = b["port"].as_u64().filter(|p| (1..=65535).contains(p)).unwrap_or(0) as u16;
                if matches!(kind, ProxyKind::Http | ProxyKind::Socks5 | ProxyKind::Ssh) && (host.is_empty() || port == 0) {
                    return err("ip and port are required for this type");
                }
                if kind == ProxyKind::Localhost && port == 0 {
                    return err("port is required for localhost");
                }
                let live = t.starts_with("live") || b["live"].as_bool().unwrap_or(false);
                let Some(old) = self.store.sessions.iter().find(|s| s.uuid == uuid && !s.trashed).map(|s| s.proxy.change_ip_url.clone()) else { return err("No such session") };
                let p = Proxy {
                    kind,
                    host,
                    port,
                    login: b["login"].as_str().unwrap_or("").into(),
                    password: b["password"].as_str().unwrap_or("").into(),
                    change_ip_url: if live { old } else { String::new() },
                    dns: b["customDns"].as_str().unwrap_or("").into(),
                };
                let running = self.running.contains_key(&uuid);
                let s = self.sess(&uuid).expect("checked above");
                s.log(format!("Connection set to {} {} through the API", p.kind.label(), p.short()));
                s.proxy = p;
                s.geo = None;
                s.modified = now();
                self.dirty = true;
                let mut o = json!({"uuid": uuid});
                if running {
                    o["note"] = json!("takes effect when the session is started again");
                }
                (200, o)
            }
            ("POST", "/sessions/set_misc") => {
                if let Err(e) = self.find_session(&uuid) {
                    return e;
                }
                const IGNORED: [&str; 3] = ["always_focus_window", "chrome_view", "ad_crusher"];
                let ignored: Vec<&str> = IGNORED.iter().copied().filter(|k| b[*k].is_boolean()).collect();
                let s = self.sess(&uuid).expect("checked above");
                let m = &mut s.misc;
                let mut n = 0;
                for (key, slot) in [
                    ("restore_open_tabs", &mut m.restore_open_tabs),
                    ("memory_saver", &mut m.memory_saving),
                    ("hardware_acceleration", &mut m.hardware_acceleration),
                    ("browsing_history", &mut m.save_history),
                    ("password_manager", &mut m.save_passwords),
                    ("disable_images", &mut m.disable_images),
                    ("disable_popups", &mut m.block_popups),
                    ("block_non_latin_input", &mut m.block_non_latin),
                    ("enable_quic", &mut m.enable_quic),
                    ("do_not_track", &mut m.do_not_track),
                ] {
                    if let Some(v) = b[key].as_bool() {
                        *slot = v;
                        n += 1;
                    }
                }
                if n == 0 && ignored.is_empty() {
                    return err("give at least one flag, e.g. memory_saver, browsing_history, password_manager, disable_popups");
                }
                s.modified = now();
                self.dirty = true;
                if ignored.is_empty() {
                    (200, json!({}))
                } else {
                    (200, json!({"ignored": ignored}))
                }
            }
            ("POST", "/sessions/geo") => {
                if b["latitude"].is_number() || b["longitude"].is_number() {
                    return err("latitude and longitude are not supported: the browser's geolocation is not modelled yet");
                }
                if let Err(e) = self.find_session(&uuid) {
                    return e;
                }
                let tz = b["timezone"].as_str().map(|t| t.trim().to_string());
                let lang = b["language"].as_str().and_then(|l| l.split(',').next()).map(|l| l.split(';').next().unwrap_or("").trim().to_string());
                if tz.is_none() && lang.is_none() {
                    return err("give timezone and/or language");
                }
                if tz.as_ref().is_some_and(|t| t.is_empty() || t.len() > 64 || !t.chars().all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))) || lang.as_ref().is_some_and(|l| l.is_empty() || l.len() > 16) {
                    return err("timezone or language looks wrong");
                }
                let s = self.sess(&uuid).expect("checked above");
                if let Some(t) = tz {
                    s.fp.timezone = t;
                }
                if let Some(l) = lang {
                    s.fp.language = l;
                }
                s.fp.geo_from_proxy = false;
                s.modified = now();
                self.dirty = true;
                (200, json!({"uuid": uuid}))
            }
            ("POST", "/sessions/set_useragent") => {
                let ua = b["useragent"].as_str().unwrap_or("").trim().to_string();
                if ua.is_empty() || ua.len() > 500 || ua.chars().any(|c| c.is_control()) {
                    return err("useragent needs 1 to 500 printable characters");
                }
                match self.sess(&uuid) {
                    Some(s) => {
                        s.fp.user_agent = ua;
                        s.modified = now();
                        self.dirty = true;
                        (200, json!({"uuid": uuid}))
                    }
                    None => err("No such session"),
                }
            }
            ("POST", "/sessions/clone") => {
                let before = self.store.sessions.len();
                self.act(ctx, Action::Clone(uuid.clone()));
                match self.store.sessions.get(before) {
                    Some(c) => (200, json!({"name": c.name, "uuid": c.uuid})),
                    None => err("No such session"),
                }
            }
            ("POST", "/sessions/delete") | ("DELETE", "/sessions") => match self.existing_ids(b) {
                Ok(v) => {
                    for i in &v {
                        self.act(ctx, Action::Trash(i.clone()));
                    }
                    reply_ids(b, &v)
                }
                Err(e) => e,
            },
            ("POST", "/sessions/set_proxy") => {
                let kind = match b["kind"].as_str().unwrap_or("http").to_ascii_lowercase().as_str() {
                    "socks5" | "socks" => ProxyKind::Socks5,
                    _ => ProxyKind::Http,
                };
                match Proxy::parse(b["proxy"].as_str().unwrap_or(""), kind) {
                    Some(p) => match self.sess(&uuid) {
                        Some(s) => {
                            s.log(format!("Proxy set to {} {}", p.kind.label(), p.short()));
                            s.proxy = p;
                            s.geo = None;
                            s.modified = now();
                            self.dirty = true;
                            (200, json!({"uuid": uuid}))
                        }
                        None => err("No such session"),
                    },
                    None => err("Can't read the proxy line"),
                }
            }
            ("POST", "/sessions/check_proxy") => match self.existing_ids(b) {
                Ok(v) => {
                    for i in &v {
                        self.spawn_check(ctx, i);
                    }
                    // The check runs in the background; its result is in the session's action log.
                    let mut o = json!({"result": "Success", "uuid": v[0]});
                    if v.len() > 1 {
                        o["uuids"] = json!(v);
                    }
                    (200, o)
                }
                Err(e) => e,
            },
            ("POST", "/sessions/import_cookies") => {
                let text = if let Some(p) = b["file_path"].as_str() {
                    match std::fs::read_to_string(p) {
                        Ok(t) => t,
                        Err(e) => return err(format!("Can't read {p}: {e}")),
                    }
                } else {
                    match (&b["json"], &b["cookies"]) {
                        (Value::String(t), _) | (_, Value::String(t)) => t.clone(),
                        (Value::Null, Value::Null) => String::new(),
                        (Value::Null, v) | (v, _) => v.to_string(),
                    }
                };
                match (browser::parse_cookies(&text), self.find_session(&uuid).is_ok()) {
                    (Ok(mut c), true) => {
                        let n = c.len();
                        if let Some(r) = self.running.get(&uuid) {
                            let conn = r.conn.clone();
                            std::thread::spawn(move || {
                                let _ = browser::import_cookies(&conn, &c);
                            });
                        } else {
                            let mut all = crate::import::load_pending(&uuid).unwrap_or_default();
                            all.append(&mut c);
                            crate::import::save_pending(&uuid, &all);
                        }
                        (200, json!({"uuid": uuid, "imported": n, "applied": if self.running.contains_key(&uuid) { "now" } else { "at the next start" }}))
                    }
                    (Err(e), _) => err(e),
                    (_, false) => err("No such session"),
                }
            }
            ("POST", "/sessions/export_cookies") => {
                let v = match self.existing_ids(b) {
                    Ok(v) => v,
                    Err(e) => return e,
                };
                if let Some(not) = v.iter().find(|i| !self.running.contains_key(*i)) {
                    return err(format!("The session must be running to export its cookies: {not}"));
                }
                let folder = b["folder_path"].as_str();
                if folder.is_none() && v.len() != 1 {
                    return err("folder_path is required for more than one session");
                }
                let mut out = vec![];
                for i in &v {
                    let cookies = match browser::export_cookies(&self.running[i].conn) {
                        Ok(c) => c,
                        Err(e) => return err(e),
                    };
                    let Some(dir) = folder else { return (200, json!(cookies)) }; // no folder: the cookies themselves
                    let name: String = self.find_session(i).map(|s| s.name.clone()).unwrap_or_default().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
                    let path = std::path::Path::new(dir).join(format!("{name}_{}.json", now()));
                    if let Err(e) = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, serde_json::to_vec_pretty(&cookies).unwrap_or_default())) {
                        return err(format!("Can't write {}: {e}", path.display()));
                    }
                    out.push(json!({"cookie_path": path.to_string_lossy(), "uuid": i}));
                }
                (200, json!(out))
            }
            ("POST", "/sessions/start_warmup") => {
                let s = match self.find_session(&uuid) {
                    Ok(s) => s.clone(),
                    Err(e) => return e,
                };
                let urls: Vec<String> = match b["urls"].as_array() {
                    Some(a) => a.iter().filter_map(|u| u.as_str()).map(String::from).take(browser::MAX_URLS).collect(),
                    None => {
                        let st = &self.store.settings;
                        let cc = s.geo.as_ref().filter(|g| g.ok).map(|g| g.country_code.clone()).unwrap_or_default();
                        let list = if st.warmup_urls == crate::model::DEFAULT_WARMUP_URLS { crate::model::regional_sites(&cc) } else { st.warmup_urls.clone() };
                        list.lines().map(String::from).collect()
                    }
                };
                let get = |k: &str, d: u64, r: (u32, u32)| -> Result<u32, Out> {
                    let v = b[k].as_u64().unwrap_or(d);
                    if v < r.0 as u64 || v > r.1 as u64 {
                        return Err(err(format!("{k} must be {} to {}", r.0, r.1)));
                    }
                    Ok(v as u32)
                };
                let (count, time, depth) = match (get("url_count", 7, browser::URL_COUNT), get("time_per_url", 3, browser::TIME_PER_URL), get("view_depth", 3, browser::VIEW_DEPTH)) {
                    (Ok(c), Ok(t), Ok(d)) => (c, t, d),
                    (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return e,
                };
                if self.busy.contains_key(&uuid) {
                    return err("This session is busy, try again when it finishes.");
                }
                let job = browser::Warmup {
                    url_count: if b["urls"].is_array() { urls.len().clamp(1, browser::MAX_URLS) as u32 } else { count },
                    urls,
                    time_per_url: time,
                    view_depth: depth,
                    accept_cookies: self.store.settings.warmup_accept_cookies,
                    referrer: dialogs::REFERRERS[1].1.into(),
                };
                let id = uuid.clone();
                self.with_browser(ctx, &uuid, "warming up", false, move |conn, tx| {
                    let (tx2, id2) = (tx.clone(), id.clone());
                    let (n, cookies) = browser::run_warmup(conn, &job, &move |m| {
                        let _ = tx2.send(Msg::Busy(id2.clone(), Some(m)));
                    })?;
                    let _ = tx.send(Msg::Warmed(id));
                    Ok(format!("Warm-up finished: {n} sites visited, {cookies} new cookies"))
                });
                (200, json!({"uuid": uuid}))
            }
            ("POST", "/sessions/backup") | ("POST", "/sessions/restore_backup") | ("POST", "/sessions/delete_backup") | ("GET", "/sessions/backups") => {
                // Not in the original's public API (it keeps backups in its cloud); local copies of the session's data.
                if let Err(e) = self.find_session(&uuid) {
                    return e;
                }
                if req.method == "GET" {
                    let l: Vec<Value> = crate::backup::list(&uuid).iter().map(|x| json!({"id": x.id, "created": x.id, "size": x.bytes})).collect();
                    return (200, json!({"uuid": uuid, "max": crate::backup::MAX, "backups": l}));
                }
                if self.running.contains_key(&uuid) || self.busy.contains_key(&uuid) {
                    return err("Stop the session first");
                }
                let bid = b["id"].as_u64();
                match (req.path.as_str(), bid) {
                    ("/sessions/backup", _) => match crate::backup::create(&uuid) {
                        Ok(id) => {
                            self.log(&uuid, "Backup created");
                            (200, json!({"uuid": uuid, "id": id}))
                        }
                        Err(e) => err(e),
                    },
                    (_, None) => err("id of the backup is required (see GET /sessions/backups)"),
                    ("/sessions/restore_backup", Some(id)) => match crate::backup::restore(&uuid, id) {
                        Ok(()) => {
                            self.log(&uuid, "Restored from backup");
                            (200, json!({"uuid": uuid}))
                        }
                        Err(e) => err(e),
                    },
                    (_, Some(id)) => {
                        crate::backup::delete(&uuid, id);
                        (200, json!({"uuid": uuid}))
                    }
                }
            }
            ("POST", "/sessions/stop") | ("POST", "/sessions/force_stop") => match self.existing_ids(b) {
                Ok(v) => {
                    for i in &v {
                        self.act(ctx, Action::Stop(i.clone()));
                    }
                    reply_ids(b, &v)
                }
                Err(e) => e,
            },
            ("POST", "/desktops/unlock_stopped_sessions") => {
                // Nothing here stays locked after a crash; the stopped sessions are reported as unlocked.
                let v: Vec<Value> = self.store.sessions.iter().filter(|s| !s.trashed && s.desktop == self.store.current_desktop && !self.running.contains_key(&s.uuid)).map(|s| json!({"isLocked": "false", "uuid": s.uuid})).collect();
                (200, json!(v))
            }
            ("GET", "/providers") | ("GET", "/presets") => {
                let mut v: Vec<String> = Preset::ALL.iter().map(|p| p.label().to_string()).collect();
                v.extend(self.store.presets.iter().map(|p| p.name.clone()));
                if req.path == "/presets" {
                    return (200, json!({"presets": v, "active": self.store.preset_label(self.preset)}));
                }
                (200, json!(v.iter().map(|n| json!({"name": n, "uuid": n})).collect::<Vec<_>>()))
            }
            ("POST", "/providers") => self.set_active_preset(&uuid),
            ("POST", "/presets/set_active") => self.set_active_preset(b["name"].as_str().unwrap_or("")),
            ("DELETE", "/providers") => {
                let Some(i) = self.store.presets.iter().position(|p| p.name == uuid) else { return err("only your own presets can be deleted") };
                self.store.presets.remove(i);
                if self.store.active_preset.as_deref() == Some(uuid.as_str()) {
                    self.store.active_preset = None;
                }
                self.dirty = true;
                (200, json!({}))
            }
            ("GET", "/desktops") => (200, json!(self.store.desktops.iter().map(|d| json!({"name": d.name, "uuid": d.uuid})).collect::<Vec<_>>())),
            ("PUT", "/desktops") => {
                let n = b["name"].as_str().unwrap_or("").trim().to_string();
                if n.is_empty() || n.chars().count() > 50 {
                    return err("name needs 1 to 50 characters");
                }
                let d = Desktop { uuid: uuid::Uuid::new_v4().to_string(), name: n };
                let id = d.uuid.clone();
                self.store.desktops.push(d);
                self.dirty = true;
                (200, json!({"uuid": id}))
            }
            ("POST", "/desktops") => {
                if !self.store.desktops.iter().any(|d| d.uuid == uuid) {
                    return err("No such desktop");
                }
                self.store.current_desktop = uuid.clone();
                self.dirty = true;
                (200, json!({"uuid": uuid}))
            }
            ("DELETE", "/desktops") => {
                if !self.store.desktops.iter().any(|d| d.uuid == uuid) {
                    return err("No such desktop");
                }
                if self.store.desktops.len() == 1 || self.store.current_desktop == uuid {
                    return err("The last desktop and the active one cannot be deleted");
                }
                if self.store.sessions.iter().any(|s| s.desktop == uuid && !s.trashed) {
                    return err("The desktop still has sessions: move or delete them first");
                }
                self.store.desktops.retain(|d| d.uuid != uuid);
                let gone: Vec<String> = self.store.sessions.iter().filter(|s| s.desktop == uuid).map(|s| s.uuid.clone()).collect();
                for g in gone {
                    let _ = std::fs::remove_dir_all(profile_dir(&g));
                }
                self.store.sessions.retain(|s| s.desktop != uuid);
                self.dirty = true;
                (200, json!({}))
            }
            ("POST", "/app/optimize_storage") => {
                // Caches of the stopped sessions only; cookies, storage and passwords stay.
                let (mut freed, mut n) = (0, 0);
                let stopped: Vec<String> = self.store.sessions.iter().filter(|s| !self.running.contains_key(&s.uuid) && !self.busy.contains_key(&s.uuid)).map(|s| s.uuid.clone()).collect();
                for id in stopped {
                    let data = profile_dir(&id).join("data");
                    for rel in ["Default/Cache", "Default/Code Cache", "Default/GPUCache", "Default/DawnGraphiteCache", "Default/DawnWebGPUCache", "GrShaderCache", "ShaderCache", "GraphiteDawnCache"] {
                        let p = data.join(rel);
                        if p.is_dir() {
                            freed += dir_size(&p);
                            n += std::fs::remove_dir_all(&p).is_ok() as u32;
                        }
                    }
                }
                (200, json!({"cleared_folders": n, "freed_bytes": freed}))
            }
            ("POST", p) if p.starts_with("/app/api_threads_count/") => {
                let max = std::thread::available_parallelism().map_or(2, |n| n.get());
                match p["/app/api_threads_count/".len()..].parse::<usize>() {
                    // The server runs one thread per connection, so there is nothing to size; the value is only checked.
                    Ok(n) if (2..=max.max(2)).contains(&n) => (200, json!({})),
                    _ => err(format!("count must be 2 to {}", max.max(2))),
                }
            }
            _ => (404, json!({"error": "unknown endpoint"})),
        }
    }
}
