---
type: Decision
title: What a broken mirror triggers
description: HTTP error statuses, transfers that break off and package databases that do not verify now make mtx avoid the mirror and use another; failovers and checksum retries are counted apart; tested over real HTTP by fault_tests with a fault-injecting server.
tags: [decision, mirrors, failover, testing, fault-injection]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T15:20:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T15:15:00Z }
---

# Context

PLAN.md §8 asks for fault injection: corrupted archives, mirrors switching in the middle
of a session, being offline. Only a corrupt archive over `file://` and a refused
connection to a pinned mirror were tested. `fault_server.rs` (tests only) serves the
signed test repository over HTTP with per-file faults (truncated body, flipped byte,
HTTP status, HTML page with 200, dropped connection), can switch the directory it serves
(a mirror that syncs), or redirects to mirrors in turn like `mirror.ctan.org`.

# What the tests found (2026-10-08)

Four of the first eleven tests failed. A refused connection and a corrupt archive already
switched mirrors, but:

- an **HTTP error status** for an archive (404 from a mirror mid-sync, 503) failed the
  install (`bail!` with the status, not a typed error);
- a **transfer that broke off** ("Peer disconnected") failed the install (an I/O error
  from the body reader);
- a **package database that does not verify** (bad signature, not matching its signed
  checksum, not xz) failed `refresh` outright, also with a redirector that could name a
  good mirror;
- with the next test, a **good mirror one revision ahead** was avoided for 24 hours:
  `install` counted failovers and checksum retries together, so after failing over from a
  broken mirror the new mirror's first mismatch was taken as the second and it was
  rejected before the database was refreshed.

# Decision

- `repo::MirrorError` (the mirror answered, but not with the file): non-200 statuses and
  read errors during an HTTP transfer. `is_mirror_failure` = network error or
  `MirrorError`; `Ctx::failover` (redirector reachable → avoid the mirror, pin another;
  unreachable → offline) handles both. A `MirrorError` alone never marks mtx offline.
- Database verification failures are `ChecksumMismatch { file: tlpkg/texlive.tlpdb }`;
  `refresh` avoids that mirror and tries another once. Nothing unverified is ever used:
  with no good mirror, `refresh` fails and keeps the old database.
- `install` counts failovers and checksum retries separately (two each): a mismatch first
  refreshes the database from the same mirror, the second avoids the mirror.

# Consequences

- `fault_tests.rs`, 13 tests: each archive fault on one of two mirrors (install finishes
  from the other, the broken one avoided, nothing half-downloaded left); a single broken
  mirror installs nothing and works once fixed; unverifiable databases; a vanished mirror;
  a mirror that syncs mid-session (database refreshed, mirror not avoided); failover to a
  newer mirror; offline (fails fast, no network use, recovers).
- `tools/make_test_repo.sh` also writes `testdata/tlnet-synced` (r4243, `bar` changed);
  regenerating changed the throw-away test key and every test repository.
