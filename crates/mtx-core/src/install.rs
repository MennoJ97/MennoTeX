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
use crate::repo::{ChecksumMismatch, Repo, is_network_error, sha512_file};
use crate::tlpdb::{Package, Tlpdb};

/// Files mtx provides itself. tlnet's versions are never unpacked over them.
pub const PROTECTED: &[&str] = &[
    "bin/universal-darwin/mtx",
    "bin/universal-darwin/mktextex",
    "bin/universal-darwin/mktextfm",
    "bin/universal-darwin/mktexfmt",
    "bin/universal-darwin/texdoc",
    "bin/universal-darwin/latexmk",
];

#[derive(Debug, Default)]
pub struct Report {
    /// (package, revision) newly installed or upgraded.
    pub installed: Vec<(String, u64)>,
    pub files: usize,
    pub bytes_downloaded: u64,
}

/// Whether the last installation of `pkg` was interrupted: its journal
/// entry stays until the whole transaction (ls-R, configuration, font maps,
/// shims) is done, so its files may be there without the rest.
pub fn interrupted(root: &crate::root::Root, pkg: &str) -> bool {
    root.journal_dir().join(pkg).exists()
}

/// Crash tests (`tests/run_crash.sh`): with `MTX_CRASH_AT=<point>`, mtx
/// kills itself with SIGKILL at that point of an install, as a power cut
/// or `kill -9` would.
fn crash_point(point: &str) {
    if std::env::var("MTX_CRASH_AT").is_ok_and(|v| v == point) {
        let _ = Command::new("/bin/kill").args(["-KILL", &std::process::id().to_string()]).status();
    }
}

/// Packages in `names` that are missing, older than in `tlpdb`, or whose
/// last installation was interrupted (still journaled).
fn pending(ctx: &Ctx, tlpdb: &Tlpdb, names: &[String]) -> Result<Vec<String>> {
    let installed = ctx.db.installed()?;
    let journal = ctx.root.journal_dir();
    Ok(names
        .iter()
        .filter(|n| {
            let want = tlpdb.get(n).map(|p| p.revision).unwrap_or(0);
            installed.get(n.as_str()).is_none_or(|i| i.revision < want) || journal.join(n.as_str()).exists()
        })
        .cloned()
        .collect())
}

