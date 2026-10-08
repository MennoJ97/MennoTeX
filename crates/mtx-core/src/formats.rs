//! `mktexfmt`: build a missing format, safely under concurrency.
//!
//! kpathsea runs `mktexfmt NAME.fmt` when a format is missing (`NAME.base`
//! for METAFONT, which mktexpk runs to make bitmap fonts). TeX Live's
//! own mktexfmt (fmtutil) builds in a temporary directory and then *copies*
//! the result over the destination. When several TeX runs miss the same
//! format at once, each rebuilds and overwrites it while others are already
//! reading it ("Could not undump … pdflatex.fmt", seen in
//! tests/run_concurrent.sh). Here a per-format lock serializes builds, a
//! re-check skips rebuilding what another process just built, and the
//! result is renamed into place atomically. A format no installed package
//! defines (`mf.base` before `metafont` is installed) has its package
//! installed first, subject to the `autoinstall` policy.
//!
//! A format bakes in its engine, the files of its `fmttriggers` packages
//! and the hyphenation patterns. Each build writes a stamp of those next to
//! the format (`pdflatex.fmt.stamp`); after every transaction mtx deletes
//! the formats whose stamp no longer matches ([`stale`]), and kpathsea
//! rebuilds them on next use (PLAN.md §5.7).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::configfiles::{Format, parse_add_format};
use crate::ctx::Ctx;
use crate::db::{Installed, Reason};
use crate::lsr;
use crate::root::Root;
use crate::tlpdb::Package;

/// Format file extensions: TeX engines dump `.fmt`, METAFONT `.base`,
/// MetaPost before 1.5 `.mem`.
const EXTENSIONS: [&str; 3] = ["fmt", "base", "mem"];

/// The file `engine` dumps format `name` to (`pdflatex.fmt`, `mf.base`).
pub fn format_file(name: &str, engine: &str) -> String {
    let ext = if engine.starts_with("mf") {
        "base"
    } else if engine.starts_with("mpost") {
        "mem"
    } else {
        "fmt"
    };
    format!("{name}.{ext}")
}

/// Split a request (`pdflatex.fmt`, `mf.base`, `pdflatex`) into the format
/// name and the file's extension (`fmt` if none was given).
fn split_request(request: &str) -> (&str, &str) {
    match request.rsplit_once('.') {
        Some((name, ext)) if EXTENSIONS.contains(&ext) => (name, ext),
        _ => (request, "fmt"),
    }
}

/// Whether `request` names a format file `mktexfmt` builds itself.
pub fn is_format_request(request: &str) -> bool {
    !request.starts_with('-') && (!request.contains('.') || split_request(request).0 != request)
}

/// Install the package whose `AddFormat` defines the requested format when
/// no installed package does: `metafont` for `mf.base`, which mktexpk needs
/// for fonts that exist only as METAFONT sources (`bbm10`).
pub fn ensure_format_package(root: &Root, request: &str) -> Result<()> {
    let (name, _) = split_request(request);
    let mut ctx = Ctx::open(root.clone())?;
    let tlpdb = ctx.tlpdb()?;
    let defines = |p: &Package| p.executes_of("AddFormat").filter_map(parse_add_format).any(|f| f.enabled && f.name == name);
    if ctx.db.installed()?.keys().filter_map(|n| tlpdb.get(n)).any(defines) {
        return Ok(());
    }
    let Some(pkg) = tlpdb.content_packages().find(|p| defines(p)).map(|p| p.name.clone()) else {
        return Ok(()); // fmtutil reports the unknown format
    };
    drop(tlpdb);
    ctx.log(format!("{request} → package {pkg}"));
    ctx.ask_for = Some(request.to_string());
    ctx.refresh(false)?;
    crate::install::install(&mut ctx, &[&pkg], Reason::Auto)?;
    Ok(())
}

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

