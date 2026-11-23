use crate::browser::{self, Running};
use crate::devices::{self, Preset};
use crate::model::*;
use crate::proxy;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod dialogs;
mod view;

// Accent from the original's default "customColor" (0x140591ed0).
const ACCENT: Color32 = Color32::from_rgb(0x13, 0x68, 0xCA);
const ICON: Color32 = Color32::from_rgb(0x5B, 0x8D, 0xEF);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xF0);
const DIM: Color32 = Color32::from_rgb(0x8A, 0x90, 0xA0);
const FAINT: Color32 = Color32::from_rgb(0x4E, 0x54, 0x62);
const BORDER: Color32 = Color32::from_rgb(0x24, 0x28, 0x31);
const GREEN: Color32 = Color32::from_rgb(0x3B, 0xB2, 0x73);
const RED: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x4A);
const BAR: Color32 = Color32::from_rgb(0x09, 0x0B, 0x0F);
const PANEL: Color32 = Color32::from_rgba_premultiplied(11, 13, 18, 238);

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

enum Msg {
    Geo(String, GeoInfo),
    Log(String, String),
    Busy(String, Option<String>),
    Warmed(String),
    Bulk(u64, usize, GeoInfo),
    DialogGeo(GeoInfo),
    Api(crate::api::Req),
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Created,
    Used,
    Modified,
    Timer,
}

#[derive(PartialEq)]
enum Page {
    Sessions,
    Preferences,
}

struct MassDlg {
    n: u32,
    cookies: Vec<PathBuf>,
    table: Option<PathBuf>,
    use_file_name: bool,
}

impl MassDlg {
    fn new() -> Self {
        MassDlg { n: 10, cookies: vec![], table: None, use_file_name: false }
    }
}

enum Action {
    Start(String),
    Stop(String),
    Check(String),
    ChangeIp(String),
    Edit(String),
    SetProxy(Vec<String>),
    Warm(String),
    ImportCookies(String),
    ExportCookies(String),
    RemoveData(String),
    Clone(String),
    Trash(String),
    Restore(String),
    Purge(String),
    Pin(String),
    Log(String),
    Desc(String),
    MoveTo(String, String),
}

struct EditState {
    s: Session,
    new: bool,
    tags: String,
    line: String,
}

struct ProxyDlg {
    targets: Vec<String>,
    proxy: Proxy,
    line: String,
    result: Option<GeoInfo>,
    checking: bool,
}

struct WarmDlg {
    uuid: String,
    urls: String,
    count: u32,
    time: u32,
    depth: u32,
    accept: bool,
    /// Index into `dialogs::REFERRERS`.
    referrer: usize,
    set_warmed: bool,
    /// Country of the session's proxy/geo check, for the regional site list.
    cc: String,
}

struct Checker {
    text: String,
    kind: ProxyKind,
    rows: Vec<(String, Option<Proxy>, Option<GeoInfo>)>,
    gen: u64,
}

pub struct App {
    store: Store,
    running: HashMap<String, Running>,
    busy: HashMap<String, String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    page: Page,
    search: String,
    sort: Sort,
    desc: bool,
    quick: bool,
    preset: Preset,
    proxy_filter: Option<ProxyKind>,
    running_only: bool,
    pinned_only: bool,
    trash_view: bool,
    tag_filter: Option<String>,
    selected: HashSet<String>,
    edit: Option<EditState>,
    proxy_dlg: Option<ProxyDlg>,
    warm_dlg: Option<WarmDlg>,
    log_dlg: Option<String>,
    desc_dlg: Option<(String, String)>,
    checker: Option<Checker>,
    multiple: Option<MassDlg>,
    tags_dlg: Option<String>,
    bell: bool,
    help: bool,
    ports_open: bool,
    new_desktop: Option<String>,
    toast: Option<(String, Instant)>,
    stars: Vec<(f32, f32, f32, u8)>,
    shot: Option<PathBuf>,
    frames: u32,
    dirty: bool,
    browser: Option<(PathBuf, Option<u32>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>) -> App {
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let store = Store::load();
        if store.settings.api_port > 0 {
            let (atx, actx) = (tx.clone(), cc.egui_ctx.clone());
            if let Err(e) = crate::api::start(store.settings.api_port, move |r| {
                let _ = atx.send(Msg::Api(r));
                actx.request_repaint();
            }) {
                eprintln!("Local API not started: {e}");
            }
        }
        let mut rng = fastrand::Rng::with_seed(7);
        let stars = (0..700).map(|_| (rng.f32(), rng.f32(), 0.4 + rng.f32() * rng.f32() * 1.4, 60 + rng.u8(..195))).collect();
        let browser = detect(&store.settings);
        App {
            store,
            running: HashMap::new(),
            busy: HashMap::new(),
            tx,
            rx,
            page: if std::env::args().any(|a| a == "--preferences") { Page::Preferences } else { Page::Sessions },
            search: String::new(),
            sort: Sort::Created,
            desc: true,
            quick: true,
            preset: Preset::Desktop,
            proxy_filter: None,
            running_only: false,
            pinned_only: false,
            trash_view: false,
            tag_filter: None,
            selected: HashSet::new(),
            edit: None,
            proxy_dlg: None,
            warm_dlg: None,
            log_dlg: None,
            desc_dlg: None,
            checker: None,
            multiple: None,
            tags_dlg: None,
            bell: false,
            help: false,
            ports_open: false,
            new_desktop: None,
            toast: None,
            stars,
            shot,
            frames: 0,
            dirty: false,
            browser,
        }
    }

