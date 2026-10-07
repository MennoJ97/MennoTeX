//! kpathsea's `ls-R` file-name databases.
//!
//! Trees marked `!!` in `TEXMF` are searched only through their `ls-R`, so
//! every installed file must be listed. kpathsea accepts the same directory
//! appearing in several blocks (that is how `mktexupd` appends), so new
//! files are appended cheaply and the file is rebuilt only occasionally.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

pub const HEADER: &str = "% ls-R -- filename database for kpathsea; do not change this line.\n";

/// Append `rel_paths` (relative to `tree`) to `tree/ls-R`, creating it by a
/// full scan if it does not exist yet.
pub fn append<'a>(tree: &Path, rel_paths: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let lsr = tree.join("ls-R");
    if !lsr.exists() {
        return rebuild(tree);
    }
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for p in rel_paths {
        let (dir, base) = p.rsplit_once('/').unwrap_or(("", p));
        by_dir.entry(dir).or_default().push(base);
    }
    if by_dir.is_empty() {
        return Ok(());
    }
    let mut out = String::new();
    for (dir, names) in by_dir {
        out.push_str(&format!("\n./{dir}:\n"));
        for n in names {
            out.push_str(n);
            out.push('\n');
        }
    }
    let mut f = OpenOptions::new().append(true).open(&lsr).with_context(|| format!("opening {}", lsr.display()))?;
    f.write_all(out.as_bytes())?;
    f.sync_data()?;
    Ok(())
}

/// Rewrite `tree/ls-R` from a full directory scan (like `mktexlsr`).
pub fn rebuild(tree: &Path) -> Result<()> {
    fs::create_dir_all(tree)?;
    let mut out = String::from(HEADER);
    let mut stack = vec![String::new()];
    while let Some(rel) = stack.pop() {
        let dir = if rel.is_empty() { tree.to_path_buf() } else { tree.join(&rel) };
        let mut names: Vec<(String, bool)> = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || (rel.is_empty() && name.starts_with("ls-")) {
                continue;
            }
            // Follow symlinks like `ls -L`.
            let is_dir = fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false);
            names.push((name, is_dir));
        }
        names.sort();
        out.push_str(&format!("\n./{rel}:\n"));
        for (name, _) in &names {
            out.push_str(name);
            out.push('\n');
        }
        // Push in reverse so directories come out in sorted order.
        for (name, _) in names.iter().filter(|(_, d)| *d).rev() {
            stack.push(if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") });
        }
    }
    let lsr = tree.join("ls-R");
    let tmp = tree.join(".ls-R.tmp");
    fs::write(&tmp, out)?;
    fs::rename(&tmp, &lsr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_then_append() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path();
        fs::create_dir_all(t.join("tex/latex/a")).unwrap();
        fs::write(t.join("tex/latex/a/a.sty"), "").unwrap();
        fs::write(t.join(".hidden"), "").unwrap();
        rebuild(t).unwrap();
        let s = fs::read_to_string(t.join("ls-R")).unwrap();
        assert!(s.starts_with(HEADER));
        assert!(s.contains("\n./tex/latex/a:\na.sty\n"));
        assert!(!s.contains(".hidden"));

        append(t, ["tex/latex/b/b.sty", "tex/latex/b/b.cls", "top.cfg"]).unwrap();
        let s = fs::read_to_string(t.join("ls-R")).unwrap();
        assert!(s.ends_with("\n./:\ntop.cfg\n\n./tex/latex/b:\nb.sty\nb.cls\n"));
    }
}