/// Build (or find) the format requested as `request` (`pdflatex.fmt`,
/// `mf.base` or `pdflatex`) and return its path.
pub fn mkfmt(root: &Root, request: &str) -> Result<PathBuf> {
    let (name, ext) = split_request(request);
    if name.is_empty() || name.contains(['/', '\0']) || name.starts_with('-') || name.starts_with('.') {
        bail!("invalid format name {request:?}");
    }
    let file = format!("{name}.{ext}");
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
        if let Some(stamp) = current_stamp(root, name) {
            fs::write(stamp_path(&dest), stamp)?;
        }
        Ok(dest)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

use std::os::fd::AsFd;

/// Where the stamp of the format file `fmt` is kept.
pub fn stamp_path(fmt: &Path) -> PathBuf {
    let mut s = fmt.as_os_str().to_owned();
    s.push(".stamp");
    PathBuf::from(s)
}

/// What format `f` depends on, as text: the engine program (size and
/// modification time: `install-binaries` replaces it), the installed
/// revision of each `fmttriggers` package (and when a CTAN overlay of it
/// was installed), and a hash of the hyphenation
/// configuration (`language.dat`, `.def`, `.dat.lua`).
pub fn stamp(root: &Root, f: &Format, installed: &BTreeMap<String, Installed>) -> String {
    use sha2::{Digest, Sha256};
    let engine = fs::metadata(root.bin_dir().join(&f.engine))
        .ok()
        .and_then(|m| Some(format!("{} {}", m.len(), m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos())))
        .unwrap_or_else(|| "missing".into());
    let mut out = format!("engine {} {engine}\n", f.engine);
    for t in &f.fmttriggers {
        let rev = installed.get(t).map_or("-".to_string(), |i| format!("r{}", i.revision));
        // A CTAN overlay's files come first (ctan.rs).
        let overlay = installed.get(&crate::ctan::entry(t)).map_or(String::new(), |i| format!(" ctan@{}", i.installed_at));
        out.push_str(&format!("trigger {t} {rev}{overlay}\n"));
    }
    let mut h = Sha256::new();
    for name in ["language.dat", "language.def", "language.dat.lua"] {
        h.update(fs::read(root.texmf_var().join("tex/generic/config").join(name)).unwrap_or_default());
        h.update([0]);
    }
    out.push_str(&format!("hyphenation {}\n", &hex::encode(h.finalize())[..16]));
    out
}

/// The stamp format `name` would get if it were built now, or `None` if
/// no installed package defines it.
fn current_stamp(root: &Root, name: &str) -> Option<String> {
    let ctx = Ctx::open(root.clone()).ok()?;
    let tlpdb = ctx.tlpdb().ok()?;
    let installed = ctx.db.installed().ok()?;
    let f = installed
        .keys()
        .filter_map(|p| tlpdb.get(p))
        .flat_map(|p| p.executes_of("AddFormat").filter_map(parse_add_format))
        .find(|f| f.name == name)?;
    Some(stamp(root, &f, &installed))
}

/// Built formats of the `installed` packages whose stamp is missing or no
/// longer matches: (format name, path).
pub fn stale(root: &Root, installed_pkgs: &[&Package], installed: &BTreeMap<String, Installed>) -> Vec<(String, PathBuf)> {
    let web2c = root.texmf_var().join("web2c");
    let mut out = Vec::new();
    for p in installed_pkgs {
        for f in p.executes_of("AddFormat").filter_map(parse_add_format) {
            let fmt = web2c.join(&f.engine).join(format_file(&f.name, &f.engine));
            if fmt.exists() && fs::read_to_string(stamp_path(&fmt)).ok().as_deref() != Some(&stamp(root, &f, installed)) {
                out.push((f.name, fmt));
            }
        }
    }
    out
}

/// Delete a format with its log and stamp; kpathsea rebuilds it on next use.
pub fn remove(fmt: &Path) -> Result<()> {
    fs::remove_file(fmt)?;
    let _ = fs::remove_file(fmt.with_extension("log"));
    let _ = fs::remove_file(stamp_path(fmt));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_requests_and_files() {
        assert_eq!(split_request("pdflatex.fmt"), ("pdflatex", "fmt"));
        assert_eq!(split_request("mf.base"), ("mf", "base"));
        assert_eq!(split_request("pdflatex"), ("pdflatex", "fmt"));
        assert!(is_format_request("mf.base") && is_format_request("lualatex") && is_format_request("x.mem"));
        assert!(!is_format_request("--byfmt") && !is_format_request("foo.tex"));
        assert_eq!(format_file("mf", "mf-nowin"), "mf.base");
        assert_eq!(format_file("pdflatex", "pdftex"), "pdflatex.fmt");
    }

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

    #[test]
    fn stale_formats_are_found_by_their_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path());
        fs::create_dir_all(root.bin_dir()).unwrap();
        fs::write(root.bin_dir().join("pdftex"), "engine v1").unwrap();
        let config = root.texmf_var().join("tex/generic/config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("language.dat"), "english hyphen.tex\n").unwrap();
        let db = crate::tlpdb::Tlpdb::parse(
            "name latex-bin\ncategory Package\nrevision 1\n\
             execute AddFormat name=pdflatex engine=pdftex patterns=language.dat options=\"*pdflatex.ini\" fmttriggers=latex\n",
        )
        .unwrap();
        let pkgs = [db.get("latex-bin").unwrap()];
        let installed = |rev| {
            BTreeMap::from([("latex".to_string(), Installed { name: "latex".into(), revision: rev, reason: "auto".into(), installed_at: 0 })])
        };
        let fmt = root.texmf_var().join("web2c/pdftex/pdflatex.fmt");
        fs::create_dir_all(fmt.parent().unwrap()).unwrap();
        fs::write(&fmt, "dump").unwrap();
        // No stamp (built before stamps, or by hand): stale.
        assert_eq!(stale(&root, &pkgs, &installed(5)), vec![("pdflatex".to_string(), fmt.clone())]);
        let f = pkgs[0].executes_of("AddFormat").find_map(parse_add_format).unwrap();
        let fresh = stamp(&root, &f, &installed(5));
        assert!(fresh.contains("trigger latex r5\n"), "{fresh}");
        fs::write(stamp_path(&fmt), &fresh).unwrap();
        assert!(stale(&root, &pkgs, &installed(5)).is_empty());
        // Rewriting the same hyphenation file keeps it; any change does not.
        fs::write(config.join("language.dat"), "english hyphen.tex\n").unwrap();
        assert!(stale(&root, &pkgs, &installed(5)).is_empty());
        assert_eq!(stale(&root, &pkgs, &installed(6)).len(), 1, "trigger updated");
        fs::write(config.join("language.dat"), "english hyphen.tex\ngerman dehyph.tex\n").unwrap();
        assert_eq!(stale(&root, &pkgs, &installed(5)).len(), 1, "patterns changed");
        fs::write(config.join("language.dat"), "english hyphen.tex\n").unwrap();
        fs::write(root.bin_dir().join("pdftex"), "engine v2, longer").unwrap();
        assert_eq!(stale(&root, &pkgs, &installed(5)).len(), 1, "engine replaced");
        remove(&fmt).unwrap();
        assert!(!fmt.exists() && !stamp_path(&fmt).exists());
        assert!(stale(&root, &pkgs, &installed(5)).is_empty(), "nothing built, nothing stale");
    }
}
