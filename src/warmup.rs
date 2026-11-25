//! Warm-up ("cookie robot"): visits sites in the session's own browser the way a person would, so a new profile
//! collects ordinary cookies, storage and history before real use.
//!
//! All input goes through `Input.dispatchMouseEvent` / `Input.dispatchKeyEvent`: trusted events with a curved mouse
//! path, wheel scrolling and per-key typing. Page-script `click()` / `scrollBy` / `location.href=` (the old way) give
//! `isTrusted=false` events, no mouse movement and no wheel, which anti-bot scripts tell apart.
//! Sites can be reached through a real search (type the name, click the result) instead of a faked Referer.

use crate::browser::{export_cookies, Conn};
use crate::cdp;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Warmup {
    /// The session's uuid, so `cancel` can stop it.
    pub session: String,
    /// Sites (with or without `https://`) and search queries (a line with spaces or without a dot, or starting with `?`).
    pub urls: Vec<String>,
    pub url_count: u32,
    pub time_per_url: u32,
    pub view_depth: u32,
    pub accept_cookies: bool,
    /// Search page the visits appear to come from (sent as the Referer); empty: direct visits.
    pub referrer: String,
    /// Reach each site through a real search on the referrer's engine (Google when none) instead of opening it.
    pub real_search: bool,
}

/// Limits from the original's "Start warmup" API schema (string at 0x14279ce20).
pub const URL_COUNT: (u32, u32) = (1, 50);
pub const TIME_PER_URL: (u32, u32) = (0, 99);
pub const VIEW_DEPTH: (u32, u32) = (1, 10);
pub const MAX_URLS: usize = 50;
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const STOPPED: &str = "Warm-up stopped";

#[derive(Default, Debug)]
pub struct Report {
    pub sites: usize,
    pub pages: usize,
    pub searches: usize,
    pub captchas: usize,
    pub cookies: usize,
    pub stopped: bool,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Warm-up {}: {} sites, {} pages, {} searches, {} new cookies",
            if self.stopped { "stopped" } else { "finished" },
            self.sites,
            self.pages,
            self.searches,
            self.cookies
        );
        if self.captchas > 0 {
            s += &format!(", {} captcha pages skipped", self.captchas);
        }
        s
    }
}

static CANCELLED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Asks a running warm-up of `session` to stop at its next step (within about 0.2 s of a pause).
pub fn cancel(session: &str) {
    if let Ok(mut c) = CANCELLED.lock() {
        if !c.iter().any(|s| s == session) {
            c.push(session.to_string());
        }
    }
}

fn cancelled(session: &str) -> bool {
    CANCELLED.lock().map(|c| c.iter().any(|s| s == session)).unwrap_or(false)
}

fn clear_cancel(session: &str) {
    if let Ok(mut c) = CANCELLED.lock() {
        c.retain(|s| s != session);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Entry {
    Site(String),
    Query(String),
}

pub fn parse_entry(line: &str) -> Option<Entry> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    if let Some(q) = t.strip_prefix('?') {
        let q = q.trim();
        return (!q.is_empty()).then(|| Entry::Query(q.to_string()));
    }
    if t.contains("://") {
        return Some(Entry::Site(t.to_string()));
    }
    if !t.contains(' ') && t.contains('.') {
        return Some(Entry::Site(format!("https://{t}")));
    }
    Some(Entry::Query(t.to_string()))
}

pub fn pick_entries(w: &Warmup) -> Vec<Entry> {
    let mut e: Vec<Entry> = w.urls.iter().filter_map(|u| parse_entry(u)).take(MAX_URLS).collect();
    fastrand::shuffle(&mut e);
    e.truncate(w.url_count.clamp(URL_COUNT.0, URL_COUNT.1) as usize);
    e
}

/// `www.allegro.pl` from `https://www.allegro.pl/x`.
fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase()
}

/// What a person types to find a site: usually its name ("allegro"), sometimes the whole domain.
fn site_query(host: &str, roll: u8) -> String {
    let h = host.strip_prefix("www.").unwrap_or(host);
    if roll < 60 {
        h.split('.').next().unwrap_or(h).to_string()
    } else {
        h.to_string()
    }
}

/// Start page of the referrer's search engine; Google when the referrer is none or not a known engine.
fn engine_home(referrer: &str) -> &'static str {
    for (key, home) in [("bing.", "https://www.bing.com/"), ("duckduckgo.", "https://duckduckgo.com/"), ("yahoo.", "https://search.yahoo.com/")] {
        if referrer.contains(key) {
            return home;
        }
    }
    "https://www.google.com/"
}

