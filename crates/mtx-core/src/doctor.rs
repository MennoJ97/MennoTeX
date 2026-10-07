//! `mtx doctor`: consistency and environment checks.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::bootstrap::HookMode;
use crate::ctx::Ctx;
use crate::db::now_secs;
use crate::index::Index;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Severity {
    Ok,
    Warning,
    Problem,
}

#[derive(Debug)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

fn push(out: &mut Vec<Finding>, severity: Severity, message: impl Into<String>) {
    out.push(Finding { severity, message: message.into() });
}

/// The first executable named `prog` on `path` (a `PATH`-style string).
pub fn which(prog: &str, path: &str) -> Option<PathBuf> {
    path.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join(prog)).find(|p| p.is_file())
}

/// Programs users run directly; if another TeX installation provides them
/// earlier on PATH, ours are shadowed.
const PROGRAMS: &[&str] = &["tex", "pdflatex", "xelatex", "lualatex", "kpsewhich", "latexmk"];

pub fn check(ctx: &Ctx, path_env: &str) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    let root = &ctx.root;
    let bin = root.bin_dir();

    // PATH: our bin directory, and nothing shadowing it.
    let on_path = path_env.split(':').any(|d| Path::new(d) == bin);
    if on_path {
        push(&mut out, Severity::Ok, format!("{} is on PATH", bin.display()));
    } else {
        push(&mut out, Severity::Problem, format!("{} is not on PATH; kpathsea finds mtx's hooks through PATH", bin.display()));
    }
    for prog in PROGRAMS {
        let Some(found) = which(prog, path_env) else { continue };
        if found.parent() != Some(bin.as_path()) {
            let target = fs::canonicalize(&found).unwrap_or(found.clone());
            let ours = bin.join(prog).exists();
            let sev = if ours { Severity::Problem } else { Severity::Warning };
            push(
                &mut out,
                sev,
                format!("`{prog}` resolves to {} ({}), not MennoTeX", found.display(), target.display()),
            );
        }
    }

    // Hook configuration matches the mode.
    let mode = HookMode::current(ctx)?;
    let cnf = fs::read_to_string(root.dir.join("texmf.cnf")).unwrap_or_default();
    let want = match mode {
        HookMode::Mktex => "MKTEXTEX = 1",
        HookMode::Kpathsea => "MKTEXTEX = 0",
    };
    if cnf.contains(want) {
        push(&mut out, Severity::Ok, format!("hook mode {} ({want})", mode.as_str()));
    } else {
        push(&mut out, Severity::Problem, format!("hook mode {} but texmf.cnf lacks `{want}`; run `mtx bootstrap`", mode.as_str()));
    }
    if let Some(build) = ctx.db.get("binaries_build")? {
        push(&mut out, Severity::Ok, format!("binaries: {build}"));
    }
    for hook in ["mktextex", "mktextfm"] {
        if fs::symlink_metadata(bin.join(hook)).is_err() {
            push(&mut out, Severity::Problem, format!("{hook} is missing from {}", bin.display()));
        }
    }

    // Index and database agree.
    match Index::open(&root.index_path()) {
        Ok(idx) => {
            let db_rev = ctx.db.get_u64("tlpdb_revision")?;
            if db_rev == Some(idx.tlpdb_revision) {
                push(&mut out, Severity::Ok, format!("index r{} with {} files", idx.tlpdb_revision, idx.file_count()));
            } else {
                push(&mut out, Severity::Problem, format!("index is r{}, database r{db_rev:?}; run `mtx refresh`", idx.tlpdb_revision));
            }
        }
        Err(e) => push(&mut out, Severity::Problem, format!("index unreadable: {e:#}")),
    }

    // Interrupted installs and leftovers.
    if let Ok(entries) = fs::read_dir(root.journal_dir()) {
        let names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        if !names.is_empty() {
            push(&mut out, Severity::Warning, format!("interrupted installs (redone on next use): {}", names.join(", ")));
        }
    }
    if let Ok(entries) = fs::read_dir(root.dir.join(".staging")) {
        let n = entries.count();
        if n > 0 {
            push(&mut out, Severity::Warning, format!("{n} leftover staging director{} in {}", if n == 1 { "y" } else { "ies" }, root.dir.join(".staging").display()));
        }
    }

    // Every recorded file exists.
    let installed = ctx.db.installed()?;
    let mut missing = Vec::new();
    let mut total = 0usize;
    for name in installed.keys() {
        for f in ctx.db.files_of(name)? {
            total += 1;
            if fs::symlink_metadata(root.dir.join(&f)).is_err() {
                missing.push(f);
            }
        }
    }
    if missing.is_empty() {
        push(&mut out, Severity::Ok, format!("{} packages, all {total} files present", installed.len()));
    } else {
        let sample: Vec<&str> = missing.iter().take(5).map(String::as_str).collect();
        push(&mut out, Severity::Problem, format!("{} of {total} installed files are missing, e.g. {}", missing.len(), sample.join(", ")));
    }

    // Fonts whose map package is missing (roots created before the
    // font-map rule): pdfTeX falls back to bitmaps and fails.
    if let Ok(tlpdb) = ctx.tlpdb() {
        let names: Vec<String> = installed.keys().cloned().collect();
        let missing_maps = crate::fontmaps::map_packages_for(&tlpdb, &names, &|p| installed.contains_key(p));
        if !missing_maps.is_empty() {
            let list: Vec<&str> = missing_maps.iter().map(String::as_str).collect();
            push(&mut out, Severity::Problem, format!("installed fonts lack their map packages: {} (run `mtx repair`)", list.join(", ")));
        }
    }

    // Release: tlnet may have moved on to the next TeX Live (ctx.rs pins
    // the frozen repository then).
    if let Some(newer) = ctx.db.get_u64("newer_release")? {
        let release = crate::root::RELEASE;
        push(
            &mut out,
            Severity::Warning,
            format!(
                "TeX Live {newer} is out; this installation stays on TeX Live {release} from its frozen final \
                 repository and gets no more package updates. To move, install MennoTeX for {newer} and run \
                 `mtx bootstrap --from {}` with it",
                root.dir.display()
            ),
        );
    }

    // Installs that failed or were declined recently: editors show only
    // TeX's log, which never contains mtx's messages.
    let now = now_secs();
    let problems = crate::logview::tail(root).into_iter().filter(|e| e.is_problem() && now.saturating_sub(e.at) < 86400).count();
    if problems > 0 {
        push(&mut out, Severity::Warning, format!("{problems} failed or declined install(s) in the last 24 hours; see `mtx log --problems`"));
    }
    if let Ok(p) = crate::consent::policy(ctx) {
        if p != crate::consent::Policy::Yes {
            push(&mut out, Severity::Ok, format!("autoinstall is {}", p.as_str()));
        }
    }

    // Network state.
    if ctx.offline()? {
        push(&mut out, Severity::Warning, "offline marker set (a network error happened in the last minute)");
    }
    for (host, at) in ctx.db.bad_mirrors()? {
        if now.saturating_sub(at) < crate::ctx::MIRROR_PIN_SECS {
            push(&mut out, Severity::Warning, format!("avoiding mirror {host} (failed verification or TLS {} min ago)", now.saturating_sub(at) / 60));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_follows_path_order() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fs::write(b.path().join("tex"), "").unwrap();
        fs::write(a.path().join("tex"), "").unwrap();
        let path = format!("{}:{}", b.path().display(), a.path().display());
        assert_eq!(which("tex", &path).unwrap(), b.path().join("tex"));
        assert!(which("nope", &path).is_none());
    }
}
