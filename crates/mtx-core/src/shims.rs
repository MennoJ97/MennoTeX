//! Command shims: on-demand installation for programs.
//!
//! kpathsea hooks cover files TeX looks up, but not commands a user or a
//! build tool runs (`biber`, `latexmk`, `makeglossaries`, …). For every
//! program in a tlnet binary package (`*.universal-darwin`) that is not
//! installed, `bin/universal-darwin/` gets a small shim. Its first run
//! installs the package, which unpacks the real program over the shim,
//! and then runs it with the original arguments.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;

use crate::ctx::Ctx;
use crate::tlpdb::{ARCH, Tlpdb};

/// First line after the shebang; identifies files that are shims.
pub const MARKER: &str = "# mtx-shim";

/// Names that must never get a shim even if a TeX Live package ships them.
/// `man` is a directory link TeX Live uses to locate its man pages.
const NEVER: &[&str] = &["man", "mtx", "mktextex", "mktextfm"];

fn shim_script(program: &str, package: &str) -> String {
    format!(
        "#!/bin/sh\n{MARKER} {program} {package}\n\
         # Installed by MennoTeX: the first run installs TeX Live package `{package}`,\n\
         # which replaces this file with the real `{program}`, then runs it.\n\
         if [ -n \"$MTX_SHIM_ACTIVE\" ]; then\n\
         \x20 echo \"mtx: installing {package} did not provide {program}\" >&2; exit 127\n\
         fi\n\
         d=$(cd \"$(dirname \"$0\")\" && pwd)\n\
         \"$d/mtx\" install --for {program} {package} >&2 || exit 127\n\
         MTX_SHIM_ACTIVE=1 exec \"$d/{program}\" \"$@\"\n"
    )
}

pub fn is_shim(path: &Path) -> bool {
    fs::read_to_string(path).map(|s| s.lines().nth(1).is_some_and(|l| l.starts_with(MARKER))).unwrap_or(false)
}

/// Program name → package (the base package, e.g. `biber` for
/// `biber.universal-darwin`) for every program in a binary package.
pub fn programs(tlpdb: &Tlpdb) -> BTreeMap<String, String> {
    let prefix = format!("bin/{ARCH}/");
    let mut out = BTreeMap::new();
    for p in tlpdb.packages.values() {
        let Some(base) = p.name.strip_suffix(&format!(".{ARCH}")) else { continue };
        for f in p.binfiles.get(ARCH).into_iter().flatten() {
            if let Some(prog) = f.strip_prefix(&prefix) {
                if !prog.contains('/') {
                    out.entry(prog.to_string()).or_insert_with(|| base.to_string());
                }
            }
        }
    }
    out
}

/// Create shims for programs whose packages are not installed and remove
/// shims whose programs are now provided. Returns (created, removed).
pub fn sync(ctx: &Ctx, tlpdb: &Tlpdb) -> Result<(usize, usize)> {
    let bin = ctx.root.bin_dir();
    fs::create_dir_all(&bin)?;
    let installed = ctx.db.installed()?;
    let mut created = 0;
    let mut removed = 0;
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for (prog, pkg) in programs(tlpdb) {
        let arch_pkg = format!("{pkg}.{ARCH}");
        if NEVER.contains(&prog.as_str())
            || installed.contains_key(&arch_pkg)
            || Path::new("/usr/bin").join(&prog).exists()
            || Path::new("/bin").join(&prog).exists()
        {
            continue;
        }
        wanted.insert(prog.clone());
        let path = bin.join(&prog);
        let script = shim_script(&prog, &pkg);
        if path.symlink_metadata().is_ok() {
            // A real program from elsewhere stays; a shim from an older mtx
            // is rewritten.
            if !is_shim(&path) || fs::read_to_string(&path).is_ok_and(|s| s == script) {
                continue;
            }
        }
        let tmp = bin.join(format!(".{prog}.shim-tmp"));
        fs::write(&tmp, script)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        fs::rename(&tmp, &path)?;
        created += 1;
    }
    // Shims for programs that are now installed, or no longer exist upstream.
    for entry in fs::read_dir(&bin)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !wanted.contains(&name) && entry.file_type()?.is_file() && is_shim(&entry.path()) {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok((created, removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_map_to_base_packages() {
        let db = Tlpdb::parse(
            "name biber\ncategory Package\nrevision 1\ndepend biber.ARCH\n\n\
             name biber.universal-darwin\ncategory Package\nrevision 1\nbinfiles arch=universal-darwin size=1\n bin/universal-darwin/biber\n\n\
             name biber.x86_64-linux\ncategory Package\nrevision 1\nbinfiles arch=x86_64-linux size=1\n bin/x86_64-linux/biber\n",
        )
        .unwrap();
        let p = programs(&db);
        assert_eq!(p.len(), 1);
        assert_eq!(p["biber"], "biber");
    }

    #[test]
    fn shim_is_recognized() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("biber");
        fs::write(&f, shim_script("biber", "biber")).unwrap();
        assert!(is_shim(&f));
        fs::write(&f, "#!/bin/sh\necho real\n").unwrap();
        assert!(!is_shim(&f));
    }

    #[test]
    fn sync_rewrites_outdated_shims_only() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::open(crate::root::Root::new(dir.path())).unwrap();
        let db = Tlpdb::parse(
            "name biber\ncategory Package\nrevision 1\ndepend biber.ARCH\n\n\
             name biber.universal-darwin\ncategory Package\nrevision 1\nbinfiles arch=universal-darwin size=1\n bin/universal-darwin/biber\n bin/universal-darwin/mtxtestreal\n",
        )
        .unwrap();
        let bin = ctx.root.bin_dir();
        fs::create_dir_all(&bin).unwrap();
        let old = shim_script("biber", "biber").replace(" --for biber", "");
        fs::write(bin.join("biber"), &old).unwrap();
        fs::write(bin.join("mtxtestreal"), "#!/bin/sh\necho real\n").unwrap();
        sync(&ctx, &db).unwrap();
        assert_eq!(fs::read_to_string(bin.join("biber")).unwrap(), shim_script("biber", "biber"));
        assert_eq!(fs::read_to_string(bin.join("mtxtestreal")).unwrap(), "#!/bin/sh\necho real\n");
    }
}
