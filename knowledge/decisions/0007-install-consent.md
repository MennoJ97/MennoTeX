---
type: Decision
title: Asking before automatic installs
description: An autoinstall setting (yes, no, ask) decided in mtx; ask prompts on the terminal, else a dialog, else a fallback; one answer can cover a whole compile; failures and refusals go to mtx.log, shown by mtx log and mtx doctor.
tags: [decision, policy, ask, logging, ux]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T11:40:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T00:30:00Z }
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
  else `ask_fallback` (default yes).
- **One answer per build:** "all" and "none" are stored as `ask_run:<id>` for an hour,
  where the id is the nearest latexmk ancestor (found with `ps`), else mtx's parent (the
  TeX engine, which kpathsea forks mtx from). The first version used the parent only;
  the user saw "Install All" followed by more dialogs, because one latexmk build runs
  mtx from the prefetch and from every pdfLaTeX pass, each a different parent.
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
- Writing a note into TeX's own log is still not possible (kpathsea cannot see that file).
- Shims written by older mtx versions are rewritten on the next shim sync (any install
  or `mtx repair`), which gives them `--for`.
