//! Fault injection (PLAN.md §8): mirrors that break files, drop
//! connections, answer with error pages or vanish, served over real HTTP
//! by [`FaultServer`] from the signed test repository `testdata/tlnet`
//! (`foo` depends on `bar`). Verification is never relaxed: a broken
//! mirror is avoided and another one used, or the operation fails and
//! nothing is installed.

use std::fs;
use std::path::PathBuf;

use crate::ctx::{Ctx, Freshness};
use crate::db::{Reason, now_secs};
use crate::ensure::{self, NotInstalled};
use crate::fault_server::{Fault, FaultServer};
use crate::install;
use crate::root::Root;

const KEY: &str = include_str!("../testdata/test-key.asc");
const FPR: &str = include_str!("../testdata/test-key.fpr");

fn repo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tlnet")
}

/// The same release one revision later (r4243): bar's archive changed.
fn synced_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tlnet-synced")
}

/// A fresh root whose configured repository is `url`.
fn setup(url: &str) -> (tempfile::TempDir, Root, Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let root = Root::new(dir.path().join("root"));
    let mut ctx = Ctx::open(root.clone()).unwrap();
    ctx.set_quiet(true);
    ctx.test_key = Some((KEY, FPR));
    ctx.db.set("repository", url).unwrap();
    (dir, root, ctx)
}

/// Two mirrors of the test repository behind a redirector that hands them
/// out in turn, the first one first.
fn two_mirrors() -> (FaultServer, FaultServer, FaultServer) {
    let a = FaultServer::serve(repo_dir());
    let b = FaultServer::serve(repo_dir());
    let r = FaultServer::redirector(vec![a.base.clone(), b.base.clone()]);
    (a, b, r)
}

fn installed(ctx: &Ctx) -> Vec<String> {
    ctx.db.installed().unwrap().into_keys().collect()
}

fn avoided(ctx: &Ctx, s: &FaultServer) -> bool {
    ctx.db.get(&format!("bad_mirror:{}", s.host())).unwrap().is_some()
}

/// No half-downloaded archive may stay behind in the cache.
fn no_partial_files(root: &Root) {
    let leftovers: Vec<_> = fs::read_dir(root.cache_dir())
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".part")).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "partial downloads left: {leftovers:?}");
}

/// Install foo (and bar) while mirror A breaks bar's archive with `fault`:
/// the install must finish from mirror B, and A be avoided.
fn install_survives(fault: Fault) {
    let (a, b, r) = two_mirrors();
    let (_d, root, mut ctx) = setup(&r.base);
    ctx.refresh(true).unwrap();
    assert!(ctx.repo().unwrap().base == a.base, "pinned the first mirror");
    a.break_file("archive/bar.tar.xz", fault);
    let report = install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap_or_else(|e| panic!("{fault:?}: {e:#}"));
    assert_eq!(report.installed.len(), 2, "{fault:?}");
    assert_eq!(installed(&ctx), ["bar", "foo"]);
    assert!(root.texmf_dist().join("tex/latex/bar/bar.sty").exists());
    assert!(avoided(&ctx, &a), "{fault:?}: mirror A not avoided");
    assert!(!avoided(&ctx, &b));
    assert!(b.requests().iter().any(|p| p == "archive/bar.tar.xz"), "{fault:?}: bar not fetched from B");
    assert!(!ctx.offline().unwrap(), "{fault:?}: a bad mirror is not being offline");
    no_partial_files(&root);
}

#[test]
fn corrupt_archive_switches_mirrors() {
    install_survives(Fault::Corrupt);
}

#[test]
fn truncated_download_switches_mirrors() {
    install_survives(Fault::Truncate(100));
}

#[test]
fn dropped_connection_switches_mirrors() {
    install_survives(Fault::Reset);
}

#[test]
fn missing_archive_switches_mirrors() {
    // A mirror in the middle of a sync: the database names a file it has
    // already deleted, or not yet received.
    install_survives(Fault::Status(404));
}

#[test]
fn server_error_switches_mirrors() {
    install_survives(Fault::Status(503));
}

