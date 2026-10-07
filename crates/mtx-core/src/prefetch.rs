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
    let mut seen_docs = BTreeSet::new();
    let mut docs = vec![doc.to_path_buf()];
    while let Some(d) = docs.pop() {
        if !seen_docs.insert(d.clone()) || seen_docs.len() > 200 {
            continue;
        }
        let text = fs::read_to_string(&d).with_context(|| format!("reading {}", d.display()))?;
        wanted.extend(scan(&text));
        docs.extend(local_inputs(&text, d.parent().unwrap_or(Path::new("."))));
    }

    let mut total = Report::default();
    let mut scanned_files: BTreeSet<String> = BTreeSet::new();
    // Each round installs what is known so far, then reads the installed
    // .sty/.cls files for further \RequirePackage lines.
    for _round in 0..8 {
        let idx = Index::open(&ctx.root.index_path())?;
        let installed = ctx.db.installed()?;
        let mut packages: BTreeSet<String> = BTreeSet::new();
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
        for rel in to_scan {
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
    fn escaped_percent_is_not_a_comment() {
        assert!(scan(r"50\% \usepackage{siunitx}").contains(&("tex", "siunitx.sty".to_string())));
    }
}
