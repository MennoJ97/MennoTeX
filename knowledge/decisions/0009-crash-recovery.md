---
type: Decision
title: Crash recovery of installs
description: A package stays journaled until its whole install (ls-R, configuration, font maps, shims) is done; the next lookup of its files, the next install of anything, or mtx repair finishes it; tests/run_crash.sh kills mtx at each step.
tags: [decision, install, journal, crash, robustness]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T15:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T14:20:00Z }
  - { by: process:tests/run_crash.sh, at: 2026-10-08T14:35:00Z }
---

# Context

PLAN.md's Phase 2 exit asks that "a forced crash in the middle of a transaction
recovers cleanly". Installs already had a journal (`tlpkg/mtx/journal/<pkg>`, written
before unpacking), files are renamed into place whole from a staging directory, and
the database records a package in one SQLite transaction. But the journal entry was
removed as soon as the package was recorded, while ls-R, the configuration files
(`fmtutil.cnf`, `updmap.cfg`, `language.*`), `updmap` (font maps) and the shims came
after the loop over all packages. A crash there left a package recorded and unjournaled
but without its font map: nothing would ever redo it, and pdfTeX would silently lack
the fonts. Found by reading `install_once` on 2026-10-08, before any test existed.

# Decision

- **Journal until the end.** Every package of a transaction keeps its journal entry
  until ls-R, regeneration and shims are done; the entries are removed last.
- **Who finishes an interrupted install:**
  - a lookup of one of its files: `mtx ensure` / `ensure --path` no longer return an
    existing file of a journaled package directly but reinstall the package first
    (no new consent: the install was already agreed to). When that is impossible
    (offline, autoinstall off) they return the file anyway: files are complete.
  - the patched kpathsea (`mtx-ondemand.c`) does the same check (`mtx_interrupted`, a
    `stat` of the journal entry, only on ls-R misses) before taking a readable file;
    this needs the next binary build to reach installations.
  - **any later install** adds journaled packages to its plan, keeping their recorded
    reason (`Reason::Upgrade` semantics);
  - `mtx repair`, as before; `mtx doctor` warns about interrupted installs.
- **Leftover staging directories** are removed by the next install under the install
  lock (all unpacking happens under it, so they belong to killed processes).
- **Crash points for tests:** `MTX_CRASH_AT=unpacked|recorded|listed|regenerated` makes
  mtx send itself SIGKILL at that step; `tests/run_crash.sh` uses them, plus a real
  `kill -9` from outside while `cm-super` (62 MiB) unpacks.

# Consequences

- A crash after ls-R was updated (`listed`, `regenerated`) is invisible to TeX's own
  lookups (kpathsea finds the files in ls-R without asking mtx), so a TeX run in that
  state may lack the package's font map until the next install, `mtx repair`, or a
  lookup that misses. The window is the few seconds `updmap` runs.
- A process that sees another process's live journal entries adds them to its plan;
  after taking the lock it re-checks and skips them (the other process finished).
