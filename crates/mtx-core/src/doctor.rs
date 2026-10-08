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

    // PATH: our bin directory, and nothing shadowing it. The root is a
    // real path; PATH may reach it through `…/MennoTeX/current`.
    let is_bin = |d: &Path| d == bin || fs::canonicalize(d).is_ok_and(|r| r == bin);
    let entry = path_env.split(':').map(Path::new).find(|d| is_bin(d));
    if let Some(entry) = entry {
        push(&mut out, Severity::Ok, format!("{} is on PATH", entry.display()));
        if let Some(link) = crate::release::current_link(root).filter(|l| l.exists()) {
            let via_link = link.join("bin").join(crate::tlpdb::ARCH);
            if entry != via_link {
                push(
                    &mut out,
                    Severity::Ok,
                    format!("PATH could use {} instead, so `mtx upgrade-release` needs no PATH change", via_link.display()),
                );
            }
        }
    } else {
        push(&mut out, Severity::Problem, format!("{} is not on PATH; kpathsea finds mtx's hooks through PATH", bin.display()));
    }
    for prog in PROGRAMS {
        let Some(found) = which(prog, path_env) else { continue };
        if !found.parent().is_some_and(is_bin) {
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

    // latexmk: an rc file (~/.latexmkrc) may point it at another TeX
    // installation's programs, mixing versions (biber must match biblatex).
    if root.texmf_dist().join("scripts/latexmk/latexmk.pl").exists() {
        let commands = std::process::Command::new(bin.join("latexmk"))
            .arg("-commands")
            .env("PATH", format!("{}:{path_env}", bin.display()))
            .output();
        if let Ok(commands) = commands {
            for (prog, path) in foreign_latexmk_programs(&String::from_utf8_lossy(&commands.stdout), &root.dir) {
                push(
                    &mut out,
                    Severity::Warning,
                    format!("latexmk runs {prog} from {path} (set in a latexmk rc file such as ~/.latexmkrc), not MennoTeX's"),
                );
            }
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
    let recent: Vec<_> = crate::logview::tail(root).into_iter().filter(|e| now.saturating_sub(e.at) < 86400).collect();
    let problems = recent.iter().filter(|e| e.is_problem()).count();
    if problems > 0 {
        push(&mut out, Severity::Warning, format!("{problems} failed or declined install(s) in the last 24 hours; see `mtx log --problems`"));
    }
    // Under `ask`, prompts that could not be shown (the fallback decided).
    let unasked: Vec<_> = recent.iter().filter_map(|e| crate::consent::unasked_reason(&e.msg)).collect();
    if let Some(why) = unasked.last() {
        push(
            &mut out,
            Severity::Warning,
            format!("{} install prompt(s) in the last 24 hours could not be shown; the last: {why}", unasked.len()),
        );
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

/// Programs in `latexmk -commands` output that run from an absolute path
/// outside the installation and the system directories: (program, path).
pub fn foreign_latexmk_programs(commands: &str, root: &Path) -> Vec<(String, String)> {
    const SYSTEM: &[&str] = &["/usr/bin/", "/bin/", "/usr/sbin/", "/sbin/"];
    let mut out = Vec::new();
    for line in commands.lines() {
        let Some(rest) = line.trim().strip_prefix("To run ") else { continue };
        let Some((prog, cmd)) = rest.split_once(", I use \"") else { continue };
        let cmd = cmd.strip_suffix('"').unwrap_or(cmd);
        // Quoted paths may contain spaces ("/Library/Application Support/…").
        let mut paths: Vec<&str> = cmd.split('"').skip(1).step_by(2).collect();
        paths.extend(cmd.split('"').step_by(2).flat_map(str::split_whitespace));
        if let Some(p) = paths.into_iter().find(|p| {
            p.starts_with('/') && !Path::new(p).starts_with(root) && !SYSTEM.iter().any(|s| p.starts_with(s))
        }) {
            out.push((prog.to_string(), p.to_string()));
        }
    }
    out
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

    #[test]
    fn latexmk_programs_from_other_installations() {
        let commands = "Commands used by latexmk:\n   To run pdflatex, I use \"pdflatex %O %S\"\n\
            \x20  To run biber, I use \"arch -arm64 \"/Library/Application Support/MiKTeX/biber\" %O %S\"\n\
            \x20  To run makeindex, I use \"/usr/bin/env makeindex %O %S\"\n\
            \x20  To run xelatex, I use \"/r/bin/universal-darwin/xelatex %O %S\"\n\
            \x20  To run bibtex, I use \"/usr/local/texlive/2025/bin/universal-darwin/bibtex %O %S\"\n";
        assert_eq!(
            foreign_latexmk_programs(commands, Path::new("/r")),
            vec![
                ("biber".to_string(), "/Library/Application Support/MiKTeX/biber".to_string()),
                ("bibtex".to_string(), "/usr/local/texlive/2025/bin/universal-darwin/bibtex".to_string()),
            ]
        );
    }
}