#[test]
fn html_page_instead_of_archive_switches_mirrors() {
    install_survives(Fault::Html);
}

/// With a single, fixed mirror there is nowhere to go: the install fails,
/// nothing is installed, and the next attempt works once the file is fixed.
#[test]
fn broken_only_mirror_installs_nothing() {
    let a = FaultServer::serve(repo_dir());
    let (_d, root, mut ctx) = setup(&a.base);
    ctx.refresh(true).unwrap();
    for fault in [Fault::Corrupt, Fault::Truncate(100), Fault::Status(404), Fault::Reset] {
        a.break_file("archive/bar.tar.xz", fault);
        assert!(install::install(&mut ctx, &["foo"], Reason::Explicit).is_err(), "{fault:?}");
        assert!(installed(&ctx).is_empty(), "{fault:?}: something was installed");
        assert!(!root.texmf_dist().join("tex/latex/foo/foo.sty").exists());
        no_partial_files(&root);
    }
    a.heal("archive/bar.tar.xz");
    ctx.db.unset(&format!("bad_mirror:{}", a.host())).unwrap();
    install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap();
    assert_eq!(installed(&ctx), ["bar", "foo"]);
}

/// A mirror whose package database does not verify (a bad signature, a
/// database that does not match its signed checksum, or garbage) is
/// avoided, and the database comes from another mirror.
#[test]
fn unverifiable_database_switches_mirrors() {
    // Not `.asc` with `Corrupt`: the middle byte may fall in the unhashed
    // subpackets (the issuer), which the signature does not cover, so the
    // signature still verifies (seen when the test key was regenerated).
    for (file, fault) in [
        ("tlpkg/texlive.tlpdb.sha512", Fault::Corrupt),
        ("tlpkg/texlive.tlpdb.sha512.asc", Fault::Truncate(100)),
        ("tlpkg/texlive.tlpdb.xz", Fault::Corrupt),
        ("tlpkg/texlive.tlpdb.xz", Fault::Truncate(100)),
        ("tlpkg/texlive.tlpdb.xz", Fault::Html),
    ] {
        let (a, b, r) = two_mirrors();
        a.break_file(file, fault);
        let (_d, root, mut ctx) = setup(&r.base);
        let got = ctx.refresh(true).unwrap_or_else(|e| panic!("{file} {fault:?}: {e:#}"));
        assert!(matches!(got, Freshness::Updated { .. }), "{file} {fault:?}");
        assert!(avoided(&ctx, &a) && !avoided(&ctx, &b), "{file} {fault:?}: A {} B {}; log: {:?}", avoided(&ctx, &a), avoided(&ctx, &b), fs::read_to_string(root.log_path()));
        assert_eq!(ctx.repo().unwrap().base, b.base);
        assert!(root.tlpdb_path().exists());
    }
}

/// With only a mirror serving an unverifiable database, refresh fails and
/// keeps the database it had.
#[test]
fn unverifiable_database_on_the_only_mirror_is_not_used() {
    let a = FaultServer::serve(repo_dir());
    let (_d, root, mut ctx) = setup(&a.base);
    a.break_file("tlpkg/texlive.tlpdb.sha512", Fault::Corrupt);
    assert!(ctx.refresh(true).is_err());
    assert!(!root.tlpdb_path().exists() && !root.index_path().exists());
    assert!(!ctx.offline().unwrap(), "bad data is not being offline");
}

/// The pinned mirror syncs between the database refresh and the download:
/// bar's archive no longer matches the database mtx has. mtx fetches the
/// newer database from the same mirror and installs the new bar; the
/// mirror did nothing wrong and is not avoided.
#[test]
fn mirror_synced_mid_session_refreshes_the_database() {
    let a = FaultServer::serve(repo_dir());
    let (_d, root, mut ctx) = setup(&a.base);
    ctx.refresh(true).unwrap();
    assert_eq!(ctx.db.get_u64("tlpdb_revision").unwrap(), Some(4242));
    a.switch_to(synced_dir());
    install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap();
    assert_eq!(ctx.db.get_u64("tlpdb_revision").unwrap(), Some(4243));
    let bar = fs::read_to_string(root.texmf_dist().join("tex/latex/bar/bar.sty")).unwrap();
    assert!(bar.contains("revised"), "{bar}");
    assert!(!avoided(&ctx, &a));
    no_partial_files(&root);
}

