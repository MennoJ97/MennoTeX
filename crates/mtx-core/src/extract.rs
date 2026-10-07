//! Unpacking tlnet `.tar.xz` containers into the root.
//!
//! Containers of relocatable packages store paths relative to
//! `texmf-dist/`; others store root-relative paths. Every container also
//! carries `tlpkg/tlpobj/<pkg>.tlpobj`, which we skip (the package database
//! already has that data).
//!
//! Files are first written to a staging directory inside the root (same
//! filesystem) and then renamed into place, so a reader never sees a
//! half-written file. Paths are sanitized: no absolute paths, no `..`, and
//! symlinks must point inside the root.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

pub struct Unpacked {
    /// Root-relative paths of everything placed, in archive order.
    pub files: Vec<String>,
}

enum Item {
    File { rel: String, staged: PathBuf },
    Symlink { rel: String, target: PathBuf },
}

fn sanitize(p: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str().context("non-UTF-8 path in archive")?.to_string()),
            Component::CurDir => {}
            _ => bail!("unsafe path in archive: {}", p.display()),
        }
    }
    if parts.is_empty() {
        bail!("empty path in archive");
    }
    Ok(parts.join("/"))
}

/// Lexically resolve `target` relative to the directory of `rel` and check
/// that it stays inside the root.
fn symlink_stays_inside(rel: &str, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth: Vec<&str> = rel.split('/').collect();
    depth.pop(); // the link itself
    for c in target.components() {
        match c {
            Component::ParentDir => {
                if depth.pop().is_none() {
                    return false;
                }
            }
            Component::Normal(s) => depth.push(s.to_str().unwrap_or("")),
            Component::CurDir => {}
            _ => return false,
        }
    }
    true
}

/// Unpack `archive` into `root`. Paths for which `skip` returns true are
/// not installed (used for files mtx provides itself).
pub fn unpack(archive: &Path, relocated: bool, root: &Path, skip: &dyn Fn(&str) -> bool) -> Result<Unpacked> {
    let staging = root.join(".staging").join(format!("{}-{}", std::process::id(), crate::db::now_secs()));
    fs::create_dir_all(&staging)?;
    let result = unpack_inner(archive, relocated, root, &staging, skip);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn unpack_inner(archive: &Path, relocated: bool, root: &Path, staging: &Path, skip: &dyn Fn(&str) -> bool) -> Result<Unpacked> {
    let file = fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut tar = tar::Archive::new(liblzma::read::XzDecoder::new(io::BufReader::new(file)));
    let mut items = Vec::new();

    for (n, entry) in tar.entries()?.enumerate() {
        let mut entry = entry?;
        let raw = sanitize(&entry.path()?)?;
        if raw.starts_with("tlpkg/tlpobj/") {
            continue;
        }
        let rel = if relocated { format!("texmf-dist/{raw}") } else { raw };
        if skip(&rel) {
            continue;
        }
        match entry.header().entry_type() {
            tar::EntryType::Directory => {}
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                let staged = staging.join(n.to_string());
                let mut out = fs::File::create(&staged)?;
                io::copy(&mut entry, &mut out).with_context(|| format!("unpacking {rel}"))?;
                // Keep the executable bit (scripts), drop anything unusual.
                let mode = entry.header().mode().unwrap_or(0o644);
                let mode = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
                fs::set_permissions(&staged, fs::Permissions::from_mode(mode))?;
                items.push(Item::File { rel, staged });
            }
            tar::EntryType::Symlink => {
                let target = entry.link_name()?.context("symlink without target")?.into_owned();
                if !symlink_stays_inside(&rel, &target) {
                    bail!("symlink {rel} -> {} points outside the installation", target.display());
                }
                items.push(Item::Symlink { rel, target });
            }
            other => bail!("unsupported archive entry type {other:?} for {rel}"),
        }
    }

    let mut files = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Item::File { rel, staged } => {
                let dest = root.join(&rel);
                fs::create_dir_all(dest.parent().unwrap())?;
                if dest.is_symlink() || dest.is_dir() {
                    remove_any(&dest)?;
                }
                fs::rename(&staged, &dest).with_context(|| format!("installing {rel}"))?;
                files.push(rel);
            }
            Item::Symlink { rel, target } => {
                let dest = root.join(&rel);
                fs::create_dir_all(dest.parent().unwrap())?;
                if dest.symlink_metadata().is_ok() {
                    remove_any(&dest)?;
                }
                std::os::unix::fs::symlink(&target, &dest).with_context(|| format!("linking {rel}"))?;
                files.push(rel);
            }
        }
    }
    Ok(Unpacked { files })
}

fn remove_any(p: &Path) -> io::Result<()> {
    if p.is_dir() && !p.is_symlink() { fs::remove_dir_all(p) } else { fs::remove_file(p) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_archive(dir: &Path, build: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> PathBuf {
        let mut b = tar::Builder::new(Vec::new());
        build(&mut b);
        let tar_bytes = b.into_inner().unwrap();
        let mut xz = Vec::new();
        io::copy(&mut liblzma::read::XzEncoder::new(&tar_bytes[..], 6), &mut xz).unwrap();
        let p = dir.join("t.tar.xz");
        fs::write(&p, xz).unwrap();
        p
    }

    fn add_file(b: &mut tar::Builder<Vec<u8>>, path: &str, data: &[u8], mode: u32) {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(mode);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, path, data).unwrap();
    }

    fn add_link(b: &mut tar::Builder<Vec<u8>>, path: &str, target: &str) {
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_entry_type(tar::EntryType::Symlink);
        b.append_link(&mut h, path, target).unwrap();
    }

    #[test]
    fn relocated_paths_symlinks_and_skips() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let a = make_archive(dir.path(), |b| {
            add_file(b, "tex/latex/x/x.sty", b"x", 0o644);
            add_file(b, "scripts/x/run.sh", b"#!/bin/sh", 0o755);
            add_file(b, "tlpkg/tlpobj/x.tlpobj", b"name x", 0o644);
        });
        let u = unpack(&a, true, &root, &|_| false).unwrap();
        assert_eq!(u.files, vec!["texmf-dist/tex/latex/x/x.sty", "texmf-dist/scripts/x/run.sh"]);
        assert_eq!(fs::read(root.join("texmf-dist/tex/latex/x/x.sty")).unwrap(), b"x");
        let mode = fs::metadata(root.join("texmf-dist/scripts/x/run.sh")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert!(!root.join(".staging").read_dir().unwrap().next().is_some());

        let b = make_archive(dir.path(), |b| {
            add_link(b, "bin/universal-darwin/x", "../../texmf-dist/scripts/x/run.sh");
            add_file(b, "bin/universal-darwin/mktextfm", b"tl", 0o755);
        });
        let u = unpack(&b, false, &root, &|p| p.ends_with("/mktextfm")).unwrap();
        assert_eq!(u.files, vec!["bin/universal-darwin/x"]);
        assert_eq!(fs::read_link(root.join("bin/universal-darwin/x")).unwrap(), Path::new("../../texmf-dist/scripts/x/run.sh"));
    }

    #[test]
    fn rejects_escaping_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let a = make_archive(dir.path(), |b| add_link(b, "bin/evil", "../../../etc/passwd"));
        assert!(unpack(&a, false, &root, &|_| false).is_err());
        assert!(!root.join("bin/evil").exists());
    }

    #[test]
    fn sanitize_rejects_parent_and_absolute() {
        assert!(sanitize(Path::new("../x")).is_err());
        assert!(sanitize(Path::new("/etc/passwd")).is_err());
        assert_eq!(sanitize(Path::new("./a/b")).unwrap(), "a/b");
    }
}