    fn toast(&mut self, t: impl Into<String>) {
        self.toast = Some((t.into(), Instant::now()));
    }

    fn sess(&mut self, uuid: &str) -> Option<&mut Session> {
        self.store.find(uuid)
    }

    fn log(&mut self, uuid: &str, msg: impl Into<String>) {
        let msg = msg.into();
        if let Some(s) = self.store.find(uuid) {
            let n = format!("{}: {msg}", s.name);
            s.log(msg);
            self.store.notify(n);
        }
        self.dirty = true;
    }

    fn ipqs_key(&self) -> String {
        if self.store.settings.enable_ip_stats { self.store.settings.ipqs_key.clone() } else { String::new() }
    }

    fn spawn_check(&mut self, ctx: &egui::Context, uuid: &str) {
        let Some(s) = self.store.sessions.iter().find(|s| s.uuid == uuid) else { return };
        let (p, key, tx, ctx, id) = (s.proxy.clone(), self.ipqs_key(), self.tx.clone(), ctx.clone(), uuid.to_string());
        self.busy.insert(id.clone(), "checking proxy".into());
        std::thread::spawn(move || {
            let g = proxy::check(&p, &key);
            let _ = tx.send(Msg::Geo(id.clone(), g));
            let _ = tx.send(Msg::Busy(id, None));
            ctx.request_repaint();
        });
    }

    /// Runs `job` against the session's browser: the open one, or a temporary one started for it.
    fn with_browser(
        &mut self,
        ctx: &egui::Context,
        uuid: &str,
        what: &str,
        headless: bool,
        job: impl FnOnce(u16, &Sender<Msg>) -> Result<String, String> + Send + 'static,
    ) {
        if self.busy.contains_key(uuid) {
            self.toast("This session is busy, try again when it finishes.");
            return;
        }
        let Some(s) = self.store.sessions.iter().find(|s| s.uuid == uuid).cloned() else { return };
        let port = self.running.get(uuid).map(|r| r.port);
        let settings = self.store.settings.clone();
        let (tx, ctx, id) = (self.tx.clone(), ctx.clone(), uuid.to_string());
        self.busy.insert(id.clone(), what.into());
        std::thread::spawn(move || {
            let res = match port {
                Some(p) => job(p, &tx),
                None => match browser::launch(&settings, &s, headless, None) {
                    Ok(mut r) => {
                        let out = browser::wait_ready(r.port, 30).and_then(|_| job(r.port, &tx));
                        r.stop();
                        out
                    }
                    Err(e) => Err(e),
                },
            };
            let _ = tx.send(Msg::Log(id.clone(), res.unwrap_or_else(|e| format!("Failed: {e}"))));
            let _ = tx.send(Msg::Busy(id, None));
            ctx.request_repaint();
        });
    }

