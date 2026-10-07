//! Operation context: root, installed database, pinned mirror, logging,
//! and keeping the local copy of the package database fresh.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha512};

use crate::db::{Db, now_secs};
use crate::index;
use crate::repo::{DEFAULT_REPOSITORY, Repo, is_network_error};
use crate::root::{RELEASE, Root};
use crate::tlpdb::Tlpdb;
use crate::verify::Verifier;

/// How long a freshness check of the package database stays valid.
pub const FRESHNESS_TTL_SECS: u64 = 3600;
/// How long a resolved mirror stays pinned.
pub const MIRROR_PIN_SECS: u64 = 24 * 3600;
/// After a network failure, fail fast for this long instead of waiting again.
pub const OFFLINE_SECS: u64 = 60;

#[derive(Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Checked recently; nothing fetched.
    Fresh,
    /// Checked with the mirror; the database has not changed.
    Unchanged,
    Updated { from: Option<u64>, to: u64 },
}

pub struct Ctx {
    pub root: Root,
    pub db: Db,
    repo: Option<Repo>,
    quiet: bool,
}

impl Ctx {
    pub fn open(root: Root) -> Result<Ctx> {
        fs::create_dir_all(root.mtx_dir()).with_context(|| format!("creating {}", root.mtx_dir().display()))?;
        let db = Db::open(&root.db_path())?;
        Ok(Ctx { root, db, repo: None, quiet: false })
    }

    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    /// Log to stderr (prefixed `mtx:`) and append to `tlpkg/mtx/mtx.log`.
    /// Never writes to stdout: `mtx ensure` reserves stdout for kpathsea.
    pub fn log(&self, msg: impl AsRef<str>) {
        let msg = msg.as_ref();
        if !self.quiet {
            eprintln!("mtx: {msg}");
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(self.root.log_path()) {
            let _ = writeln!(f, "{} [{}] {msg}", now_secs(), std::process::id());
        }
    }

    /// The configured repository: `$MTX_REPOSITORY`, then the stored
    /// setting, then `mirror.ctan.org`.
    pub fn repository_url(&self) -> Result<String> {
        if let Some(r) = std::env::var_os("MTX_REPOSITORY").filter(|v| !v.is_empty()) {
            return Ok(r.to_string_lossy().into_owned());
        }
        Ok(self.db.get("repository")?.unwrap_or_else(|| DEFAULT_REPOSITORY.to_string()))
    }

    pub fn offline(&self) -> Result<bool> {
        Ok(self.db.get_u64("offline_until")?.is_some_and(|t| t > now_secs()))
    }

    pub fn mark_offline(&self) {
        let _ = self.db.set("offline_until", &(now_secs() + OFFLINE_SECS).to_string());
    }

    /// The pinned mirror, resolving the redirector when the pin is stale.
    pub fn repo(&mut self) -> Result<&Repo> {
        if self.repo.is_none() {
            let configured = self.repository_url()?;
            let pinned_for = self.db.get("mirror_for")?;
            let pinned_at = self.db.get_u64("mirror_pinned_at")?.unwrap_or(0);
            let repo = match self.db.get("mirror")? {
                Some(m) if pinned_for.as_deref() == Some(configured.as_str())
                    && now_secs().saturating_sub(pinned_at) < MIRROR_PIN_SECS =>
                {
                    Repo::at(&m)
                }
                _ => {
                    // The redirector picks a mirror at random; skip ones that
                    // recently served data failing verification.
                    let mut r = None;
                    for _ in 0..6 {
                        let cand = Repo::resolve(&configured).inspect_err(|e| {
                            if is_network_error(e) {
                                self.mark_offline();
                            }
                        })?;
                        let bad = self.db.get_u64(&format!("bad_mirror:{}", cand.host()))?;
                        let is_bad = bad.is_some_and(|t| now_secs().saturating_sub(t) < MIRROR_PIN_SECS);
                        r = Some(cand);
                        if !is_bad {
                            break;
                        }
                    }
                    let r = r.expect("at least one resolution attempt");
                    self.db.set("mirror", &r.base)?;
                    self.db.set("mirror_for", &configured)?;
                    self.db.set("mirror_pinned_at", &now_secs().to_string())?;
                    if r.base != configured {
                        self.log(format!("using mirror {}", r.base));
                    }
                    r
                }
            };
            self.repo = Some(repo);
        }
        Ok(self.repo.as_ref().unwrap())
    }

    /// Forget the pinned mirror, e.g. after it served inconsistent data.
    pub fn unpin_mirror(&mut self) -> Result<()> {
        self.repo = None;
        self.db.set("mirror_pinned_at", "0")
    }

    /// Avoid the current mirror for a day and pick another one.
    pub fn reject_mirror(&mut self) -> Result<()> {
        if let Some(r) = &self.repo {
            let host = r.host().to_string();
            if !host.is_empty() {
                self.log(format!("avoiding mirror {host} for 24 hours"));
                self.db.set(&format!("bad_mirror:{host}"), &now_secs().to_string())?;
            }
        }
        self.unpin_mirror()
    }

    /// Make sure the local package database is current (see PLAN.md §4.3).
    /// With `force`, always ask the mirror; otherwise only when the last
    /// check is older than [`FRESHNESS_TTL_SECS`].
    pub fn refresh(&mut self, force: bool) -> Result<Freshness> {
        let have_local = self.root.tlpdb_path().exists() && self.root.index_path().exists();
        let checked_at = self.db.get_u64("tlpdb_checked_at")?.unwrap_or(0);
        if !force && have_local && now_secs().saturating_sub(checked_at) < FRESHNESS_TTL_SECS {
            return Ok(Freshness::Fresh);
        }
        match self.refresh_from_mirror(have_local) {
            Err(e) if is_network_error(&e) => {
                self.failover(&e)?;
                self.refresh_from_mirror(have_local).inspect_err(|e| {
                    if is_network_error(e) {
                        self.mark_offline();
                    }
                })
            }
            other => other,
        }
    }

    /// The pinned mirror failed at the network/TLS level. If the redirector
    /// still answers, the mirror itself is broken (seen: an expired TLS
    /// certificate), so avoid it and pin another one. If the redirector
    /// fails too, we are offline: record that and return the error.
    pub fn failover(&mut self, cause: &anyhow::Error) -> Result<()> {
        let configured = self.repository_url()?;
        if let Err(e) = Repo::resolve(&configured) {
            if is_network_error(&e) {
                self.mark_offline();
            }
            return Err(e.context(format!("after: {cause:#}")));
        }
        self.log(format!("mirror failed ({cause:#}); switching mirrors"));
        self.reject_mirror()?;
        self.repo()?;
        Ok(())
    }

    fn refresh_from_mirror(&mut self, have_local: bool) -> Result<Freshness> {
        let keyring = self.root.tl_keyring();
        let repo = self.repo()?.clone();
        let sha_file = repo.get_bytes("tlpkg/texlive.tlpdb.sha512")?;
        let sha_text = String::from_utf8_lossy(&sha_file).into_owned();
        let Some(expected) = sha_text.split_whitespace().next().map(str::to_ascii_lowercase) else {
            bail!("empty texlive.tlpdb.sha512 on {}", repo.base);
        };
        if have_local && self.db.get("tlpdb_sha512")?.as_deref() == Some(expected.as_str()) {
            self.db.set("tlpdb_checked_at", &now_secs().to_string())?;
            return Ok(Freshness::Unchanged);
        }

        let asc = String::from_utf8(repo.get_bytes("tlpkg/texlive.tlpdb.sha512.asc")?)?;
        let verified = Verifier::new(Some(&keyring))?
            .verify_detached(&sha_file, &asc)
            .with_context(|| format!("verifying the package database from {}", repo.base))?;
        if verified.expired_key_warning {
            self.log("warning: the TeX Live signing subkey had expired at signing time; update texlive.infra");
        }

        let xz = repo.get_bytes("tlpkg/texlive.tlpdb.xz")?;
        let mut text = Vec::new();
        liblzma::read::XzDecoder::new(&xz[..]).read_to_end(&mut text).context("decompressing texlive.tlpdb.xz")?;
        let got = hex::encode(Sha512::digest(&text));
        if got != expected {
            bail!("texlive.tlpdb from {} does not match its signed checksum", repo.base);
        }
        let text = String::from_utf8(text).context("texlive.tlpdb is not UTF-8")?;
        let tlpdb = Tlpdb::parse(&text)?;
        if tlpdb.config.release != RELEASE {
            bail!(
                "{} serves TeX Live {}, but this mtx manages TeX Live {RELEASE}; \
                 release upgrades are not implemented yet",
                repo.base,
                tlpdb.config.release
            );
        }

        let tlpdb_path = self.root.tlpdb_path();
        let tmp = tlpdb_path.with_extension("tlpdb.tmp");
        fs::write(&tmp, &text)?;
        fs::rename(&tmp, &tlpdb_path)?;
        index::write_atomic(&self.root.index_path(), &index::build(&tlpdb))?;

        let from = self.db.get_u64("tlpdb_revision")?;
        let to = tlpdb.config.revision;
        self.db.set("tlpdb_sha512", &expected)?;
        self.db.set("tlpdb_revision", &to.to_string())?;
        self.db.set("tlpdb_checked_at", &now_secs().to_string())?;
        self.log(format!("package database r{to} ({} packages) verified, key {}", tlpdb.packages.len(), verified.signing_key));
        Ok(Freshness::Updated { from, to })
    }

    /// Parse the local copy of the package database.
    pub fn tlpdb(&self) -> Result<Tlpdb> {
        let path = self.root.tlpdb_path();
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {} (run `mtx bootstrap` first)", path.display()))?;
        Tlpdb::parse(&text)
    }
}
