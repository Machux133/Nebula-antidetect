//! Dev tool: serves tests/probe, opens probe.html in a browser and prints the checks.
//! Usage: cargo run --release --bin probe -- [browser.exe] [extra browser args...]
//! Add `--headless=new` etc. as extra args. Does not use Runtime.enable (the probe detects it).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The Nebula Chromium packaged by build.ps1, used when no executable is given.
fn candidates() -> Vec<String> {
    vec![format!("{}\\dist\\Nebula\\chromium\\chrome.exe", env!("CARGO_MANIFEST_DIR"))]
}

fn serve(mut s: TcpStream, root: PathBuf) {
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").split('?').next().unwrap_or("/").to_string();
    let mut hdrs = serde_json::Map::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) <= 2 {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            hdrs.insert(k.trim().to_lowercase(), json!(v.trim()));
        }
    }
    let (code, ctype, body): (u16, &str, Vec<u8>) = if path == "/__headers" {
        (200, "application/json", Value::Object(hdrs).to_string().into_bytes())
    } else {
        let name = path.trim_start_matches('/');
        let f = root.join(if name.is_empty() { "probe.html" } else { name });
        match std::fs::read(&f) {
            Ok(b) if !name.contains("..") => {
                let ct = match f.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("js") => "text/javascript",
                    _ => "application/octet-stream",
                };
                (200, ct, b)
            }
            _ => (404, "text/plain", b"not found".to_vec()),
        }
    };
    let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: {ctype}; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    let _ = s.write_all(&body);
}

fn main() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let exe = if args.first().is_some_and(|a| Path::new(a).is_file()) {
        args.remove(0)
    } else {
        candidates().into_iter().find(|c| Path::new(c).is_file()).unwrap_or_else(|| "chrome".into())
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/probe");
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let web = l.local_addr().unwrap().port();
    let r2 = root.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let r = r2.clone();
            std::thread::spawn(move || serve(c, r));
        }
    });
    let dbg = {
        let t = TcpListener::bind("127.0.0.1:0").unwrap();
        t.local_addr().unwrap().port()
    };
    let prof = std::env::temp_dir().join(format!("nebula-probe-{}", std::process::id()));
    let mut child = Command::new(&exe)
        .args([format!("--remote-debugging-port={dbg}"), format!("--user-data-dir={}", prof.display()), "--no-first-run".into(), "--no-default-browser-check".into()])
        .args(&args)
        .arg(format!("http://127.0.0.1:{web}/probe.html?skip={}", std::env::var("PROBE_SKIP").unwrap_or_default()))
        .spawn()
        .map_err(|e| format!("cannot start {exe}: {e}"))?;
    let res = collect(dbg);
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(500));
    let _ = std::fs::remove_dir_all(&prof);
    let probe = res?;
    let mut counts = [0; 3];
    println!("browser: {exe}\nUA: {}\n", probe["ua"].as_str().unwrap_or("?"));
    for c in probe["checks"].as_array().unwrap_or(&vec![]) {
        let st = c["status"].as_str().unwrap_or("?");
        counts[match st {
            "pass" => 0,
            "warn" => 1,
            _ => 2,
        }] += 1;
        if st != "pass" || std::env::var("PROBE_ALL").is_ok() {
            println!("{:<5} {:<28} {}", st.to_uppercase(), c["id"].as_str().unwrap_or(""), c["detail"].as_str().unwrap_or(""));
        }
    }
    println!("\npass {}  warn {}  fail {}  (set PROBE_ALL=1 to list passes)", counts[0], counts[1], counts[2]);
    let out = root.join("last.json");
    std::fs::write(&out, serde_json::to_string_pretty(&probe).unwrap()).map_err(|e| e.to_string())?;
    println!("wrote {}", out.display());
    if counts[2] > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn collect(dbg: u16) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
    let t = Instant::now();
    let ws = loop {
        if t.elapsed() > Duration::from_secs(40) {
            return Err("browser did not expose a probe page".into());
        }
        if let Ok(r) = agent.get(&format!("http://127.0.0.1:{dbg}/json/list")).call() {
            let v: Value = r.into_json().unwrap_or(Value::Null);
            let page = v.as_array().and_then(|a| a.iter().find(|p| p["type"] == "page" && p["url"].as_str().is_some_and(|u| u.contains("probe.html"))));
            if let Some(u) = page.and_then(|p| p["webSocketDebuggerUrl"].as_str()) {
                break u.to_string();
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    };
    let (mut sock, _) = tungstenite::connect(ws).map_err(|e| e.to_string())?;
    let mut id = 0;
    while t.elapsed() < Duration::from_secs(90) {
        id += 1;
        let req = json!({"id": id, "method": "Runtime.evaluate", "params": {"expression": "window.__probe && window.__probe.done ? JSON.stringify(window.__probe) : ''", "returnByValue": true}});
        sock.send(tungstenite::Message::text(req.to_string())).map_err(|e| e.to_string())?;
        loop {
            if let tungstenite::Message::Text(m) = sock.read().map_err(|e| e.to_string())? {
                let v: Value = serde_json::from_str(m.as_str()).unwrap_or(Value::Null);
                if v["id"] == id {
                    if let Some(s) = v["result"]["result"]["value"].as_str().filter(|s| !s.is_empty()) {
                        return serde_json::from_str(s).map_err(|e| e.to_string());
                    }
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // Timed out: show what the page managed to do, which is usually the answer.
    let dump = json!({"id": 9999, "method": "Runtime.evaluate", "params": {"expression": "document.readyState + ' | ' + (document.body ? document.body.innerText.slice(0, 600) : 'no body') + ' | errors: ' + (window.__errs || []).join(' ; ')", "returnByValue": true}});
    if sock.send(tungstenite::Message::text(dump.to_string())).is_ok() {
        for _ in 0..20 {
            if let Ok(tungstenite::Message::Text(m)) = sock.read() {
                let v: Value = serde_json::from_str(m.as_str()).unwrap_or(Value::Null);
                if v["id"] == 9999 {
                    eprintln!("page state: {}", v["result"]["result"]["value"].as_str().unwrap_or("?"));
                    break;
                }
            }
        }
    }
    Err("probe timed out".into())
}