    fn act(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Start(id) => {
                if let Err(e) = self.start_session(ctx, &id, false, None) {
                    self.toast(e);
                }
            }
            Action::Stop(id) => {
                if let Some(mut r) = self.running.remove(&id) {
                    r.stop();
                    let secs = r.started.elapsed().as_secs();
                    if let Some(s) = self.sess(&id) {
                        s.run_secs += secs;
                    }
                    self.log(&id, "Browser stopped");
                }
            }
            Action::Check(id) => self.spawn_check(ctx, &id),
            Action::ChangeIp(id) => {
                let Some(p) = self.store.sessions.iter().find(|s| s.uuid == id).map(|s| s.proxy.clone()) else { return };
                if p.change_ip_url.trim().is_empty() {
                    self.toast("Add the provider's change-IP link in Set proxy first.");
                    return;
                }
                let (tx, c, i, key) = (self.tx.clone(), ctx.clone(), id.clone(), self.ipqs_key());
                self.busy.insert(id, "changing IP".into());
                std::thread::spawn(move || {
                    let m = match proxy::change_ip(&p.change_ip_url) {
                        Ok(r) => format!("IP change requested ({r})"),
                        Err(e) => format!("IP change failed: {e}"),
                    };
                    let _ = tx.send(Msg::Log(i.clone(), m));
                    std::thread::sleep(Duration::from_secs(5)); // estimated: give the provider time to rotate
                    let _ = tx.send(Msg::Geo(i.clone(), proxy::check(&p, &key)));
                    let _ = tx.send(Msg::Busy(i, None));
                    c.request_repaint();
                });
            }
            Action::Edit(id) => {
                if let Some(s) = self.store.sessions.iter().find(|s| s.uuid == id).cloned() {
                    self.edit = Some(EditState { tags: s.tags.join(", "), s, new: false, line: String::new() });
                }
            }
            Action::SetProxy(ids) => {
                let p = ids.first().and_then(|i| self.store.sessions.iter().find(|s| &s.uuid == i)).map(|s| s.proxy.clone()).unwrap_or_default();
                self.proxy_dlg = Some(ProxyDlg { targets: ids, proxy: p, line: String::new(), result: None, checking: false });
            }
            Action::Warm(id) => {
                let st = &self.store.settings;
                let cc = self.store.sessions.iter().find(|s| s.uuid == id).and_then(|s| s.geo.as_ref()).filter(|g| g.ok).map(|g| g.country_code.clone()).unwrap_or_default();
                // The list is the country's popular sites unless the user saved their own in Preferences.
                let urls = if st.warmup_urls == crate::model::DEFAULT_WARMUP_URLS { crate::model::regional_sites(&cc) } else { st.warmup_urls.clone() };
                // count/time/depth defaults are estimated; their limits come from the original's API schema
                self.warm_dlg = Some(WarmDlg { uuid: id, urls, count: 10, time: 15, depth: 2, accept: st.warmup_accept_cookies, referrer: 1, set_warmed: true, cc });
            }
            Action::ImportCookies(id) => {
                let Some(path) = rfd::FileDialog::new().add_filter("Cookies", &["json", "txt"]).pick_file() else { return };
                let cookies = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| browser::parse_cookies(&t)) {
                    Ok(c) => c,
                    Err(e) => {
                        self.toast(e);
                        return;
                    }
                };
                self.with_browser(ctx, &id, "importing cookies", true, move |port, _| {
                    browser::import_cookies(port, &cookies).map(|n| format!("Imported {n} cookies"))
                });
            }
            Action::ExportCookies(id) => {
                let name = self.store.sessions.iter().find(|s| s.uuid == id).map(|s| s.name.clone()).unwrap_or_default();
                let Some(path) = rfd::FileDialog::new().set_file_name(format!("{name}.json")).save_file() else { return };
                self.with_browser(ctx, &id, "exporting cookies", true, move |port, _| {
                    let c = browser::export_cookies(port)?;
                    std::fs::write(&path, serde_json::to_vec_pretty(&c).unwrap_or_default()).map_err(|e| e.to_string())?;
                    Ok(format!("Exported {} cookies to {}", c.len(), path.display()))
                });
            }
            Action::RemoveData(id) => {
                if self.running.contains_key(&id) || self.busy.contains_key(&id) {
                    self.toast("Stop the session first.");
                    return;
                }
                let _ = std::fs::remove_dir_all(profile_dir(&id).join("data"));
                if let Some(s) = self.sess(&id) {
                    s.warmed = false;
                }
                self.log(&id, "All browser data removed (cookies, history, cache)");
            }
            Action::Clone(id) => {
                if let Some(s) = self.store.sessions.iter().find(|s| s.uuid == id).cloned() {
                    let t = now();
                    let name = format!("{} copy", s.name).chars().take(50).collect();
                    let mut c = Session { uuid: uuid::Uuid::new_v4().to_string(), name, created: t, modified: t, used: 0, run_secs: 0, log: vec![], warmed: false, ..s };
                    c.fp.seed = fastrand::u32(..);
                    c.log("Cloned");
                    self.store.sessions.push(c);
                    self.dirty = true;
                }
            }
            Action::Trash(id) => {
                self.act(ctx, Action::Stop(id.clone()));
                if let Some(s) = self.sess(&id) {
                    s.trashed = true;
                    s.trashed_at = now();
                }
                self.selected.remove(&id);
                self.dirty = true;
            }
            Action::Restore(id) => {
                if let Some(s) = self.sess(&id) {
                    s.trashed = false;
                }
                self.dirty = true;
            }
            Action::Purge(id) => {
                let _ = std::fs::remove_dir_all(profile_dir(&id));
                self.store.sessions.retain(|s| s.uuid != id);
                self.dirty = true;
            }
            Action::Pin(id) => {
                if let Some(s) = self.sess(&id) {
                    s.pinned = !s.pinned;
                }
                self.dirty = true;
            }
            Action::Log(id) => self.log_dlg = Some(id),
            Action::Desc(id) => {
                let d = self.store.sessions.iter().find(|s| s.uuid == id).map(|s| s.description.clone()).unwrap_or_default();
                self.desc_dlg = Some((id, d));
            }
            Action::MoveTo(id, desk) => {
                if let Some(s) = self.sess(&id) {
                    s.desktop = desk;
                    s.modified = now();
                }
                self.dirty = true;
            }
        }
    }

    pub(crate) fn new_session(&mut self, quick: bool) {
        let mut s = self.next_session();
        s.log("Session created");
        if quick {
            self.store.sessions.push(s);
            self.dirty = true;
        } else {
            self.edit = Some(EditState { tags: String::new(), s, new: true, line: String::new() });
        }
    }

    fn visible(&self) -> Vec<String> {
        let q = self.search.to_lowercase();
        let mut v: Vec<&Session> = self
            .store
            .sessions
            .iter()
            .filter(|s| s.desktop == self.store.current_desktop && s.trashed == self.trash_view)
            .filter(|s| self.proxy_filter.is_none_or(|k| s.proxy.kind == k))
            .filter(|s| !self.running_only || self.running.contains_key(&s.uuid))
            .filter(|s| !self.pinned_only || s.pinned)
            .filter(|s| self.tag_filter.as_ref().is_none_or(|t| s.tags.contains(t)))
            .filter(|s| {
                q.is_empty() || {
                    let geo = s.geo.as_ref().map(|g| format!("{} {}", g.ip, g.place())).unwrap_or_default();
                    format!("{} {} {} {} {} {}", s.name, s.description, s.tags.join(" "), s.proxy.host, geo, devices::get(&s.fp.device).label)
                        .to_lowercase()
                        .contains(&q)
                }
            })
            .collect();
        v.sort_by(|a, b| {
            let o = match self.sort {
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::Created => a.created.cmp(&b.created),
                Sort::Used => a.used.cmp(&b.used),
                Sort::Modified => a.modified.cmp(&b.modified),
                Sort::Timer => a.run_secs.cmp(&b.run_secs),
            };
            b.pinned.cmp(&a.pinned).then(if self.desc { o.reverse() } else { o })
        });
        v.into_iter().map(|s| s.uuid.clone()).collect()
    }

    fn start_session(&mut self, ctx: &egui::Context, id: &str, headless: bool, debug_port: Option<u16>) -> Result<u16, String> {
        if let Some(r) = self.running.get(id) {
            return Ok(r.port);
        }
        if self.busy.contains_key(id) {
            return Err("This session is busy, try again when it finishes.".into());
        }
        let s = self.store.sessions.iter().find(|s| s.uuid == id && !s.trashed).cloned().ok_or("No such session")?;
        match browser::launch(&self.store.settings, &s, headless, debug_port) {
            Ok(r) => {
                if let Some(c) = crate::import::load_pending(id) {
                    let port = r.port;
                    let p = crate::import::pending_cookies_path(id);
                    std::thread::spawn(move || {
                        if browser::wait_ready(port, 30).is_ok() && browser::import_cookies(port, &c).is_ok() {
                            let _ = std::fs::remove_file(p);
                        }
                    });
                }
                let port = r.port;
                self.running.insert(id.to_string(), r);
                if let Some(x) = self.sess(id) {
                    x.used = now();
                }
                let d = devices::get(&s.fp.device).label;
                self.log(id, format!("Browser started as {d} via {}", s.proxy.short()));
                if s.proxy.kind != ProxyKind::Direct && s.geo.is_none() {
                    self.spawn_check(ctx, id);
                }
                Ok(port)
            }
            Err(e) => {
                self.log(id, format!("Could not start: {e}"));
                Err(e)
            }
        }
    }

    fn next_session(&self) -> Session {
        let label = self.store.preset_label(self.preset);
        let n = self.store.sessions.iter().filter(|s| s.name.starts_with(&label)).count() + 1;
        self.store.session_from_preset(format!("{label} {n}"), &self.store.current_desktop, self.preset)
    }

    /// Menu entries: the built-in OS presets, then the user's saved ones (✕ deletes).
    pub(crate) fn preset_list(&mut self, ui: &mut Ui) {
        for p in Preset::ALL {
            if ui.selectable_label(self.store.active_preset.is_none() && self.preset == p, p.label()).clicked() {
                self.preset = p;
                self.store.active_preset = None;
                self.dirty = true;
                ui.close();
            }
        }
        let names: Vec<String> = self.store.presets.iter().map(|p| p.name.clone()).collect();
        if !names.is_empty() {
            ui.separator();
        }
        for n in names {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.store.active_preset.as_ref() == Some(&n), &n).clicked() {
                    self.store.active_preset = Some(n.clone());
                    self.dirty = true;
                    ui.close();
                }
                if ui.small_button("✕").on_hover_text("Delete this preset").clicked() {
                    self.store.presets.retain(|p| p.name != n);
                    if self.store.active_preset.as_ref() == Some(&n) {
                        self.store.active_preset = None;
                    }
                    self.dirty = true;
                }
            });
        }
    }

    fn handle_api(&mut self, ctx: &egui::Context, req: crate::api::Req) {
        use serde_json::json;
        let uuid = req.body["uuid"].as_str().unwrap_or("").to_string();
        let out = match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/sessions") => {
                let v: Vec<_> = self.store.sessions.iter().filter(|s| !s.trashed).map(|s| json!({"uuid": s.uuid, "name": s.name, "debug_port": self.running.get(&s.uuid).map(|r| r.port)})).collect();
                (200, json!(v))
            }
            ("POST", "/sessions/create_quick") => {
                let mut s = self.next_session();
                s.log("Session created through the API");
                let r = json!({"name": s.name, "uuid": s.uuid});
                self.store.sessions.push(s);
                self.dirty = true;
                (200, r)
            }
            ("POST", "/sessions/start") => {
                let port = req.body["debug_port"].as_u64().filter(|p| (1..=65535).contains(p)).map(|p| p as u16);
                match self.start_session(ctx, &uuid, req.body["headless"].as_bool().unwrap_or(false), port) {
                    Ok(p) => (200, json!({"debug_port": p, "uuid": uuid})),
                    Err(e) => (400, json!({"error": e})),
                }
            }
            ("GET", "/presets") => {
                let mut v: Vec<String> = Preset::ALL.iter().map(|p| p.label().to_string()).collect();
                v.extend(self.store.presets.iter().map(|p| p.name.clone()));
                (200, json!({"presets": v, "active": self.store.preset_label(self.preset)}))
            }
            ("POST", "/presets/set_active") => {
                let n = req.body["name"].as_str().unwrap_or("");
                if let Some(p) = Preset::ALL.iter().find(|p| p.label() == n) {
                    self.preset = *p;
                    self.store.active_preset = None;
                    (200, json!({"active": n}))
                } else if self.store.presets.iter().any(|p| p.name == n) {
                    self.store.active_preset = Some(n.to_string());
                    (200, json!({"active": n}))
                } else {
                    (400, json!({"error": "no such preset"}))
                }
            }
            ("POST", "/sessions/clone") => {
                let before = self.store.sessions.len();
                self.act(ctx, Action::Clone(uuid.clone()));
                match self.store.sessions.get(before) {
                    Some(c) => (200, json!({"name": c.name, "uuid": c.uuid})),
                    None => (400, json!({"error": "No such session"})),
                }
            }
            ("POST", "/sessions/delete") => {
                if self.store.sessions.iter().any(|s| s.uuid == uuid) {
                    self.act(ctx, Action::Trash(uuid.clone()));
                    (200, json!({"uuid": uuid}))
                } else {
                    (400, json!({"error": "No such session"}))
                }
            }
            ("POST", "/sessions/set_proxy") => {
                let kind = match req.body["kind"].as_str().unwrap_or("http").to_ascii_lowercase().as_str() {
                    "socks5" | "socks" => ProxyKind::Socks5,
                    _ => ProxyKind::Http,
                };
                match Proxy::parse(req.body["proxy"].as_str().unwrap_or(""), kind) {
                    Some(p) => match self.sess(&uuid) {
                        Some(s) => {
                            s.log(format!("Proxy set to {} {}", p.kind.label(), p.short()));
                            s.proxy = p;
                            s.geo = None;
                            s.modified = now();
                            self.dirty = true;
                            (200, json!({"uuid": uuid}))
                        }
                        None => (400, json!({"error": "No such session"})),
                    },
                    None => (400, json!({"error": "Can't read the proxy line"})),
                }
            }
            ("POST", "/sessions/check_proxy") => {
                if self.store.sessions.iter().any(|s| s.uuid == uuid) {
                    self.spawn_check(ctx, &uuid);
                    (200, json!({"uuid": uuid, "note": "check started, see the session's action log"}))
                } else {
                    (400, json!({"error": "No such session"}))
                }
            }
            ("POST", "/sessions/import_cookies") => {
                let text = match &req.body["cookies"] {
                    serde_json::Value::String(t) => t.clone(),
                    serde_json::Value::Null => String::new(),
                    v => v.to_string(),
                };
                match (browser::parse_cookies(&text), self.store.sessions.iter().any(|s| s.uuid == uuid)) {
                    (Ok(mut c), true) => {
                        let n = c.len();
                        if let Some(r) = self.running.get(&uuid) {
                            let port = r.port;
                            std::thread::spawn(move || {
                                let _ = browser::import_cookies(port, &c);
                            });
                        } else {
                            let mut all = crate::import::load_pending(&uuid).unwrap_or_default();
                            all.append(&mut c);
                            crate::import::save_pending(&uuid, &all);
                        }
                        (200, json!({"imported": n, "applied": if self.running.contains_key(&uuid) { "now" } else { "at the next start" }}))
                    }
                    (Err(e), _) => (400, json!({"error": e})),
                    (_, false) => (400, json!({"error": "No such session"})),
                }
            }
            ("POST", "/sessions/export_cookies") => match self.running.get(&uuid) {
                Some(r) => match browser::export_cookies(r.port) {
                    Ok(c) => (200, json!(c)),
                    Err(e) => (400, json!({"error": e})),
                },
                None => (400, json!({"error": "The session must be running"})),
            },
            ("POST", "/sessions/stop") => {
                self.act(ctx, Action::Stop(uuid.clone()));
                (200, json!({"uuid": uuid}))
            }
            _ => (404, json!({"error": "unknown endpoint"})),
        };
        let _ = req.reply.send(out);
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Api(req) => self.handle_api(ctx, req),
                Msg::Geo(id, g) => {
                    let msg = if g.ok { format!("Proxy works: {} {} ({} ms)", g.ip, g.place(), g.latency_ms) } else { format!("Proxy check failed: {}", g.error) };
                    if let Some(s) = self.sess(&id) {
                        if g.ok && s.fp.geo_from_proxy && s.fp.language == "en-US" {
                            s.fp.language = guess_language(&g.country_code).into();
                        }
                        s.geo = Some(g);
                    }
                    self.log(&id, msg);
                }
                Msg::Log(id, t) => self.log(&id, t),
                Msg::Busy(id, Some(b)) => {
                    self.busy.insert(id, b);
                }
                Msg::Busy(id, None) => {
                    self.busy.remove(&id);
                }
                Msg::Warmed(id) => {
                    if let Some(s) = self.sess(&id) {
                        s.warmed = true;
                    }
                    self.dirty = true;
                }
                Msg::Bulk(gen, i, g) => {
                    if let Some(r) = self.checker.as_mut().filter(|c| c.gen == gen).and_then(|c| c.rows.get_mut(i)) {
                        r.2 = Some(g);
                    }
                }
                Msg::DialogGeo(g) => {
                    if let Some(d) = self.proxy_dlg.as_mut() {
                        d.result = Some(g);
                        d.checking = false;
                    }
                }
            }
        }
    }

    fn poll_running(&mut self) {
        let ended: Vec<String> = self.running.iter_mut().filter_map(|(k, r)| (!r.alive()).then(|| k.clone())).collect();
        for id in ended {
            if let Some(r) = self.running.remove(&id) {
                let secs = r.started.elapsed().as_secs();
                if let Some(s) = self.sess(&id) {
                    s.run_secs += secs;
                }
                self.log(&id, "Browser closed");
            }
        }
    }
}

