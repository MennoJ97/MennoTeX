//! Reading `tlpkg/mtx/mtx.log` back: `mtx log` and `mtx doctor`. TeX's own
//! log never sees mtx's stderr, so this is where editor users find out why
//! a file was not installed.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::root::Root;

/// How much of the end of the log is read.
const TAIL_BYTES: u64 = 512 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub at: u64,
    pub pid: u32,
    pub msg: String,
}

impl Entry {
    /// A failed (`error:`) or refused (`declined:`) install.
    pub fn is_problem(&self) -> bool {
        self.msg.starts_with("error:") || self.msg.starts_with("declined:")
    }
}

fn parse(line: &str) -> Option<Entry> {
    let (at, rest) = line.split_once(' ')?;
    let (pid, msg) = rest.strip_prefix('[')?.split_once("] ")?;
    Some(Entry { at: at.parse().ok()?, pid: pid.parse().ok()?, msg: msg.to_string() })
}

/// Entries from the last part of the log, oldest first.
pub fn tail(root: &Root) -> Vec<Entry> {
    let Ok(mut f) = File::open(root.log_path()) else { return Vec::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    if start > 0 {
        lines.next(); // probably cut in the middle
    }
    lines.filter_map(parse).collect()
}

/// "3 min ago" style age.
pub fn age(now: u64, at: u64) -> String {
    let d = now.saturating_sub(at);
    match d {
        0..60 => format!("{d} s ago"),
        60..7200 => format!("{} min ago", d / 60),
        7200..172800 => format!("{} h ago", d / 3600),
        _ => format!("{} days ago", d / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_entries_and_problems() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path());
        std::fs::create_dir_all(root.mtx_dir()).unwrap();
        std::fs::write(
            root.log_path(),
            "100 [7] foo.sty → installing package foo\n101 [7] error: cannot install foo: offline\nnot a log line\n\
             102 [8] declined: x.sty needs package x (0.1 MiB) (declined)\n",
        )
        .unwrap();
        let e = tail(&root);
        assert_eq!(e.len(), 3);
        assert_eq!(e[1], Entry { at: 101, pid: 7, msg: "error: cannot install foo: offline".into() });
        assert_eq!(e.iter().filter(|e| e.is_problem()).count(), 2);
        assert_eq!(age(1000, 940), "1 min ago");
    }

    /// Lines from concurrent writers stay whole (each a single `write`).
    #[test]
    fn concurrent_writers_keep_lines_whole() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path());
        std::fs::create_dir_all(root.mtx_dir()).unwrap();
        std::thread::scope(|s| {
            for t in 0..8 {
                let root = &root;
                s.spawn(move || (0..200).for_each(|i| crate::ctx::append_log(root, &format!("writer {t} line {i}"))));
            }
        });
        let log = std::fs::read_to_string(root.log_path()).unwrap();
        assert_eq!(log.lines().count(), 1600);
        for line in log.lines() {
            let e = parse(line).unwrap_or_else(|| panic!("broken line: {line:?}"));
            assert!(e.msg.starts_with("writer ") && e.msg.split(' ').count() == 4, "{line:?}");
        }
    }
}