/// `[x, y, label]` of a cookie-banner button in the viewport (`x` < 0: hidden, already clicked by script), or null.
/// Looks in the page, in open shadow roots (Usercentrics) and in same-origin iframes; cross-site iframes
/// (Sourcepoint, most TCF banners) run in another renderer and are handled by `accept_in_frames`.
const ACCEPT_JS: &str = r#"(()=>{const re=/^(accept( all)?( cookies)?|allow( all)?( cookies)?|i agree|agree( and continue)?|got it|ok|consent|zaakceptuj( wszystko| wszystkie)?|akceptuj(ę)?( wszystko| wszystkie)?|zgadzam się|alle akzeptieren|akzeptieren|tout accepter|accepter|aceptar( todo)?|accetta( tutto)?|aceitar( tudo)?)$/i;
const sel='#onetrust-accept-btn-handler,#L2AGLb,#didomi-notice-agree-button,#CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll,#truste-consent-button,.sp_choice_type_11,.fc-cta-consent,[data-testid=uc-accept-all-button],#uc-btn-accept-banner';
const hit=(e,ox,oy)=>{const r=e.getBoundingClientRect();const t=(e.innerText||e.value||e.title||e.id||'').trim().slice(0,40);const top=r.top+oy;if(r.width>4&&r.height>4&&top>=0&&top+r.height<=innerHeight)return [r.left+ox+r.width*(0.3+Math.random()*0.4),top+r.height*(0.3+Math.random()*0.4),t];e.click();return [-1,-1,t]};
const look=(root,ox,oy)=>{const k=root.querySelector(sel);if(k&&k.getClientRects().length)return hit(k,ox,oy);
for(const e of root.querySelectorAll('button,a,[role=button],input[type=button],input[type=submit]')){const t=(e.innerText||e.value||e.title||'').trim().replace(/\s+/g,' ');if(t&&t.length<40&&re.test(t)&&e.getClientRects().length)return hit(e,ox,oy)}
for(const h of root.querySelectorAll('*')){if(h.shadowRoot){const r=look(h.shadowRoot,ox,oy);if(r)return r}}return null};
let r=look(document,0,0);if(r)return r;
for(const f of document.querySelectorAll('iframe')){let d=null;try{d=f.contentDocument}catch(_){}if(!d)continue;const b=f.getBoundingClientRect();r=look(d,b.left+f.clientLeft,b.top+f.clientTop);if(r)return r}return null})()"#;

/// `[left, top]` of the iframe whose address has the given origin, as placed on the page, or null.
const FRAME_AT_JS: &str = r#"(o=>{for(const f of document.querySelectorAll('iframe')){try{if(f.src&&new URL(f.src,location.href).origin===o){const b=f.getBoundingClientRect();if(b.width>4&&b.height>4)return [b.left+f.clientLeft,b.top+f.clientTop]}}catch(_){}}return null})"#;

/// `[x, y, href]` of a random same-site link fully in the viewport and not covered, or null. Skips account,
/// cart, logout and download links and anything opening a new tab.
const LINK_JS: &str = r#"(()=>{const bad=/(log ?out|sign ?out|log ?in|sign ?in|register|sign ?up|cart|basket|checkout|account|password|unsubscribe|delete|download|print|wyloguj|zaloguj|koszyk)/i;
const ext=/\.(pdf|zip|rar|7z|exe|msi|dmg|apk|mp3|mp4|avi|mov|jpe?g|png|gif|webp|docx?|xlsx?|pptx?)$/i;
const site=h=>h.replace(/^www\./,''),me=site(location.host),here=location.href.split('#')[0];
const l=[...document.links].filter(a=>{const h=site(a.host);if(h!==me&&!h.endsWith('.'+me))return false;
if(!/^https?:$/.test(a.protocol)||a.target==='_blank'||a.hasAttribute('download'))return false;
if(a.href.split('#')[0]===here||ext.test(a.pathname)||bad.test(a.href)||bad.test(a.innerText||''))return false;
const r=a.getBoundingClientRect();if(r.width<8||r.height<8||r.top<40||r.bottom>innerHeight-10||r.left<0||r.right>innerWidth)return false;
const e=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return !!e&&(e===a||a.contains(e))});
if(!l.length)return null;const a=l[Math.floor(Math.random()*l.length)],r=a.getBoundingClientRect();
return [r.left+r.width*(0.3+Math.random()*0.4),r.top+r.height*(0.3+Math.random()*0.4),a.href]})()"#;

