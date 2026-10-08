//! A tiny HTTP server for fault-injection tests (PLAN.md §8): serves a
//! repository directory, optionally breaking chosen files, or acts as a
//! redirector that hands out mirrors in turn, like `mirror.ctan.org`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// How a file is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Announce the full length, send this many bytes, close.
    Truncate(usize),
    /// Flip one byte in the middle.
    Corrupt,
    /// Answer with this HTTP status and no file.
    Status(u16),
    /// A captive-portal style HTML page with status 200.
    Html,
    /// Close the connection without answering.
    Reset,
}

#[derive(Default)]
struct State {
    /// The repository served (none for a redirector).
    dir: Option<PathBuf>,
    faults: HashMap<String, Fault>,
    /// Mirror base URLs; non-empty makes this a redirector.
    mirrors: Vec<String>,
    next: usize,
    requests: Vec<String>,
}

pub struct FaultServer {
    pub base: String,
    state: Arc<Mutex<State>>,
}

impl FaultServer {
    /// Serve the files under `dir`.
    pub fn serve(dir: PathBuf) -> FaultServer {
        Self::start(Some(dir), Vec::new())
    }

    /// Redirect every request to the next of `mirrors` (base URLs).
    pub fn redirector(mirrors: Vec<String>) -> FaultServer {
        Self::start(None, mirrors)
    }

    fn start(dir: Option<PathBuf>, mirrors: Vec<String>) -> FaultServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State { dir, mirrors, ..State::default() }));
        let st = state.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let st = st.clone();
                std::thread::spawn(move || {
                    let _ = handle(conn, &st);
                });
            }
        });
        FaultServer { base, state }
    }

    /// Break `path` (relative, e.g. `archive/bar.tar.xz`) from now on.
    pub fn break_file(&self, path: &str, fault: Fault) {
        self.state.lock().unwrap().faults.insert(path.to_string(), fault);
    }

    /// Serve `dir` from now on, as a mirror does after it synced.
    pub fn switch_to(&self, dir: PathBuf) {
        self.state.lock().unwrap().dir = Some(dir);
    }

    pub fn heal(&self, path: &str) {
        self.state.lock().unwrap().faults.remove(path);
    }

    /// Paths requested so far (GET and HEAD).
    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    /// `host:port`, as mtx records bad mirrors.
    pub fn host(&self) -> String {
        self.base.trim_start_matches("http://").trim_end_matches('/').to_string()
    }
}

fn handle(conn: TcpStream, st: &Mutex<State>) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").trim_start_matches('/').to_string();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" {
            break;
        }
    }
    let mut out = conn;
    let (fault, redirect, dir) = {
        let mut s = st.lock().unwrap();
        s.requests.push(path.clone());
        let redirect = (!s.mirrors.is_empty()).then(|| {
            let m = s.mirrors[s.next % s.mirrors.len()].clone();
            s.next += 1;
            m
        });
        (s.faults.get(&path).copied(), redirect, s.dir.clone())
    };
    if let Some(m) = redirect {
        return write!(out, "HTTP/1.1 302 Found\r\nLocation: {m}{path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    let body = dir.and_then(|d| std::fs::read(d.join(&path)).ok());
    let send = |out: &mut TcpStream, status: &str, body: &[u8], len: usize| -> std::io::Result<()> {
        write!(out, "HTTP/1.1 {status}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n")?;
        if method != "HEAD" {
            out.write_all(body)?;
        }
        out.flush()
    };
    match (fault, body) {
        (Some(Fault::Reset), _) => Ok(()),
        (Some(Fault::Status(code)), _) => send(&mut out, &format!("{code} Injected"), b"", 0),
        (Some(Fault::Html), _) => {
            let page = b"<html><body>Please log in to the network.</body></html>";
            send(&mut out, "200 OK", page, page.len())
        }
        (_, None) => send(&mut out, "404 Not Found", b"", 0),
        (Some(Fault::Truncate(n)), Some(b)) => send(&mut out, "200 OK", &b[..n.min(b.len())], b.len()),
        (Some(Fault::Corrupt), Some(mut b)) => {
            let mid = b.len() / 2;
            b[mid] ^= 0x55;
            send(&mut out, "200 OK", &b, b.len())
        }
        (None, Some(b)) => send(&mut out, "200 OK", &b, b.len()),
    }
}
