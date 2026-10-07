//! MennoTeX-built programs (TeX Live built from source with the kpathsea
//! on-demand patch, see `build/build-texlive.sh`).
//!
//! They are recorded as files of the pseudo-package [`BIN_PACKAGE`]; the
//! installer never unpacks tlnet's unpatched binaries over them, while the
//! rest of a binary package (symlinks such as `pdflatex -> pdftex`, scripts)
//! still comes from tlnet.

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::bootstrap::{HookMode, install_hooks};
use crate::ctx::Ctx;
use crate::db::{Reason, now_secs};
use crate::tlpdb::ARCH;

pub const BIN_PACKAGE: &str = "mennotex-binaries";

/// Mach-O thin (64-bit) or universal ("fat") executable?
fn is_macho(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok()
        && matches!(magic, [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
}

/// Delete every `*.fmt` (and its `.log`) under `dir`; returns how many.
fn remove_formats(dir: &Path) -> Result<usize> {
    let mut n = 0;
    let Ok(entries) = fs::read_dir(dir) else { return Ok(0) };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type()?.is_dir() {
            n += remove_formats(&p)?;
        } else if p.extension().is_some_and(|x| x == "fmt") {
            fs::remove_file(&p)?;
            let _ = fs::remove_file(p.with_extension("log"));
            n += 1;
        }
    }
    Ok(n)
}

/// Copy every Mach-O executable in `dir` (e.g. `inst/bin/<triplet>/` of a
/// texlive-source build) into the installation, and switch on-demand
/// installation from the mktex hooks to the kpathsea patch.
pub fn install_binaries(ctx: &mut Ctx, dir: &Path) -> Result<usize> {
    let bin = ctx.root.bin_dir();
    fs::create_dir_all(&bin)?;
    let mut names: Vec<String> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false) && is_macho(&e.path()))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    names.sort();
    if names.is_empty() {
        bail!("no Mach-O executables in {}", dir.display());
    }

    let lock = fs::File::create(ctx.root.lock_path())?;
    lock.lock()?;
    let mut files = Vec::with_capacity(names.len());
    for name in &names {
        let dest = bin.join(name);
        let tmp = bin.join(format!(".{name}.mtx-tmp"));
        fs::copy(dir.join(name), &tmp).with_context(|| format!("copying {name}"))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        // rename: running programs keep their old inode.
        fs::rename(&tmp, &dest)?;
        files.push(format!("bin/{ARCH}/{name}"));
    }
    ctx.db.record(BIN_PACKAGE, now_secs(), Reason::Explicit, &files)?;
    ctx.db.set("hook_mode", HookMode::Kpathsea.as_str())?;
    // A format only works with the exact engine build that dumped it.
    let removed = remove_formats(&ctx.root.texmf_var().join("web2c"))?;
    drop(lock);
    if removed > 0 {
        ctx.log(format!("removed {removed} formats built by the previous engines; they are rebuilt on next use"));
    }
    install_hooks(&ctx.root, HookMode::Kpathsea)?;
    ctx.log(format!("installed {} MennoTeX-built binaries from {}", files.len(), dir.display()));
    Ok(files.len())
}
