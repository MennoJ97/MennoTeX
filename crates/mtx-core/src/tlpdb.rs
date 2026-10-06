//! Parser for TeX Live's package database (`texlive.tlpdb`).
//!
//! The format is a sequence of blank-line-separated records. Each record is
//! `key value` lines; file lists (`runfiles`, `docfiles`, `srcfiles`,
//! `binfiles arch=...`) are followed by lines indented with one space.
//! Paths of relocatable packages start with `RELOC/`, which stands for
//! `texmf-dist/` in an installation.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

/// Platform name used by TeX Live for macOS binaries (universal arm64+x86_64).
pub const ARCH: &str = "universal-darwin";

#[derive(Debug, Default, Clone)]
pub struct Package {
    pub name: String,
    pub category: String,
    pub revision: u64,
    pub shortdesc: String,
    pub relocated: bool,
    pub depends: Vec<String>,
    pub executes: Vec<String>,
    pub container_size: u64,
    pub container_checksum: String,
    pub doc_container_size: u64,
    pub doc_container_checksum: String,
    /// Root-relative paths (`RELOC/` already mapped to `texmf-dist/`).
    pub runfiles: Vec<String>,
    pub docfiles: Vec<String>,
    /// Binary files per platform, root-relative.
    pub binfiles: BTreeMap<String, Vec<String>>,
    pub catalogue_version: Option<String>,
}

impl Package {
    /// True for the per-platform binary packages such as `pdftex.universal-darwin`.
    pub fn is_arch_package(&self) -> bool {
        arch_suffix(&self.name).is_some()
    }

    /// True for collections and schemes, which only group other packages.
    pub fn is_meta(&self) -> bool {
        self.category == "Collection" || self.category == "Scheme"
    }

    /// Dependencies with `.ARCH` resolved to our platform.
    pub fn resolved_depends(&self) -> impl Iterator<Item = String> + '_ {
        self.depends.iter().map(|d| match d.strip_suffix(".ARCH") {
            Some(base) => format!("{base}.{ARCH}"),
            None => d.clone(),
        })
    }

    /// The `execute` directives of a given kind, e.g. `AddFormat`.
    pub fn executes_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.executes.iter().filter_map(move |e| {
            let (k, rest) = e.split_once(char::is_whitespace)?;
            (k == kind).then_some(rest.trim())
        })
    }
}

/// Repository-wide settings from the `00texlive.config` record.
#[derive(Debug, Default, Clone)]
pub struct RepoConfig {
    pub release: u32,
    pub minrelease: u32,
    pub revision: u64,
    pub frozen: bool,
}

#[derive(Debug, Default)]
pub struct Tlpdb {
    pub packages: BTreeMap<String, Package>,
    pub config: RepoConfig,
}

/// If `name` is a per-platform binary package (`foo.universal-darwin`),
/// return the platform part.
pub fn arch_suffix(name: &str) -> Option<&str> {
    if name.starts_with("00texlive") {
        return None;
    }
    let (_, suffix) = name.rsplit_once('.')?;
    // Package names like `texlive.infra` also contain dots; platform names
    // always contain a hyphen or are `windows`.
    (suffix.contains('-') || suffix == "windows").then_some(suffix)
}

/// Map a path as written in the tlpdb to a path relative to the root.
pub fn root_relative(path: &str) -> String {
    match path.strip_prefix("RELOC/") {
        Some(rest) => format!("texmf-dist/{rest}"),
        None => path.to_string(),
    }
}

#[derive(Clone, Copy)]
enum Section {
    None,
    Run,
    Doc,
    Src,
    Bin,
}

