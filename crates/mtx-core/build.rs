//! Embeds the git commit mtx is built from, so `mtx self-update` can tell
//! whether a release is newer: `MTX_COMMIT` (9 hex digits) and
//! `MTX_COMMIT_TIME` (Unix time of the commit). Set in the environment,
//! they win (CI sets them); outside a git checkout they are `unknown`
//! and 0, and every release counts as newer.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = Command::new("git").args(args).current_dir(dir).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

fn main() {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let commit = env("MTX_COMMIT").or_else(|| git(&["rev-parse", "--short=9", "HEAD"])).unwrap_or_else(|| "unknown".into());
    let time = env("MTX_COMMIT_TIME").or_else(|| git(&["log", "-1", "--format=%ct"])).unwrap_or_else(|| "0".into());
    println!("cargo:rustc-env=MTX_COMMIT={commit}");
    println!("cargo:rustc-env=MTX_COMMIT_TIME={time}");
    println!("cargo:rerun-if-env-changed=MTX_COMMIT");
    println!("cargo:rerun-if-env-changed=MTX_COMMIT_TIME");
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        // A new commit moves HEAD's branch ref (or HEAD itself, detached).
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!("cargo:rerun-if-changed={git_dir}/{r}");
        }
        println!("cargo:rerun-if-changed={git_dir}/packed-refs");
    }
}