fn detect(st: &Settings) -> Option<(PathBuf, Option<u32>)> {
    browser::find_browser(&st.browser_path).map(|p| {
        let m = browser::browser_major(&p);
        (p, m)
    })
}

/// Browser language for a proxy's country, so the locale follows the exit IP.
fn guess_language(cc: &str) -> &'static str {
    match cc {
        "PL" => "pl-PL",
        "DE" | "AT" => "de-DE",
        "CH" => "de-CH",
        "FR" => "fr-FR",
        "ES" => "es-ES",
        "MX" | "AR" | "CO" | "CL" => "es-419",
        "IT" => "it-IT",
        "PT" => "pt-PT",
        "BR" => "pt-BR",
        "NL" => "nl-NL",
        "GB" | "IE" => "en-GB",
        "UA" => "uk-UA",
        "TR" => "tr-TR",
        "JP" => "ja-JP",
        "KR" => "ko-KR",
        "CN" => "zh-CN",
        "CZ" => "cs-CZ",
        "SE" => "sv-SE",
        _ => "en-US",
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let load = |f: &str| std::fs::read(format!(r"{windir}\Fonts\{f}")).ok();
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut boldfam = prop.clone();
    if let Some(b) = load("segoeui.ttf") {
        fonts.font_data.insert("ui".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.insert(0, "ui".into());
    }
    if let Some(b) = load("seguisb.ttf").or_else(|| load("segoeuib.ttf")) {
        fonts.font_data.insert("uib".into(), Arc::new(egui::FontData::from_owned(b)));
        boldfam.insert(0, "uib".into());
    }
    if let Some(b) = load("seguisym.ttf") {
        fonts.font_data.insert("sym".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.push("sym".into());
        boldfam.push("sym".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("bold".into()), boldfam);
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = Color32::from_rgb(0x12, 0x15, 0x1C);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = Color32::from_rgb(0x1A, 0x1D, 0x24);
    v.selection.bg_fill = ACCENT;
    v.hyperlink_color = ICON;
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x1C, 0x20, 0x29);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x1C, 0x20, 0x29);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x26, 0x2B, 0x37);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.menu_corner_radius = 8.into();
    v.window_corner_radius = 10.into();
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.text_styles.insert(egui::TextStyle::Body, body(14.0));
        s.text_styles.insert(egui::TextStyle::Button, body(14.0));
    });
}

