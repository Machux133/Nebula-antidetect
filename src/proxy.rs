//! Proxy checker and the local relay that lets Chromium use proxies with a login
//! (Chromium can't send SOCKS5 credentials itself; the original also relays through `socks5://127.0.0.1:<port>`).

use crate::model::{now, GeoInfo, Proxy, ProxyKind};
use base64::Engine;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CHECK_TIMEOUT_SECS: u64 = 15; // estimated: the original's timeout was not found in the code
const GEO_URL: &str = "http://ip-api.com/json/?fields=status,message,country,countryCode,city,zip,timezone,isp,lat,lon,query";

pub fn check(proxy: &Proxy, ipqs_key: &str) -> GeoInfo {
    let mut g = GeoInfo { checked: now(), ..Default::default() };
    let mut b = ureq::AgentBuilder::new().timeout(Duration::from_secs(CHECK_TIMEOUT_SECS));
    let tunnel = if proxy.kind == ProxyKind::Ssh {
        match open_ssh(proxy) {
            Ok(t) => Some(t),
            Err(e) => {
                g.error = short_err(&e);
                return g;
            }
        }
    } else {
        None
    };
    let url = match &tunnel {
        Some(t) => Some(format!("socks5://127.0.0.1:{}", t.port)),
        None => proxy.url(),
    };
    if let Some(url) = url {
        match ureq::Proxy::new(&url) {
            Ok(p) => b = b.proxy(p),
            Err(e) => {
                g.error = format!("Bad proxy: {e}");
                return g;
            }
        }
    }
    let t = Instant::now();
    let resp = b.build().get(GEO_URL).call();
    g.latency_ms = t.elapsed().as_millis() as u64;
    let v: serde_json::Value = match resp.map_err(|e| e.to_string()).and_then(|r| r.into_json().map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => {
            g.error = short_err(&e);
            return g;
        }
    };
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    if s("status") != "success" {
        g.error = format!("Lookup failed: {}", s("message"));
        return g;
    }
    g.ok = true;
    g.ip = s("query");
    g.country_code = s("countryCode");
    g.country = s("country");
    g.city = s("city");
    g.zip = s("zip");
    g.timezone = s("timezone");
    g.isp = s("isp");
    (g.lat, g.lon) = (v["lat"].as_f64(), v["lon"].as_f64());
    if !ipqs_key.trim().is_empty() {
        // Same endpoint shape as the original (string at 0x14277c408): key, then ip.
        let url = format!("https://www.ipqualityscore.com/api/json/ip/{}/{}", ipqs_key.trim(), g.ip);
        if let Ok(r) = ureq::AgentBuilder::new().timeout(Duration::from_secs(CHECK_TIMEOUT_SECS)).build().get(&url).call() {
            if let Ok(j) = r.into_json::<serde_json::Value>() {
                g.fraud_score = j["fraud_score"].as_i64();
            }
        }
    }
    g
}

fn short_err(e: &str) -> String {
    let e = e.replace('\n', " ");
    if e.to_lowercase().contains("timed out") {
        return "Timed out".into();
    }
    e.chars().take(80).collect()
}

/// Calls a provider's "change IP" link (mobile/rotating proxies).
pub fn change_ip(url: &str) -> Result<String, String> {
    let r = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build().get(url).call().map_err(|e| e.to_string())?;
    Ok(r.into_string().unwrap_or_default().chars().take(120).collect())
}

pub struct Relay {
    pub port: u16,
    stop: Arc<AtomicBool>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port)); // wake the accept loop
    }
}

/// A no-login SOCKS5 server on 127.0.0.1 that forwards every connection through `proxy`.
pub fn start_relay(mut proxy: Proxy) -> io::Result<Relay> {
    if proxy.kind == ProxyKind::Localhost {
        proxy.kind = if local_is_socks(proxy.port) { ProxyKind::Socks5 } else { ProxyKind::Http };
        proxy.host = "127.0.0.1".into();
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let proxy = Arc::new(proxy);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(c) = conn {
                let p = proxy.clone();
                std::thread::spawn(move || {
                    let _ = serve(c, &p);
                });
            }
        }
    });
    Ok(Relay { port, stop })
}

