//! Documentation on demand (PLAN.md §5.9, `docs = on-texdoc`).
//!
//! tlnet ships a package's documentation as a separate container,
//! `archive/<pkg>.doc.tar.xz` (`doccontainersize`/`doccontainerchecksum` in
//! the tlpdb). Documentation is 70% of tlnet, so it is only installed when
//! asked for: `mtx docs PKG`, or `texdoc NAME`, which is mtx (multi-call)
//! installing the matching documentation before it runs TeX Live's texdoc.
//! Installed documentation is recorded as the pseudo-package `<pkg>.doc`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::ctx::Ctx;
use crate::db::Reason;
use crate::repo::{ChecksumMismatch, is_network_error, sha512_file};
use crate::tlpdb::{Package, Tlpdb};

pub const SUFFIX: &str = ".doc";

/// The database entry for `pkg`'s documentation.
pub fn entry(pkg: &str) -> String {
    format!("{pkg}{SUFFIX}")
}

/// The package whose documentation a `<pkg>.doc` entry holds.
pub fn base(entry: &str) -> Option<&str> {
    entry.strip_suffix(SUFFIX)
}

/// Packages with documentation for `texdoc NAME`: the package called NAME,
/// else every package shipping a documentation file whose name without
/// extension is NAME (`pgfmanual` → pgf).
pub fn packages_for(tlpdb: &Tlpdb, name: &str) -> Vec<String> {
    let has_docs = |p: &Package| p.doc_container_size > 0;
    if let Some(p) = tlpdb.get(name).filter(|p| has_docs(p)) {
        return vec![p.name.clone()];
    }
    let want = name.to_ascii_lowercase();
    let mut out: Vec<String> = tlpdb
        .content_packages()
        .filter(|p| has_docs(p))
        .filter(|p| {
            p.docfiles.iter().any(|f| {
                let file = f.rsplit('/').next().unwrap_or(f);
                let stem = file.split_once('.').map_or(file, |(s, _)| s);
                stem.eq_ignore_ascii_case(&want)
            })
        })
        .map(|p| p.name.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Install the documentation of `pkgs` that is missing or older than the
/// package database. Returns the packages whose documentation was installed.
pub fn install(ctx: &mut Ctx, pkgs: &[&str], reason: Reason) -> Result<Vec<String>> {
    let mut attempt = 0;
    loop {
        let tlpdb = ctx.tlpdb()?;
        match install_once(ctx, &tlpdb, pkgs, reason) {
            Err(e) if is_network_error(&e) && attempt < 2 => {
                attempt += 1;
                ctx.failover(&e)?;
            }
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

fn install_once(ctx: &mut Ctx, tlpdb: &Tlpdb, pkgs: &[&str], reason: Reason) -> Result<Vec<String>> {
    let installed = ctx.db.installed()?;
    let mut todo: Vec<&Package> = Vec::new();
    for name in pkgs {
        let p = tlpdb.get(name).with_context(|| format!("unknown package `{name}`"))?;
        if p.doc_container_size == 0 {
            ctx.log(format!("{name} has no documentation in TeX Live"));
            continue;
        }
        if installed.get(&entry(name)).is_none_or(|i| i.revision < p.revision) {
            todo.push(p);
        }
    }
    if todo.is_empty() {
        return Ok(Vec::new());
    }
    let size: u64 = todo.iter().map(|p| p.doc_container_size).sum();
    let names: Vec<&str> = todo.iter().map(|p| p.name.as_str()).collect();
    ctx.log(format!("installing documentation of {}, {:.1} MiB", names.join(", "), size as f64 / 1048576.0));

    let cache = ctx.root.cache_dir();
    fs::create_dir_all(&cache)?;
    let repo = ctx.repo()?.clone();
    let mut archives: Vec<(&Package, PathBuf)> = Vec::new();
    for p in &todo {
        let dest = cache.join(format!("{}.doc.r{}.tar.xz", p.name, p.revision));
        if !(dest.exists() && sha512_file(&dest)? == p.doc_container_checksum) {
            repo.download_verified(&format!("archive/{}.doc.tar.xz", p.name), &dest, p.doc_container_size, &p.doc_container_checksum)
                .with_context(|| format!("downloading the documentation of {}", p.name))?;
        }
        archives.push((p, dest));
    }

    let lock = fs::File::create(ctx.root.lock_path())?;
    lock.lock()?;
    let protected = |rel: &str| crate::install::PROTECTED.contains(&rel);
    let mut done = Vec::new();
    for (p, archive) in archives {
        let e = entry(&p.name);
        let old: BTreeSet<String> = ctx.db.files_of(&e)?.into_iter().collect();
        let unpacked = crate::extract::unpack(&archive, p.relocated, &ctx.root.dir, &protected)
            .with_context(|| format!("unpacking the documentation of {}", p.name))?;
        let new: BTreeSet<&str> = unpacked.files.iter().map(String::as_str).collect();
        for f in old.iter().filter(|f| !new.contains(f.as_str())) {
            if ctx.db.other_owners(f, &e)? == 0 {
                let _ = fs::remove_file(ctx.root.dir.join(f));
            }
        }
        ctx.db.record(&e, p.revision, reason, &unpacked.files)?;
        done.push(p.name.clone());
    }
    // Rebuilt, not appended to: texdoc reads ls-R itself and stops at the
    // first directory after the ./doc section, so appended doc blocks would
    // be invisible to it.
    crate::lsr::rebuild(&ctx.root.texmf_dist())?;
    drop(lock);
    Ok(done)
}
