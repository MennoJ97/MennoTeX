//! Installing packages: plan → download (no lock held) → commit under the
//! install lock → regenerate configuration → update ls-R.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::configfiles::{self, Regen, parse_add_format};
use crate::ctx::Ctx;
use crate::db::Reason;
use crate::extract;
use crate::lsr;
use crate::repo::{ChecksumMismatch, Repo, sha512_file};
use crate::tlpdb::{Package, Tlpdb};

/// Files mtx provides itself. tlnet's versions are never unpacked over them.
pub const PROTECTED: &[&str] =
    &["bin/universal-darwin/mtx", "bin/universal-darwin/mktextex", "bin/universal-darwin/mktextfm"];

#[derive(Debug, Default)]
pub struct Report {
    /// (package, revision) newly installed or upgraded.
    pub installed: Vec<(String, u64)>,
    pub files: usize,
    pub bytes_downloaded: u64,
}

/// Packages in `names` that are missing or older than in `tlpdb`.
fn pending(ctx: &Ctx, tlpdb: &Tlpdb, names: &[String]) -> Result<Vec<String>> {
    let installed = ctx.db.installed()?;
    Ok(names
        .iter()
        .filter(|n| {
            let want = tlpdb.get(n).map(|p| p.revision).unwrap_or(0);
            installed.get(n.as_str()).is_none_or(|i| i.revision < want)
        })
        .cloned()
        .collect())
}

fn archive_name(p: &Package) -> String {
    format!("{}.r{}.tar.xz", p.name, p.revision)
}