fn serve(mut c: TcpStream, proxy: &Proxy) -> io::Result<()> {
    c.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut h = [0u8; 2];
    c.read_exact(&mut h)?;
    if h[0] != 5 {
        return Err(io::Error::other("not socks5"));
    }
    let mut methods = vec![0u8; h[1] as usize];
    c.read_exact(&mut methods)?;
    c.write_all(&[5, 0])?;
    let mut r = [0u8; 4];
    c.read_exact(&mut r)?;
    let host = match r[3] {
        1 => {
            let mut a = [0u8; 4];
            c.read_exact(&mut a)?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        3 => {
            let mut l = [0u8; 1];
            c.read_exact(&mut l)?;
            let mut d = vec![0u8; l[0] as usize];
            c.read_exact(&mut d)?;
            String::from_utf8_lossy(&d).into_owned()
        }
        4 => {
            let mut a = [0u8; 16];
            c.read_exact(&mut a)?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => return Err(io::Error::other("bad address")),
    };
    let mut p = [0u8; 2];
    c.read_exact(&mut p)?;
    let port = u16::from_be_bytes(p);
    let host = if proxy.kind != ProxyKind::Tor && !proxy.dns.trim().is_empty() && host.parse::<IpAddr>().is_err() {
        dns_resolve(&proxy.dns, &host).unwrap_or(host)
    } else {
        host
    };
    if r[1] != 1 {
        c.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0])?;
        return Ok(());
    }
    let up = match upstream(proxy, &host, port) {
        Ok(u) => u,
        Err(e) => {
            c.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0])?;
            return Err(e);
        }
    };
    c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
    c.set_read_timeout(None)?;
    pipe(c, up)
}

fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| io::Error::other("no address"))?;
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(CHECK_TIMEOUT_SECS))?;
    s.set_read_timeout(Some(Duration::from_secs(CHECK_TIMEOUT_SECS)))?;
    Ok(s)
}

fn upstream(proxy: &Proxy, host: &str, port: u16) -> io::Result<TcpStream> {
    let (ph, pp) = proxy.endpoint();
    let s = match proxy.kind {
        ProxyKind::Direct => connect(host, port)?,
        ProxyKind::Http => {
            let mut s = connect(&ph, pp)?;
            let mut req = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n");
            if !proxy.login.is_empty() {
                let b = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", proxy.login, proxy.password));
                req += &format!("Proxy-Authorization: Basic {b}\r\n");
            }
            req += "\r\n";
            s.write_all(req.as_bytes())?;
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if head.len() > 16384 || s.read(&mut b)? == 0 {
                    return Err(io::Error::other("proxy closed"));
                }
                head.push(b[0]);
            }
            let line = String::from_utf8_lossy(&head);
            if !line.split_whitespace().nth(1).is_some_and(|c| c == "200") {
                return Err(io::Error::other("proxy refused"));
            }
            s
        }
        ProxyKind::Socks5 | ProxyKind::Tor | ProxyKind::Localhost | ProxyKind::Ssh => {
            let mut s = connect(&ph, pp)?;
            let auth = !proxy.login.is_empty();
            s.write_all(if auth { &[5, 2, 0, 2] } else { &[5, 1, 0] })?;
            let mut m = [0u8; 2];
            s.read_exact(&mut m)?;
            match m[1] {
                0 => {}
                2 if auth => {
                    let (u, p) = (proxy.login.as_bytes(), proxy.password.as_bytes());
                    let mut a = vec![1, u.len() as u8];
                    a.extend_from_slice(u);
                    a.push(p.len() as u8);
                    a.extend_from_slice(p);
                    s.write_all(&a)?;
                    let mut st = [0u8; 2];
                    s.read_exact(&mut st)?;
                    if st[1] != 0 {
                        return Err(io::Error::other("login refused"));
                    }
                }
                _ => return Err(io::Error::other("no auth method")),
            }
            let mut req = vec![5, 1, 0, 3, host.len() as u8];
            req.extend_from_slice(host.as_bytes());
            req.extend_from_slice(&port.to_be_bytes());
            s.write_all(&req)?;
            let mut rep = [0u8; 4];
            s.read_exact(&mut rep)?;
            if rep[1] != 0 {
                return Err(io::Error::other("connect refused"));
            }
            let skip = match rep[3] {
                1 => 6,
                4 => 18,
                3 => {
                    let mut l = [0u8; 1];
                    s.read_exact(&mut l)?;
                    l[0] as usize + 2
                }
                _ => return Err(io::Error::other("bad reply")),
            };
            let mut rest = vec![0u8; skip];
            s.read_exact(&mut rest)?;
            s
        }
    };
    s.set_read_timeout(None)?;
    let _ = s.set_nodelay(true);
    Ok(s)
}

fn pipe(a: TcpStream, b: TcpStream) -> io::Result<()> {
    let (mut a2, mut b2) = (a.try_clone()?, b.try_clone()?);
    let (mut a, mut b) = (a, b);
    let t = std::thread::spawn(move || {
        let _ = io::copy(&mut a2, &mut b2);
        let _ = b2.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut b, &mut a);
    let _ = a.shutdown(Shutdown::Write);
    let _ = t.join();
    Ok(())
}

/// True when the local proxy on `port` answers a SOCKS5 greeting; otherwise it is treated as HTTP.
pub fn local_is_socks(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(3)) else { return true };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let mut r = [0u8; 2];
    s.write_all(&[5, 1, 0]).is_ok() && s.read_exact(&mut r).is_ok() && r[0] == 5
}

