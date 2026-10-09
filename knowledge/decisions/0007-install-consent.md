---
type: Decision
title: Asking before automatic installs
description: An autoinstall setting (yes, no, ask) decided in mtx; ask prompts on the terminal, else a dialog, else a fallback; one answer can cover a whole compile; failures and refusals go to mtx.log, shown by mtx log and mtx doctor.
tags: [decision, policy, ask, logging, ux]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-09T10:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-09T10:00:00Z }
  - { by: human:MennoJ97, at: 2026-10-08T14:30:00Z }
---

# Context

PLAN.md §5.9 asks for `MTX_AUTOINSTALL` = `1` / `0` / `ask`, with `ask` prompting on
the terminal, else an `osascript` dialog with a 30 s timeout, else a fallback of `yes`
(MiKTeX's headless "ask" silently means no). The user asked for this on 2026-10-08,
together with logging of failed installs: TeX's own `.log`, which editors parse,
never contains mtx's stderr, so an editor user only sees "File `foo.sty' not found".

# Decision

- **Where it is decided:** in mtx (`consent.rs`), not in the kpathsea resolver. The
  resolver skips mtx only for values starting with `0`, `n` or `f`, so `ask` reaches
  mtx with no C change, and the CI binaries stay valid.
- **Policy source:** `$MTX_AUTOINSTALL`, else the `autoinstall` setting
  (`mtx config autoinstall yes|no|ask`, stored in `installed.sqlite`), else yes. An
  `ask` written into `texmf.cnf` is seen by C but not by mtx; use `mtx config`.
- **What is gated:** automatic installs only: kpathsea misses (`ensure`,
  `ensure_path`, fonts by name) and command shims (`mtx install --for PROGRAM`). The
  gate is in `install_once`, after planning and before downloading, so the question
  names the package, how many more come with it, and the size. Explicit `mtx install`,
  `prefetch`, `update`, `repair` and `bootstrap` never ask.
- **How it asks:** `/dev/tty` if the process has a terminal (`[Y]es, [a]ll for this
  run, [n]o, n[o]ne for this run`), else (not over SSH, and unless `ask_dialog no`) an
  `osascript` dialog with Don't Install / Install All / Install, giving up after 30 s,
  else `ask_fallback` (default yes). The script stores the dialog's answer in a
  variable: until 2026-10-08 it read AppleScript's `result` after an `if` statement had
  replaced it, so every click failed with error -2753 and fell back silently.
- **What the prompt shows** (the user, 2026-10-08: list the packages instead of "and N
  more", with collapsible dependencies and scrolling): the prompt gets the request as a
  tree, each package the request is for with the planned packages its dependencies bring
  along (`install::prompt_items`). The terminal lists every package with its size and
  summary and a `+ dep, dep` line. The dialog is an `NSAlert` built in JavaScript for
  Automation (`data/ask-dialog.js`, `osascript -l JavaScript`) with a scrollable
  `NSOutlineView` (Package, Size, Description; a package expands to its dependencies),
  the same three buttons (Escape = Don't Install) and a 30 s timer that aborts the modal
  run. If that script fails, the plain `display dialog` is used. The log line keeps the
  one-line form (`mtx log` parses it). A headless mode (`check`) prints the outline's
  expanded rows for the unit test; rendering an alert that was never shown does not draw
  its contents (tried with `dataWithPDFInsideRect` and `cacheDisplayInRect`). Shown to
  the user on 2026-10-08 with sample packages: it appears in front, rows expand and
  scroll, it gives up after 30 s (`timeout`), and a click on Install All returns
  `Install All`.
- **One answer per build:** "all" and "none" are stored as `ask_run:<id>` for an hour,
  where the id is the nearest latexmk ancestor (found with `ps`), else mtx's parent (the
  TeX engine, which kpathsea forks mtx from). The first version used the parent only;
  the user saw "Install All" followed by more dialogs, because one latexmk build runs
  mtx from the prefetch and from every pdfLaTeX pass, each a different parent.
- **Concurrent requests ask once** (added 2026-10-09): LaTeX Workshop's build-on-save
  and an agent's `latexmk` on the same document showed two identical dialogs for `zref`
  one second apart (pids 84066 and 84063 in `mtx.log`, both `by dialog: all`); the
  second install then found nothing to do. When `install_once` would prompt (policy
  `ask` and no answer for this run, `consent::will_prompt`), it first takes
  `flock(tlpkg/mtx/ask.lock)`, logging `waiting for another install prompt` if another
  process holds it, then re-plans with `pending` (always, since the plan may predate a
  prompt that just ended) and returns without asking if nothing is left (`not asking
  about <trigger>: another process installed it`). The lock is held until this
  install has committed, so a waiter re-checks only after the first install is done;
  it waits at most the 30 s dialog plus that install. It is a separate lock because
  `install_once` takes `tlpkg/mtx/lock` later, and holding both in that order cannot
  deadlock (no path takes the install lock before the ask lock). Policies `yes`/`no`
  and a remembered "all"/"none" never take it. A waiter from the same latexmk run
  also sees an "all" stored meanwhile, since `decide` re-reads it.
- **Logging:** failures are logged as `error: …`, refusals as `declined: …`; `main`
  and the Phase 0 hooks append their errors to `mtx.log` too. `mtx log [--problems]`
  shows recent entries; `mtx doctor` warns about problems in the last 24 hours.
- **Every `ask` outcome is logged with its source** (added 2026-10-08 after a build fell
  back to yes 18 times with no reason recorded): `asked about … by terminal|dialog:
  yes|all|no|none (from …)`, or `could not ask about … (<why>; from …); ask_fallback is
  yes`. `<why>` says what failed at each step (`no terminal`, `ask_dialog is no`, SSH,
  or osascript's exit status, stderr and run time); `from` is the chain of processes
  above mtx from `ps` (`pdflatex < latexmk.pl < zsh < Visual Studio Code`), so a build
  from VS Code can be told from one run by a terminal or an agent. `mtx doctor` warns
  when prompts could not be shown in the last 24 hours and quotes the last reason.

# Consequences

- Tested: unit and offline tests for policy parsing, the fallback, "all"/"none" covering
  later installs, refusals leaving nothing installed, and the dialog's result parsing.
  End to end with the CI binaries: under `expect`, one terminal answer `a` covered three
  installs of one pdfLaTeX run, and (after the fix above) a `latexmk` build's prefetch and
  later pdfLaTeX passes with one prompt; the dialog was used by the user in VS Code; without a terminal and with `ask_fallback no`, the run
  failed with "File `epigraph.sty' not found", and `mtx log --problems` and `mtx doctor`
  explained why. The dialog's AppleScript compiles (`osacompile`), but a dialog has not
  been clicked through yet.
- Concurrent prompts (2026-10-09): `consent_tests::concurrent_requests_for_one_package_ask_once`
  runs two installs of `foo` at once with a slow prompter and expects one prompt; without
  the ask lock it counts two. End to end on a scratch root with the release programs,
  `autoinstall ask` and `ask_dialog no` (fallback yes, so no window appeared), two
  pdfLaTeX runs of different documents needing `zref-savepos.sty` started together:
  both compiled; for `zref` and each of its dependencies one process went through
  consent and the other logged `waiting for another install prompt` and `not asking
  about …: another process installed it`.
- TeX's own log gets a `Package mtx Warning` line since 2026-10-08
  ([decision 0014](0014-install-warnings-in-tex-log.md)).
- Shims written by older mtx versions are rewritten on the next shim sync (any install
  or `mtx repair`), which gives them `--for`.