// ---------------------------------------------------------------- small widgets

fn toggle(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(30.0, 17.0), Sense::click());
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(r.id, *on);
    let bg = if *on { Color32::from_rgb(0x3D, 0x6F, 0xE8) } else { Color32::from_rgb(0x3A, 0x3F, 0x4B) };
    ui.painter().rect_filled(rect, 9, bg);
    let x = egui::lerp(rect.left() + 8.5..=rect.right() - 8.5, t);
    ui.painter().circle_filled(Pos2::new(x, rect.center().y), 6.0, Color32::WHITE);
    r
}

fn text_w(ui: &Ui, t: &str, f: FontId) -> f32 {
    ui.fonts(|fo| fo.layout_no_wrap(t.to_string(), f, TEXT).size().x)
}

/// Icon glyph + bold label, like the original's toolbar buttons.
fn tool_button(ui: &mut Ui, icon: &str, label: &str, icon_col: Color32, enabled: bool) -> egui::Response {
    let f = bold(14.5);
    let w = 28.0 + text_w(ui, label, f.clone());
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 30.0), if enabled { Sense::click() } else { Sense::hover() });
    let hov = r.hovered() && enabled;
    let tc = if !enabled { FAINT } else if hov { Color32::WHITE } else { TEXT };
    let ic = if enabled { icon_col } else { FAINT };
    let p = ui.painter();
    paint_icon(p, icon, Pos2::new(rect.left() + 9.0, rect.center().y), 18.0, ic);
    p.text(Pos2::new(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, label, f, tc);
    r
}

