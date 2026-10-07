//! Generated configuration files, written the way `tlmgr` writes them
//! (TeX Live `TLUtils.pm` `create_*`, `TLPOBJ.pm` `*_lines`):
//!
//! | File | Built from | Location |
//! |---|---|---|
//! | `fmtutil.cnf` | `AddFormat` | `texmf-dist/web2c/` |
//! | `updmap.cfg` | `addMap`/`addMixedMap`/`addKanjiMap` | `texmf-dist/web2c/` |
//! | `language.dat`, `.def`, `.dat.lua` | `AddHyphen` | `texmf-var/tex/generic/config/` |
//!
//! Each starts with a header file shipped by TeX Live and lists only
//! installed packages, sorted by name, like `tlmgr`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::tlpdb::Package;

/// Split like Perl's `quotewords('\s+', 0, ...)`: whitespace-separated,
/// double quotes group words and are removed.
pub fn quotewords(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut have = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                have = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if have {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            c => {
                cur.push(c);
                have = true;
            }
        }
    }
    if have {
        out.push(cur);
    }
    out
}

fn key_values(s: &str) -> BTreeMap<String, String> {
    quotewords(s)
        .into_iter()
        .filter_map(|w| w.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// One `AddFormat` directive.
#[derive(Debug, Clone)]
pub struct Format {
    pub name: String,
    pub engine: String,
    pub patterns: String,
    pub options: String,
    pub enabled: bool,
    pub fmttriggers: Vec<String>,
}

pub fn parse_add_format(s: &str) -> Option<Format> {
    let kv = key_values(s);
    Some(Format {
        name: kv.get("name")?.clone(),
        engine: kv.get("engine")?.clone(),
        patterns: kv.get("patterns").filter(|p| !p.is_empty()).cloned().unwrap_or_else(|| "-".into()),
        options: kv.get("options").cloned().unwrap_or_default(),
        enabled: kv.get("mode").map(String::as_str) != Some("disabled"),
        fmttriggers: kv.get("fmttriggers").map(|t| t.split(',').map(String::from).collect()).unwrap_or_default(),
    })
}

/// One `AddHyphen` directive.
#[derive(Debug, Clone, Default)]
pub struct Hyphen {
    pub name: String,
    pub lefthyphenmin: String,
    pub righthyphenmin: String,
    pub file: String,
    pub file_patterns: Option<String>,
    pub file_exceptions: Option<String>,
    pub luaspecial: Option<String>,
    pub synonyms: Vec<String>,
    pub databases: Vec<String>,
    pub comment: Option<String>,
}

pub fn parse_add_hyphen(s: &str) -> Option<Hyphen> {
    let kv = key_values(s);
    let list = |k: &str| -> Vec<String> {
        kv.get(k).map(|v| v.split(',').filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default()
    };
    let databases = match list("databases") {
        d if d.is_empty() => vec!["dat".into(), "def".into(), "lua".into()],
        d => d,
    };
    Some(Hyphen {
        name: kv.get("name")?.clone(),
        lefthyphenmin: kv.get("lefthyphenmin")?.clone(),
        righthyphenmin: kv.get("righthyphenmin")?.clone(),
        file: kv.get("file")?.clone(),
        file_patterns: kv.get("file_patterns").cloned(),
        file_exceptions: kv.get("file_exceptions").cloned(),
        luaspecial: kv.get("luaspecial").cloned(),
        synonyms: list("synonyms"),
        databases,
        comment: kv.get("comment").cloned(),
    })
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

/// `fmtutil.cnf` lines contributed by one package.
pub fn fmtutil_lines(p: &Package) -> Vec<String> {
    let mut out = Vec::new();
    for f in p.executes_of("AddFormat").filter_map(parse_add_format) {
        if out.is_empty() {
            out.push(format!("#\n# from {}:\n", p.name));
        }
        let mode = if f.enabled { "" } else { "#! " };
        out.push(format!("{mode}{} {} {} {}\n", f.name, f.engine, f.patterns, f.options));
    }
    out
}

/// `updmap.cfg` lines contributed by one package (sorted by map name).
pub fn updmap_lines(p: &Package) -> Vec<String> {
    let mut maps = BTreeMap::new();
    for (kind, word) in [("addMap", "Map"), ("addMixedMap", "MixedMap"), ("addKanjiMap", "KanjiMap")] {
        for m in p.executes_of(kind) {
            maps.insert(m.to_string(), word);
        }
    }
    maps.into_iter().map(|(m, w)| format!("{w} {m}\n")).collect()
}

/// Which hyphenation database a set of lines is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LangDb {
    Dat,
    Def,
    Lua,
}

/// `language.dat` / `language.def` / `language.dat.lua` lines of one package.
pub fn language_lines(p: &Package, db: LangDb) -> Vec<String> {
    let (key, cc) = match db {
        LangDb::Dat => ("dat", "%"),
        LangDb::Def => ("def", "%"),
        LangDb::Lua => ("lua", "--"),
    };
    let mut out = Vec::new();
    for h in p.executes_of("AddHyphen").filter_map(parse_add_hyphen) {
        if !h.databases.iter().any(|d| d == key) {
            continue;
        }
        if out.is_empty() {
            out.push(format!("{cc} from {}:\n", p.name));
        }
        if let Some(c) = h.comment.as_deref().filter(|c| !c.is_empty()) {
            out.push(format!("{cc} {c}\n"));
        }
        let (name, file, l, r) = (&h.name, &h.file, &h.lefthyphenmin, &h.righthyphenmin);
        match db {
            LangDb::Dat => {
                out.push(format!("{name} {file}\n"));
                out.extend(h.synonyms.iter().map(|s| format!("={s}\n")));
            }
            LangDb::Def => {
                for n in std::iter::once(name).chain(h.synonyms.iter()) {
                    out.push(format!("\\addlanguage{{{n}}}{{{file}}}{{}}{{{l}}}{{{r}}}\n"));
                }
            }
            LangDb::Lua => {
                let syn: Vec<String> = h.synonyms.iter().map(|s| format!("'{s}'")).collect();
                let mut lines = vec![
                    format!("['{name}'] = {{"),
                    format!("\tloader = '{file}',"),
                    format!("\tlefthyphenmin = {l},"),
                    format!("\trighthyphenmin = {r},"),
                    format!("\tsynonyms = {{ {} }},", syn.join(", ")),
                ];
                if let Some(v) = non_empty(&h.file_patterns) {
                    lines.push(format!("\tpatterns = '{v}',"));
                }
                if let Some(v) = non_empty(&h.file_exceptions) {
                    lines.push(format!("\thyphenation = '{v}',"));
                }
                if let Some(v) = non_empty(&h.luaspecial) {
                    lines.push(format!("\tspecial = '{v}',"));
                }
                lines.push("},".into());
                out.extend(lines.into_iter().map(|l| format!("\t{l}\n")));
            }
        }
    }
    out
}

/// Which generated files to rewrite.
#[derive(Debug, Default, Clone, Copy)]
pub struct Regen {
    pub formats: bool,
    pub maps: bool,
    pub hyphen: bool,
}

impl Regen {
    pub fn all() -> Regen {
        Regen { formats: true, maps: true, hyphen: true }
    }
    pub fn any(&self) -> bool {
        self.formats || self.maps || self.hyphen
    }
    pub fn merge(&mut self, o: Regen) {
        self.formats |= o.formats;
        self.maps |= o.maps;
        self.hyphen |= o.hyphen;
    }
    /// What has to be regenerated after installing or removing `p`.
    /// The packages shipping the headers count too.
    pub fn for_package(p: &Package) -> Regen {
        Regen {
            formats: p.executes_of("AddFormat").next().is_some() || p.name == "kpathsea" || p.name == "texlive.infra",
            maps: ["addMap", "addMixedMap", "addKanjiMap"].iter().any(|k| p.executes_of(k).next().is_some())
                || p.name == "texlive-scripts"
                || p.name == "texlive.infra",
            hyphen: p.executes_of("AddHyphen").next().is_some() || p.name == "hyphen-base",
        }
    }
}

struct Spec {
    header: &'static str,
    dest: &'static str,
    comment: &'static str,
    keep_first_line: bool,
    footer: &'static [&'static str],
}

const FMTUTIL: Spec = Spec {
    header: "texmf-dist/web2c/fmtutil-hdr.cnf",
    dest: "texmf-dist/web2c/fmtutil.cnf",
    comment: "#",
    keep_first_line: false,
    footer: &[],
};
const UPDMAP: Spec = Spec {
    header: "texmf-dist/web2c/updmap-hdr.cfg",
    dest: "texmf-dist/web2c/updmap.cfg",
    comment: "#",
    keep_first_line: false,
    footer: &[],
};
const LANG_DAT: Spec = Spec {
    header: "texmf-dist/tex/generic/config/language.us",
    dest: "texmf-var/tex/generic/config/language.dat",
    comment: "%",
    keep_first_line: false,
    footer: &[],
};
const LANG_DEF: Spec = Spec {
    header: "texmf-dist/tex/generic/config/language.us.def",
    dest: "texmf-var/tex/generic/config/language.def",
    comment: "%",
    keep_first_line: true,
    footer: &[
        "%%% No changes may be made beyond this point.\n",
        "\n",
        "\\uselanguage {USenglish}             %%% This MUST be the last line of the file.\n",
    ],
};
const LANG_LUA: Spec = Spec {
    header: "texmf-dist/tex/generic/config/language.us.lua",
    dest: "texmf-var/tex/generic/config/language.dat.lua",
    comment: "--",
    keep_first_line: false,
    footer: &["}\n"],
};

fn write_one(root: &Path, spec: &Spec, lines: Vec<String>) -> Result<Option<String>> {
    let header_path = root.join(spec.header);
    let Ok(header) = fs::read_to_string(&header_path) else {
        // The header ships with texlive.infra / hyphen-base; not installed yet.
        return Ok(None);
    };
    let mut out = String::new();
    if !spec.keep_first_line {
        out.push_str(&format!("{} Generated by mtx (MennoTeX); manual edits are lost on the next install.\n", spec.comment));
    }
    out.push_str(&header);
    lines.iter().for_each(|l| out.push_str(l));
    spec.footer.iter().for_each(|l| out.push_str(l));
    let dest = root.join(spec.dest);
    fs::create_dir_all(dest.parent().unwrap())?;
    let tmp = dest.with_extension("mtx-tmp");
    fs::write(&tmp, out).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &dest)?;
    Ok(Some(spec.dest.to_string()))
}

/// Rewrite the selected files for the `installed` packages (any order).
/// Returns the root-relative paths written.
pub fn write(root: &Path, installed: &[&Package], what: Regen) -> Result<Vec<String>> {
    let mut pkgs: Vec<&Package> = installed.to_vec();
    pkgs.sort_by(|a, b| a.name.cmp(&b.name));
    let collect = |f: &dyn Fn(&Package) -> Vec<String>| pkgs.iter().flat_map(|p| f(p)).collect::<Vec<_>>();
    let mut written = Vec::new();
    let mut emit = |spec: &Spec, lines: Vec<String>| -> Result<()> {
        if let Some(p) = write_one(root, spec, lines)? {
            written.push(p);
        }
        Ok(())
    };
    if what.formats {
        emit(&FMTUTIL, collect(&fmtutil_lines))?;
    }
    if what.maps {
        emit(&UPDMAP, collect(&updmap_lines))?;
    }
    if what.hyphen {
        emit(&LANG_DAT, collect(&|p| language_lines(p, LangDb::Dat)))?;
        emit(&LANG_DEF, collect(&|p| language_lines(p, LangDb::Def)))?;
        emit(&LANG_LUA, collect(&|p| language_lines(p, LangDb::Lua)))?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tlpdb::Tlpdb;

    const DB: &str = "\
name latex-bin
category TLCore
revision 1
execute AddFormat name=pdflatex engine=pdftex           patterns=language.dat           options=\"-translate-file=cp227.tcx *pdflatex.ini\"           fmttriggers=l3kernel,latex
execute AddFormat name=old engine=pdftex mode=disabled options=\"old.ini\"

name hyphen-german
category TLCore
revision 2
execute AddHyphen name=german synonyms= lefthyphenmin=2 righthyphenmin=2 file=loadhyph-de-1901.tex file_patterns=hyph-de-1901.pat.txt file_exceptions= comment=\"old spelling\"
execute AddHyphen name=ngerman synonyms=ngerman-x,nde lefthyphenmin=2 righthyphenmin=2 file=loadhyph-de-1996.tex file_patterns=hyph-de-1996.pat.txt file_exceptions=
execute addMap dummy.map
execute addMixedMap a.map
";

    #[test]
    fn quotewords_like_perl() {
        assert_eq!(quotewords("a=1  b=\"x y\" c="), vec!["a=1", "b=x y", "c="]);
    }

    #[test]
    fn format_lines_match_tlmgr() {
        let db = Tlpdb::parse(DB).unwrap();
        let f = parse_add_format(db.get("latex-bin").unwrap().executes_of("AddFormat").next().unwrap()).unwrap();
        assert_eq!(f.fmttriggers, vec!["l3kernel", "latex"]);
        assert_eq!(
            fmtutil_lines(db.get("latex-bin").unwrap()).concat(),
            "#\n# from latex-bin:\npdflatex pdftex language.dat -translate-file=cp227.tcx *pdflatex.ini\n#! old pdftex - old.ini\n"
        );
        assert_eq!(updmap_lines(db.get("hyphen-german").unwrap()).concat(), "MixedMap a.map\nMap dummy.map\n");
    }

    #[test]
    fn language_lines_match_tlmgr() {
        let db = Tlpdb::parse(DB).unwrap();
        let p = db.get("hyphen-german").unwrap();
        assert_eq!(
            language_lines(p, LangDb::Dat).concat(),
            "% from hyphen-german:\n% old spelling\ngerman loadhyph-de-1901.tex\nngerman loadhyph-de-1996.tex\n=ngerman-x\n=nde\n"
        );
        assert!(language_lines(p, LangDb::Def).concat().contains("\\addlanguage{nde}{loadhyph-de-1996.tex}{}{2}{2}\n"));
        let lua = language_lines(p, LangDb::Lua).concat();
        assert!(lua.contains("\t['german'] = {\n\t\tloader = 'loadhyph-de-1901.tex',\n"));
        assert!(lua.contains("\t\tsynonyms = {  },\n\t\tpatterns = 'hyph-de-1901.pat.txt',\n\t},\n"));
        assert!(!lua.contains("hyphenation ="));
    }

    #[test]
    fn writes_with_headers_and_skips_missing_headers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let db = Tlpdb::parse(DB).unwrap();
        let pkgs: Vec<&Package> = db.packages.values().collect();
        assert!(write(root, &pkgs, Regen::all()).unwrap().is_empty());

        fs::create_dir_all(root.join("texmf-dist/tex/generic/config")).unwrap();
        fs::write(root.join("texmf-dist/tex/generic/config/language.us.def"), "%% header\n").unwrap();
        let written = write(root, &pkgs, Regen::all()).unwrap();
        assert_eq!(written, vec!["texmf-var/tex/generic/config/language.def"]);
        let def = fs::read_to_string(root.join(&written[0])).unwrap();
        assert!(def.starts_with("%% header\n% from hyphen-german:\n"));
        assert!(def.ends_with("This MUST be the last line of the file.\n"));
    }
}
