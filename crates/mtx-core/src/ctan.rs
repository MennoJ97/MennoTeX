//! The CTAN overlay channel (PLAN.md §4.2, decision 0019).
//!
//! `mtx install --from-ctan PKG` puts CTAN's current TDS archive of a
//! package into `<root>/texmf-ctan`, which TeX searches before `texmf-dist`
//! (`TEXMFAUXTREES`), for the day or two before tlnet carries a new
//! version, or for a package CTAN has and TeX Live does not. It is never
//! used on its own: only for packages named on that command line.
//!
//! The TeX Live package is installed first, from tlnet and verified, so
//! dependencies, font maps and formats come from there and dropping the
//! overlay leaves a working package. CTAN publishes no signatures or
//! checksums for its archives: the archive comes over HTTPS from CTAN's
//! mirror redirector, must be a zip, and only TDS directories that hold
//! TeX inputs are taken from it.
//!
//! The overlay is recorded under the TeX Live package name (CTAN's can
//! differ) as the pseudo-package `<pkg>.ctan` (its files, for removal and
//! in format stamps) plus the setting `ctan:<pkg>` (versions). `mtx update`
//! drops it once tlnet has caught up: when tlnet's catalogue version equals
//! the CTAN version, or tlnet's version changed since the overlay was
//! installed (tlnet takes its versions from CTAN). Packages without a
//! catalogue version in TeX Live (`l3kernel`, `amsmath`) are compared by
//! revision instead: any tlnet change counts.

use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::ctx::Ctx;
use crate::db::Reason;
use crate::install;
use crate::repo::Repo;
use crate::tlpdb::Tlpdb;

pub const SUFFIX: &str = ".ctan";
/// The tree overlays go to, relative to the root.
pub const TREE: &str = "texmf-ctan";
pub const API: &str = "https://ctan.org/json/2.0/pkg/";
pub const INSTALL: &str = "https://mirrors.ctan.org/install/";
/// TDS top-level directories taken from an archive. Not `source/` (not
/// needed to typeset), nor `web2c/` or anything else that could configure
/// the installation.
const TDS_DIRS: &[&str] =
    &["tex", "fonts", "bibtex", "makeindex", "metapost", "metafont", "mft", "dvips", "tex4ht", "context", "scripts", "doc"];
/// No TDS archive on CTAN comes near this.
const MAX_BYTES: usize = 1 << 30;

/// The database entry for `pkg`'s overlay.
pub fn entry(pkg: &str) -> String {
    format!("{pkg}{SUFFIX}")
}

/// The package an `<pkg>.ctan` entry overlays.
pub fn base(entry: &str) -> Option<&str> {
    entry.strip_suffix(SUFFIX)
}

/// What CTAN's JSON API says about a package.
#[derive(Debug, PartialEq, Eq)]
pub struct Info {
    pub name: String,
    pub version: String,
    /// Path of the TDS archive below CTAN's `install/` tree.
    pub install: Option<String>,
    /// The TeX Live package name, if TeX Live has it.
    pub texlive: Option<String>,
}

pub fn parse_info(json: &str) -> Result<Info> {
    let v: serde_json::Value = serde_json::from_str(json).context("CTAN's answer is not JSON")?;
    if let Some(err) = v.get("errors").and_then(|e| e.as_array()).and_then(|e| e.first()) {
        bail!("CTAN: {}", err.as_str().unwrap_or("error"));
    }
    let s = |key: &str| v.get(key).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from);
    let version = v.get("version").map(|ver| {
        let get = |k: &str| ver.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        match (get("number"), get("date")) {
            (n, _) if !n.is_empty() => n,
            (_, d) => d,
        }
    });
    Ok(Info {
        name: s("id").context("CTAN's answer names no package")?,
        version: version.filter(|v| !v.is_empty()).unwrap_or_else(|| "unversioned".into()),
        install: s("install"),
        texlive: s("texlive"),
    })
}

/// (CTAN version, CTAN's name, tlnet's catalogue version when installed)
/// of `pkg`'s overlay.
pub fn recorded(ctx: &Ctx, pkg: &str) -> Result<Option<(String, String, String)>> {
    Ok(ctx.db.get(&format!("ctan:{pkg}"))?.and_then(|v| {
        let mut f = v.split('\t').map(String::from);
        Some((f.next()?, f.next()?, f.next()?))
    }))
}