impl Tlpdb {
    pub fn parse(text: &str) -> Result<Tlpdb> {
        let mut db = Tlpdb::default();
        let mut cur: Option<Package> = None;
        let mut section = Section::None;
        let mut bin_arch = String::new();

        for (lineno, line) in text.lines().enumerate() {
            if line.is_empty() {
                if let Some(p) = cur.take() {
                    db.insert(p);
                }
                section = Section::None;
                continue;
            }
            if let Some(path) = line.strip_prefix(' ') {
                let Some(p) = cur.as_mut() else {
                    bail!("tlpdb line {}: file entry outside a package", lineno + 1);
                };
                // Doc entries carry attributes after the path (details="...").
                let path = root_relative(path.split(' ').next().unwrap_or(path));
                match section {
                    Section::Run => p.runfiles.push(path),
                    Section::Doc => p.docfiles.push(path),
                    Section::Bin => p.binfiles.entry(bin_arch.clone()).or_default().push(path),
                    Section::Src | Section::None => {}
                }
                continue;
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            if key == "name" {
                if let Some(p) = cur.take() {
                    db.insert(p);
                }
                cur = Some(Package { name: value.to_string(), ..Default::default() });
                section = Section::None;
                continue;
            }
            let Some(p) = cur.as_mut() else {
                bail!("tlpdb line {}: `{key}` before any `name`", lineno + 1);
            };
            section = Section::None;
            match key {
                "category" => p.category = value.to_string(),
                "revision" => {
                    p.revision = value
                        .parse()
                        .with_context(|| format!("tlpdb line {}: bad revision", lineno + 1))?
                }
                "shortdesc" => p.shortdesc = value.to_string(),
                "relocated" => p.relocated = value == "1",
                "depend" => p.depends.push(value.to_string()),
                "execute" => p.executes.push(value.to_string()),
                "containersize" => p.container_size = value.parse().unwrap_or(0),
                "containerchecksum" => p.container_checksum = value.to_string(),
                "doccontainersize" => p.doc_container_size = value.parse().unwrap_or(0),
                "doccontainerchecksum" => p.doc_container_checksum = value.to_string(),
                "catalogue-version" => p.catalogue_version = Some(value.to_string()),
                "runfiles" => section = Section::Run,
                "docfiles" => section = Section::Doc,
                "srcfiles" => section = Section::Src,
                "binfiles" => {
                    section = Section::Bin;
                    bin_arch = value
                        .split(' ')
                        .find_map(|kv| kv.strip_prefix("arch="))
                        .unwrap_or_default()
                        .to_string();
                }
                _ => {}
            }
        }
        if let Some(p) = cur.take() {
            db.insert(p);
        }
        if db.packages.is_empty() {
            bail!("tlpdb contains no packages");
        }
        Ok(db)
    }

    fn insert(&mut self, p: Package) {
        if p.name == "00texlive.config" {
            for d in &p.depends {
                if let Some((k, v)) = d.split_once('/') {
                    match k {
                        "release" => self.config.release = v.parse().unwrap_or(0),
                        "minrelease" => self.config.minrelease = v.parse().unwrap_or(0),
                        "revision" => self.config.revision = v.parse().unwrap_or(0),
                        "frozen" => self.config.frozen = v == "1",
                        _ => {}
                    }
                }
            }
        }
        self.packages.insert(p.name.clone(), p);
    }

    pub fn get(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    /// Packages whose files can be looked up by TeX: everything except
    /// collections, schemes, platform binaries and the `00texlive.*` records.
    pub fn content_packages(&self) -> impl Iterator<Item = &Package> {
        self.packages
            .values()
            .filter(|p| !p.is_meta() && !p.is_arch_package() && !p.name.starts_with("00texlive"))
    }

    /// The transitive `depend` closure of `roots`, following real packages
    /// and our platform's binary packages. Collections and schemes are
    /// followed only if they are among the roots themselves.
    pub fn closure<'a>(&self, roots: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>> {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack: Vec<(String, bool)> = Vec::new();
        for r in roots {
            if !self.packages.contains_key(r) {
                bail!("unknown package `{r}`");
            }
            stack.push((r.to_string(), true));
        }
        while let Some((name, is_root)) = stack.pop() {
            let Some(p) = self.packages.get(&name) else { continue };
            if p.is_meta() && !is_root {
                continue;
            }
            if !seen.insert(name.clone()) {
                continue;
            }
            for d in p.resolved_depends() {
                if let Some(suffix) = arch_suffix(&d) {
                    if suffix != ARCH {
                        continue;
                    }
                }
                if self.packages.contains_key(&d) {
                    stack.push((d, false));
                }
            }
        }
        Ok(seen.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
name 00texlive.config
category TLCore
revision 71515
depend release/2026
depend minrelease/2016
depend revision/80505
depend frozen/0

name amsmath
category Package
revision 79234
shortdesc AMS mathematical facilities for LaTeX
relocated 1
containersize 31724
containerchecksum abc
docfiles size=709
 RELOC/doc/latex/amsmath/README.md details=\"General README\" language=\"en\"
runfiles size=36
 RELOC/tex/latex/amsmath/amsmath.sty
 RELOC/tex/latex/amsmath/amsbsy.sty
catalogue-version 2.17z

name pdftex
category TLCore
revision 1
depend cm
depend pdftex.ARCH
execute AddFormat name=pdftex engine=pdftex patterns=language.def options=\"*pdfetex.ini\"
execute addMap dummy-space.map

name pdftex.universal-darwin
category TLCore
revision 2
binfiles arch=universal-darwin size=10
 bin/universal-darwin/pdftex

name pdftex.x86_64-linux
category TLCore
revision 2

name cm
category Package
revision 3

name scheme-tiny
category Scheme
revision 4
depend pdftex
";

    #[test]
    fn parses_records_and_config() {
        let db = Tlpdb::parse(SAMPLE).unwrap();
        assert_eq!(db.config.release, 2026);
        assert_eq!(db.config.revision, 80505);
        let ams = db.get("amsmath").unwrap();
        assert_eq!(ams.revision, 79234);
        assert!(ams.relocated);
        assert_eq!(ams.runfiles[0], "texmf-dist/tex/latex/amsmath/amsmath.sty");
        assert_eq!(ams.docfiles, vec!["texmf-dist/doc/latex/amsmath/README.md"]);
        assert_eq!(ams.catalogue_version.as_deref(), Some("2.17z"));
        let bin = db.get("pdftex.universal-darwin").unwrap();
        assert_eq!(bin.binfiles[ARCH], vec!["bin/universal-darwin/pdftex"]);
    }

    #[test]
    fn executes_and_arch_detection() {
        let db = Tlpdb::parse(SAMPLE).unwrap();
        let pdftex = db.get("pdftex").unwrap();
        assert_eq!(pdftex.executes_of("addMap").collect::<Vec<_>>(), vec!["dummy-space.map"]);
        assert_eq!(pdftex.executes_of("AddFormat").count(), 1);
        assert_eq!(arch_suffix("pdftex.universal-darwin"), Some("universal-darwin"));
        assert_eq!(arch_suffix("texlive.infra"), None);
        assert_eq!(arch_suffix("texlive.infra.windows"), Some("windows"));
    }

    #[test]
    fn closure_follows_arch_and_skips_other_platforms() {
        let db = Tlpdb::parse(SAMPLE).unwrap();
        let c = db.closure(["scheme-tiny"]).unwrap();
        assert_eq!(c, vec!["cm", "pdftex", "pdftex.universal-darwin", "scheme-tiny"]);
        assert!(db.closure(["nope"]).is_err());
    }
}
