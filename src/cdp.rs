//! DevTools protocol connection to a session's browser.
//!
//! By default it is a private pipe: two anonymous pipes whose child ends the browser inherits
//! (`--remote-debugging-pipe`), so nothing listens on the network and no other process can attach. Only when the
//! local API asks for a `debug_port` (outside automation such as Selenium or Playwright) does the browser open
//! that port, and this connection is a WebSocket on it.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// One connection per browser, shared by the threads that drive it (a call holds the lock only while it runs).
pub type Conn = Arc<Mutex<Cdp>>;

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Cdp {
    id: u64,
    t: Transport,
}

enum Transport {
    Pipe { write: std::fs::File, rx: mpsc::Receiver<Value> },
    Port { port: u16, ws: Option<WebSocket<MaybeTlsStream<TcpStream>>> },
}

impl Cdp {
    /// Over the pipe pair made for the browser: `write` goes to its stdin side, `read` comes from it.
    pub fn pipe(write: std::fs::File, mut read: std::fs::File) -> Conn {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Messages are JSON texts each followed by a NUL byte.
            let (mut buf, mut chunk) = (Vec::<u8>::new(), [0u8; 65536]);
            loop {
                match read.read(&mut chunk) {
                    Ok(0) | Err(_) => break, // the browser closed its end: it is gone
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
                while let Some(end) = buf.iter().position(|b| *b == 0) {
                    let msg: Vec<u8> = buf.drain(..=end).collect();
                    if let Ok(v) = serde_json::from_slice::<Value>(&msg[..msg.len() - 1]) {
                        if tx.send(v).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Arc::new(Mutex::new(Cdp { id: 0, t: Transport::Pipe { write, rx } }))
    }

    /// Over the debug port the browser was told to open; connects on first use (it may still be starting).
    pub fn port(port: u16) -> Conn {
        Arc::new(Mutex::new(Cdp { id: 0, t: Transport::Port { port, ws: None } }))
    }

    fn connect_port(port: u16, secs: u64) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
        let start = Instant::now();
        loop {
            if let Ok(r) = agent.get(&format!("http://127.0.0.1:{port}/json/version")).call() {
                let v: Value = r.into_json().map_err(|e| e.to_string())?;
                let url = v["webSocketDebuggerUrl"].as_str().ok_or("no browser socket")?.to_string();
                let (ws, _) = tungstenite::connect(url.as_str()).map_err(|e| e.to_string())?;
                if let MaybeTlsStream::Plain(s) = ws.get_ref() {
                    let _ = s.set_read_timeout(Some(CALL_TIMEOUT));
                }
                return Ok(ws);
            }
            if start.elapsed() > Duration::from_secs(secs) {
                return Err("The browser did not open its control port in time".into());
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    /// Waits until the browser answers.
    pub fn ready(&mut self, secs: u64) -> Result<(), String> {
        if let Transport::Port { port, ws } = &mut self.t {
            if ws.is_none() {
                *ws = Some(Self::connect_port(*port, secs)?);
            }
        }
        let start = Instant::now();
        loop {
            match self.call_in(None, "Browser.getVersion", json!({})) {
                Ok(_) => return Ok(()),
                Err(e) if start.elapsed() > Duration::from_secs(secs) => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(300)),
            }
        }
    }

    /// `session`: a flattened target session id (from `Target.attachToTarget`), or None for the browser itself.
    pub fn call_in(&mut self, session: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let reply = |v: &Value| -> Option<Result<Value, String>> {
            if v["id"].as_u64() != Some(id) {
                return None;
            }
            Some(match v.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("error").to_string()),
                None => Ok(v["result"].clone()),
            })
        };
        match &mut self.t {
            Transport::Pipe { write, rx } => {
                let mut bytes = msg.to_string().into_bytes();
                bytes.push(0);
                write.write_all(&bytes).and_then(|_| write.flush()).map_err(|_| "The browser is closed".to_string())?;
                let deadline = Instant::now() + CALL_TIMEOUT;
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    match rx.recv_timeout(left) {
                        Ok(v) => {
                            if let Some(r) = reply(&v) {
                                return r;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => return Err("The browser did not answer in time".into()),
                        Err(mpsc::RecvTimeoutError::Disconnected) => return Err("The browser is closed".into()),
                    }
                }
            }
            Transport::Port { port, ws } => {
                if ws.is_none() {
                    *ws = Some(Self::connect_port(*port, 30)?);
                }
                let ws = ws.as_mut().expect("just connected");
                ws.send(Message::text(msg.to_string())).map_err(|e| e.to_string())?;
                loop {
                    if let Message::Text(t) = ws.read().map_err(|e| e.to_string())? {
                        let v: Value = serde_json::from_str(t.as_str()).unwrap_or(Value::Null);
                        if let Some(r) = reply(&v) {
                            return r;
                        }
                    }
                }
            }
        }
    }
}

/// `Runtime.evaluate` of `js` in `session`, returning the value.
pub fn eval(conn: &Conn, session: Option<&str>, js: &str) -> Result<Value, String> {
    let r = call(conn, session, "Runtime.evaluate", json!({"expression": js, "returnByValue": true}))?;
    Ok(r["result"]["value"].clone())
}

pub fn call(conn: &Conn, session: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
    conn.lock().map_err(|_| "The browser connection failed".to_string())?.call_in(session, method, params)
}

pub fn wait_ready(conn: &Conn, secs: u64) -> Result<(), String> {
    conn.lock().map_err(|_| "The browser connection failed".to_string())?.ready(secs)
}

// ---------------------------------------------------------------- the pipe pair (Windows)

/// The two pipes handed to the browser: it reads from one and writes to the other. Only the child's ends are
/// inheritable, and they are closed here once the browser has started (so the browser exiting ends our reads).
#[cfg(windows)]
pub mod winpipe {
    use std::ffi::c_void;
    use std::fs::File;
    use std::os::windows::io::FromRawHandle;

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreatePipe(read: *mut Handle, write: *mut Handle, attributes: *const c_void, size: u32) -> i32;
        fn SetHandleInformation(h: Handle, mask: u32, flags: u32) -> i32;
        fn CloseHandle(h: Handle) -> i32;
    }

    const HANDLE_FLAG_INHERIT: u32 = 1;

    /// The browser's ends; closed on drop.
    pub struct ChildEnds {
        read: Handle,
        write: Handle,
    }

    impl ChildEnds {
        /// `--remote-debugging-io-pipes=<read>,<write>` for the browser.
        pub fn switch(&self) -> String {
            format!("--remote-debugging-io-pipes={},{}", self.read as usize, self.write as usize)
        }
    }

    impl Drop for ChildEnds {
        fn drop(&mut self) {
            // SAFETY: both handles were created by CreatePipe in `create` and are closed only here.
            unsafe {
                CloseHandle(self.read);
                CloseHandle(self.write);
            }
        }
    }

    /// (our write end, our read end, the browser's ends)
    pub fn create() -> std::io::Result<(File, File, ChildEnds)> {
        let none = std::ptr::null_mut::<c_void>();
        let (mut browser_reads, mut we_write, mut we_read, mut browser_writes) = (none, none, none, none);
        // SAFETY: plain Win32 calls with valid out-pointers; the handles are wrapped or closed below.
        unsafe {
            if CreatePipe(&mut browser_reads, &mut we_write, std::ptr::null(), 0) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            if CreatePipe(&mut we_read, &mut browser_writes, std::ptr::null(), 0) == 0 {
                let e = std::io::Error::last_os_error();
                CloseHandle(browser_reads);
                CloseHandle(we_write);
                return Err(e);
            }
            let inheritable = SetHandleInformation(browser_reads, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) != 0
                && SetHandleInformation(browser_writes, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) != 0;
            if !inheritable {
                let e = std::io::Error::last_os_error();
                for h in [browser_reads, we_write, we_read, browser_writes] {
                    CloseHandle(h);
                }
                return Err(e);
            }
            Ok((
                File::from_raw_handle(we_write as _),
                File::from_raw_handle(we_read as _),
                ChildEnds { read: browser_reads, write: browser_writes },
            ))
        }
    }
}

#[cfg(not(windows))]
pub mod winpipe {
    /// No pipe transport off Windows: the caller falls back to a debug port.
    pub struct ChildEnds;
    impl ChildEnds {
        pub fn switch(&self) -> String {
            String::new()
        }
    }
    pub fn create() -> std::io::Result<(std::fs::File, std::fs::File, ChildEnds)> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "pipes are Windows-only here"))
    }
}