/// Download (or reuse from the cache) the containers of `names`, in
/// parallel. Returns the cached paths and the number of bytes fetched.
fn download(ctx: &mut Ctx, tlpdb: &Tlpdb, names: &[String]) -> Result<(HashMap<String, PathBuf>, u64)> {
    let cache = ctx.root.cache_dir();
    fs::create_dir_all(&cache)?;
    let repo: Repo = ctx.repo()?.clone();
    let jobs: Vec<&Package> = names.iter().filter_map(|n| tlpdb.get(n)).filter(|p| !p.is_meta()).collect();

    let results: Vec<Result<(String, PathBuf, u64)>> = std::thread::scope(|s| {
        let chunks = jobs.chunks(jobs.len().div_ceil(8).max(1));
        let handles: Vec<_> = chunks
            .map(|chunk| {
                let repo = repo.clone();
                let cache = cache.clone();
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|p| {
                            let dest = cache.join(archive_name(p));
                            if dest.exists() && sha512_file(&dest)? == p.container_checksum {
                                return Ok((p.name.clone(), dest, 0));
                            }
                            repo.download_verified(
                                &format!("archive/{}.tar.xz", p.name),
                                &dest,
                                p.container_size,
                                &p.container_checksum,
                            )
                            .with_context(|| format!("downloading {}", p.name))?;
                            Ok((p.name.clone(), dest, p.container_size))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("download thread panicked")).collect()
    });

    let mut paths = HashMap::new();
    let mut bytes = 0;
    for r in results {
        let (name, path, n) = r?;
        bytes += n;
        paths.insert(name, path);
    }
    Ok((paths, bytes))
}

/// Install `roots` and their dependencies, refreshing the package database
/// and retrying when the mirror changed underneath us (PLAN.md §4.3).
pub fn install(ctx: &mut Ctx, roots: &[&str], reason: Reason) -> Result<Report> {
    let mut attempt = 0;
    loop {
        let tlpdb = ctx.tlpdb()?;
        match install_once(ctx, &tlpdb, roots, reason) {
            Err(e) if e.chain().any(|c| c.downcast_ref::<ChecksumMismatch>().is_some()) && attempt < 2 => {
                attempt += 1;
                ctx.log(format!("{e:#}; refreshing the package database and retrying"));
                if attempt == 2 {
                    ctx.reject_mirror()?;
                }
                ctx.refresh(true)?;
            }
            other => return other,
        }
    }
}

fn install_once(ctx: &mut Ctx, tlpdb: &Tlpdb, roots: &[&str], reason: Reason) -> Result<Report> {
    let closure = tlpdb.closure(roots.iter().copied())?;
    let plan = pending(ctx, tlpdb, &closure)?;
    if plan.is_empty() {
        return Ok(Report::default());
    }
    let size: u64 = plan.iter().filter_map(|n| tlpdb.get(n)).map(|p| p.container_size).sum();
    ctx.log(format!("installing {} package(s), {:.1} MiB: {}", plan.len(), size as f64 / 1048576.0, summarize(&plan)));
    let (archives, bytes) = download(ctx, tlpdb, &plan)?;

    let lock = fs::File::create(ctx.root.lock_path())?;
    if lock.try_lock().is_err() {
        ctx.log("waiting for another mtx process to finish");
        lock.lock()?;
    }
    // Another process may have installed some of these while we downloaded.
    let plan = pending(ctx, tlpdb, &plan)?;
    let root_set: BTreeSet<&str> = roots.iter().copied().collect();
    let protected = |rel: &str| PROTECTED.contains(&rel);

    let mut report = Report { bytes_downloaded: bytes, ..Default::default() };
    let mut regen = Regen::default();
    let mut dist_files: Vec<String> = Vec::new();
    for name in &plan {
        let p = tlpdb.get(name).unwrap();
        let why = if root_set.contains(name.as_str()) { reason } else { Reason::Dependency };
        let files = if p.is_meta() {
            Vec::new()
        } else {
            let archive = archives.get(name).with_context(|| format!("no archive for {name}"))?;
            let old: BTreeSet<String> = ctx.db.files_of(name)?.into_iter().collect();
            let unpacked = extract::unpack(archive, p.relocated, &ctx.root.dir, &protected)
                .with_context(|| format!("unpacking {name}"))?;
            // Remove files the new revision no longer ships (unless shared).
            let new: BTreeSet<&str> = unpacked.files.iter().map(String::as_str).collect();
            for f in old.iter().filter(|f| !new.contains(f.as_str())) {
                if ctx.db.other_owners(f, name)? == 0 {
                    let _ = fs::remove_file(ctx.root.dir.join(f));
                }
            }
            dist_files.extend(unpacked.files.iter().filter(|f| !old.contains(*f)).cloned());
            unpacked.files
        };
        ctx.db.record(name, p.revision, why, &files)?;
        report.files += files.len();
        report.installed.push((name.clone(), p.revision));
        regen.merge(Regen::for_package(p));
    }
    append_lsr(ctx, &dist_files)?;
    if regen.any() {
        apply_regen(ctx, tlpdb, regen, &plan)?;
    }
    drop(lock);
    Ok(report)
}

fn summarize(names: &[String]) -> String {
    match names.len() {
        0..=6 => names.join(", "),
        n => format!("{}, … ({} more)", names[..5].join(", "), n - 5),
    }
}

/// Add newly installed `texmf-dist` files to its ls-R.
fn append_lsr(ctx: &Ctx, files: &[String]) -> Result<()> {
    let rel: Vec<&str> = files.iter().filter_map(|f| f.strip_prefix("texmf-dist/")).collect();
    lsr::append(&ctx.root.texmf_dist(), rel)
}

/// Regenerate configuration files, font maps and ls-R after a transaction,
/// and invalidate formats whose `fmttriggers` packages changed.
pub fn apply_regen(ctx: &Ctx, tlpdb: &Tlpdb, regen: Regen, changed: &[String]) -> Result<()> {
    let installed = ctx.db.installed()?;
    let pkgs: Vec<&Package> = installed.keys().filter_map(|n| tlpdb.get(n)).collect();
    let written = configfiles::write(&ctx.root.dir, &pkgs, regen)?;
    let new_dist: Vec<String> = written
        .iter()
        .filter(|p| p.starts_with("texmf-dist/"))
        .filter(|p| ctx.db.other_owners(p, "").unwrap_or(0) == 0) // not already listed via a package
        .cloned()
        .collect();
    append_lsr(ctx, &new_dist)?;

    if regen.maps && ctx.root.bin_dir().join("updmap-sys").exists() {
        let out = Command::new(ctx.root.bin_dir().join("updmap-sys"))
            .args(["--nohash", "--quiet"])
            .env("PATH", ctx.root.tool_path())
            .output()
            .context("running updmap-sys")?;
        if !out.status.success() {
            bail!("updmap-sys failed:\n{}", String::from_utf8_lossy(&out.stderr));
        }
    }
    invalidate_formats(ctx, &pkgs, regen, changed)?;
    lsr::rebuild(&ctx.root.texmf_var())?;
    Ok(())
}

/// Delete built formats that depend on changed packages; kpathsea's
/// `mktexfmt` rebuilds them on next use.
fn invalidate_formats(ctx: &Ctx, installed: &[&Package], regen: Regen, changed: &[String]) -> Result<()> {
    let changed: BTreeSet<&str> = changed.iter().map(String::as_str).collect();
    for p in installed {
        for f in p.executes_of("AddFormat").filter_map(parse_add_format) {
            let stale = regen.hyphen || f.fmttriggers.iter().any(|t| changed.contains(t.as_str()));
            let fmt = ctx.root.texmf_var().join("web2c").join(&f.engine).join(format!("{}.fmt", f.name));
            if stale && fmt.exists() {
                fs::remove_file(&fmt)?;
                ctx.log(format!("format {} will be rebuilt on next use", f.name));
            }
        }
    }
    Ok(())
}
