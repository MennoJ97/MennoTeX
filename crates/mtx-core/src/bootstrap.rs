//! `mtx bootstrap`: create a minimal installation that can install
//! everything else on demand.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};

use crate::configfiles::Regen;
use crate::ctx::Ctx;
use crate::db::Reason;
use crate::install;
use crate::lsr;
use crate::root::Root;

/// The core set (PLAN.md §5.10). Hyphenation packages are added on top,
/// because pdfTeX/XeTeX formats bake patterns in at build time.
/// `amsfonts` carries the Type 1 Computer Modern fonts and their maps:
/// with the Phase 0 hooks, pdfTeX's font-file lookups cannot trigger
/// installs, so the default fonts must be present up front.
pub const CORE: &[&str] = &["scheme-infraonly", "latex-bin", "amsfonts", "hyphen-base"];

/// kpathsea overrides for the installation (read before texmf-dist's texmf.cnf).
pub const ROOT_TEXMF_CNF: &str = "\
% texmf.cnf for MennoTeX, written by `mtx bootstrap`; rewritten on every bootstrap.
% Values here override texmf-dist/web2c/texmf.cnf.

% Personal packages live where MacTeX users expect them.
TEXMFHOME = ~/Library/texmf

% Keep all generated files inside the installation. TeX Live's scripts
% require the user and system trees to differ (TLUtils.pm refuses
% `mktexfmt` otherwise); mktexfmt writes formats to TEXMFSYSVAR anyway
% when it is writable, which it is for a per-user installation.
TEXMFVAR = $TEXMFROOT/texmf-user-var
TEXMFCONFIG = $TEXMFROOT/texmf-user-config

% Phase 0 on-demand installation: when an input file is missing, kpathsea
% runs `mktextex`, which asks mtx to install the package providing it.
MKTEXTEX = 1
";

pub const MKTEXTEX: &str = r#"#!/bin/sh
# MennoTeX: kpathsea runs this when a TeX input file is missing (MKTEXTEX=1).
# mtx installs the package that provides it and prints the file's path.
exec "$(dirname "$0")/mtx" ensure --format tex -- "$1"
"#;

pub const MKTEXTFM: &str = r#"#!/bin/sh
# MennoTeX: kpathsea runs this when a TFM file is missing. First try to
# install the package that ships it; otherwise fall back to TeX Live's
# mktextfm, which generates the font with METAFONT.
d=$(dirname "$0")
"$d/mtx" ensure --format tfm -- "$1" && exit 0
exec "$d/../../texmf-dist/scripts/texlive/mktextfm" "$@"
"#;

fn write_executable(path: &Path, content: &[u8]) -> Result<()> {
    let tmp = path.with_extension("mtx-tmp");
    fs::write(&tmp, content)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Install the hook scripts, the root texmf.cnf, and a copy of the running
/// mtx into the installation.
pub fn install_hooks(root: &Root) -> Result<()> {
    let bin = root.bin_dir();
    fs::create_dir_all(&bin)?;
    fs::write(root.dir.join("texmf.cnf"), ROOT_TEXMF_CNF)?;
    write_executable(&bin.join("mktextex"), MKTEXTEX.as_bytes())?;
    write_executable(&bin.join("mktextfm"), MKTEXTFM.as_bytes())?;
    let exe = std::env::current_exe().context("locating the mtx executable")?;
    let dest = bin.join("mtx");
    if fs::canonicalize(&exe).ok() != fs::canonicalize(&dest).ok() {
        write_executable(&dest, &fs::read(&exe)?)?;
    }
    Ok(())
}

/// Create or repair the installation at `root`.
pub fn bootstrap(root: &Root, repository: Option<&str>) -> Result<install::Report> {
    fs::create_dir_all(&root.dir).with_context(|| format!("creating {}", root.dir.display()))?;
    let mut ctx = Ctx::open(root.clone())?;
    if let Some(r) = repository {
        ctx.db.set("repository", r)?;
        ctx.unpin_mirror()?;
    }
    ctx.refresh(true)?;
    let tlpdb = ctx.tlpdb()?;
    // Hooks and texmf.cnf first: updmap and fmtutil read texmf.cnf.
    install_hooks(root)?;

    let hyphen: Vec<&str> =
        tlpdb.content_packages().filter(|p| p.executes_of("AddHyphen").next().is_some()).map(|p| p.name.as_str()).collect();
    // Package names change between releases (l3backend merged into
    // l3kernel in 2026); skip core names the repository no longer has.
    let core = CORE.iter().copied().filter(|n| {
        let known = tlpdb.get(n).is_some();
        if !known {
            ctx.log(format!("warning: core package {n} is not in this repository; skipping"));
        }
        known
    });
    let roots: Vec<&str> = core.chain(hyphen).collect();
    let report = install::install(&mut ctx, &roots, Reason::Bootstrap)?;

    lsr::rebuild(&root.texmf_dist())?;
    let tlpdb = ctx.tlpdb()?;
    install::apply_regen(&ctx, &tlpdb, Regen::all(), &[])?;
    Ok(report)
}
