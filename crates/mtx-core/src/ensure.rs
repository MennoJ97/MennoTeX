//! `mtx ensure`: called by kpathsea (through the `mktextex`/`mktextfm`
//! hooks in Phase 0, through `ondemand.c` in Phase 1) when a file is
//! missing. Prints the path of the file after installing its package.
//!
//! The miss path must stay cheap: LaTeX probes many files that exist
//! nowhere (optional `.cfg` files, the document's own `.aux`), so a name
//! that is not in the index returns without opening the database or the
//! network.

use std::path::PathBuf;

use anyhow::Result;

use crate::ctx::{Ctx, Freshness};
use crate::db::Reason;
use crate::index::{Hit, Index, flags};
use crate::install;
use crate::repo::is_network_error;
use crate::root::Root;

/// A kpathsea file format, as far as on-demand installation cares.
#[derive(Debug, Clone, Copy)]
pub struct Kind {
    pub name: &'static str,
    /// Standard suffixes, then alternatives (kpathsea `suffix`/`alt_suffix`).
    pub suffixes: &'static [&'static str],
    /// Root-relative directories the format's search path covers.
    pub dirs: &'static [&'static str],
}

pub const KINDS: &[Kind] = &[
    Kind {
        name: "tex",
        suffixes: &[".tex", ".sty", ".cls", ".fd", ".aux", ".bbl", ".def", ".clo", ".ldf"],
        dirs: &["texmf-dist/tex/"],
    },
    Kind { name: "tfm", suffixes: &[".tfm"], dirs: &["texmf-dist/fonts/tfm/"] },
    Kind { name: "vf", suffixes: &[".vf"], dirs: &["texmf-dist/fonts/vf/"] },
    Kind { name: "type1 fonts", suffixes: &[".pfb", ".pfa"], dirs: &["texmf-dist/fonts/type1/"] },
    Kind { name: "opentype fonts", suffixes: &[".otf"], dirs: &["texmf-dist/fonts/opentype/"] },
    Kind { name: "truetype fonts", suffixes: &[".ttf", ".ttc", ".dfont"], dirs: &["texmf-dist/fonts/truetype/"] },
    Kind { name: "enc files", suffixes: &[".enc"], dirs: &["texmf-dist/fonts/enc/"] },
    Kind { name: "map", suffixes: &[".map"], dirs: &["texmf-dist/fonts/map/"] },
    Kind { name: "lua", suffixes: &[".lua", ".luatex", ".luc"], dirs: &["texmf-dist/tex/", "texmf-dist/scripts/"] },
    Kind { name: "bib", suffixes: &[".bib"], dirs: &["texmf-dist/bibtex/bib/"] },
    Kind { name: "bst", suffixes: &[".bst"], dirs: &["texmf-dist/bibtex/bst/"] },
    Kind { name: "mf", suffixes: &[".mf"], dirs: &["texmf-dist/fonts/source/", "texmf-dist/metafont/"] },
    Kind { name: "mp", suffixes: &[".mp"], dirs: &["texmf-dist/metapost/"] },
];

pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

/// Names to look up, in kpathsea's order: if `name` already has one of the
/// format's suffixes it is used as is, otherwise the standard suffix is
/// tried first.
pub fn candidates(kind: &Kind, name: &str) -> Vec<String> {
    if kind.suffixes.iter().any(|s| name.ends_with(s)) {
        vec![name.to_string()]
    } else {
        vec![format!("{name}{}", kind.suffixes[0]), name.to_string()]
    }
}

/// Lower is better. Phase 0 does not know the program name, so it prefers
/// LaTeX/generic locations and avoids `-dev` packages; Phase 1 matches
/// against kpathsea's real search path instead.
fn rank(idx: &Index, hit: &Hit) -> (u32, u32, u32) {
    let p = idx.package(hit.pkg);
    let dev = u32::from(p.flags & flags::IS_DEV != 0 || hit.dir.contains("-dev/"));
    let latexish = u32::from(!(hit.dir.starts_with("texmf-dist/tex/latex/") || hit.dir.starts_with("texmf-dist/tex/generic/")));
    (dev, latexish, p.container_size)
}

