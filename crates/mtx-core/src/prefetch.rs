//! `mtx prefetch doc.tex`: install what a document will need before TeX
//! asks for it, in a few large transactions with parallel downloads,
//! instead of one on-demand install per file during the run.
//!
//! This is static analysis, so it is a heuristic: it may fetch a package
//! that is only loaded conditionally, and on-demand installation still
//! covers anything it misses.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::ctx::Ctx;
use crate::db::Reason;
use crate::ensure::{kind, resolve};
use crate::index::Index;
use crate::install::{self, Report};

/// Drop TeX comments (`%` not preceded by a backslash).
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| {
            let b = line.as_bytes();
            let cut = (0..b.len()).find(|&i| b[i] == b'%' && (i == 0 || b[i - 1] != b'\\'));
            &line[..cut.unwrap_or(b.len())]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Arguments of `\cmd[opt]{arg}` (optional argument skipped), for each occurrence.
fn command_args<'a>(text: &'a str, cmd: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find(cmd) {
        let after = &rest[pos + cmd.len()..];
        rest = after;
        // `\usepackagefoo` is a different command.
        if after.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let mut s = after.trim_start();
        if s.starts_with('[') {
            match s.find(']') {
                Some(end) => s = s[end + 1..].trim_start(),
                None => continue,
            }
        }
        if let Some(body) = s.strip_prefix('{') {
            if let Some(end) = body.find('}') {
                out.push(&body[..end]);
            }
        }
    }
    out
}

fn split_list(arg: &str) -> impl Iterator<Item = String> + '_ {
    arg.split(',').map(str::trim).filter(|s| !s.is_empty() && !s.contains(['\\', '#', '$'])).map(String::from)
}

/// File names a piece of TeX source asks for, as (kpathsea format, name).
pub fn scan(text: &str) -> BTreeSet<(&'static str, String)> {
    let text = strip_comments(text);
    let mut out = BTreeSet::new();
    for cmd in ["\\documentclass", "\\LoadClass", "\\LoadClassWithOptions"] {
        for a in command_args(&text, cmd) {
            out.extend(split_list(a).map(|n| ("tex", format!("{n}.cls"))));
        }
    }
    for cmd in ["\\usepackage", "\\RequirePackage", "\\RequirePackageWithOptions"] {
        for a in command_args(&text, cmd) {
            out.extend(split_list(a).map(|n| ("tex", format!("{n}.sty"))));
        }
    }
    for a in command_args(&text, "\\usetikzlibrary") {
        out.extend(split_list(a).map(|n| ("tex", format!("tikzlibrary{n}.code.tex"))));
    }
    for a in command_args(&text, "\\usepgfplotslibrary") {
        out.extend(split_list(a).map(|n| ("tex", format!("pgfplotslibrary{n}.code.tex"))));
    }
    for a in command_args(&text, "\\bibliographystyle") {
        out.extend(split_list(a).map(|n| ("bst", format!("{n}.bst"))));
    }
    out
}

/// Font names given to fontspec (`\setmainfont{TeX Gyre Pagella}`,
/// `\newfontfamily\x[…]{Font}`, `\babelfont{rm}{Font}`, …).
pub fn scan_fonts(text: &str) -> BTreeSet<String> {
    let text = strip_comments(text);
    let mut out = BTreeSet::new();
    let plain = ["\\setmainfont", "\\setsansfont", "\\setmonofont", "\\setmathfont", "\\setromanfont", "\\fontspec"];
    for cmd in plain {
        out.extend(command_args(&text, cmd).into_iter().map(|a| a.trim().to_string()));
    }
    // A control sequence or a {family} argument comes before the font.
    for cmd in ["\\newfontfamily", "\\newfontface", "\\setfontfamily", "\\babelfont"] {
        let mut rest = text.as_str();
        while let Some(pos) = rest.find(cmd) {
            rest = &rest[pos + cmd.len()..];
            let mut s = rest.trim_start();
            if let Some(cs) = s.strip_prefix('\\') {
                s = cs.trim_start_matches(|c: char| c.is_ascii_alphabetic()).trim_start();
            } else if s.starts_with('{') && cmd == "\\babelfont" {
                match s.find('}') {
                    Some(end) => s = s[end + 1..].trim_start(),
                    None => continue,
                }
            }
            out.extend(command_args(&format!("\\x{s}"), "\\x").into_iter().take(1).map(|a| a.trim().to_string()));
        }
    }
    out.retain(|f| !f.is_empty() && !f.contains(['\\', '#']));
    out
}

/// Local files `\input`/`\include`d by `text`, relative to `dir`.
fn local_inputs(text: &str, dir: &Path) -> Vec<PathBuf> {
    let text = strip_comments(text);
    let mut out = Vec::new();
    for cmd in ["\\input", "\\include", "\\subfile"] {
        for a in command_args(&text, cmd) {
            for cand in [dir.join(a), dir.join(format!("{a}.tex"))] {
                if cand.is_file() {
                    out.push(cand);
                    break;
                }
            }
        }
    }
    out
}