const SEARCH_INPUT_JS: &str = r#"(()=>{const e=[...document.querySelectorAll('textarea[name=q],input[name=q],input[name=p],input[type=search]')].find(e=>e.offsetParent);if(!e)return null;const r=e.getBoundingClientRect();return [r.left+Math.min(r.width*0.5,240)*(0.4+Math.random()*0.6),r.top+r.height*(0.35+Math.random()*0.3),'']})()"#;

const CAPTCHA_JS: &str = r#"(()=>/\/sorry\/|captcha|challenge/i.test(location.pathname)||!!document.querySelector('iframe[src*="recaptcha"],iframe[src*="hcaptcha"],iframe[src*="challenges.cloudflare"],#challenge-form,#cf-challenge-running')||/just a moment|unusual traffic|nietypowy ruch/i.test(document.title+' '+((document.body&&document.body.innerText)||'').slice(0,600)))()"#;

/// `[x, y, href]` of a search result in the viewport, `0` when results exist only below it, null when none.
/// `want` (a domain, lowercase) picks the result for that site among the first ten; empty: any of the first five.
fn result_js(want: &str) -> String {
    format!(
        r#"(()=>{{const want={};const c=[...document.querySelectorAll('a')].filter(a=>/^https?:/.test(a.href)&&(a.querySelector('h3')||a.closest('h2,h3')));
let l=want?c.slice(0,10).filter(a=>(a.href+' '+((a.closest('li,div')||a).innerText||'')).toLowerCase().includes(want)):c.slice(0,5);
if(!l.length)return null;const v=l.filter(a=>{{const r=a.getBoundingClientRect();return r.width>8&&r.height>8&&r.top>60&&r.bottom<innerHeight-10}});
if(!v.length)return 0;const a=want?v[0]:v[Math.floor(Math.random()*v.length)],r=a.getBoundingClientRect();
return [r.left+Math.min(r.width,300)*(0.2+Math.random()*0.5),r.top+r.height*(0.3+Math.random()*0.4),a.href]}})()"#,
        Value::String(want.to_string())
    )
}

fn rand_ms(lo: u64, hi: u64) -> u64 {
    fastrand::u64(lo..=hi)
}

/// Seconds on a page around the chosen time: 60–150 %, so pages don't all take the same.
fn dwell_ms(secs: u32) -> u64 {
    (secs as f64 * 1000.0 * (0.6 + fastrand::f64() * 0.9)) as u64
}

/// Point on a cubic Bézier curve at `t`.
fn bezier(p: [(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0, a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1)
}

enum Searched {
    Clicked,
    NoResult,
    Blocked,
}

/// One tab driven like a person: it remembers where the mouse is.
struct Tab<'a> {
    conn: &'a Conn,
    sid: String,
    session: &'a str,
    x: f64,
    y: f64,
    url: String,
}

