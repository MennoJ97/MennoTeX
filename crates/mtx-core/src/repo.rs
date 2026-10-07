//! Access to a TeX Live network repository (`tlnet`).
//!
//! `mirror.ctan.org` redirects every request to a possibly different
//! mirror, and mirrors sync at different times. Since tlnet archive names
//! are not versioned, the database and the archives must come from the same
//! host, so we resolve the redirector once and pin the result.
//!
//! A repository can also be a local directory (`file:///…` or a plain
//! path), which is used by tests and for offline mirrors.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha512};

pub const DEFAULT_REPOSITORY: &str = "https://mirror.ctan.org/systems/texlive/tlnet/";
const PROBE: &str = "tlpkg/texlive.tlpdb.sha512";

#[derive(Clone)]
enum Transport {
    Http(ureq::Agent),
    Local(PathBuf),
}

#[derive(Clone)]
pub struct Repo {
    /// Base URL (with trailing slash) or local directory.
    pub base: String,
    transport: Transport,
}

/// A network failure, as opposed to bad data; callers use this to decide
/// whether to go into offline mode.
#[derive(Debug, thiserror::Error)]
#[error("network error fetching {url}: {source}")]
pub struct NetworkError {
    pub url: String,
    #[source]
    pub source: ureq::Error,
}

fn agent(follow_redirects: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("mtx/", env!("CARGO_PKG_VERSION"), " (MennoTeX)"))
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .max_redirects(if follow_redirects { 5 } else { 0 })
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .build()
        .into()
}

fn local_path(base: &str) -> Option<PathBuf> {
    if let Some(p) = base.strip_prefix("file://") {
        Some(PathBuf::from(p))
    } else if base.starts_with('/') {
        Some(PathBuf::from(base))
    } else {
        None
    }
}

impl Repo {
    /// A repository at a fixed location, without redirector resolution.
    pub fn at(base: &str) -> Repo {
        let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
        let transport = match local_path(&base) {
            Some(p) => Transport::Local(p),
            None => Transport::Http(agent(true)),
        };
        Repo { base, transport }
    }

    /// Resolve a redirector (such as `mirror.ctan.org`) to the concrete
    /// mirror it currently points to. Non-redirecting URLs are returned as-is.
    pub fn resolve(base: &str) -> Result<Repo> {
        let repo = Repo::at(base);
        let Transport::Http(_) = repo.transport else { return Ok(repo) };
        let url = format!("{}{PROBE}", repo.base);
        let resp = agent(false)
            .head(&url)
            .call()
            .map_err(|source| NetworkError { url: url.clone(), source })?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow!("redirect from {url} without Location"))?;
            let pinned = loc
                .strip_suffix(PROBE)
                .ok_or_else(|| anyhow!("unexpected redirect target {loc}"))?;
            return Ok(Repo::at(pinned));
        }
        if status != 200 {
            bail!("{url}: HTTP {status}");
        }
        Ok(repo)
    }

    fn open(&self, rel: &str) -> Result<Box<dyn Read + Send>> {
        match &self.transport {
            Transport::Local(dir) => {
                let p = dir.join(rel);
                Ok(Box::new(fs::File::open(&p).with_context(|| format!("opening {}", p.display()))?))
            }
            Transport::Http(agent) => {
                let url = format!("{}{rel}", self.base);
                let resp = agent.get(&url).call().map_err(|source| NetworkError { url: url.clone(), source })?;
                let status = resp.status().as_u16();
                if status != 200 {
                    bail!("{url}: HTTP {status}");
                }
                Ok(Box::new(resp.into_body().into_reader()))
            }
        }
    }

    pub fn get_bytes(&self, rel: &str) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.open(rel)?.read_to_end(&mut buf).with_context(|| format!("reading {}{rel}", self.base))?;
        Ok(buf)
    }

    /// Download `rel` to `dest`, checking size and SHA-512 while streaming.
    /// The file only appears at `dest` once it has been verified.
    pub fn download_verified(&self, rel: &str, dest: &Path, size: u64, sha512_hex: &str) -> Result<()> {
        let mut reader = self.open(rel)?;
        let tmp = dest.with_extension(format!("part{}", std::process::id()));
        let result = (|| -> Result<()> {
            let mut out = fs::File::create(&tmp)?;
            let mut hasher = Sha512::new();
            let mut buf = vec![0u8; 1 << 16];
            let mut total = 0u64;
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e).with_context(|| format!("downloading {}{rel}", self.base)),
                };
                total += n as u64;
                if size > 0 && total > size {
                    return Err(ChecksumMismatch { file: rel.to_string(), detail: format!("larger than the expected {size} bytes") }.into());
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
            }
            if size > 0 && total != size {
                return Err(ChecksumMismatch { file: rel.to_string(), detail: format!("got {total} bytes, expected {size}") }.into());
            }
            let got = hex::encode(hasher.finalize());
            if !got.eq_ignore_ascii_case(sha512_hex) {
                return Err(ChecksumMismatch { file: rel.to_string(), detail: "SHA-512 differs".into() }.into());
            }
            out.sync_all()?;
            fs::rename(&tmp, dest)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

/// The archive on the mirror does not match the signed database: either the
/// mirror synced in between, or it serves a broken file. The caller should
/// refresh and retry, then switch mirrors.
#[derive(Debug, thiserror::Error)]
#[error("integrity check failed for {file}: {detail}")]
pub struct ChecksumMismatch {
    pub file: String,
    pub detail: String,
}

impl Repo {
    /// Host part of the base URL (empty for local repositories).
    pub fn host(&self) -> &str {
        self.base.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("")
    }
}

/// SHA-512 of a file already on disk, as lowercase hex.
pub fn sha512_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        match f.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn is_network_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.downcast_ref::<NetworkError>().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_repository_download_is_verified() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("archive")).unwrap();
        fs::write(dir.path().join("archive/x.tar.xz"), b"hello").unwrap();
        let repo = Repo::resolve(&format!("file://{}", dir.path().display())).unwrap();
        let sum = hex::encode(Sha512::digest(b"hello"));
        let dest = dir.path().join("out");
        repo.download_verified("archive/x.tar.xz", &dest, 5, &sum).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"hello");

        let bad = dir.path().join("bad");
        let err = repo.download_verified("archive/x.tar.xz", &bad, 5, &"0".repeat(128)).unwrap_err();
        assert!(err.downcast_ref::<ChecksumMismatch>().is_some());
        assert!(!bad.exists());
        assert!(repo.download_verified("archive/x.tar.xz", &bad, 4, &sum).is_err());
    }
}