/// The best index entry for `name` of format `kind`, if any package has it.
pub fn resolve<'a>(idx: &'a Index, kind: &Kind, name: &str) -> Option<Hit<'a>> {
    let (subdir, base) = match name.rsplit_once('/') {
        Some((d, b)) => (Some(d), b),
        None => (None, name),
    };
    for cand in candidates(kind, base) {
        let mut hits: Vec<Hit> = idx
            .lookup(&cand)
            .into_iter()
            .filter(|h| kind.dirs.iter().any(|d| h.dir.starts_with(d) || format!("{}/", h.dir) == *d))
            .filter(|h| subdir.is_none_or(|s| h.dir.ends_with(&format!("/{s}"))))
            .collect();
        hits.sort_by_key(|h| rank(idx, h));
        if let Some(h) = hits.first() {
            return Some(*h);
        }
    }
    None
}

/// Why a file that a package provides was not made available. `mtx ensure`
/// exits with [`NotInstalled::exit_code`], and kpathsea's resolver
/// (`mtx-ondemand.c`) turns that into a warning in TeX's own log, where
/// editors look; the details are in `mtx.log`.
#[derive(Debug, thiserror::Error)]
pub enum NotInstalled {
    /// Refused by the `autoinstall` policy, `$MTX_AUTOINSTALL` or the user.
    #[error("package {0} was not installed: declined")]
    Declined(String),
    /// The download or the install failed (offline, no working mirror).
    #[error("package {0} could not be installed")]
    Failed(String),
}

impl NotInstalled {
    pub const DECLINED: u8 = 3;
    pub const FAILED: u8 = 4;

    pub fn exit_code(&self) -> u8 {
        match self {
            NotInstalled::Declined(_) => Self::DECLINED,
            NotInstalled::Failed(_) => Self::FAILED,
        }
    }
}

/// `present` if the file exists after all (its package's install was
/// interrupted, but files are renamed into place whole), otherwise `why`.
fn or_not_installed<T>(present: Option<T>, why: NotInstalled) -> Result<Option<T>> {
    match present {
        Some(p) => Ok(Some(p)),
        None => Err(why.into()),
    }
}

fn is_declined(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.downcast_ref::<crate::consent::Declined>().is_some())
}

fn autoinstall_enabled() -> bool {
    !matches!(std::env::var("MTX_AUTOINSTALL").as_deref(), Ok("0" | "no" | "false"))
}

/// Make the file `name` of format `kind` available; return its path, or
/// `None` if no package provides it or it cannot be installed now.
pub fn ensure(root: &Root, kind: &Kind, name: &str) -> Result<Option<PathBuf>> {
    if name.is_empty() || name.starts_with('/') || name.split('/').any(|c| c == "..") {
        return Ok(None);
    }
    let idx = Index::open(&root.index_path())?;
    let Some(hit) = resolve(&idx, kind, name) else { return Ok(None) };
    let rel = hit.path();
    let path = root.dir.join(&rel);
    let pkg = idx.package(hit.pkg).name.to_string();
    drop(idx);
    let redo = install::interrupted(root, &pkg);
    // Files are renamed into place whole, so one that exists is complete,
    // even when its package's install was interrupted.
    let present = path.exists().then(|| path.clone());
    if present.is_some() && !redo {
        return Ok(present); // installed, just not in this process's ls-R view
    }
    if !autoinstall_enabled() {
        return or_not_installed(present, NotInstalled::Declined(pkg));
    }

    let mut ctx = Ctx::open(root.clone())?;
    if ctx.offline()? {
        ctx.log(format!("error: cannot install {pkg} for {name}: the network was unreachable a moment ago (mtx retries after a minute)"));
        return or_not_installed(present, NotInstalled::Failed(pkg));
    }
    // A newer database may move the file to another package.
    let target = match ctx.refresh(false) {
        Ok(Freshness::Updated { .. }) => {
            let idx = Index::open(&root.index_path())?;
            resolve(&idx, kind, name).map(|h| (idx.package(h.pkg).name.to_string(), h.path()))
        }
        Ok(_) => Some((pkg, rel)),
        Err(e) if is_network_error(&e) => {
            ctx.log(format!("error: cannot install {pkg} for {name}: {e:#}"));
            return or_not_installed(present, NotInstalled::Failed(pkg));
        }
        Err(e) => return Err(e),
    };
    let Some((pkg, rel)) = target else { return Ok(None) };
    // Finishing an interrupted install needs no new consent.
    if redo {
        ctx.log(format!("{name}: finishing the interrupted install of {pkg}"));
    } else {
        ctx.log(format!("{name} → package {pkg}"));
        ctx.ask_for = Some(name.to_string());
    }
    if let Err(e) = install::install(&mut ctx, &[&pkg], Reason::Auto) {
        if is_declined(&e) {
            return or_not_installed(present, NotInstalled::Declined(pkg));
        }
        if is_network_error(&e) {
            ctx.log(format!("error: cannot install {pkg} for {name}: {e:#}"));
            return or_not_installed(present, NotInstalled::Failed(pkg));
        }
        return Err(e);
    }
    let path = root.dir.join(rel);
    Ok(path.exists().then_some(path))
}