impl Tab<'_> {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        cdp::call(self.conn, Some(&self.sid), method, params)
    }

    fn eval(&self, js: &str) -> Value {
        cdp::eval(self.conn, Some(&self.sid), js).unwrap_or(Value::Null)
    }

    /// Sleeps, but gives up within 0.2 s of a `cancel`.
    fn pause(&self, ms: u64) -> Result<(), String> {
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            if cancelled(self.session) {
                return Err(STOPPED.into());
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            std::thread::sleep(left.min(Duration::from_millis(200)));
        }
    }

    fn mouse(&self, kind: &str, extra: Value) -> Result<(), String> {
        let mut p = json!({ "type": kind, "x": self.x, "y": self.y });
        if let (Some(p), Value::Object(e)) = (p.as_object_mut(), extra) {
            p.extend(e);
        }
        self.call("Input.dispatchMouseEvent", p).map(|_| ())
    }

    fn viewport(&self) -> (f64, f64, bool) {
        let v = self.eval("[innerWidth, innerHeight, scrollY + innerHeight >= document.documentElement.scrollHeight - 8]");
        (v[0].as_f64().unwrap_or(1200.0), v[1].as_f64().unwrap_or(700.0), v[2].as_bool().unwrap_or(false))
    }

    /// Moves along a curve with easing (slow start and end), about 10–20 px per event.
    fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        let (x0, y0) = (self.x, self.y);
        let dist = ((x - x0).powi(2) + (y - y0).powi(2)).sqrt();
        let steps = ((dist / 14.0) as usize).clamp(5, 45);
        let bend = |s: f64| (fastrand::f64() - 0.5) * dist * s;
        let ctrl = [(x0, y0), (x0 + (x - x0) * 0.3 + bend(0.3), y0 + (y - y0) * 0.3 + bend(0.3)), (x0 + (x - x0) * 0.7 + bend(0.2), y0 + (y - y0) * 0.7 + bend(0.2)), (x, y)];
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let (px, py) = bezier(ctrl, t * t * (3.0 - 2.0 * t));
            (self.x, self.y) = (px.max(0.0), py.max(0.0));
            self.mouse("mouseMoved", json!({}))?;
            std::thread::sleep(Duration::from_millis(rand_ms(6, 18)));
        }
        Ok(())
    }

    fn click_at(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.url = self.eval("location.href").as_str().unwrap_or("").to_string();
        self.move_to(x, y)?;
        self.pause(rand_ms(80, 260))?;
        self.mouse("mousePressed", json!({ "button": "left", "buttons": 1, "clickCount": 1 }))?;
        std::thread::sleep(Duration::from_millis(rand_ms(50, 140)));
        self.mouse("mouseReleased", json!({ "button": "left", "buttons": 0, "clickCount": 1 }))
    }

    /// Wheel notches of 100 px like a mouse wheel; negative scrolls up.
    fn wheel(&mut self, notches: i32) -> Result<(), String> {
        let dy = if notches < 0 { -100.0 } else { 100.0 };
        for _ in 0..notches.unsigned_abs() {
            self.mouse("mouseWheel", json!({ "deltaX": 0, "deltaY": dy }))?;
            std::thread::sleep(Duration::from_millis(rand_ms(40, 150)));
        }
        Ok(())
    }

    /// One key press; `shift` sets the modifier on both events, as a real Shift+key does.
    fn key(&self, key: &str, code: &str, vk: u32, text: &str, shift: bool) -> Result<(), String> {
        let m = if shift { 8 } else { 0 };
        let mut down = json!({ "type": "keyDown", "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk, "modifiers": m });
        if !text.is_empty() {
            down["text"] = json!(text);
            down["unmodifiedText"] = json!(text);
        }
        self.call("Input.dispatchKeyEvent", down)?;
        std::thread::sleep(Duration::from_millis(rand_ms(30, 110)));
        self.call("Input.dispatchKeyEvent", json!({ "type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk, "modifiers": m }))
            .map(|_| ())
    }

    /// Key by key at `delay` ms (random in the range), with an occasional longer pause like someone thinking.
    /// A new line is Enter and a tab is Tab, as typed on a keyboard.
    fn type_text(&self, s: &str, delay: (u64, u64)) -> Result<usize, String> {
        let mut n = 0;
        for c in s.chars() {
            match c {
                '\r' => continue,
                '\n' => self.key("Enter", "Enter", 13, "\r", false)?,
                '\t' => self.key("Tab", "Tab", 9, "", false)?,
                _ => {
                    let (code, vk, shift) = key_meta(c);
                    let t = c.to_string();
                    self.key(&t, &code, vk, &t, shift)?;
                }
            }
            n += 1;
            self.pause(rand_ms(delay.0, delay.1) + if fastrand::u8(0..100) < 6 { rand_ms(300, 900) } else { 0 })?;
        }
        Ok(n)
    }

    fn navigate(&mut self, url: &str, referrer: &str) -> Result<(), String> {
        self.url = self.eval("location.href").as_str().unwrap_or("").to_string();
        let mut nav = json!({ "url": url });
        if !referrer.is_empty() {
            nav["referrer"] = json!(referrer);
        }
        self.call("Page.navigate", nav).map(|_| ())
    }

    /// Waits for the address to change (up to 4 s; a click may not navigate) and the page to finish loading.
    fn wait_load(&self) -> Result<(), String> {
        self.pause(rand_ms(700, 1200))?;
        let start = Instant::now();
        loop {
            let v = self.eval("[document.readyState, location.href]");
            let (state, url) = (v[0].as_str().unwrap_or(""), v[1].as_str().unwrap_or(""));
            let moved = url != self.url || start.elapsed() > Duration::from_secs(4);
            if (moved && state == "complete") || start.elapsed() > LOAD_TIMEOUT {
                return Ok(());
            }
            self.pause(300)?;
        }
    }

    fn captcha(&self) -> bool {
        self.eval(CAPTCHA_JS).as_bool().unwrap_or(false)
    }

    fn accept_cookies(&mut self, progress: &dyn Fn(String)) -> Result<(), String> {
        let found = match self.eval(ACCEPT_JS) {
            Value::Array(a) => Some((a[0].as_f64().unwrap_or(-1.0), a[1].as_f64().unwrap_or(-1.0), a[2].as_str().unwrap_or("").to_string())),
            _ => self.find_in_frames()?,
        };
        if let Some((x, y, label)) = found {
            if x >= 0.0 {
                self.pause(rand_ms(600, 1800))?; // reading the banner
                self.click_at(x, y)?;
            }
            progress(format!("Accepted cookies ({label})"));
            self.pause(rand_ms(500, 1200))?;
        }
        Ok(())
    }

    /// A banner button inside a cross-site iframe (its own renderer and DevTools target): found there, returned as a
    /// point on the page, so the click is a real mouse click that Chrome routes into the frame.
    fn find_in_frames(&self) -> Result<Option<(f64, f64, String)>, String> {
        let targets = cdp::call(self.conn, None, "Target.getTargets", json!({}))?;
        let frames: Vec<(String, String)> = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["type"] == "iframe")
            .filter_map(|t| Some((t["targetId"].as_str()?.to_string(), t["url"].as_str()?.to_string())))
            .filter(|(_, u)| u.starts_with("http"))
            .collect();
        for (tid, url) in frames {
            let Some(origin) = url.match_indices('/').nth(2).map(|(i, _)| url[..i].to_string()) else { continue };
            let Value::Array(at) = self.eval(&format!("{FRAME_AT_JS}({})", Value::String(origin))) else { continue }; // not in our page
            let Ok(att) = cdp::call(self.conn, None, "Target.attachToTarget", json!({ "targetId": tid, "flatten": true })) else { continue };
            let Some(sid) = att["sessionId"].as_str() else { continue };
            let hit = cdp::eval(self.conn, Some(sid), ACCEPT_JS).unwrap_or(Value::Null);
            let _ = cdp::call(self.conn, None, "Target.detachFromTarget", json!({ "sessionId": sid }));
            if let Value::Array(a) = hit {
                let (x, y) = (a[0].as_f64().unwrap_or(-1.0), a[1].as_f64().unwrap_or(-1.0));
                let label = a[2].as_str().unwrap_or("").to_string();
                let (ox, oy) = (at[0].as_f64().unwrap_or(0.0), at[1].as_f64().unwrap_or(0.0));
                return Ok(Some(if x >= 0.0 { (x + ox, y + oy, label) } else { (-1.0, -1.0, label) }));
            }
        }
        Ok(None)
    }

    /// Reads the page for about `secs`: wheel bursts down, glances back up, idle mouse moves.
    fn read_page(&mut self, secs: u32) -> Result<(), String> {
        let end = Instant::now() + Duration::from_millis(dwell_ms(secs));
        let mut first = true;
        loop {
            let (w, h, bottom) = self.viewport();
            match fastrand::u8(0..100) {
                _ if first => self.wheel(fastrand::i32(1..=4))?,
                0..=54 if !bottom => self.wheel(fastrand::i32(1..=5))?,
                55..=64 => self.wheel(-fastrand::i32(1..=2))?,
                _ => self.move_to(w * (0.1 + fastrand::f64() * 0.8), h * (0.15 + fastrand::f64() * 0.75))?,
            }
            first = false;
            let left = end.saturating_duration_since(Instant::now()).as_millis() as u64;
            if left == 0 {
                return Ok(());
            }
            self.pause(rand_ms(700, 3200).min(left))?;
        }
    }

    /// Clicks a same-site link, scrolling to find one; false when the page has none.
    fn click_link(&mut self) -> Result<bool, String> {
        for _ in 0..4 {
            if let Value::Array(a) = self.eval(LINK_JS) {
                if let (Some(x), Some(y)) = (a[0].as_f64(), a[1].as_f64()) {
                    self.click_at(x, y)?;
                    return Ok(true);
                }
            }
            self.wheel(fastrand::i32(2..=4))?;
            self.pause(rand_ms(500, 1300))?;
        }
        Ok(false)
    }

    /// Types `query` on the engine's start page and clicks a result (the one on `want`'s domain when given).
    fn search(&mut self, home: &str, query: &str, want: &str, accept: bool, progress: &dyn Fn(String)) -> Result<Searched, String> {
        self.navigate(home, "")?;
        self.wait_load()?;
        if accept {
            self.accept_cookies(progress)?;
        }
        if self.captcha() {
            return Ok(Searched::Blocked);
        }
        let Value::Array(a) = self.eval(SEARCH_INPUT_JS) else { return Ok(Searched::NoResult) };
        let (Some(x), Some(y)) = (a[0].as_f64(), a[1].as_f64()) else { return Ok(Searched::NoResult) };
        self.click_at(x, y)?;
        self.pause(rand_ms(300, 900))?;
        self.type_text(query, (70, 240))?;
        self.pause(rand_ms(400, 1400))?;
        self.url = self.eval("location.href").as_str().unwrap_or("").to_string();
        self.key("Enter", "Enter", 13, "\r", false)?;
        self.wait_load()?;
        if self.captcha() {
            return Ok(Searched::Blocked);
        }
        self.pause(rand_ms(1200, 3500))?; // looking over the results
        let js = result_js(want);
        for _ in 0..4 {
            match self.eval(&js) {
                Value::Array(a) => {
                    if let (Some(x), Some(y)) = (a[0].as_f64(), a[1].as_f64()) {
                        self.click_at(x, y)?;
                        return Ok(Searched::Clicked);
                    }
                    return Ok(Searched::NoResult);
                }
                Value::Number(_) => {
                    self.wheel(fastrand::i32(2..=4))?;
                    self.pause(rand_ms(600, 1500))?;
                }
                _ => return Ok(Searched::NoResult),
            }
        }
        Ok(Searched::NoResult)
    }

    fn visit(&mut self, e: &Entry, w: &Warmup, r: &mut Report, search_ok: &mut bool, progress: &dyn Fn(String)) -> Result<(), String> {
        let home = engine_home(&w.referrer);
        let (query, want, direct) = match e {
            Entry::Site(u) => {
                let host = host_of(u);
                let want = host.strip_prefix("www.").unwrap_or(&host).to_string();
                (site_query(&host, fastrand::u8(0..100)), want, Some(u.clone()))
            }
            Entry::Query(q) => (q.clone(), String::new(), None),
        };
        let mut arrived = false;
        if (w.real_search || direct.is_none()) && *search_ok {
            progress(format!("Searching \"{query}\""));
            match self.search(home, &query, &want, w.accept_cookies, progress)? {
                Searched::Clicked => {
                    r.searches += 1;
                    arrived = true;
                }
                Searched::NoResult => r.searches += 1,
                Searched::Blocked => {
                    r.captchas += 1;
                    *search_ok = false; // more searches would only meet more captchas
                    progress("The search engine shows a captcha: the rest of the sites are opened directly".into());
                }
            }
        }
        if !arrived {
            let Some(u) = direct else { return Ok(()) }; // a query whose search failed: nothing to open
            // After a search without a matching result, the visit still looks like it came from the engine.
            let referrer = if w.real_search { home } else { w.referrer.as_str() };
            self.navigate(&u, referrer)?;
        }
        let depth = w.view_depth.clamp(VIEW_DEPTH.0, VIEW_DEPTH.1);
        for d in 0..depth {
            self.wait_load()?;
            if self.captcha() {
                r.captchas += 1;
                progress("Captcha page: skipping this site".into());
                break;
            }
            r.pages += 1;
            if w.accept_cookies {
                self.accept_cookies(progress)?;
            }
            self.read_page(w.time_per_url.clamp(TIME_PER_URL.0, TIME_PER_URL.1))?;
            if d + 1 < depth && !self.click_link()? {
                break;
            }
        }
        Ok(())
    }
}

/// US-keyboard `code`, virtual-key code and Shift for a character; other characters (ą, é, emoji) get only their text,
/// like keys from a layout the page cannot map.
fn key_meta(c: char) -> (String, u32, bool) {
    const SHIFTED: &str = "~!@#$%^&*()_+{}|:\"<>?";
    const PLAIN: &str = "`1234567890-=[]\\;',./";
    const CODES: [(&str, u32); 21] = [("Backquote", 192), ("Digit1", 49), ("Digit2", 50), ("Digit3", 51), ("Digit4", 52), ("Digit5", 53), ("Digit6", 54), ("Digit7", 55), ("Digit8", 56), ("Digit9", 57), ("Digit0", 48), ("Minus", 189), ("Equal", 187), ("BracketLeft", 219), ("BracketRight", 221), ("Backslash", 220), ("Semicolon", 186), ("Quote", 222), ("Comma", 188), ("Period", 190), ("Slash", 191)];
    if c.is_ascii_alphabetic() {
        let u = c.to_ascii_uppercase();
        return (format!("Key{u}"), u as u32, c.is_ascii_uppercase());
    }
    if c == ' ' {
        return ("Space".into(), 32, false);
    }
    if let Some(i) = PLAIN.find(c) {
        return (CODES[i].0.into(), CODES[i].1, false);
    }
    if let Some(i) = SHIFTED.find(c) {
        return (CODES[i].0.into(), CODES[i].1, true);
    }
    (String::new(), 0, false)
}

/// Longest text one `type_like_human` call takes, the same cap as "Paste like a human" (0019).
pub const MAX_TYPE_CHARS: usize = 20_000;

/// `[x, y]` inside the element, `[notches]` to wheel when it is outside the viewport, or null when there is none.
const FIND_JS: &str = r#"(sel=>{const e=document.querySelector(sel);if(!e)return null;const r=e.getBoundingClientRect();if(!r.width&&!r.height)return null;
if(r.top<0||r.bottom>innerHeight)return [Math.round((r.top+r.height/2-innerHeight/2)/100)];
return [r.left+Math.min(r.width,300)*(0.2+Math.random()*0.6),r.top+r.height*(0.3+Math.random()*0.4)]})"#;

/// Types `text` key by key into the front tab of the session's running browser: into the focused element, or into
/// `selector` after moving the mouse to it and clicking. Each character sends keydown, keypress/input and keyup
/// (unlike "Paste like a human", which only inserts text). `Stop` on the session ends it early.
pub fn type_like_human(conn: &Conn, session: &str, selector: &str, text: &str, delay: (u64, u64)) -> Result<usize, String> {
    let targets = cdp::call(conn, None, "Target.getTargets", json!({}))?;
    let page = targets["targetInfos"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["type"] == "page" && !t["url"].as_str().unwrap_or("").starts_with("devtools://")))
        .and_then(|t| t["targetId"].as_str())
        .ok_or("The browser has no open tab")?
        .to_string();
    let attached = cdp::call(conn, None, "Target.attachToTarget", json!({ "targetId": page, "flatten": true }))?;
    let sid = attached["sessionId"].as_str().ok_or("Could not attach to the tab")?.to_string();
    clear_cancel(session);
    let mut t = Tab { conn, sid: sid.clone(), session, x: 0.0, y: 0.0, url: String::new() };
    let res = (|| {
        let (w, h, _) = t.viewport();
        (t.x, t.y) = (w * (0.3 + fastrand::f64() * 0.4), h * (0.3 + fastrand::f64() * 0.4));
        if !selector.is_empty() {
            let js = format!("{FIND_JS}({})", Value::String(selector.to_string()));
            let mut found = false;
            for _ in 0..6 {
                match t.eval(&js) {
                    Value::Array(a) if a.len() == 2 => {
                        t.click_at(a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0))?;
                        t.pause(rand_ms(250, 700))?;
                        found = true;
                        break;
                    }
                    Value::Array(a) => {
                        t.wheel(a[0].as_i64().unwrap_or(0).clamp(-30, 30) as i32)?;
                        t.pause(rand_ms(400, 900))?;
                    }
                    _ => return Err(format!("No visible element matches {selector}")),
                }
            }
            if !found {
                return Err(format!("Could not scroll to {selector}"));
            }
        }
        t.type_text(text, delay)
    })();
    let _ = cdp::call(conn, None, "Target.detachFromTarget", json!({ "sessionId": sid }));
    clear_cancel(session);
    res.map_err(|e| if e == STOPPED { "Typing stopped".into() } else { e })
}