/// A running `ssh -D` tunnel; closed when dropped.
pub struct SshTunnel {
    pub port: u16,
    child: Child,
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Opens a SOCKS5 port on 127.0.0.1 that exits through the SSH server. Uses the system OpenSSH client;
/// a password goes through SSH_ASKPASS (a temporary script, deleted once the tunnel is up or has failed).
pub fn open_ssh(p: &Proxy) -> Result<SshTunnel, String> {
    if p.host.is_empty() || p.login.is_empty() {
        return Err("SSH needs host and login".into());
    }
    let port = TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
    let mut cmd = Command::new("ssh");
    cmd.args(["-N", "-D", &format!("127.0.0.1:{port}"), "-p", &(if p.port == 0 { 22 } else { p.port }).to_string()])
        .args(["-o", "StrictHostKeyChecking=accept-new", "-o", "ExitOnForwardFailure=yes", "-o", "ServerAliveInterval=30"])
        .arg(format!("{}@{}", p.login, p.host))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // no console window
    let script = (!p.password.is_empty()).then(|| {
        let f = std::env::temp_dir().join(format!("nebula-askpass-{}.{}", uuid::Uuid::new_v4(), if cfg!(windows) { "cmd" } else { "sh" }));
        let body = if cfg!(windows) {
            let esc: String = p.password.chars().flat_map(|c| if "^&|<>()".contains(c) { vec!['^', c] } else if c == '%' { vec!['%', '%'] } else { vec![c] }).collect();
            format!("@echo off\r\necho {esc}\r\n")
        } else {
            format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", p.password.replace('\'', "'\\''"))
        };
        let _ = std::fs::write(&f, body);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o700));
        }
        f
    });
    match &script {
        Some(f) => cmd.env("SSH_ASKPASS", f).env("SSH_ASKPASS_REQUIRE", "force").env("DISPLAY", "nebula:0"),
        None => cmd.args(["-o", "BatchMode=yes"]),
    };
    let spawned = cmd.spawn().map_err(|e| format!("Could not run ssh (is OpenSSH installed?): {e}"));
    let out = spawned.and_then(|mut child| {
        let t = Instant::now();
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                return Err(format!("SSH failed: {}", err.lines().last().unwrap_or("connection closed")));
            }
            if TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok() {
                return Ok(SshTunnel { port, child });
            }
            if t.elapsed() > Duration::from_secs(20) {
                let _ = child.kill();
                return Err("SSH timed out".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    if let Some(f) = script {
        let _ = std::fs::remove_file(f);
    }
    out
}

/// The first A record for `name` from the first of `servers` (`;`, `,` or space separated) that answers.
pub fn dns_resolve(servers: &str, name: &str) -> Option<String> {
    let mut q = vec![0x4e, 0x42, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for l in name.trim_end_matches('.').split('.') {
        if l.is_empty() || l.len() > 63 {
            return None;
        }
        q.push(l.len() as u8);
        q.extend_from_slice(l.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    for srv in servers.split([';', ',', ' ']).filter(|s| !s.trim().is_empty()) {
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { continue };
        let _ = sock.set_read_timeout(Some(Duration::from_secs(3)));
        if sock.connect((srv.trim(), 53)).is_err() || sock.send(&q).is_err() {
            continue;
        }
        let mut buf = [0u8; 512];
        if let Ok(n) = sock.recv(&mut buf) {
            if let Some(ip) = parse_a(&buf[..n]) {
                return Some(ip);
            }
        }
    }
    None
}

fn skip_name(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let l = *b.get(i)? as usize;
        if l == 0 {
            return Some(i + 1);
        }
        if l & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        i += 1 + l;
    }
}

fn parse_a(b: &[u8]) -> Option<String> {
    if b.len() < 12 || b[3] & 0x0F != 0 {
        return None;
    }
    let an = u16::from_be_bytes([b[6], b[7]]);
    let mut i = skip_name(b, 12)? + 4;
    for _ in 0..an {
        i = skip_name(b, i)?;
        let h = b.get(i..i + 10)?;
        let rd = u16::from_be_bytes([h[8], h[9]]) as usize;
        i += 10;
        if h[..2] == [0, 1] && rd == 4 {
            let a = b.get(i..i + 4)?;
            return Some(format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]));
        }
        i += rd;
    }
    None
}

#[cfg(test)]
mod dns_tests {
    #[test]
    fn parses_a_record() {
        // header (1 answer), question example.com A IN, answer pointing back at the name with 93.184.216.34
        let mut r = vec![0x4e, 0x42, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0, 7];
        r.extend_from_slice(b"example\x03com\0");
        r.extend_from_slice(&[0, 1, 0, 1, 0xC0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184, 216, 34]);
        assert_eq!(super::parse_a(&r).as_deref(), Some("93.184.216.34"));
    }
}