fn icon_click(ui: &mut Ui, rect: Rect, icon: &str, size: f32, col: Color32, tip: &str) -> bool {
    let r = ui.interact(rect, ui.id().with((tip, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let c = if r.hovered() { Color32::WHITE } else { col };
    paint_icon(ui.painter(), icon, rect.center(), size, c);
    r.on_hover_text(tip).clicked()
}

/// Glyph icons, plus a few drawn by hand where the system fonts have no good glyph.
fn paint_icon(p: &egui::Painter, icon: &str, c: Pos2, size: f32, col: Color32) {
    let st = Stroke::new(1.5, col);
    let k = size / 18.0;
    match icon {
        "proxy" => {
            let (a, b, d) = (c + Vec2::new(-5.0, -7.0) * k, c + Vec2::new(-5.0, 7.0) * k, c + Vec2::new(5.0, -3.0) * k);
            for x in [a, b, d] {
                p.circle_stroke(x, 2.6 * k, st);
            }
            p.line_segment([a + Vec2::new(0.0, 2.6 * k), b - Vec2::new(0.0, 2.6 * k)], st);
            let mid = c + Vec2::new(-5.0, 3.0) * k;
            p.add(egui::Shape::line(vec![d + Vec2::new(0.0, 2.6 * k), d + Vec2::new(0.0, 4.5 * k), mid + Vec2::new(0.0, -0.5 * k)], st));
        }
        "bell" => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 16.0);
                    c + Vec2::new(-a.cos() * 5.0, -a.sin() * 6.0 + 1.0) * k
                })
                .collect();
            p.add(egui::Shape::line(pts, st));
            p.line_segment([c + Vec2::new(-5.0, 1.0) * k, c + Vec2::new(-6.5, 5.0) * k], st);
            p.line_segment([c + Vec2::new(5.0, 1.0) * k, c + Vec2::new(6.5, 5.0) * k], st);
            p.line_segment([c + Vec2::new(-6.5, 5.0) * k, c + Vec2::new(6.5, 5.0) * k], st);
            p.line_segment([c + Vec2::new(-1.8, 7.5) * k, c + Vec2::new(1.8, 7.5) * k], st);
        }
        "multi" => {
            let s = 5.0 * k;
            for (dx, dy) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                p.rect_stroke(Rect::from_center_size(c + Vec2::new(dx * 4.0, dy * 4.0) * k, Vec2::splat(s * 1.2)), 1, st, StrokeKind::Middle);
            }
            let q = c + Vec2::new(4.0, -4.0) * k;
            p.line_segment([q - Vec2::new(3.0 * k, 0.0), q + Vec2::new(3.0 * k, 0.0)], st);
            p.line_segment([q - Vec2::new(0.0, 3.0 * k), q + Vec2::new(0.0, 3.0 * k)], st);
        }
        _ => {
            p.text(c, Align2::CENTER_CENTER, icon, body(size), col);
        }
    }
}

