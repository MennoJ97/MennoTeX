//! MennoTeX releases: `mtx self-update` and `mtx upgrade-release`.
//!
//! A release (made by `.github/workflows/build-binaries.yml` as a draft, then
//! signed and published with `tools/sign_release.sh`) is tagged
//! `mennotex-<TeX Live release>-<commit>` and holds:
//!
//! - `mtx-<release>-<commit>-arm64-darwin.tar.xz`: the mtx program and a
//!   `VERSION` file (`release=`, `commit=`, `commit_time=`, `binaries=`);
//! - `mennotex-bin-<release>-<id>-arm64-darwin.tar.xz`: TeX Live's programs
//!   (the archive `VERSION` names);
//! - `SHA256SUMS`, and `SHA256SUMS.minisig`: a minisign signature made with
//!   the maintainer's key, whose trusted comment is `mennotex release <tag>`.
//!
//! Nothing from a release is used before the signature (against the public
//! key built into mtx) and every file's SHA-256 check out. The secret key
//! never leaves the maintainer's Mac (decision 0011).
//!
//! Downloads use plain HTTPS from the public repository (GitHub's API for
//! the list of releases), falling back to `gh` if the repository is private.
//!
//! The old mtx only downloads, verifies and swaps itself; the new mtx then
//! installs the programs and repairs the root, so later releases can change
//! everything after the swap.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use minisign_verify::{PublicKey, Signature};

use crate::ctx::Ctx;
use crate::root::{RELEASE, Root};

/// The commit this mtx was built from (see build.rs).
pub const COMMIT: &str = env!("MTX_COMMIT");
/// `mtx --version`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (commit ", env!("MTX_COMMIT"), ")");

/// Unix time of [`COMMIT`]; 0 when unknown (every release is newer).
pub fn commit_time() -> u64 {
    env!("MTX_COMMIT_TIME").parse().unwrap_or(0)
}

/// The minisign public key releases are signed with.
const RELEASE_KEY: &str = include_str!("../data/release-key.pub");

/// `mennotex-<release>-<commit>` → (release, commit).
pub fn parse_tag(tag: &str) -> Option<(u32, &str)> {
    let (release, commit) = tag.strip_prefix("mennotex-")?.split_once('-')?;
    (!commit.is_empty() && commit.bytes().all(|b| b.is_ascii_hexdigit())).then_some(())?;
    Some((release.parse().ok()?, commit))
}

/// The `VERSION` file of an mtx archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub release: u32,
    pub commit: String,
    pub commit_time: u64,
    /// Name of the TeX Live programs archive, without `.tar.xz`.
    pub binaries: String,
}

impl Version {
    pub fn parse(text: &str) -> Result<Version> {
        let get = |k: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
                .map(str::trim)
                .with_context(|| format!("VERSION has no {k}="))
        };
        Ok(Version {
            release: get("release")?.parse().context("VERSION: release")?,
            commit: get("commit")?.to_string(),
            commit_time: get("commit_time")?.parse().context("VERSION: commit_time")?,
            binaries: get("binaries")?.to_string(),
        })
    }

    pub fn tag(&self) -> String {
        format!("mennotex-{}-{}", self.release, self.commit)
    }
}

/// Check `dir` (a release's files) against `SHA256SUMS` and its signature:
/// the signature must be by `key` with the trusted comment `mennotex
/// release <tag>`, and every file in `dir` must be listed with its SHA-256.
pub fn verify(dir: &Path, tag: &str, key: &str) -> Result<()> {
    let key = key.lines().find(|l| !l.starts_with("untrusted comment:") && !l.trim().is_empty()).unwrap_or("");
    let key = PublicKey::from_base64(key.trim())
        .map_err(|e| anyhow::anyhow!("this mtx has no valid release signing key ({e}); it cannot update itself"))?;
    let sums = fs::read(dir.join("SHA256SUMS")).context("the release has no SHA256SUMS")?;
    let sig = fs::read_to_string(dir.join("SHA256SUMS.minisig")).context("the release is not signed (no SHA256SUMS.minisig)")?;
    let sig = Signature::decode(&sig).map_err(|e| anyhow::anyhow!("SHA256SUMS.minisig: {e}"))?;
    key.verify(&sums, &sig, false).map_err(|e| anyhow::anyhow!("SHA256SUMS: bad signature ({e})"))?;
    let want = format!("mennotex release {tag}");
    if sig.trusted_comment() != want {
        bail!("SHA256SUMS is signed for {:?}, not {want:?}", sig.trusted_comment());
    }
    let sums = String::from_utf8(sums).context("SHA256SUMS is not text")?;
    for e in fs::read_dir(dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.file_type()?.is_file() || name == "SHA256SUMS" || name == "SHA256SUMS.minisig" {
            continue;
        }
        let listed = sums
            .lines()
            .find_map(|l| l.split_once("  ").filter(|(_, f)| *f == name).map(|(h, _)| h.to_string()))
            .with_context(|| format!("{name} is not in the signed SHA256SUMS"))?;
        let got = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(fs::read(e.path())?);
            hex::encode(h.finalize())
        };
        if got != listed {
            bail!("{name}: SHA-256 does not match the signed SHA256SUMS");
        }
    }
    Ok(())
}

