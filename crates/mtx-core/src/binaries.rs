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

/// Delete every `*.fmt` (with its `.log` and stamp) under `dir`; returns how many.
fn remove_formats(dir: &Path) -> Result<usize> {
    let mut n = 0;
    let Ok(entries) = fs::read_dir(dir) else { return Ok(0) };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type()?.is_dir() {
            n += remove_formats(&p)?;
        } else if p.extension().is_some_and(|x| x == "fmt") {
            crate::formats::remove(&p)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Install from a release archive (`mennotex-bin-*.tar.xz`, as built by
/// `.github/workflows/build-binaries.yml`), optionally checking it against
/// a `SHA256SUMS` file first.
pub fn install_binaries_archive(ctx: &mut Ctx, archive: &Path, sums: Option<&Path>) -> Result<usize> {
    let name = archive.file_name().and_then(|n| n.to_str()).context("archive name")?;
    if let Some(sums) = sums {
        let want = fs::read_to_string(sums)?
            .lines()
            .find_map(|l| l.split_once("  ").filter(|(_, f)| *f == name).map(|(h, _)| h.to_string()))
            .with_context(|| format!("{name} is not listed in {}", sums.display()))?;
        let got = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(fs::read(archive)?);
            hex::encode(h.finalize())
        };
        if got != want {
            bail!("{name}: SHA-256 does not match {}", sums.display());
        }
    }
    let tmp = ctx.root.mtx_dir().join(format!("unpack-{}", std::process::id()));
    fs::create_dir_all(&tmp)?;
    let result = (|| {
        let file = fs::File::open(archive)?;
        let mut tar = tar::Archive::new(liblzma::read::XzDecoder::new(std::io::BufReader::new(file)));
        // unpack() refuses absolute paths and `..` components.
        tar.unpack(&tmp).with_context(|| format!("unpacking {name}"))?;
        let dir = fs::read_dir(&tmp)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.is_dir())
            .context("archive has no top-level directory")?;
        install_binaries(ctx, &dir)
    })();
    let _ = fs::remove_dir_all(&tmp);
    if result.is_ok() {
        ctx.db.set("binaries_build", name.trim_end_matches(".tar.xz"))?;
    }
    result
}

/// Fetch binaries from GitHub (see [`crate::github`]) and install them.
/// Returns `None` when that build is installed already (unless `force`).
pub fn install_binaries_github(ctx: &mut Ctx, source: &crate::github::Source, force: bool) -> Result<Option<usize>> {
    let repo = ctx.db.get("binaries_repo")?.unwrap_or_else(|| crate::github::DEFAULT_BINARIES_REPO.to_string());
    let mut source = source.clone();
    if let Some((id, name)) = crate::github::run_artifact(&repo, &source)? {
        source = crate::github::Source::Run(id); // the run checked here, even if a newer one finishes
        if !force && ctx.db.get("binaries_build")?.as_deref() == Some(name.as_str()) {
            ctx.log(format!("{name} (run {id}) is installed already; --force reinstalls it"));
            return Ok(None);
        }
        ctx.log(format!("fetching {name} from run {id} of {repo}"));
    } else if let crate::github::Source::Release(tag) = &source {
        ctx.log(format!("fetching the binaries of {repo}'s release {tag}"));
    }
    let dir = ctx.root.mtx_dir().join(format!("download-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let result = crate::github::fetch(&repo, &source, &dir)
        .and_then(|(archive, sums)| install_binaries_archive(ctx, &archive, Some(&sums)));
    let _ = fs::remove_dir_all(&dir);
    result.map(Some)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::Root;

    /// Pack `files` (path, contents) into `<dir>/<name>.tar.xz` under the
    /// top-level directory `<name>/`, as the workflow's Package step does.
    fn pack(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = dir.join(format!("{name}.tar.xz"));
        let xz = liblzma::write::XzEncoder::new(fs::File::create(&path).unwrap(), 1);
        let mut tar = tar::Builder::new(xz);
        for (p, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, format!("{name}/{p}"), *data).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        path
    }

    fn sha256(path: &Path) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(fs::read(path).unwrap()))
    }

    #[test]
    fn installs_only_the_programs_of_a_license_complete_archive() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path().join("root"));
        let mut ctx = Ctx::open(root.clone()).unwrap();
        ctx.set_quiet(true);
        let name = "mennotex-bin-2026-6a3001880-arm64-darwin";
        let macho: &[u8] = &[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1];
        let archive = pack(
            dir.path(),
            name,
            &[
                ("pdftex", macho),
                ("xetex", macho),
                ("texlive-source.rev", b"6a300188053b8f2ded89dbd52293732a706b9c0e\n"),
                ("LICENSING.md", b"# Licensing\n"),
                ("LICENSE-MIT", b"MIT\n"),
                ("LICENSE-APACHE", b"Apache\n"),
                ("COPYING.LESSERv2", b"LGPL\n"),
                ("COPYINGv2", b"GPL\n"),
                ("licenses/texk/kpathsea/COPYING.LESSERv2", b"LGPL\n"),
                ("licenses/libs/icu/icu-src/LICENSE", b"ICU\n"),
                ("kpathsea-ondemand/mtx-ondemand.c", b"/* public domain */\n"),
                ("kpathsea-ondemand/patches/0001-kpathsea-ondemand.patch", b"--- a\n+++ b\n"),
            ],
        );
        // A release's SHA256SUMS also lists the source archive.
        let sums = dir.path().join("SHA256SUMS");
        let bin_line = format!("{}  {name}.tar.xz\n", sha256(&archive));
        fs::write(&sums, format!("{}  mennotex-src-2026-6a3001880.tar.xz\n{bin_line}", "0".repeat(64))).unwrap();

        assert_eq!(install_binaries_archive(&mut ctx, &archive, Some(&sums)).unwrap(), 2);
        let bin = root.bin_dir();
        assert!(bin.join("pdftex").is_file() && bin.join("xetex").is_file());
        for extra in ["texlive-source.rev", "LICENSING.md", "LICENSE-MIT", "COPYINGv2", "licenses", "LICENSE", "kpathsea-ondemand", "mtx-ondemand.c"] {
            assert!(bin.join(extra).symlink_metadata().is_err(), "{extra} was installed");
        }
        let mut files = ctx.db.files_of(BIN_PACKAGE).unwrap();
        files.sort();
        assert_eq!(files, [format!("bin/{ARCH}/pdftex"), format!("bin/{ARCH}/xetex")]);
        assert_eq!(ctx.db.get("binaries_build").unwrap().as_deref(), Some(name));
        // The unpack directory is cleaned up.
        assert!(!fs::read_dir(root.mtx_dir()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("unpack-")));

        // A tampered archive is refused before anything is unpacked.
        fs::write(&sums, bin_line.replace(&sha256(&archive), &"1".repeat(64))).unwrap();
        let err = install_binaries_archive(&mut ctx, &archive, Some(&sums)).unwrap_err();
        assert!(err.to_string().contains("SHA-256 does not match"), "{err}");
    }
}