/// Phase 1 entry point, used by the kpathsea patch (`mtx-ondemand.c`):
/// kpathsea has already chosen the package and the file with its real
/// search path. Install the package and return the file, followed (with
/// `siblings`) by every other `texmf-dist` file this call installed, so the
/// running program can add them all to its in-memory ls-R view.
pub fn ensure_path(root: &Root, pkg: &str, rel: &str, siblings: bool) -> Result<Option<Vec<PathBuf>>> {
    if rel.starts_with('/') || rel.split('/').any(|c| c == "..") || !rel.starts_with("texmf-dist/") {
        return Ok(None);
    }
    let file = rel.rsplit('/').next().unwrap_or(rel);
    install_for(root, pkg, rel, file, siblings)
}

/// Install `pkg` so that `rel` exists; `trigger` names what needed it (in
/// the log and the consent prompt). See [`ensure_path`].
fn install_for(root: &Root, pkg: &str, rel: &str, trigger: &str, siblings: bool) -> Result<Option<Vec<PathBuf>>> {
    let target = root.dir.join(rel);
    let redo = install::interrupted(root, pkg);
    // A file that exists is complete (see `ensure`).
    let present = target.exists().then(|| vec![target.clone()]);
    if present.is_some() && !redo {
        return Ok(present);
    }
    if !autoinstall_enabled() {
        return or_not_installed(present, NotInstalled::Declined(pkg.to_string()));
    }
    let mut ctx = Ctx::open(root.clone())?;
    if ctx.offline()? {
        ctx.log(format!("error: cannot install {pkg} for {rel}: the network was unreachable a moment ago (mtx retries after a minute)"));
        return or_not_installed(present, NotInstalled::Failed(pkg.to_string()));
    }
    if let Err(e) = ctx.refresh(false) {
        if is_network_error(&e) {
            ctx.log(format!("error: cannot install {pkg}: {e:#}"));
            return or_not_installed(present, NotInstalled::Failed(pkg.to_string()));
        }
        return Err(e);
    }
    // Finishing an interrupted install needs no new consent.
    if redo {
        ctx.log(format!("{trigger}: finishing the interrupted install of {pkg}"));
    } else {
        ctx.log(format!("{trigger} → package {pkg}"));
        ctx.ask_for = Some(trigger.to_string());
    }
    let report = match install::install(&mut ctx, &[pkg], Reason::Auto) {
        Ok(r) => r,
        Err(e) if is_declined(&e) => return or_not_installed(present, NotInstalled::Declined(pkg.to_string())),
        Err(e) if is_network_error(&e) => {
            ctx.log(format!("error: cannot install {pkg}: {e:#}"));
            return or_not_installed(present, NotInstalled::Failed(pkg.to_string()));
        }
        Err(e) => return Err(e),
    };
    if !target.exists() {
        return Ok(None);
    }
    let mut out = vec![target];
    if siblings {
        for (name, _) in &report.installed {
            for f in ctx.db.files_of(name)? {
                if f.starts_with("texmf-dist/") && f != rel {
                    out.push(root.dir.join(f));
                }
            }
        }
    }
    Ok(Some(out))
}