/// The one file in `dir` named `<prefix>…<suffix>`.
fn find_one(dir: &Path, prefix: &str, suffix: &str) -> Result<PathBuf> {
    let found: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(suffix)))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("the release has no {prefix}*{suffix}"),
        _ => bail!("the release has more than one {prefix}*{suffix}"),
    }
}

/// Unpack the `mtx` program and `VERSION` of an mtx archive into `into`.
fn unpack_mtx(archive: &Path, into: &Path) -> Result<(PathBuf, Version)> {
    fs::create_dir_all(into)?;
    let file = fs::File::open(archive)?;
    let mut tar = tar::Archive::new(liblzma::read::XzDecoder::new(std::io::BufReader::new(file)));
    let (mut program, mut version) = (None, None);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !entry.header().entry_type().is_file() || path.components().count() != 2 {
            continue; // only <top>/mtx and <top>/VERSION
        }
        match name {
            "mtx" => {
                let dest = into.join("mtx");
                entry.unpack(&dest)?;
                fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
                program = Some(dest);
            }
            "VERSION" => {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut entry, &mut s)?;
                version = Some(Version::parse(&s)?);
            }
            _ => {}
        }
    }
    Ok((program.context("the mtx archive has no mtx")?, version.context("the mtx archive has no VERSION")?))
}

/// A release's files, downloaded (or given) and verified.
pub struct Verified {
    pub version: Version,
    /// The new mtx, unpacked.
    pub mtx: PathBuf,
    pub binaries: PathBuf,
    pub sums: PathBuf,
}

/// Verify the release in `dir` with `key` and unpack its mtx into `dir/new`.
pub fn open(dir: &Path, key: &str) -> Result<Verified> {
    let archive = find_one(dir, "mtx-", "-arm64-darwin.tar.xz")?;
    // The tag comes from the archive name; the signature must name it.
    let name = archive.file_name().unwrap().to_string_lossy();
    let mid = name.strip_prefix("mtx-").and_then(|n| n.strip_suffix("-arm64-darwin.tar.xz")).unwrap_or("");
    let tag = format!("mennotex-{mid}");
    verify(dir, &tag, key)?;
    let (mtx, version) = unpack_mtx(&archive, &dir.join("new"))?;
    if version.tag() != tag {
        bail!("{name} says it is {}, not {tag}", version.tag());
    }
    let binaries = dir.join(format!("{}.tar.xz", version.binaries));
    if !binaries.is_file() {
        bail!("the release lacks {}", binaries.file_name().unwrap().to_string_lossy());
    }
    Ok(Verified { version, mtx, binaries, sums: dir.join("SHA256SUMS") })
}

fn repo(ctx: &Ctx) -> Result<String> {
    Ok(ctx.db.get("binaries_repo")?.unwrap_or_else(|| crate::github::DEFAULT_BINARIES_REPO.to_string()))
}

