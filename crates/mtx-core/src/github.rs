//! Fetching MennoTeX-built binaries from GitHub (`mtx install-binaries
//! --github`): the artifact of a run of `.github/workflows/build-binaries.yml`,
//! or a GitHub Release it published. The repository is private, so this
//! goes through the GitHub CLI (`gh`), which holds the user's credentials.
//! The archive is then checked against the `SHA256SUMS` next to it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::root::RELEASE;

pub const DEFAULT_BINARIES_REPO: &str = "MennoJ97/MennoTeX";
pub const WORKFLOW: &str = "build-binaries.yml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The artifact of the newest successful workflow run.
    LatestRun,
    Run(u64),
    /// A release tag, or `latest`.
    Release(String),
}

/// `mennotex-bin-<release>-<rev>-arm64-darwin.tar.xz` → (release, rev).
pub fn parse_archive_name(name: &str) -> Option<(u32, String)> {
    let rest = name.strip_prefix("mennotex-bin-")?.strip_suffix("-arm64-darwin.tar.xz")?;
    let (release, rev) = rest.split_once('-')?;
    Some((release.parse().ok()?, rev.to_string()))
}

fn gh() -> Result<PathBuf> {
    let on_path = std::env::var("PATH").unwrap_or_default();
    on_path
        .split(':')
        .chain(["/opt/homebrew/bin", "/usr/local/bin"])
        .map(|d| Path::new(d).join("gh"))
        .find(|p| p.is_file())
        .context("the GitHub CLI (`gh`) is needed to fetch binaries from the private repository; install it and run `gh auth login`")
}

fn run_gh(args: &[&str]) -> Result<String> {
    let out = Command::new(gh()?).args(args).output().context("running gh")?;
    if !out.status.success() {
        bail!("gh {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The name of a run's binary artifact (`mennotex-bin-…-arm64-darwin`),
/// without downloading it; `LatestRun` is resolved to a run id first.
pub fn run_artifact(repo: &str, source: &Source) -> Result<Option<(u64, String)>> {
    let id: u64 = match source {
        Source::Run(id) => *id,
        Source::LatestRun => {
            let id = run_gh(&[
                "run", "list", "-R", repo, "--workflow", WORKFLOW, "--status", "success", "--limit", "1", "--json",
                "databaseId", "--jq", ".[0].databaseId",
            ])?;
            id.parse().with_context(|| format!("{repo} has no successful run of {WORKFLOW}"))?
        }
        Source::Release(_) => return Ok(None),
    };
    let name = run_gh(&[
        "api", &format!("repos/{repo}/actions/runs/{id}/artifacts"), "--jq",
        "[.artifacts[] | select(.expired | not) | .name | select(startswith(\"mennotex-bin-\"))][0] // empty",
    ])?;
    if name.is_empty() {
        bail!("run {id} of {repo} has no (unexpired) binary artifact");
    }
    Ok(Some((id, name)))
}

/// Download the binary archive and its `SHA256SUMS` from `repo` into `dir`
/// (which should be empty); returns their paths.
pub fn fetch(repo: &str, source: &Source, dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let dir_s = dir.to_str().context("download directory is not UTF-8")?;
    match source {
        Source::LatestRun | Source::Run(_) => {
            let (id, name) = run_artifact(repo, source)?.expect("runs have artifacts");
            run_gh(&["run", "download", &id.to_string(), "-R", repo, "-n", &name, "-D", dir_s])?;
        }
        Source::Release(tag) => {
            let mut args = vec!["release", "download"];
            if tag != "latest" {
                args.push(tag);
            }
            args.extend(["-R", repo, "-p", "mennotex-bin-*.tar.xz", "-p", "SHA256SUMS", "-D", dir_s]);
            run_gh(&args)?;
        }
    }
    // `gh run download` puts each artifact in its own directory.
    let mut archive = None;
    let mut sums = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d)?.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                stack.push(p);
            } else if name == "SHA256SUMS" {
                sums = Some(p);
            } else if let Some((release, _)) = parse_archive_name(&name) {
                if release != RELEASE {
                    bail!("{name} is for TeX Live {release}, but this mtx manages TeX Live {RELEASE}");
                }
                if archive.replace(p).is_some() {
                    bail!("more than one binary archive in the download");
                }
            }
        }
    }
    let archive = archive.context("the download contains no mennotex-bin-*-arm64-darwin.tar.xz")?;
    let sums = sums.context("the download contains no SHA256SUMS")?;
    Ok((archive, sums))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_names() {
        assert_eq!(
            parse_archive_name("mennotex-bin-2026-6a3001880-arm64-darwin.tar.xz"),
            Some((2026, "6a3001880".to_string()))
        );
        assert_eq!(parse_archive_name("mennotex-bin-2026-6a3001880-x86_64-linux.tar.xz"), None);
        assert_eq!(parse_archive_name("SHA256SUMS"), None);
    }
}
