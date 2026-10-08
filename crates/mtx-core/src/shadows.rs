//! Packages whose files would shadow files of the packages being installed.
//!
//! kpathsea asks MennoTeX only when a lookup misses. When two packages ship
//! a file of the same name in different `tex/` directories, a full TeX Live
//! finds the copy in the directory the engine searches first, but MennoTeX
//! finds whichever copy is installed and never misses: `pstricks.con` is in
//! `pstricks` (`tex/generic`) and in `xetex-pstricks` (`tex/xelatex`, which
//! XeLaTeX searches first), so XeLaTeX silently used another PSTricks driver
//! configuration (found by `tests/compare_texlive.py`, 2026-10-08). Like the
//! font-map rule ([`crate::fontmaps`]), installing a package therefore also
//! installs the packages with such higher-priority copies.

use std::collections::BTreeSet;

use crate::index::Index;
use crate::tlpdb::Tlpdb;

/// `tex/` subdirectories in kpathsea's search order for the LaTeX formats,
/// from TeX Live 2026's texmf.cnf (`TEXINPUTS.<format>`). The plain formats
/// search `generic` before `latex`, the other way round; including them
/// would install packages for plain TeX whenever a LaTeX package shares a
/// file name with a generic one.
const ORDERS: &[&[&str]] = &[
    &["latex", "generic"],                       // latex, pdflatex
    &["xelatex", "latex", "xetex", "generic"],   // xelatex
    &["lualatex", "latex", "luatex", "generic"], // lualatex
];

/// The directory under `texmf-dist/tex/` a file is in.
fn tex_top(path: &str) -> Option<&str> {
    path.strip_prefix("texmf-dist/tex/")?.split('/').next()
}

/// Whether some format finds a file in `tex/<over>` before one in `tex/<under>`.
fn searched_before(over: &str, under: &str) -> bool {
    ORDERS.iter().any(|order| {
        let pos = |d: &str| order.iter().position(|o| *o == d);
        matches!((pos(over), pos(under)), (Some(a), Some(b)) if a < b)
    })
}

/// Packages (not in `have`, not in `packages`) with a file that some format
/// finds before a same-named file of one of `packages`.
pub fn shadow_packages_for(idx: &Index, tlpdb: &Tlpdb, packages: &[String], have: &dyn Fn(&str) -> bool) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for name in packages {
        let Some(p) = tlpdb.get(name) else { continue };
        for f in &p.runfiles {
            let Some(under) = tex_top(f) else { continue };
            let base = f.rsplit('/').next().unwrap_or(f);
            if !base.contains('.') {
                continue; // README and the like are never input
            }
            for hit in idx.lookup(base) {
                let other = idx.package(hit.pkg).name;
                if other == name || have(other) || packages.iter().any(|n| n == other) || hit.basename != base {
                    continue;
                }
                let path = hit.path();
                if tex_top(&path).is_some_and(|over| searched_before(over, under)) && tlpdb.get(other).is_some() {
                    out.insert(other.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_orders() {
        assert!(searched_before("xelatex", "generic")); // pstricks.con
        assert!(searched_before("latex", "generic"));
        assert!(searched_before("lualatex", "luatex"));
        assert!(!searched_before("generic", "latex"));
        assert!(!searched_before("latex", "latex"));
        assert!(!searched_before("context", "generic"));
        assert_eq!(tex_top("texmf-dist/tex/xelatex/xetex-pstricks/pstricks.con"), Some("xelatex"));
        assert_eq!(tex_top("texmf-dist/fonts/tfm/x.tfm"), None);
    }
}
