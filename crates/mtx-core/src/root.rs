//! The on-disk layout of a MennoTeX installation. It mirrors TeX Live's,
//! so kpathsea's `SELFAUTOPARENT` logic and TeX Live's own scripts work
//! unchanged:
//!
//! ```text
//! <root>/bin/universal-darwin/   programs, symlinks to scripts, mtx itself
//! <root>/texmf-dist/             packages (with ls-R)
//! <root>/texmf-var/              generated files: formats, font maps, language.*
//! <root>/texmf-mtx/              mtx's overlay, searched first (TEXMFAUXTREES)
//! <root>/texmf-ctan/             packages from CTAN (`install --from-ctan`), next
//! <root>/texmf.cnf               our kpathsea overrides
//! <root>/tlpkg/mtx/              index, installed database, cache, lock, log
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::tlpdb::ARCH;

/// The TeX Live release whose packages and binaries this mtx manages.
pub const RELEASE: u32 = 2026;

#[derive(Debug, Clone)]
pub struct Root {
    pub dir: PathBuf,
}

impl Root {
    pub fn new(dir: impl Into<PathBuf>) -> Root {
        Root { dir: dir.into() }
    }

    /// Find the root: explicit argument, then `$MTX_ROOT`, then the root
    /// containing the running executable (`<root>/bin/<arch>/mtx`), then
    /// `~/Library/MennoTeX/<release>`. An existing root is resolved to its
    /// real directory: reached through `…/MennoTeX/current`, it must stay
    /// the same root when `mtx upgrade-release` moves that link.
    pub fn discover(explicit: Option<&Path>) -> Result<Root> {
        let real = |p: PathBuf| Root::new(fs::canonicalize(&p).unwrap_or(p));
        if let Some(p) = explicit {
            return Ok(real(absolute(p)?));
        }
        if let Some(p) = std::env::var_os("MTX_ROOT").filter(|v| !v.is_empty()) {
            return Ok(real(absolute(Path::new(&p))?));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(root) = exe.parent().and_then(Path::parent).and_then(Path::parent) {
                if root.join("tlpkg/mtx").is_dir() {
                    return Ok(real(root.to_path_buf()));
                }
            }
        }
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(real(PathBuf::from(home).join(format!("Library/MennoTeX/{RELEASE}"))))
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.dir.join("bin").join(ARCH)
    }
    pub fn texmf_dist(&self) -> PathBuf {
        self.dir.join("texmf-dist")
    }
    pub fn texmf_var(&self) -> PathBuf {
        self.dir.join("texmf-var")
    }
    /// mtx's own small tree, searched before all others (see
    /// `bootstrap::install_overlay`).
    pub fn texmf_overlay(&self) -> PathBuf {
        self.dir.join("texmf-mtx")
    }
    pub fn texmf_config(&self) -> PathBuf {
        self.dir.join("texmf-config")
    }
    pub fn mtx_dir(&self) -> PathBuf {
        self.dir.join("tlpkg/mtx")
    }
    pub fn tlpdb_path(&self) -> PathBuf {
        self.mtx_dir().join("texlive.tlpdb")
    }
    pub fn index_path(&self) -> PathBuf {
        self.mtx_dir().join("files.idx")
    }
    pub fn db_path(&self) -> PathBuf {
        self.mtx_dir().join("installed.sqlite")
    }
    pub fn lock_path(&self) -> PathBuf {
        self.mtx_dir().join("lock")
    }
    /// One empty file per package whose installation is in progress (or was
    /// interrupted).
    pub fn journal_dir(&self) -> PathBuf {
        self.mtx_dir().join("journal")
    }
    pub fn log_path(&self) -> PathBuf {
        self.mtx_dir().join("mtx.log")
    }
    pub fn cache_dir(&self) -> PathBuf {
        match std::env::var_os("MTX_CACHE").filter(|v| !v.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => self.mtx_dir().join("cache"),
        }
    }
    /// TeX Live's own keyring, refreshed yearly through `texlive.infra`.
    pub fn tl_keyring(&self) -> PathBuf {
        self.dir.join("tlpkg/gpg/pubring.gpg")
    }

    pub fn is_bootstrapped(&self) -> bool {
        self.index_path().exists() && self.db_path().exists()
    }

    /// `PATH` for the TeX Live scripts we run: our binaries first, then only
    /// system directories, so other TeX installations cannot interfere.
    pub fn tool_path(&self) -> String {
        format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.bin_dir().display())
    }
}

fn absolute(p: &Path) -> Result<PathBuf> {
    Ok(if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir()?.join(p) })
}