/// Installed packages that have a newer revision in `tlpdb`.
pub fn outdated(ctx: &Ctx, tlpdb: &Tlpdb) -> Result<Vec<(String, u64, u64)>> {
    Ok(ctx
        .db
        .installed()?
        .into_values()
        .filter_map(|i| {
            // Documentation (`<pkg>.doc`) follows its package's revision.
            let p = tlpdb.get(crate::docs::base(&i.name).unwrap_or(&i.name))?;
            (p.revision > i.revision).then(|| (i.name, i.revision, p.revision))
        })
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

fn install_once(ctx: &mut Ctx, tlpdb: &Tlpdb, roots: &[&str], reason: Reason) -> Result<Report> {
    let mut closure = tlpdb.closure(roots.iter().copied())?;
    // Fonts need their map entries (e.g. ec's TFMs are mapped by cm-super);
    // install those packages in the same transaction, so the maps are
    // regenerated before TeX ships out its first page.
    let installed_now = ctx.db.installed()?;
    let have = |p: &str| installed_now.contains_key(p);
    let mut extra = crate::fontmaps::map_packages_for(tlpdb, &closure, &have);
    // Packages whose same-named files the engine would find first (see
    // shadows.rs); kpathsea never misses those, so it would never ask.
    if let Ok(idx) = crate::index::Index::open(&ctx.root.index_path()) {
        extra.extend(crate::shadows::shadow_packages_for(&idx, tlpdb, &closure, &have));
    }
    if !extra.is_empty() {
        let mut all: Vec<&str> = roots.to_vec();
        all.extend(extra.iter().map(String::as_str));
        closure = tlpdb.closure(all)?;
    }
    let mut plan = pending(ctx, tlpdb, &closure)?;
    // Finish installs a crash interrupted, whatever this one is for.
    if let Ok(entries) = fs::read_dir(ctx.root.journal_dir()) {
        for name in entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()) {
            if tlpdb.get(&name).is_some() && !plan.contains(&name) {
                plan.push(name);
            }
        }
    }
    if plan.is_empty() {
        return Ok(Report::default());
    }
    let size: u64 = plan.iter().filter_map(|n| tlpdb.get(n)).map(|p| p.container_size).sum();
    if let Some(trigger) = ctx.ask_for.clone() {
        // The requested packages first, for the prompt.
        let mut shown: Vec<String> = plan.iter().filter(|n| roots.contains(&n.as_str())).cloned().collect();
        shown.extend(plan.iter().filter(|n| !roots.contains(&n.as_str())).cloned());
        let req = crate::consent::Request { trigger: &trigger, packages: &shown, bytes: size };
        let prompter = ctx.prompter;
        if !crate::consent::decide(ctx, &req, &mut |c, q| prompter(c, q))? {
            return Err(crate::consent::Declined { trigger, packages: summarize(&shown), why: "see mtx log".into() }.into());
        }
        ctx.ask_for = None;
    }
    ctx.log(format!("installing {} package(s), {:.1} MiB: {}", plan.len(), size as f64 / 1048576.0, summarize(&plan)));
    let (archives, bytes) = download(ctx, tlpdb, &plan)?;

    let lock = fs::File::create(ctx.root.lock_path())?;
    if lock.try_lock().is_err() {
        ctx.log("waiting for another mtx process to finish");
        lock.lock()?;
    }
    // Unpacking happens only under this lock, so staging directories left
    // here belong to a process that was killed.
    let _ = fs::remove_dir_all(ctx.root.dir.join(".staging"));
    // Another process may have installed some of these while we downloaded.
    let plan = pending(ctx, tlpdb, &plan)?;
    let root_set: BTreeSet<&str> = roots.iter().copied().collect();
    // Our own builds of programs (see binaries.rs) are never replaced by
    // tlnet's unpatched ones.
    let ours: BTreeSet<String> = ctx.db.files_of(crate::binaries::BIN_PACKAGE)?.into_iter().collect();
    let protected = |rel: &str| PROTECTED.contains(&rel) || ours.contains(rel);

    let mut report = Report { bytes_downloaded: bytes, ..Default::default() };
    let mut regen = Regen::default();
    let mut dist_files: Vec<String> = Vec::new();
    let journal = ctx.root.journal_dir();
    fs::create_dir_all(&journal)?;
    let installed_before = ctx.db.installed()?;
    for name in &plan {
        let p = tlpdb.get(name).unwrap();
        let why = if root_set.contains(name.as_str()) || reason == Reason::Upgrade {
            reason
        } else if installed_before.contains_key(name.as_str()) {
            Reason::Upgrade // redoing an interrupted install: keep its reason
        } else {
            Reason::Dependency
        };
        // Crash safety: the journal entry stays until the transaction is
        // complete (below); until then the package counts as interrupted
        // and is redone by the next install, lookup or `mtx repair`.
        fs::write(journal.join(name), b"")?;
        let files = if p.is_meta() {
            Vec::new()
        } else {
            let archive = archives.get(name).with_context(|| format!("no archive for {name}"))?;
            let old: BTreeSet<String> = ctx.db.files_of(name)?.into_iter().collect();
            let unpacked = extract::unpack(archive, p.relocated, &ctx.root.dir, &protected)
                .with_context(|| format!("unpacking {name}"))?;
            crash_point("unpacked");
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
        crash_point("recorded");
        report.files += files.len();
        report.installed.push((name.clone(), p.revision));
        regen.merge(Regen::for_package(p));
    }
    append_lsr(ctx, &dist_files)?;
    crash_point("listed");
    if regen.any() {
        apply_regen(ctx, tlpdb, regen, &plan)?;
    }
    crash_point("regenerated");
    // A binary package replaced its shims with real programs.
    if plan.iter().any(|n| crate::tlpdb::arch_suffix(n).is_some()) {
        crate::shims::sync(ctx, tlpdb)?;
    }
    for name in &plan {
        fs::remove_file(journal.join(name))?;
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
            // We hold the install lock: a patched kpsewhich run by updmap
            // must not try to install anything (it would wait for us).
            .env("MTX_AUTOINSTALL", "0")
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
            let fmt = ctx.root.texmf_var().join("web2c").join(&f.engine).join(crate::formats::format_file(&f.name, &f.engine));
            if stale && fmt.exists() {
                fs::remove_file(&fmt)?;
                ctx.log(format!("format {} will be rebuilt on next use", f.name));
            }
        }
    }
    Ok(())
}

/// Remove installed packages. Refuses (unless `force`) when another
/// installed package depends on one of them. Files shared with other
/// packages are kept. Returns the removed package names.
pub fn remove(ctx: &mut Ctx, names: &[&str], force: bool) -> Result<Vec<String>> {
    let tlpdb = ctx.tlpdb()?;
    let installed = ctx.db.installed()?;
    // A package's documentation goes with it.
    let docs: Vec<String> = names.iter().map(|n| crate::docs::entry(n)).collect();
    let targets: BTreeSet<&str> =
        names.iter().copied().chain(docs.iter().map(String::as_str)).filter(|n| installed.contains_key(*n)).collect();
    if !force {
        for (other, _) in &installed {
            if targets.contains(other.as_str()) {
                continue;
            }
            if let Some(p) = tlpdb.get(other) {
                for dep in p.resolved_depends() {
                    if targets.contains(dep.as_str()) {
                        bail!("{other} depends on {dep}; remove it too or use --force");
                    }
                }
            }
        }
    }
    let lock = fs::File::create(ctx.root.lock_path())?;
    lock.lock()?;
    let mut regen = Regen::default();
    let mut removed = Vec::new();
    for name in &targets {
        for f in ctx.db.files_of(name)? {
            if ctx.db.other_owners(&f, name)? == 0 {
                let path = ctx.root.dir.join(&f);
                let _ = fs::remove_file(&path);
                // Drop directories that became empty (best effort).
                let mut dir = path.parent();
                while let Some(d) = dir.filter(|d| *d != ctx.root.dir && fs::remove_dir(d).is_ok()) {
                    dir = d.parent();
                }
            }
        }
        ctx.db.forget(name)?;
        if let Some(p) = tlpdb.get(name) {
            regen.merge(Regen::for_package(p));
        }
        removed.push(name.to_string());
    }
    // Appending cannot remove entries, so rebuild.
    lsr::rebuild(&ctx.root.texmf_dist())?;
    crate::shims::sync(ctx, &tlpdb)?;
    if regen.any() {
        apply_regen(ctx, &tlpdb, regen, &removed)?;
    }
    drop(lock);
    if !removed.is_empty() {
        ctx.log(format!("removed {}", removed.join(", ")));
    }
    Ok(removed)
}

/// Fix what `mtx doctor` can fix: install missing font-map packages,
/// packages that shadow installed files (shadows.rs) and core packages
/// added since the root was made, redo interrupted
/// installs, regenerate configuration, ls-R and shims.
pub fn repair(ctx: &mut Ctx) -> Result<Vec<String>> {
    let tlpdb = ctx.tlpdb()?;
    let installed = ctx.db.installed()?;
    let names: Vec<String> = installed.keys().cloned().collect();
    let mut todo: BTreeSet<String> = crate::fontmaps::map_packages_for(&tlpdb, &names, &|p| installed.contains_key(p));
    todo.extend(crate::bootstrap::CORE.iter().filter(|n| !installed.contains_key(**n)).map(|n| n.to_string()));
    if let Ok(idx) = crate::index::Index::open(&ctx.root.index_path()) {
        todo.extend(crate::shadows::shadow_packages_for(&idx, &tlpdb, &names, &|p| installed.contains_key(p)));
    }
    if let Ok(entries) = fs::read_dir(ctx.root.journal_dir()) {
        todo.extend(entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()));
    }
    let todo: Vec<&str> = todo.iter().map(String::as_str).filter(|n| tlpdb.get(n).is_some()).collect();
    let mut fixed: Vec<String> = Vec::new();
    if !todo.is_empty() {
        let r = install(ctx, &todo, Reason::Dependency)?;
        fixed.extend(r.installed.into_iter().map(|(n, _)| n));
    }
    let _ = fs::remove_dir_all(ctx.root.dir.join(".staging"));
    let mode = crate::bootstrap::HookMode::current(ctx)?;
    crate::bootstrap::install_hooks(&ctx.root, mode)?;
    lsr::rebuild(&ctx.root.texmf_dist())?;
    apply_regen(ctx, &tlpdb, Regen::all(), &[])?;
    crate::shims::sync(ctx, &tlpdb)?;
    // Roots made before `…/MennoTeX/current` existed get it here.
    if let Some(link) = crate::release::current_link(&ctx.root) {
        if link.symlink_metadata().is_err() {
            crate::release::point_current(&ctx.root)?;
        }
    }
    Ok(fixed)
}
