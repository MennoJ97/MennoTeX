//! `mtx bootstrap`: create a minimal installation that can install
//! everything else on demand.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::configfiles::Regen;
use crate::ctx::Ctx;
use crate::db::{Db, Reason};
use crate::install;
use crate::lsr;
use crate::root::Root;
use crate::tlpdb::Tlpdb;

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

% mtx's overlay tree, searched before all others. It is searched on disk:
% kpathsea loads ls-R only for the trees in TEXMFDBS.
TEXMFAUXTREES = $TEXMFROOT/texmf-mtx,

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

/// Files of the overlay tree, `<root>/texmf-mtx`: (path in the tree, content).
const OVERLAY: &[(&str, &str)] =
    &[("tex/luatex/mtx/luaotfload-main.lua", include_str!("../data/luaotfload-main.lua"))];

/// Write mtx's overlay tree, which TEXMFAUXTREES puts ahead of every other
/// tree. It holds a `luaotfload-main.lua` that makes LuaLaTeX's first run
/// work with fonts selected by name (see the file). No ls-R: kpathsea only
/// reads those for TEXMFDBS trees, and this one is searched on disk.
pub fn install_overlay(root: &Root) -> Result<()> {
    let tree = root.texmf_overlay();
    for (rel, content) in OVERLAY {
        let path = tree.join(rel);
        fs::create_dir_all(path.parent().expect("overlay files are in directories"))?;
        if fs::read(&path).ok().as_deref() != Some(content.as_bytes()) {
            let tmp = path.with_extension("mtx-tmp");
            fs::write(&tmp, content)?;
            fs::rename(&tmp, &path)?;
        }
    }
    // An ls-R written by earlier versions would be ignored; remove it.
    let _ = fs::remove_file(tree.join("ls-R"));
    Ok(())
}

/// Install the root texmf.cnf, the overlay tree, a copy of the running mtx,
/// and the kpathsea hook links for `mode`.
pub fn install_hooks(root: &Root, mode: HookMode) -> Result<()> {
    let bin = root.bin_dir();
    fs::create_dir_all(&bin)?;
    install_overlay(root)?;
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

/// Packages to carry over from another installation, typically the
/// previous release's root (`mtx bootstrap --from`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Carried {
    /// Asked for by name (`mtx install`): stay explicit.
    pub explicit: Vec<String>,
    /// Installed on demand: stay auto.
    pub auto: Vec<String>,
    /// Not in this repository (renamed or dropped between releases).
    pub unknown: Vec<String>,
}

/// What `other` has installed, minus bootstrap packages and dependencies:
/// this bootstrap and the dependency closures bring their own, and both can
/// change between releases.
pub fn carried_packages(tlpdb: &Tlpdb, other: &Root) -> Result<Carried> {
    if !other.is_bootstrapped() {
        bail!("{} is not a MennoTeX installation", other.dir.display());
    }
    let mut c = Carried::default();
    for (name, i) in Db::open(&other.db_path())?.installed()? {
        let list = match i.reason.as_str() {
            "explicit" => &mut c.explicit,
            "auto" => &mut c.auto,
            _ => continue,
        };
        if tlpdb.get(&name).is_some() {
            list.push(name);
        } else {
            c.unknown.push(name);
        }
    }
    Ok(c)
}

/// Install what [`carried_packages`] finds in `other`.
fn carry_over(ctx: &mut Ctx, tlpdb: &Tlpdb, other: &Root, report: &mut install::Report) -> Result<()> {
    let c = carried_packages(tlpdb, other)?;
    ctx.log(format!(
        "carrying over {} requested and {} on-demand package(s) from {}",
        c.explicit.len(),
        c.auto.len(),
        other.dir.display()
    ));
    if !c.unknown.is_empty() {
        ctx.log(format!("not in this release, skipped: {}", c.unknown.join(", ")));
    }
    for (names, reason) in [(&c.explicit, Reason::Explicit), (&c.auto, Reason::Auto)] {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let r = install::install(ctx, &names, reason)?;
        report.installed.extend(r.installed);
        report.files += r.files;
        report.bytes_downloaded += r.bytes_downloaded;
    }
    Ok(())
}

/// Create or repair the installation at `root`; with `from`, also install
/// the packages another installation has (see [`carried_packages`]).
pub fn bootstrap(root: &Root, repository: Option<&str>, from: Option<&Root>) -> Result<install::Report> {
    if let Some(other) = from {
        if fs::canonicalize(&other.dir).ok() == fs::canonicalize(&root.dir).ok() {
            bail!("--from must name another installation than {}", root.dir.display());
        }
    }
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
    let mut report = install::install(&mut ctx, &roots, Reason::Bootstrap)?;
    if let Some(other) = from {
        carry_over(&mut ctx, &tlpdb, other, &mut report)?;
    }

    lsr::rebuild(&root.texmf_dist())?;
    let tlpdb = ctx.tlpdb()?;
    install::apply_regen(&ctx, &tlpdb, Regen::all(), &[])?;
    crate::shims::sync(&ctx, &tlpdb)?;
    let shims = fs::read_dir(root.bin_dir())?.flatten().filter(|e| crate::shims::is_shim(&e.path())).count();
    ctx.log(format!("{shims} command shims for programs installed on first use"));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_tree_is_searched_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path());
        install_overlay(&root).unwrap();
        let lua = fs::read_to_string(root.texmf_overlay().join("tex/luatex/mtx/luaotfload-main.lua")).unwrap();
        assert!(lua.contains("require'luaotfload'"));
        // Idempotent.
        install_overlay(&root).unwrap();
        for mode in [HookMode::Mktex, HookMode::Kpathsea] {
            assert!(root_texmf_cnf(mode).contains("TEXMFAUXTREES = $TEXMFROOT/texmf-mtx,\n"));
        }
    }
}