/// Visits the sites in one tab of the running browser. A `cancel` ends it early with the counts so far.
pub fn run_warmup(conn: &Conn, w: &Warmup, progress: &dyn Fn(String)) -> Result<Report, String> {
    let entries = pick_entries(w);
    if entries.is_empty() {
        return Err("The warm-up list has no addresses".into());
    }
    clear_cancel(&w.session);
    let before = export_cookies(conn).map(|c| c.len()).unwrap_or(0);
    // One tab for the whole run, so history and back/forward look like one person browsing.
    let tab = cdp::call(conn, None, "Target.createTarget", json!({ "url": "about:blank" }))?;
    let tid = tab["targetId"].as_str().unwrap_or("").to_string();
    let attached = cdp::call(conn, None, "Target.attachToTarget", json!({ "targetId": tid, "flatten": true }))?;
    let sid = attached["sessionId"].as_str().ok_or("Could not attach to the warm-up tab")?.to_string();
    let mut t = Tab { conn, sid, session: &w.session, x: 200.0 + fastrand::f64() * 400.0, y: 150.0 + fastrand::f64() * 300.0, url: String::new() };
    let mut r = Report::default();
    let mut search_ok = true;
    let per_site = w.view_depth.clamp(VIEW_DEPTH.0, VIEW_DEPTH.1) as u64 * (2 + w.time_per_url as u64) + if w.real_search { 12 } else { 0 };
    let mut res = Ok(());
    for (i, e) in entries.iter().enumerate() {
        let left = (entries.len() - i) as u64 * per_site;
        let name = match e {
            Entry::Site(u) => u.clone(),
            Entry::Query(q) => format!("\"{q}\""),
        };
        progress(format!("Warm-up {}/{}: {name} · about {}:{:02} left", i + 1, entries.len(), left / 60, left % 60));
        res = t.visit(e, w, &mut r, &mut search_ok, progress);
        if res.is_err() {
            break;
        }
        r.sites += 1;
        if t.pause(rand_ms(800, 2500)).is_err() {
            res = Err(STOPPED.into());
            break;
        }
    }
    let _ = cdp::call(conn, None, "Target.closeTarget", json!({ "targetId": tid }));
    clear_cancel(&w.session);
    let after = export_cookies(conn).map(|c| c.len()).unwrap_or(before);
    r.cookies = after.saturating_sub(before);
    match res {
        Ok(()) => Ok(r),
        Err(e) if e == STOPPED => {
            r.stopped = true;
            Ok(r)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(urls: Vec<String>, count: u32) -> Warmup {
        Warmup { session: "t".into(), urls, url_count: count, time_per_url: 0, view_depth: 1, accept_cookies: true, referrer: String::new(), real_search: false }
    }

    #[test]
    fn respects_limits() {
        assert_eq!(pick_entries(&job((0..80).map(|i| format!("site{i}.com")).collect(), 500)).len(), URL_COUNT.1 as usize);
        assert_eq!(pick_entries(&job(vec!["a.com".into(), "".into(), "# note".into()], 0)).len(), 1);
    }

    #[test]
    fn entries_and_queries() {
        assert_eq!(parse_entry("allegro.pl"), Some(Entry::Site("https://allegro.pl".into())));
        assert_eq!(parse_entry(" https://x.com/a "), Some(Entry::Site("https://x.com/a".into())));
        assert_eq!(parse_entry("best running shoes"), Some(Entry::Query("best running shoes".into())));
        assert_eq!(parse_entry("pogoda"), Some(Entry::Query("pogoda".into())));
        assert_eq!(parse_entry("?node.js tutorial"), Some(Entry::Query("node.js tutorial".into())));
        assert_eq!(parse_entry("?  "), None);
    }

    #[test]
    fn search_words() {
        assert_eq!(host_of("https://www.Allegro.pl:443/x?y"), "www.allegro.pl");
        assert_eq!(site_query("www.allegro.pl", 10), "allegro");
        assert_eq!(site_query("www.allegro.pl", 90), "allegro.pl");
        assert_eq!(engine_home(""), "https://www.google.com/");
        assert_eq!(engine_home("https://www.bing.com/"), "https://www.bing.com/");
    }

    #[test]
    fn mouse_path_ends_on_target() {
        let p = [(0.0, 0.0), (10.0, 50.0), (80.0, -20.0), (100.0, 40.0)];
        assert_eq!(bezier(p, 0.0), (0.0, 0.0));
        assert_eq!(bezier(p, 1.0), (100.0, 40.0));
        let d = dwell_ms(10);
        assert!((6000..=15000).contains(&d));
    }

    #[test]
    fn keys_of_characters() {
        assert_eq!(key_meta('a'), ("KeyA".into(), 65, false));
        assert_eq!(key_meta('Z'), ("KeyZ".into(), 90, true));
        assert_eq!(key_meta('1'), ("Digit1".into(), 49, false));
        assert_eq!(key_meta('!'), ("Digit1".into(), 49, true));
        assert_eq!(key_meta('?'), ("Slash".into(), 191, true));
        assert_eq!(key_meta('\\'), ("Backslash".into(), 220, false));
        assert_eq!(key_meta('"'), ("Quote".into(), 222, true));
        assert_eq!(key_meta('ą'), (String::new(), 0, false));
    }

    #[test]
    fn cancel_is_per_session() {
        cancel("a");
        assert!(cancelled("a") && !cancelled("b"));
        clear_cancel("a");
        assert!(!cancelled("a"));
    }

    #[test]
    fn report_text() {
        let r = Report { sites: 2, pages: 5, searches: 1, captchas: 1, cookies: 40, stopped: true };
        assert_eq!(r.summary(), "Warm-up stopped: 2 sites, 5 pages, 1 searches, 40 new cookies, 1 captcha pages skipped");
    }
}
