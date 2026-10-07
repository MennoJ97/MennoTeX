//! `mktexfmt`: build a missing format, safely under concurrency.
//!
//! kpathsea runs `mktexfmt NAME.fmt` when a format is missing. TeX Live's
//! own mktexfmt (fmtutil) builds in a temporary directory and then *copies*
//! the result over the destination. When several TeX runs miss the same
//! format at once, each rebuilds and overwrites it while others are already
//! reading it ("Could not undump … pdflatex.fmt", seen in
//! tests/run_concurrent.sh). Here a per-format lock serializes builds, a
//! re-check skips rebuilding what another process just built, and the
//! result is renamed into place atomically.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::lsr;
use crate::root::Root;

/// The format file `name` in any engine directory under `web2c`.
fn find_format(web2c: &Path, file: &str) -> Option<PathBuf> {
    fs::read_dir(web2c)
        .ok()?
        .flatten()
        .map(|e| e.path().join(file))
        .find(|p| p.is_file() && fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
}

/// Find `file` anywhere below `dir`.
fn find_below(dir: &Path, file: &str) -> Option<PathBuf> {
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_below(&p, file) {
                return Some(found);
            }
        } else if p.file_name().is_some_and(|n| n == file) {
            return Some(p);
        }
    }
    None
}

/// Build (or find) the format requested as `request` (`pdflatex.fmt` or
/// `pdflatex`) and return its path.
pub fn mkfmt(root: &Root, request: &str) -> Result<PathBuf> {
    let name = request.strip_suffix(".fmt").unwrap_or(request);
    if name.is_empty() || name.contains(['/', '\0']) || name.starts_with('-') || name.starts_with('.') {
        bail!("invalid format name {request:?}");
    }
    let file = format!("{name}.fmt");
    let web2c = root.texmf_var().join("web2c");
    fs::create_dir_all(&web2c)?;

    let lock = fs::File::create(web2c.join(format!(".mtx-fmt-{name}.lock")))?;
    if lock.try_lock().is_err() {
        eprintln!("mtx: waiting for another process building {file}");
        lock.lock()?;
    }
    if let Some(done) = find_format(&web2c, &file) {
        return Ok(done); // built by another process while we waited
    }

    let staging = web2c.join(format!(".staging-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<PathBuf> {
        // fmtutil's progress output must not reach our stdout: kpathsea
        // reads the format's path from it.
        let status = Command::new(root.bin_dir().join("fmtutil-sys"))
            .args(["--byfmt", name, "--fmtdir"])
            .arg(&staging)
            .args(["--nohash", "--quiet"])
            .env("PATH", root.tool_path())
            .stdout(Stdio::from(std::io::stderr().as_fd().try_clone_to_owned()?))
            .status()
            .context("running fmtutil")?;
        let built = find_below(&staging, &file).filter(|_| status.success());
        let Some(built) = built else { bail!("fmtutil could not build {file}") };
        let engine_dir = built.parent().and_then(Path::file_name).context("format without engine directory")?;
        let dest_dir = web2c.join(engine_dir);
        fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(&file);
        // rename: readers of an older copy keep their inode.
        fs::rename(&built, &dest)?;
        let log = built.with_extension("log");
        if log.exists() {
            let _ = fs::rename(&log, dest.with_extension("log"));
        }
        let rel = format!("web2c/{}/{file}", engine_dir.to_string_lossy());
        lsr::append(&root.texmf_var(), [rel.as_str()])?;
        Ok(dest)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

use std::os::fd::AsFd;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_existing_formats_in_engine_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let web2c = dir.path();
        fs::create_dir_all(web2c.join("pdftex")).unwrap();
        fs::write(web2c.join("pdftex/pdflatex.fmt"), b"x").unwrap();
        fs::create_dir_all(web2c.join("luahbtex")).unwrap();
        fs::write(web2c.join("luahbtex/lualatex.fmt"), b"").unwrap(); // empty = broken
        assert_eq!(find_format(web2c, "pdflatex.fmt").unwrap(), web2c.join("pdftex/pdflatex.fmt"));
        assert!(find_format(web2c, "lualatex.fmt").is_none());
        assert!(find_below(web2c, "pdflatex.fmt").is_some());
    }

    #[test]
    fn rejects_odd_names() {
        let root = Root::new("/nonexistent");
        assert!(mkfmt(&root, "../evil.fmt").is_err());
        assert!(mkfmt(&root, "-x").is_err());
    }
}
