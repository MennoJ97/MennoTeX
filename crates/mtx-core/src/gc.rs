//! `mtx gc`: remove packages that were installed on demand and have not
//! been used for a while. On-demand installation brings them back when a
//! document needs them again.
//!
//! "Used" is the newest access time of a package's files: macOS updates
//! atime on read (lazily, at most about once a day, which is plenty for a
//! cutoff in days), and kpathsea's hits never reach mtx, so there is nothing
//! better to go on. Backups or indexing that read files only make gc keep
//! more.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::UNIX_EPOCH;

use anyhow::Result;

use crate::ctx::Ctx;
use crate::db::Installed;
use crate::tlpdb::Tlpdb;

/// When a package was last used: the newest atime of its files, but at
/// least its installation time.
pub fn last_used(ctx: &Ctx, name: &str, installed_at: u64) -> Result<u64> {
    let mut newest = installed_at;
    for f in ctx.db.files_of(name)? {
        // Symlinks (bin/pdflatex -> pdftex) say nothing about use.
        let Ok(meta) = fs::symlink_metadata(ctx.root.dir.join(&f)) else { continue };
        if !meta.is_file() {
            continue;
        }
        if let Some(t) = meta.accessed().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()) {
            newest = newest.max(t.as_secs());
        }
    }
    Ok(newest)
}

/// Packages to remove: everything installed that is not kept, where kept
/// means requested (bootstrap, explicit), not in the package database (our
/// binaries), or used since `cutoff`, plus all their dependencies and the
/// font-map packages their fonts need.
pub fn plan(tlpdb: &Tlpdb, installed: &BTreeMap<String, Installed>, used: &dyn Fn(&str) -> u64, cutoff: u64) -> Result<Vec<String>> {
    let roots: Vec<&str> = installed
        .iter()
        .filter(|(name, i)| {
            tlpdb.get(name).is_none() || matches!(i.reason.as_str(), "bootstrap" | "explicit") || used(name) >= cutoff
        })
        .map(|(name, _)| name.as_str())
        .filter(|name| tlpdb.get(name).is_some())
        .collect();
    let mut keep: BTreeSet<String> = tlpdb.closure(roots.iter().copied())?.into_iter().collect();
    let maps = crate::fontmaps::map_packages_for(tlpdb, &keep.iter().cloned().collect::<Vec<_>>(), &|_| false);
    if !maps.is_empty() {
        let all: Vec<&str> = keep.iter().map(String::as_str).chain(maps.iter().map(String::as_str)).collect();
        keep = tlpdb.closure(all)?.into_iter().collect();
    }
    let mut out: Vec<String> = installed.keys().filter(|n| tlpdb.get(n).is_some() && !keep.contains(*n)).cloned().collect();
    // Documentation: kept if asked for by `mtx docs`, or read since the cutoff.
    out.extend(installed.iter().filter_map(|(n, i)| {
        let base = crate::docs::base(n)?;
        (tlpdb.get(base).is_some() && i.reason != "explicit" && used(n) < cutoff).then(|| n.clone())
    }));
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(reason: &str) -> Installed {
        Installed { name: String::new(), revision: 1, reason: reason.into(), installed_at: 0 }
    }

    #[test]
    fn keeps_requested_recent_and_their_dependencies() {
        let db = Tlpdb::parse(
            "name core\ncategory Package\nrevision 1\n\n\
             name old\ncategory Package\nrevision 1\ndepend olddep\n\n\
             name olddep\ncategory Package\nrevision 1\n\n\
             name fresh\ncategory Package\nrevision 1\ndepend shared\n\n\
             name shared\ncategory Package\nrevision 1\n\n\
             name mine\ncategory Package\nrevision 1\ndepend minedep\n\n\
             name minedep\ncategory Package\nrevision 1\n",
        )
        .unwrap();
        let installed: BTreeMap<String, Installed> = [
            ("core", "bootstrap"),
            ("old", "auto"),
            ("olddep", "dependency"),
            ("fresh", "auto"),
            ("shared", "dependency"),
            ("mine", "explicit"),
            ("minedep", "dependency"),
            ("mennotex-binaries", "explicit"),
        ]
        .into_iter()
        .map(|(n, r)| (n.to_string(), inst(r)))
        .collect();
        let used = |n: &str| if n == "fresh" { 1000 } else { 10 };
        assert_eq!(plan(&db, &installed, &used, 500).unwrap(), vec!["old", "olddep"]);
        // A dependency that is itself in use keeps nothing else alive.
        let used = |n: &str| if n == "olddep" { 1000 } else { 10 };
        assert_eq!(plan(&db, &installed, &used, 500).unwrap(), vec!["fresh", "old", "shared"]);
    }
}