/// Packages with an overlay.
pub fn overlays(ctx: &Ctx) -> Result<Vec<String>> {
    Ok(ctx.db.installed()?.keys().filter_map(|n| base(n)).map(String::from).collect())
}

/// Install CTAN's current version of each of `names` over TeX Live's.
/// Returns (package, version) for each overlay installed.
pub fn install(ctx: &mut Ctx, names: &[&str]) -> Result<Vec<(String, String)>> {
    install_from(ctx, names, API, INSTALL)
}

pub fn install_from(ctx: &mut Ctx, names: &[&str], api: &str, mirror: &str) -> Result<Vec<(String, String)>> {
    let mut done = Vec::new();
    for name in names {
        let json = Repo::at(api).get_bytes(name).with_context(|| format!("asking CTAN about {name}"))?;
        let info = parse_info(&String::from_utf8_lossy(&json))?;
        let Some(archive) = info.install.as_deref() else {
            bail!("CTAN has no installable (TDS) archive of {name}, only its sources");
        };
        let tlpdb = ctx.tlpdb()?;
        let tl_name = info.texlive.clone().unwrap_or_else(|| info.name.clone());
        let tl = tlpdb.get(&tl_name);
        if let Some(p) = tl {
            if p.catalogue_version.as_deref() == Some(info.version.as_str()) {
                ctx.log(format!("{name}: TeX Live already has CTAN's version {}; installing it from TeX Live", info.version));
                install::install(ctx, &[&tl_name], Reason::Explicit)?;
                continue;
            }
            // Dependencies, font maps and formats come from TeX Live.
            install::install(ctx, &[&tl_name], Reason::Explicit)?;
        }
        ctx.log(format!("{name}: fetching version {} from CTAN ({archive})", info.version));
        let bytes = Repo::at(mirror).get_bytes(archive.trim_start_matches('/'))?;
        let tl_version = tl.map_or_else(|| "-".into(), tl_version);
        place(ctx, &tlpdb, &tl_name, &bytes)?;
        ctx.db.set(&format!("ctan:{tl_name}"), &format!("{}\t{}\t{tl_version}", info.version, info.name))?;
        ctx.log(format!("{tl_name}: CTAN version {} installed over TeX Live's {tl_version}", info.version));
        done.push((tl_name, info.version));
    }
    Ok(done)
}