/// The `"tag_name"` values in a GitHub releases API response, in order.
fn tag_names(json: &str) -> Vec<String> {
    json.split("\"tag_name\"")
        .skip(1)
        .filter_map(|rest| {
            let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

/// Published MennoTeX release tags of `repo`, newest first: from GitHub's
/// public API (which never lists drafts), else through `gh` (a private
/// repository).
pub fn list_tags(repo: &str) -> Result<Vec<String>> {
    let api = crate::repo::Repo::at(&format!("https://api.github.com/repos/{repo}"));
    let tags = match api.get_bytes("releases?per_page=100") {
        Ok(body) => tag_names(&String::from_utf8_lossy(&body)),
        Err(public) => crate::github::run_gh(&[
            "api", "--paginate", &format!("repos/{repo}/releases"), "--jq", ".[] | select(.draft | not) | .tag_name",
        ])
        .with_context(|| format!("listing {repo}'s releases failed ({public:#}); through gh too"))?
        .lines()
        .map(String::from)
        .collect(),
    };
    Ok(tags.into_iter().filter(|t| parse_tag(t).is_some()).collect())
}

/// Download what mtx needs of release `tag` into `dir`: `SHA256SUMS`, its
/// signature, and the mtx and programs archives it lists (not the source).
/// Plain HTTPS from a public repository, else through `gh`.
fn download(repo: &str, tag: &str, dir: &Path) -> Result<()> {
    let base = crate::repo::Repo::at(&format!("https://github.com/{repo}/releases/download/{tag}"));
    let get = |name: &str| -> Result<()> { Ok(fs::write(dir.join(name), base.get_bytes(name)?)?) };
    if let Err(public) = get("SHA256SUMS") {
        let d = dir.to_str().context("download directory is not UTF-8")?;
        crate::github::run_gh(&[
            "release", "download", tag, "-R", repo, "-D", d, "-p", "mtx-*-arm64-darwin.tar.xz", "-p",
            "mennotex-bin-*-arm64-darwin.tar.xz", "-p", "SHA256SUMS", "-p", "SHA256SUMS.minisig",
        ])
        .with_context(|| format!("downloading release {tag} failed ({public:#}); through gh too"))?;
        return Ok(());
    }
    get("SHA256SUMS.minisig").with_context(|| format!("release {tag} is not signed"))?;
    let sums = fs::read_to_string(dir.join("SHA256SUMS"))?;
    for name in sums.lines().filter_map(|l| l.split_once("  ").map(|(_, f)| f)) {
        // Names come from an unverified file: only plain archive names.
        if (name.starts_with("mtx-") || name.starts_with("mennotex-bin-"))
            && name.ends_with("-arm64-darwin.tar.xz")
            && !name.contains('/')
        {
            get(name)?;
        }
    }
    Ok(())
}

/// Where release files come from.
pub enum From<'a> {
    /// The newest published release for this TeX Live release (self-update)
    /// or for a newer one (upgrade-release), or this tag.
    GitHub(Option<&'a str>),
    /// A directory holding a release's files (offline, or a .pkg's payload).
    Dir(&'a Path),
}

/// Get and verify release files into a fresh directory under the root.
fn fetch(ctx: &Ctx, from: &From, want: impl Fn(u32) -> bool, what: &str) -> Result<(Verified, PathBuf)> {
    let dir = ctx.root.mtx_dir().join(format!("release-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let result = (|| {
        match from {
            From::Dir(src) => {
                for e in fs::read_dir(src)?.flatten() {
                    if e.file_type()?.is_file() {
                        fs::copy(e.path(), dir.join(e.file_name()))?;
                    }
                }
            }
            From::GitHub(tag) => {
                let repo = repo(ctx)?;
                let tag = match tag {
                    Some(t) => t.to_string(),
                    None => list_tags(&repo)?
                        .into_iter()
                        .find(|t| parse_tag(t).is_some_and(|(r, _)| want(r)))
                        .with_context(|| format!("{repo} has no published release {what}"))?,
                };
                ctx.log(format!("fetching release {tag} of {repo}"));
                download(&repo, &tag, &dir)?;
            }
        }
        open(&dir, release_key())
    })();
    match result {
        Ok(v) => Ok((v, dir)),
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            Err(e)
        }
    }
}

fn release_key() -> &'static str {
    RELEASE_KEY
}

/// Replace `dest` with `src` atomically (a running copy keeps its inode).
fn replace_program(src: &Path, dest: &Path) -> Result<()> {
    let tmp = dest.with_file_name(format!(".mtx-new-{}", std::process::id()));
    fs::copy(src, &tmp)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn run(program: &Path, args: &[&std::ffi::OsStr]) -> Result<()> {
    let status = Command::new(program).args(args).status().with_context(|| format!("running {}", program.display()))?;
    if !status.success() {
        bail!("{} {} failed", program.display(), args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "));
    }
    Ok(())
}

/// What `mtx self-update` found or did.
#[derive(Debug, PartialEq, Eq)]
pub enum Updated {
    /// The release is this mtx; its programs were installed if needed.
    Current,
    /// Replaced mtx (and the programs if they changed).
    To(String),
    /// `--check`: a newer release exists.
    Available(String),
    /// The release is older than this mtx (a development build).
    Older(String),
}

/// `mtx self-update`: install the newest release of mtx and of TeX Live's
/// programs for this TeX Live release. Mentions a newer TeX Live release
/// (see [`upgrade_release`]) but does not move to it.
pub fn self_update(ctx: &Ctx, from: &From, check: bool, force: bool) -> Result<Updated> {
    if let From::GitHub(None) = from {
        if let Ok(tags) = list_tags(&repo(ctx)?) {
            if let Some(newer) = tags.iter().filter_map(|t| parse_tag(t)).map(|(r, _)| r).filter(|r| *r > RELEASE).max() {
                ctx.log(format!("TeX Live {newer} is available: `mtx upgrade-release` installs it next to this one"));
            }
        }
    }
    let (rel, dir) = fetch(ctx, from, |r| r == RELEASE, &format!("for TeX Live {RELEASE}"))?;
    let result = (|| {
        let v = &rel.version;
        if v.release != RELEASE {
            bail!("{} is for TeX Live {}; this installation is TeX Live {RELEASE} (use `mtx upgrade-release`)", v.tag(), v.release);
        }
        // Newer or older is judged against the running mtx; what to replace
        // against the root's installed one, which is not the running one when
        // a local build updates an installation (the first self-update).
        let same = v.commit == COMMIT;
        let older = !same && v.commit_time <= commit_time();
        let installed = ctx.root.bin_dir().join("mtx");
        let program_differs = fs::read(&installed).ok() != Some(fs::read(&rel.mtx)?);
        let programs_differ = ctx.db.get("binaries_build")?.as_deref() != Some(v.binaries.as_str());
        if older && !force {
            return Ok(Updated::Older(v.tag()));
        }
        if !program_differs && !programs_differ {
            return Ok(Updated::Current);
        }
        if check {
            return Ok(Updated::Available(v.tag()));
        }
        if program_differs {
            replace_program(&rel.mtx, &installed)?;
            ctx.log(format!("installed mtx replaced by {} ({})", v.commit, v.tag()));
        }
        // From here on the new mtx does the work.
        let root = ctx.root.dir.as_os_str();
        if programs_differ {
            run(&installed, &["--root".as_ref(), root, "install-binaries".as_ref(), rel.binaries.as_os_str(), "--sums".as_ref(), rel.sums.as_os_str()])?;
        }
        run(&installed, &["--root".as_ref(), root, "repair".as_ref()])?;
        Ok(Updated::To(v.tag()))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// `<parent>/current`, the symlink to the release directory in use, when
/// `root` follows the default layout (`…/MennoTeX/<release>`).
pub fn current_link(root: &Root) -> Option<PathBuf> {
    let name = root.dir.file_name()?.to_str()?;
    name.parse::<u32>().ok()?;
    Some(root.dir.parent()?.join("current"))
}

/// Point `<parent>/current` at `root` (atomically).
pub fn point_current(root: &Root) -> Result<()> {
    let Some(link) = current_link(root) else { return Ok(()) };
    let target = root.dir.file_name().context("root has no name")?;
    let tmp = link.with_file_name(format!(".current-{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)?;
    fs::rename(&tmp, &link)?;
    Ok(())
}

/// `mtx upgrade-release`: install the newest MennoTeX for a newer TeX Live
/// release next to this one (`<parent>/<new release>`), carrying over the
/// installed packages (`bootstrap --from`, decision 0006), and point
/// `<parent>/current` at it. The old root stays as it is.
pub fn upgrade_release(ctx: &Ctx, from: &From) -> Result<PathBuf> {
    let (rel, dir) = fetch(ctx, from, |r| r > RELEASE, &format!("for a TeX Live release newer than {RELEASE}"))?;
    let result = (|| {
        let v = &rel.version;
        if v.release <= RELEASE {
            bail!("{} is for TeX Live {}, not newer than this installation's {RELEASE}", v.tag(), v.release);
        }
        let parent = ctx.root.dir.parent().context("installation root has no parent directory")?;
        let new_root = Root::new(parent.join(v.release.to_string()));
        if new_root.mtx_dir().exists() {
            bail!("{} exists already; run `mtx self-update` there instead", new_root.dir.display());
        }
        ctx.log(format!("installing MennoTeX for TeX Live {} at {}", v.release, new_root.dir.display()));
        run(&rel.mtx, &["--root".as_ref(), new_root.dir.as_os_str(), "bootstrap".as_ref(), "--from".as_ref(), ctx.root.dir.as_os_str()])?;
        let installed = new_root.bin_dir().join("mtx");
        run(&installed, &[
            "--root".as_ref(),
            new_root.dir.as_os_str(),
            "install-binaries".as_ref(),
            rel.binaries.as_os_str(),
            "--sums".as_ref(),
            rel.sums.as_os_str(),
        ])?;
        point_current(&new_root)?;
        Ok(new_root.dir.clone())
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = include_str!("../testdata/release/test-release.pub");

    fn testdata() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/release")
    }

    fn copy_release() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for e in fs::read_dir(testdata()).unwrap().flatten() {
            if e.file_name() != "test-release.pub" {
                fs::copy(e.path(), d.path().join(e.file_name())).unwrap();
            }
        }
        d
    }

    #[test]
    fn tags() {
        assert_eq!(parse_tag("mennotex-2026-7c496c5ab"), Some((2026, "7c496c5ab")));
        assert_eq!(parse_tag("mennotex-bin-2026-6a3001880-arm64-darwin"), None); // the first, binaries-only release
        assert_eq!(parse_tag("mennotex-2027-"), None);
    }

    #[test]
    fn a_signed_release_opens() {
        let d = copy_release();
        let rel = open(d.path(), TEST_KEY).unwrap();
        assert_eq!(rel.version, Version {
            release: 2026,
            commit: "0123456ab".into(),
            commit_time: 4102444800,
            binaries: "mennotex-bin-2026-6a3001880.0f0f0f0f-arm64-darwin".into(),
        });
        assert!(rel.mtx.is_file() && rel.binaries.is_file());
    }

    #[test]
    fn tampering_is_refused() {
        // A changed archive: its SHA-256 no longer matches.
        let d = copy_release();
        let a = d.path().join("mtx-2026-0123456ab-arm64-darwin.tar.xz");
        let mut bytes = fs::read(&a).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&a, bytes).unwrap();
        assert!(format!("{:#}", open(d.path(), TEST_KEY).err().unwrap()).contains("SHA-256"));

        // A changed SHA256SUMS: the signature breaks.
        let d = copy_release();
        let s = d.path().join("SHA256SUMS");
        let text = fs::read_to_string(&s).unwrap().replacen('a', "b", 1);
        fs::write(&s, text).unwrap();
        assert!(format!("{:#}", open(d.path(), TEST_KEY).err().unwrap()).contains("signature"));

        // An extra file that SHA256SUMS does not list.
        let d = copy_release();
        fs::write(d.path().join("extra.txt"), "x").unwrap();
        assert!(format!("{:#}", open(d.path(), TEST_KEY).err().unwrap()).contains("not in the signed"));

        // No signature, or another key.
        let d = copy_release();
        fs::remove_file(d.path().join("SHA256SUMS.minisig")).unwrap();
        assert!(format!("{:#}", open(d.path(), TEST_KEY).err().unwrap()).contains("not signed"));
        let other = "untrusted comment: minisign public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n";
        assert!(open(copy_release().path(), other).is_err());

        // Renamed to look like another release: the signed tag differs.
        let d = copy_release();
        fs::rename(
            d.path().join("mtx-2026-0123456ab-arm64-darwin.tar.xz"),
            d.path().join("mtx-2026-fedcba987-arm64-darwin.tar.xz"),
        )
        .unwrap();
        assert!(open(d.path(), TEST_KEY).is_err());
    }

    #[test]
    fn tag_names_from_the_api() {
        let json = r#"[{"url":"x","tag_name": "mennotex-2026-7c496c5ab","draft":false},{"tag_name":"mennotex-bin-2026-6a3001880-arm64-darwin"}]"#;
        assert_eq!(tag_names(json), vec!["mennotex-2026-7c496c5ab", "mennotex-bin-2026-6a3001880-arm64-darwin"]);
    }

    #[test]
    fn version_files() {
        let v = Version::parse("release=2027\ncommit=abc\ncommit_time=5\nbinaries=mennotex-bin-2027-x-arm64-darwin\n").unwrap();
        assert_eq!(v.tag(), "mennotex-2027-abc");
        assert!(Version::parse("release=2027\n").is_err());
    }

    #[test]
    fn current_link_only_for_the_release_layout() {
        assert_eq!(current_link(&Root::new("/x/MennoTeX/2026")), Some(PathBuf::from("/x/MennoTeX/current")));
        assert_eq!(current_link(&Root::new("/tmp/scratch-root")), None);
    }
}
