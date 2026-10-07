//! Which package's font map covers a TeX font.
//!
//! pdfTeX and dvipdfmx find outline fonts through map files (`ecrm1000` →
//! `sfrm1000.pfb` in cm-super's map), not through a file lookup that names
//! the font's package. So installing a package with TFMs also installs the
//! packages whose maps cover those fonts. The table comes from
//! `tools/build_fontmap_index.py` (font maps of all of tlnet, 2026-10).

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::sync::OnceLock;

use crate::tlpdb::Tlpdb;

const TABLE_XZ: &[u8] = include_bytes!("../data/fontmaps.tsv.xz");

fn table() -> &'static HashMap<String, Vec<String>> {
    static TABLE: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut text = String::new();
        if liblzma::read::XzDecoder::new(TABLE_XZ).read_to_string(&mut text).is_err() {
            return HashMap::new();
        }
        parse(&text)
    })
}

fn parse(text: &str) -> HashMap<String, Vec<String>> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(font, pkgs)| (font.to_string(), pkgs.split(',').map(String::from).collect()))
        .collect()
}

/// Packages (not in `have`) whose maps cover TFMs shipped by `packages`.
pub fn map_packages_for(tlpdb: &Tlpdb, packages: &[String], have: &dyn Fn(&str) -> bool) -> BTreeSet<String> {
    map_packages_with(table(), tlpdb, packages, have)
}

fn map_packages_with(
    table: &HashMap<String, Vec<String>>,
    tlpdb: &Tlpdb,
    packages: &[String],
    have: &dyn Fn(&str) -> bool,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for name in packages {
        let Some(p) = tlpdb.get(name) else { continue };
        for f in &p.runfiles {
            let Some(font) = f.rsplit('/').next().and_then(|b| b.strip_suffix(".tfm")) else { continue };
            let Some(providers) = table.get(font) else { continue };
            // A package that maps its own fonts needs nothing else.
            if providers.iter().any(|m| m == name || have(m) || out.contains(m)) {
                continue;
            }
            if let Some(m) = providers.iter().find(|m| tlpdb.get(m).is_some()) {
                out.insert(m.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_table_knows_cm_super() {
        assert_eq!(table().get("ecrm1000").map(|v| v.as_slice()), Some(&["cm-super".to_string()][..]));
        assert!(table().len() > 40_000);
    }

    #[test]
    fn tfm_packages_pull_in_their_maps() {
        let db = Tlpdb::parse(
            "name ec\ncategory Package\nrevision 1\nrelocated 1\nrunfiles size=1\n RELOC/fonts/tfm/jknappen/ec/ecrm1000.tfm\n RELOC/fonts/tfm/jknappen/ec/ecrm1200.tfm\n\n\
             name cm-super\ncategory Package\nrevision 1\n\n\
             name lm\ncategory Package\nrevision 1\nrelocated 1\nrunfiles size=1\n RELOC/fonts/tfm/public/lm/rm-lmr10.tfm\n",
        )
        .unwrap();
        let table = parse("ecrm1000\tcm-super\necrm1200\tcm-super\nrm-lmr10\tlm\n");
        let none = |_: &str| false;
        let got = map_packages_with(&table, &db, &["ec".into(), "lm".into()], &none);
        assert_eq!(got.into_iter().collect::<Vec<_>>(), vec!["cm-super"]); // lm maps itself
        let have = |p: &str| p == "cm-super";
        assert!(map_packages_with(&table, &db, &["ec".into()], &have).is_empty());
    }
}