/// Unpack the TDS zip `bytes` into the overlay tree as `pkg`'s overlay,
/// replacing an earlier one.
fn place(ctx: &mut Ctx, tlpdb: &Tlpdb, pkg: &str, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_BYTES || !bytes.starts_with(b"PK\x03\x04") {
        bail!("CTAN's archive of {pkg} is not a zip file");
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).with_context(|| format!("reading CTAN's archive of {pkg}"))?;
    let staging = ctx.root.dir.join(format!(".staging-ctan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let mut files: Vec<String> = Vec::new();
    let mut skipped: BTreeSet<String> = BTreeSet::new();
    let result = (|| -> Result<()> {
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let Some(path) = f.enclosed_name() else { bail!("unsafe path in CTAN's archive: {}", f.name()) };
            let rel = path.to_str().context("non-UTF-8 path in CTAN's archive")?.to_string();
            let top = rel.split('/').next().unwrap_or("");
            if f.is_symlink() || !TDS_DIRS.contains(&top) || rel.split('/').count() < 2 {
                skipped.insert(if f.is_symlink() { format!("{rel} (symlink)") } else { format!("{top}/") });
                continue;
            }
            let dest = staging.join(&rel);
            fs::create_dir_all(dest.parent().unwrap())?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            fs::write(&dest, &buf)?;
            let exec = f.unix_mode().is_some_and(|m| m & 0o111 != 0);
            fs::set_permissions(&dest, fs::Permissions::from_mode(if exec { 0o755 } else { 0o644 }))?;
            files.push(format!("{TREE}/{rel}"));
        }
        if files.is_empty() {
            bail!("CTAN's archive of {pkg} holds no TeX files");
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    if !skipped.is_empty() {
        ctx.log(format!("{pkg}: not taken from CTAN's archive: {}", skipped.into_iter().collect::<Vec<_>>().join(", ")));
    }

    let lock = fs::File::create(ctx.root.lock_path())?;
    lock.lock()?;
    let e = entry(pkg);
    let new: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    for old in ctx.db.files_of(&e)? {
        if !new.contains(old.as_str()) {
            remove_file_and_empty_dirs(&ctx.root.dir, &ctx.root.dir.join(&old));
        }
    }
    for f in &files {
        let dest = ctx.root.dir.join(f);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::rename(staging.join(f.strip_prefix(&format!("{TREE}/")).unwrap()), &dest)?;
    }
    let _ = fs::remove_dir_all(&staging);
    ctx.db.record(&e, 0, Reason::Explicit, &files)?;
    after_change(ctx, tlpdb, pkg)?;
    drop(lock);
    Ok(())
}

/// Font maps and formats see the overlay's files first, so they are
/// regenerated (maps) and invalidated (formats whose stamp names the
/// overlay) when an overlay comes or goes.
fn after_change(ctx: &Ctx, tlpdb: &Tlpdb, tl_name: &str) -> Result<()> {
    if let Some(p) = tlpdb.get(tl_name) {
        let regen = crate::configfiles::Regen::for_package(p);
        if regen.any() {
            install::apply_regen(ctx, tlpdb, regen)?;
        }
    }
    install::invalidate_installed_formats(ctx, tlpdb)
}

fn remove_file_and_empty_dirs(root: &Path, path: &Path) {
    let _ = fs::remove_file(path);
    let mut dir = path.parent();
    while let Some(d) = dir.filter(|d| *d != root && fs::remove_dir(d).is_ok()) {
        dir = d.parent();
    }
}

/// Remove `pkg`'s overlay; TeX Live's version (if installed) is used again.
pub fn drop_overlay(ctx: &mut Ctx, pkg: &str) -> Result<bool> {
    let e = entry(pkg);
    if !ctx.db.installed()?.contains_key(&e) {
        return Ok(false);
    }
    let tlpdb = ctx.tlpdb()?;
    let lock = fs::File::create(ctx.root.lock_path())?;
    lock.lock()?;
    for f in ctx.db.files_of(&e)? {
        remove_file_and_empty_dirs(&ctx.root.dir, &ctx.root.dir.join(f));
    }
    ctx.db.forget(&e)?;
    ctx.db.unset(&format!("ctan:{pkg}"))?;
    after_change(ctx, &tlpdb, pkg)?;
    drop(lock);
    ctx.log(format!("{pkg}: CTAN overlay removed"));
    Ok(true)
}

/// What tlnet's version of `p` is called: its catalogue version, else
/// `r<revision>`.
fn tl_version(p: &crate::tlpdb::Package) -> String {
    p.catalogue_version.clone().unwrap_or_else(|| format!("r{}", p.revision))
}

/// Overlays tlnet has caught up with (see the module comment), as
/// (package, CTAN version, tlnet's version now).
pub fn caught_up(ctx: &Ctx, tlpdb: &Tlpdb) -> Result<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for pkg in overlays(ctx)? {
        let Some((ctan, _, tl_then)) = recorded(ctx, &pkg)? else { continue };
        let Some(now) = tlpdb.get(&pkg).map(tl_version) else { continue };
        if now == ctan || now != tl_then {
            out.push((pkg, ctan, now));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ctan_package_records() {
        let json = r#"{"id":"tcolorbox","name":"tcolorbox","version":{"number":"6.10.0","date":"2026-05-28"},
            "ctan":{"path":"/macros/latex/contrib/tcolorbox","file":true},
            "install":"/macros/latex/contrib/tcolorbox.tds.zip","miktex":"tcolorbox","texlive":"tcolorbox"}"#;
        assert_eq!(
            parse_info(json).unwrap(),
            Info {
                name: "tcolorbox".into(),
                version: "6.10.0".into(),
                install: Some("/macros/latex/contrib/tcolorbox.tds.zip".into()),
                texlive: Some("tcolorbox".into()),
            }
        );
        // Only a date; no TDS archive; not in TeX Live.
        let info = parse_info(r#"{"id":"x","version":{"number":"","date":"2025-01-02"}}"#).unwrap();
        assert_eq!((info.version.as_str(), info.install, info.texlive), ("2025-01-02", None, None));
        assert!(parse_info(r#"{"errors":["Package not found"]}"#).unwrap_err().to_string().contains("not found"));
        assert!(parse_info("<html>").is_err());
    }
}