/// A TeX font that pdfTeX or LuaTeX found no map entry for: install the
/// package whose map covers it (which regenerates the maps) and return its
/// map file, followed (with `siblings`) by its other files, so the engine
/// finds the outlines. The engine then reads the map again (patch 0003).
/// Normally the font-map rule installed that package with the font's TFMs;
/// this repairs roots where it did not, during the compile.
pub fn ensure_font_map(root: &Root, font: &str, siblings: bool) -> Result<Option<Vec<PathBuf>>> {
    let ctx = Ctx::open(root.clone())?;
    let tlpdb = ctx.tlpdb()?;
    let installed = ctx.db.installed()?;
    let Some(pkg) = crate::fontmaps::map_package_for_font(&tlpdb, font, &|p| installed.contains_key(p)) else {
        return Ok(None);
    };
    let files = &tlpdb.get(&pkg).expect("map package is in the database").runfiles;
    let Some(rel) = files.iter().find(|f| f.ends_with(".map")).or(files.first()).cloned() else { return Ok(None) };
    drop(ctx);
    install_for(root, &pkg, &rel, &format!("{font} (font map)"), siblings)
}

/// A font requested by name (fontspec, XeTeX, luaotfload): install the
/// package shipping it and return the font file, followed (with `siblings`)
/// by the package's other font files, so the caller can make the whole
/// family available.
pub fn ensure_font_name(root: &Root, request: &str, siblings: bool) -> Result<Option<Vec<PathBuf>>> {
    let Some(font) = crate::fontnames::lookup(request) else { return Ok(None) };
    let target = root.dir.join(&font.path);
    if !target.exists() {
        let Some(_) = ensure_path(root, &font.package, &font.path, false)? else { return Ok(None) };
    }
    let mut out = vec![target];
    if siblings {
        let ctx = Ctx::open(root.clone())?;
        for f in ctx.db.files_of(&font.package)? {
            let is_font = ["otf", "ttf", "ttc"].iter().any(|e| f.to_ascii_lowercase().ends_with(&format!(".{e}")));
            if is_font && f != font.path {
                out.push(root.dir.join(f));
            }
        }
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index;
    use crate::tlpdb::Tlpdb;

    fn idx() -> (tempfile::TempDir, Index) {
        let db = Tlpdb::parse(
            "name amsmath\ncategory Package\nrevision 1\nrelocated 1\ncontainersize 10\nrunfiles size=1\n RELOC/tex/latex/amsmath/amsmath.sty\n\n\
             name latex-amsmath-dev\ncategory Package\nrevision 1\nrelocated 1\ncontainersize 1\nrunfiles size=1\n RELOC/tex/latex-dev/amsmath/amsmath.sty\n\n\
             name ctx\ncategory Package\nrevision 1\nrelocated 1\ncontainersize 1\nrunfiles size=1\n RELOC/tex/context/base/foo.tex\n RELOC/fonts/tfm/public/cm/cmr10.tfm\n\n\
             name lfoo\ncategory Package\nrevision 1\nrelocated 1\ncontainersize 100\nrunfiles size=1\n RELOC/tex/latex/foo/foo.tex\n",
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("files.idx");
        index::write_atomic(&p, &index::build(&db)).unwrap();
        let i = Index::open(&p).unwrap();
        (dir, i)
    }

    #[test]
    fn prefers_stable_latex_locations() {
        let (_d, i) = idx();
        let tex = kind("tex").unwrap();
        let h = resolve(&i, tex, "amsmath.sty").unwrap();
        assert_eq!(i.package(h.pkg).name, "amsmath");
        // \input foo → foo.tex; the LaTeX copy wins over ConTeXt despite size.
        let h = resolve(&i, tex, "foo").unwrap();
        assert_eq!(i.package(h.pkg).name, "lfoo");
        assert!(resolve(&i, tex, "missing.cfg").is_none());
    }

    #[test]
    fn format_directories_filter_hits() {
        let (_d, i) = idx();
        assert!(resolve(&i, kind("tex").unwrap(), "cmr10.tfm").is_none());
        let h = resolve(&i, kind("tfm").unwrap(), "cmr10").unwrap();
        assert_eq!(h.path(), "texmf-dist/fonts/tfm/public/cm/cmr10.tfm");
        assert!(resolve(&i, kind("tfm").unwrap(), "cm/cmr10.tfm").is_some());
        assert!(resolve(&i, kind("tfm").unwrap(), "lm/cmr10.tfm").is_none());
    }
}