/// Makes menu buttons inside `ui` draw without a box, like the original's text dropdowns.
fn flat(ui: &mut Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.weak_bg_fill = Color32::TRANSPARENT;
        s.bg_fill = Color32::TRANSPARENT;
        s.bg_stroke = Stroke::NONE;
    }
}

fn dots(ui: &Ui, c: Pos2, col: Color32) {
    for dy in [-6.0, 0.0, 6.0] {
        ui.painter().circle_filled(c + Vec2::new(0.0, dy), 1.9, col);
    }
}

fn person(p: &egui::Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c + Vec2::new(0.0, -6.0), 4.0, Stroke::new(1.6, col));
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 12.0);
            c + Vec2::new(-a.cos() * 5.5, 9.0 - a.sin() * 7.0)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.6, col)));
}

fn check_box(ui: &mut Ui, rect: Rect, on: bool) -> bool {
    let r = ui.interact(rect, ui.id().with(("cb", rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let b = Rect::from_center_size(rect.center(), Vec2::splat(16.0));
    ui.painter().rect_stroke(b, 3, Stroke::new(1.4, if r.hovered() { TEXT } else { DIM }), StrokeKind::Inside);
    if on {
        ui.painter().rect_filled(b.shrink(3.5), 1, ICON);
    }
    r.clicked()
}

fn proxy_editor(ui: &mut Ui, p: &mut Proxy, line: &mut String, kinds: bool) {
    if kinds {
        ui.horizontal(|ui| {
            for k in ProxyKind::ALL {
                ui.selectable_value(&mut p.kind, k, k.label());
            }
        });
    }
    if p.kind == ProxyKind::Direct {
        ui.label(RichText::new("No proxy: the session uses this computer's own connection.").color(DIM));
        return;
    }
    if p.kind == ProxyKind::Localhost {
        ui.horizontal(|ui| {
            ui.label("Local port");
            ui.add(egui::DragValue::new(&mut p.port).range(0..=65535));
            ui.label(RichText::new("SOCKS5 or HTTP on 127.0.0.1, no login").color(DIM));
        });
        ui.horizontal(|ui| {
            ui.label("Change-IP link");
            ui.add(egui::TextEdit::singleline(&mut p.change_ip_url).desired_width(260.0));
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(line).hint_text("Paste host:port:login:password or socks5://login:pass@host:port").desired_width(380.0));
        if ui.button("Fill in").clicked() {
            match Proxy::parse(line, p.kind) {
                Some(n) => {
                    let url = std::mem::take(&mut p.change_ip_url);
                    *p = Proxy { change_ip_url: url, ..n };
                    line.clear();
                }
                None => line.insert_str(0, "?? "),
            }
        }
    });
    egui::Grid::new(ui.id().with("pg")).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Host");
        ui.add(egui::TextEdit::singleline(&mut p.host).hint_text(if p.kind == ProxyKind::Tor { "127.0.0.1" } else { "proxy.example.com" }).desired_width(260.0));
        ui.end_row();
        ui.label("Port");
        ui.add(egui::DragValue::new(&mut p.port).range(0..=65535));
        ui.end_row();
        ui.label("Login");
        ui.add(egui::TextEdit::singleline(&mut p.login).desired_width(260.0));
        ui.end_row();
        ui.label("Password");
        ui.add(egui::TextEdit::singleline(&mut p.password).password(true).desired_width(260.0));
        ui.end_row();
        ui.label("Change-IP link");
        ui.add(egui::TextEdit::singleline(&mut p.change_ip_url).hint_text("optional, for rotating/mobile proxies").desired_width(260.0));
        ui.end_row();
        if p.kind != ProxyKind::Tor {
            ui.label("Custom DNS");
            ui.add(egui::TextEdit::singleline(&mut p.dns).hint_text("empty = proxy/system, or 1.1.1.1;8.8.8.8").desired_width(260.0));
            ui.end_row();
        }
    });
    if p.kind == ProxyKind::Tor && p.host.is_empty() {
        ui.label(RichText::new("Tor: leave host empty to use a local Tor on 127.0.0.1:9050.").color(DIM));
    }
}

fn geo_line(ui: &mut Ui, g: &GeoInfo) {
    if g.ok {
        let mut t = format!("✔ {}  {}  ·  {}  ·  {} ms", g.ip, g.place(), g.isp, g.latency_ms);
        if let Some(f) = g.fraud_score {
            t += &format!("  ·  fraud score {f}");
        }
        ui.label(RichText::new(t).color(GREEN));
    } else {
        ui.label(RichText::new(format!("✖ {}", g.error)).color(RED));
    }
}
