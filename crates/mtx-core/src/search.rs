//! `mtx search`: find packages by name and summary, or by the files they
//! ship (`--file`), like `tlmgr search`. It reads the package database, so
//! it also finds packages that are not installed.

use crate::tlpdb::{ARCH, Package, Tlpdb, arch_suffix};

/// Packages whose name or summary contains `text` (ignoring case): exact
/// name first, then names that contain it, then summaries, each by name.
/// Leaves out other platforms' binary packages.
pub fn packages<'a>(tlpdb: &'a Tlpdb, text: &str) -> Vec<&'a Package> {
    let text = text.to_lowercase();
    let mut hits: Vec<(u8, &Package)> = searchable(tlpdb)
        .filter_map(|p| {
            let name = p.name.to_lowercase();
            let rank = if name == text {
                0
            } else if name.contains(&text) {
                1
            } else if p.shortdesc.to_lowercase().contains(&text) {
                2
            } else {
                return None;
            };
            Some((rank, p))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    hits.into_iter().map(|(_, p)| p).collect()
}

/// Packages with a file whose root-relative path contains `text` (ignoring
/// case), with those files: run files, and programs for this platform.
pub fn files<'a>(tlpdb: &'a Tlpdb, text: &str) -> Vec<(&'a Package, Vec<&'a str>)> {
    let text = text.to_lowercase();
    searchable(tlpdb)
        .filter_map(|p| {
            let bins = p.binfiles.get(ARCH).into_iter().flatten();
            let found: Vec<&str> =
                p.runfiles.iter().chain(bins).map(String::as_str).filter(|f| f.to_lowercase().contains(&text)).collect();
            (!found.is_empty()).then_some((p, found))
        })
        .collect()
}

fn searchable(tlpdb: &Tlpdb) -> impl Iterator<Item = &Package> {
    tlpdb
        .packages
        .values()
        .filter(|p| !p.name.starts_with("00texlive") && arch_suffix(&p.name).is_none_or(|a| a == ARCH))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: &str = "\
name amsmath
category Package
revision 1
shortdesc AMS mathematical facilities for LaTeX
runfiles size=2
 RELOC/tex/latex/amsmath/amsmath.sty
 RELOC/tex/latex/amsmath/amsbsy.sty

name amsfonts
category Package
revision 1
shortdesc TeX fonts from the American Mathematical Society
runfiles size=1
 RELOC/fonts/tfm/public/amsfonts/symbols/msam10.tfm

name mathtools
category Package
revision 1
shortdesc Mathematical tools to use with amsmath

name biber.universal-darwin
category TLCore
revision 1
binfiles arch=universal-darwin size=1
 bin/universal-darwin/biber

name biber.x86_64-linux
category TLCore
revision 1
binfiles arch=x86_64-linux size=1
 bin/x86_64-linux/biber
";

    fn names(ps: Vec<&Package>) -> Vec<&str> {
        ps.into_iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn ranks_names_before_summaries() {
        let db = Tlpdb::parse(DB).unwrap();
        assert_eq!(names(packages(&db, "AMSMATH")), ["amsmath", "mathtools"]);
        assert_eq!(names(packages(&db, "ams")), ["amsfonts", "amsmath", "mathtools"]);
        assert_eq!(names(packages(&db, "biber")), ["biber.universal-darwin"]);
        assert!(packages(&db, "nothing like this").is_empty());
    }

    #[test]
    fn finds_files_and_this_platforms_programs() {
        let db = Tlpdb::parse(DB).unwrap();
        let hits = files(&db, "bsy");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.name, "amsmath");
        assert_eq!(hits[0].1, ["texmf-dist/tex/latex/amsmath/amsbsy.sty"]);
        let bins = files(&db, "bin/");
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].1, ["bin/universal-darwin/biber"]);
    }
}
