//! Local HTTP API on 127.0.0.1 (off unless a port is set in Preferences), shaped like Linken Sphere's:
//! `POST /sessions/create_quick`, `POST /sessions/start {uuid, headless, debug_port}`, plus `GET /sessions`
//! and `POST /sessions/stop {uuid}`. Requests are handed to the UI thread, which owns the sessions.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

pub struct Req {
    pub method: String,
    pub path: String,
    pub body: Value,
    pub reply: Sender<(u16, Value)>,
}

/// Starts the listener; `forward` delivers each request to the UI. Fails when the port is taken.
pub fn start(port: u16, forward: impl Fn(Req) + Send + Sync + 'static) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let forward = std::sync::Arc::new(forward);
    std::thread::spawn(move || {
        for c in listener.incoming().flatten() {
            let f = forward.clone();
            std::thread::spawn(move || {
                let _ = handle(c, &*f);
            });
        }
    });
    Ok(())
}

fn respond(c: &mut TcpStream, code: u16, v: &Value) -> std::io::Result<()> {
    let body = v.to_string();
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(c, "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// Splits a raw request into (method, path, headers lowercased, body); None when it is malformed or too large.
fn parse(raw: &[u8]) -> Option<(String, String, Vec<(String, String)>, Vec<u8>)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let (m, p) = (first.next()?.to_string(), first.next()?.to_string());
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    Some((m, p, headers, raw[end + 4..].to_vec()))
}

fn handle(mut c: TcpStream, forward: &dyn Fn(Req)) -> std::io::Result<()> {
    c.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let (method, path, headers, mut body) = loop {
        let n = c.read(&mut buf)?;
        if n == 0 || raw.len() > 1 << 20 {
            return Ok(());
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(p) = parse(&raw) {
            break p;
        }
    };
    let h = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    // A web page must not drive this API: browsers always send Origin on cross-origin requests, and a
    // rebinding attack arrives with a foreign Host.
    let host_ok = h("host").is_some_and(|v| v.starts_with("127.0.0.1") || v.starts_with("localhost"));
    if h("origin").is_some() || !host_ok {
        return respond(&mut c, 403, &json!({"error": "forbidden"}));
    }
    let want: usize = h("content-length").and_then(|v| v.parse().ok()).unwrap_or(0).min(1 << 20);
    while body.len() < want {
        let n = c.read(&mut buf)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    let body = if body.is_empty() { json!({}) } else { serde_json::from_slice(&body).unwrap_or(Value::Null) };
    if body.is_null() {
        return respond(&mut c, 400, &json!({"error": "body must be JSON"}));
    }
    let (tx, rx) = channel();
    forward(Req { method, path: path.split('?').next().unwrap_or("").to_string(), body, reply: tx });
    let (code, v) = rx.recv_timeout(Duration::from_secs(60)).unwrap_or((500, json!({"error": "no answer"})));
    respond(&mut c, code, &v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_request() {
        let (m, p, h, b) = super::parse(b"POST /sessions/start HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        assert_eq!((m.as_str(), p.as_str(), b.as_slice()), ("POST", "/sessions/start", b"{}".as_slice()));
        assert!(h.iter().any(|(k, _)| k == "host"));
        assert!(super::parse(b"POST / HTTP/1.1\r\nHost: x").is_none());
    }
}
