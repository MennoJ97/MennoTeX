//! Which package ships the OpenType/TrueType font with a given name.
//!
//! fontspec lets documents select fonts by name (`\setmainfont{TeX Gyre
//! Pagella}`); those requests reach mtx as names, not file names. The
//! table comes from `tools/build_fontname_index.py`, which reads the `name`
//! tables of every OpenType/TrueType font in tlnet.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

const TABLE_XZ: &[u8] = include_bytes!("../data/fontnames.tsv.xz");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFile {
    pub package: String,
    /// Root-relative path of the font file.
    pub path: String,
}

/// normalized name → (kind, file); kinds are `full`, `ps`, `family`.
type Table = HashMap<String, Vec<(String, FontFile)>>;

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut text = String::new();
        if liblzma::read::XzDecoder::new(TABLE_XZ).read_to_string(&mut text).is_err() {
            return HashMap::new();
        }
        parse(&text)
    })
}

fn parse(text: &str) -> Table {
    let mut t: Table = HashMap::new();
    for line in text.lines() {
        let mut f = line.split('\t');
        if let (Some(name), Some(kind), Some(package), Some(path)) = (f.next(), f.next(), f.next(), f.next()) {
            t.entry(name.to_string())
                .or_default()
                .push((kind.to_string(), FontFile { package: package.into(), path: path.into() }));
        }
    }
    t
}

/// Lowercase, letters and digits only; a trailing font-file or metric
/// extension and fontspec/XeTeX feature suffixes (`/B`, `:mapping=…`) are
/// dropped first.
pub fn normalize(request: &str) -> String {
    let mut name = request.split([':', '/']).next().unwrap_or("").trim();
    for ext in [".tfm", ".ofm", ".otf", ".ttf", ".ttc"] {
        if name.len() > ext.len() && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            name = &name[..name.len() - ext.len()];
        }
    }
    name.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// The font file for a requested name: a full name or PostScript name
/// selects that face; a family name selects the family's regular face.
pub fn lookup(request: &str) -> Option<FontFile> {
    lookup_in(table(), request)
}

fn lookup_in(table: &Table, request: &str) -> Option<FontFile> {
    let entries = table.get(&normalize(request))?;
    ["full", "ps", "family"]
        .iter()
        .find_map(|kind| entries.iter().find(|(k, _)| k == kind).map(|(_, f)| f.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_drops_styles_and_extensions() {
        assert_eq!(normalize("TeX Gyre Pagella"), "texgyrepagella");
        assert_eq!(normalize("TeX Gyre Pagella.tfm"), "texgyrepagella");
        assert_eq!(normalize("TeX Gyre Pagella/B:mapping=tex-text"), "texgyrepagella");
        assert_eq!(normalize("TeXGyrePagella-Bold"), "texgyrepagellabold");
    }

    #[test]
    fn embedded_table_resolves_families_and_faces() {
        let regular = lookup("TeX Gyre Pagella").unwrap();
        assert_eq!(regular.package, "tex-gyre");
        assert!(regular.path.ends_with("texgyrepagella-regular.otf"));
        assert!(lookup("TeX Gyre Pagella Bold").unwrap().path.ends_with("texgyrepagella-bold.otf"));
        assert!(lookup("TeXGyrePagella-Bold").unwrap().path.ends_with("texgyrepagella-bold.otf"));
        assert_eq!(lookup("Libertinus Serif").unwrap().package, "libertinus-fonts");
        assert!(lookup("No Such Font Anywhere").is_none());
    }

    #[test]
    fn full_name_beats_family() {
        let t = parse("x\tfamily\tp1\ta.otf\nx\tfull\tp2\tb.otf\n");
        assert_eq!(lookup_in(&t, "X").unwrap().package, "p2");
    }
}