/// Install everything `doc` (and its local includes) statically requires.
pub fn prefetch(ctx: &mut Ctx, doc: &Path) -> Result<Report> {
    let mut wanted: BTreeSet<(&'static str, String)> = BTreeSet::new();
    let mut fonts: BTreeSet<String> = BTreeSet::new();
    let mut seen_docs = BTreeSet::new();
    let mut docs = vec![doc.to_path_buf()];
    while let Some(d) = docs.pop() {
        if !seen_docs.insert(d.clone()) || seen_docs.len() > 200 {
            continue;
        }
        let text = fs::read_to_string(&d).with_context(|| format!("reading {}", d.display()))?;
        wanted.extend(scan(&text));
        fonts.extend(scan_fonts(&text));
        docs.extend(local_inputs(&text, d.parent().unwrap_or(Path::new("."))));
    }
    // Fonts by file name are ordinary lookups; by name they go through the
    // font-name index. luaotfload resolves fontspec's `name:` requests
    // before any kpathsea lookup, so installing them here is what makes a
    // first LuaLaTeX run work.
    let mut font_packages: BTreeSet<String> = BTreeSet::new();
    for f in &fonts {
        let lower = f.to_ascii_lowercase();
        if [".otf", ".ttf", ".ttc"].iter().any(|e| lower.ends_with(e)) {
            let kind = if lower.ends_with(".otf") { "opentype fonts" } else { "truetype fonts" };
            wanted.insert((kind, f.clone()));
        } else if let Some(ff) = crate::fontnames::lookup(f) {
            font_packages.insert(ff.package);
        }
    }

    // TeX has not started yet: the moment for upgrades on-demand installs
    // put off during earlier compiles (decision 0018).
    let mut total = install::catch_up(ctx);
    let follow = crate::config::prefetch_follows_requires(ctx)?;
    let mut scanned_files: BTreeSet<String> = BTreeSet::new();
    // Each round installs what is known so far, then reads the installed
    // .sty/.cls files for further \RequirePackage lines.
    for _round in 0..8 {
        let idx = Index::open(&ctx.root.index_path())?;
        let installed = ctx.db.installed()?;
        let mut packages: BTreeSet<String> = std::mem::take(&mut font_packages)
            .into_iter()
            .filter(|p| !installed.contains_key(p))
            .collect();
        let mut to_scan: Vec<String> = Vec::new();
        for (fmt, name) in &wanted {
            let Some(hit) = resolve(&idx, kind(fmt).expect("known format"), name) else { continue };
            let rel = hit.path();
            if scanned_files.insert(rel.clone()) {
                to_scan.push(rel);
            }
            let pkg = idx.package(hit.pkg).name;
            if !installed.contains_key(pkg) {
                packages.insert(pkg.to_string());
            }
        }
        drop(idx);
        if !packages.is_empty() {
            let names: Vec<&str> = packages.iter().map(String::as_str).collect();
            let r = install::install(ctx, &names, Reason::Auto)?;
            total.installed.extend(r.installed);
            total.files += r.files;
            total.bytes_downloaded += r.bytes_downloaded;
        }
        let before = wanted.len();
        for rel in to_scan.into_iter().filter(|_| follow) {
            if let Ok(text) = fs::read_to_string(ctx.root.dir.join(&rel)) {
                wanted.extend(scan(&text));
            }
        }
        if wanted.len() == before && packages.is_empty() {
            break;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_preamble_commands() {
        let src = r"\documentclass[11pt]{article} % \usepackage{commented}
\usepackage[T1]{fontenc}
\usepackage{amsmath, amssymb}
\RequirePackage{xcolor}\usetikzlibrary{arrows.meta,calc}
\usepackagefoo{not-a-package}
\bibliographystyle{plainnat}
\usepackage{\mymacro}";
        let got: Vec<String> = scan(src).into_iter().map(|(f, n)| format!("{f}:{n}")).collect();
        assert_eq!(
            got,
            vec![
                "bst:plainnat.bst",
                "tex:amsmath.sty",
                "tex:amssymb.sty",
                "tex:article.cls",
                "tex:fontenc.sty",
                "tex:tikzlibraryarrows.meta.code.tex",
                "tex:tikzlibrarycalc.code.tex",
                "tex:xcolor.sty",
            ]
        );
    }

    #[test]
    fn scans_fontspec_font_names() {
        let src = r"\setmainfont{TeX Gyre Pagella}[Numbers=OldStyle]
\setsansfont[Scale=0.9]{TeX Gyre Heros}
\newfontfamily\headingfont[Color=red]{Libertinus Sans}
\babelfont{rm}[Language=Default]{Noto Serif}
\setmathfont{texgyrepagella-math.otf}
% \setmonofont{Commented Out}";
        let got: Vec<String> = scan_fonts(src).into_iter().collect();
        assert_eq!(got, vec!["Libertinus Sans", "Noto Serif", "TeX Gyre Heros", "TeX Gyre Pagella", "texgyrepagella-math.otf"]);
    }

    #[test]
    fn escaped_percent_is_not_a_comment() {
        assert!(scan(r"50\% \usepackage{siunitx}").contains(&("tex", "siunitx.sty".to_string())));
    }
}