/// Failing over to a mirror that is a revision ahead: its archives do not
/// match the database from the first mirror, so mtx refreshes from it.
#[test]
fn failover_to_a_newer_mirror_refreshes_the_database() {
    let a = FaultServer::serve(repo_dir());
    let b = FaultServer::serve(synced_dir());
    let r = FaultServer::redirector(vec![a.base.clone(), b.base.clone()]);
    let (_d, root, mut ctx) = setup(&r.base);
    ctx.refresh(true).unwrap();
    a.break_file("archive/bar.tar.xz", Fault::Reset);
    a.break_file("archive/foo.tar.xz", Fault::Reset);
    install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap();
    assert_eq!(ctx.db.get_u64("tlpdb_revision").unwrap(), Some(4243));
    assert!(fs::read_to_string(root.texmf_dist().join("tex/latex/bar/bar.sty")).unwrap().contains("revised"));
    assert!(avoided(&ctx, &a) && !avoided(&ctx, &b));
}

/// The pinned mirror goes away in the middle of a session (connection
/// refused): mtx switches to another mirror the redirector names.
#[test]
fn vanished_mirror_switches_mirrors() {
    let b = FaultServer::serve(repo_dir());
    let gone = "http://127.0.0.1:9/";
    let r = FaultServer::redirector(vec![gone.to_string(), b.base.clone()]);
    let (_d, _root, mut ctx) = setup(&r.base);
    ctx.refresh(true).unwrap();
    assert_eq!(ctx.repo().unwrap().base, b.base, "the dead mirror was skipped at refresh");
    assert!(ctx.db.get("bad_mirror:127.0.0.1:9").unwrap().is_some());
    install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap();
    assert_eq!(installed(&ctx), ["bar", "foo"]);
}

/// Offline: the mirror and the redirector both unreachable. The failure is
/// recorded, later lookups fail fast without touching the network, and
/// once the marker expires and the network is back, installs work again.
#[test]
fn offline_fails_fast_and_recovers() {
    let a = FaultServer::serve(repo_dir());
    let (_d, root, mut ctx) = setup(&a.base);
    ctx.refresh(true).unwrap();
    // The network goes away: the configured repository and the pinned
    // mirror both refuse connections.
    ctx.db.set("repository", "http://127.0.0.1:9/").unwrap();
    ctx.db.set("mirror", "http://127.0.0.1:9/").unwrap();
    ctx.db.set("mirror_for", "http://127.0.0.1:9/").unwrap();
    ctx.db.set("mirror_pinned_at", &now_secs().to_string()).unwrap();
    ctx.unpin_mirror().unwrap();
    ctx.db.set("mirror_pinned_at", &now_secs().to_string()).unwrap();
    let e = install::install(&mut ctx, &["foo"], Reason::Explicit).unwrap_err();
    assert!(crate::repo::is_network_error(&e), "{e:#}");
    assert!(ctx.offline().unwrap(), "offline not recorded");
    drop(ctx);

    let before = a.requests().len();
    let started = std::time::Instant::now();
    let e = ensure::ensure_path(&root, "foo", "texmf-dist/tex/latex/foo/foo.sty", false).unwrap_err();
    assert_eq!(e.downcast_ref::<NotInstalled>().map(NotInstalled::exit_code), Some(NotInstalled::FAILED));
    assert!(started.elapsed().as_millis() < 500, "offline lookup took {:?}", started.elapsed());
    assert_eq!(a.requests().len(), before);

    // Back online after the marker expired.
    let ctx = Ctx::open(root.clone()).unwrap();
    ctx.clear_offline();
    ctx.db.set("repository", &a.base).unwrap();
    drop(ctx);
    let got = ensure::ensure_path(&root, "foo", "texmf-dist/tex/latex/foo/foo.sty", false).unwrap().unwrap();
    assert!(got[0].exists());
}
