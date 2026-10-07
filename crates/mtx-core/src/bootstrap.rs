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

/// How missing files reach mtx.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    /// Phase 0: unmodified binaries; kpathsea runs `mktextex`/`mktextfm`
    /// (symlinks to mtx) for lookups with `must_exist`.
    Mktex,
    /// Phase 1: MennoTeX-built binaries whose kpathsea consults the index
    /// on every miss (`kpathsea-ondemand/mtx-ondemand.c`).
    Kpathsea,
}

impl HookMode {
    pub fn as_str(self) -> &'static str {
        match self {
            HookMode::Mktex => "mktex",
            HookMode::Kpathsea => "kpathsea",
        }
    }
    pub fn current(ctx: &Ctx) -> Result<HookMode> {
        Ok(match ctx.db.get("hook_mode")?.as_deref() {
            Some("kpathsea") => HookMode::Kpathsea,
            _ => HookMode::Mktex,
        })
    }
}

/// kpathsea overrides for the installation (read before texmf-dist's texmf.cnf).
pub fn root_texmf_cnf(mode: HookMode) -> String {
    let hook = match mode {
        HookMode::Mktex => "\
% Phase 0 on-demand installation: when an input file is missing, kpathsea
% runs `mktextex`, which asks mtx to install the package providing it.
MKTEXTEX = 1
",
        HookMode::Kpathsea => "\
% On-demand installation happens inside kpathsea (MennoTeX patch), for every
% missing file; the mktextex hook would only repeat that work.
MKTEXTEX = 0
",
    };
    format!(
        "\
% texmf.cnf for MennoTeX, written by mtx; rewritten on every bootstrap.
% Values here override texmf-dist/web2c/texmf.cnf.

% Personal packages live where MacTeX users expect them.
TEXMFHOME = ~/Library/texmf

% Keep all generated files inside the installation. TeX Live's scripts
% require the user and system trees to differ (TLUtils.pm refuses
% `mktexfmt` otherwise); mktexfmt writes formats to TEXMFSYSVAR anyway
% when it is writable, which it is for a per-user installation.
TEXMFVAR = $TEXMFROOT/texmf-user-var
TEXMFCONFIG = $TEXMFROOT/texmf-user-config

{hook}"
    )
}

fn write_executable(path: &Path, content: &[u8]) -> Result<()> {
    let tmp = path.with_extension("mtx-tmp");
    fs::write(&tmp, content)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Install the root texmf.cnf, a copy of the running mtx, and the kpathsea
/// hook links for `mode`.
pub fn install_hooks(root: &Root, mode: HookMode) -> Result<()> {
    let bin = root.bin_dir();
    fs::create_dir_all(&bin)?;
    fs::write(root.dir.join("texmf.cnf"), root_texmf_cnf(mode))?;
    let exe = std::env::current_exe().context("locating the mtx executable")?;
    let dest = bin.join("mtx");
    if fs::canonicalize(&exe).ok() != fs::canonicalize(&dest).ok() {
        write_executable(&dest, &fs::read(&exe)?)?;
    }
    // Phase 0 hooks are mtx itself, dispatched on the program name. With
    // the kpathsea patch, mktextfm goes back to TeX Live's METAFONT script
    // (kpathsea has already tried the index by then).
    let mktextfm = match mode {
        HookMode::Mktex => "mtx",
        HookMode::Kpathsea => "../../texmf-dist/scripts/texlive/mktextfm",
    };
    // mktexfmt is mtx in every mode: it serializes concurrent format builds
    // and installs the result atomically (see formats.rs).
    for (hook, target) in [("mktextex", "mtx"), ("mktextfm", mktextfm), ("mktexfmt", "mtx")] {
        let link = bin.join(hook);
        if link.symlink_metadata().is_ok() {
            fs::remove_file(&link)?;
        }
        std::os::unix::fs::symlink(target, &link)?;
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
    install_hooks(root, HookMode::current(&ctx)?)?;

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
    crate::shims::sync(&ctx, &tlpdb)?;
    let shims = fs::read_dir(root.bin_dir())?.flatten().filter(|e| crate::shims::is_shim(&e.path())).count();
    ctx.log(format!("{shims} command shims for programs installed on first use"));
    Ok(report)
}
