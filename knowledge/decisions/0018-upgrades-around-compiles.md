---
type: Decision
title: Upgrading dependencies and the kernel, but not during a compile
description: Installs outside a compile upgrade outdated packages in the new package's dependency closure, plus the LaTeX kernel when the new package is newer than it (PLAN.md §4.3); installs during a compile only add missing packages and defer upgrades, which mtx prefetch does before the next latexmk build and mtx update does anyway.
tags: [decision, installer, upgrades, kernel, prefetch]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T18:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T18:20:00Z }
---

# Context

PLAN.md §4.3 asks that installing a package also upgrades the installed packages in its
`depend` closure, plus `l3kernel`, `l3backend` and `latex` when the new package is newer
than they are: tlnet is rolling, and a package installed months after its dependencies
may need their newer versions.

Before this decision the installer upgraded outdated packages of the closure always, also
during a compile (`mtx ensure`), and never the kernel. Facts that shaped the change
(2026-10-08):

- 374 of 7156 packages in TeX Live 2026 have `depend` lines; some name the kernel
  (`acro` depends on `l3kernel` and `l3packages`), so an on-demand install could
  replace `l3kernel` in the middle of a compile.
- A running TeX has the kernel from its format, and may already have read an installed
  package's files. Replacing files under it mixes revisions in one run. LaTeX checks one
  case itself: `l3backend-*.def` calls `\__kernel_dependency_version_check:nn` and stops
  with "Mismatched LaTeX support files detected" when the format's expl3 date is older
  than the date the file needs ([package quirks](/upstream/package-quirks.md)).
- In TeX Live 2026 `l3backend` is part of `l3kernel`, and the first-aid package is
  called `firstaid`.

# Decision

- `install::plan_upgrades` decides what a transaction installs: missing packages and
  interrupted installs always; the outdated packages of the closure; the
  [`KERNEL`](/architecture/mtx-core.md) packages (`latex`, `l3kernel`, `firstaid`) when
  outdated and a package installed now has a newer revision than the installed kernel.
- During a compile (`Ctx::during_compile`, set by the `mtx ensure` entry points) only
  missing packages are installed. The upgrades are logged once and kept in the database
  (`deferred_upgrades`).
- Outside a compile deferred upgrades join the next transaction. `mtx prefetch`, which
  latexmk runs before TeX starts, catches up first (`install::catch_up`), without a
  prompt: they follow from installs already allowed, like `mtx update`. A failure is
  logged and the upgrades stay deferred.
- `mtx doctor` warns while upgrades wait. An upgraded kernel makes formats stale through
  their stamps ([decision 0016](/decisions/0016-format-stamps.md)), so they are rebuilt.

# Consequences

- Without latexmk (an editor calling `pdflatex` directly) deferred upgrades wait for the
  next `mtx install`, `mtx prefetch` or `mtx update`; the warning in `mtx doctor` and the
  line in `mtx.log` say so.
- A new package that needs a newer dependency than the installed one can still fail its
  first compile; the next latexmk build upgrades first. Before, the upgrade happened
  mid-run, when the old files might already be loaded.
- Checked on a scratch root: a document using `acro` with `l3kernel` marked outdated
  compiled without touching the kernel; `mtx prefetch` then upgraded `l3kernel` and
  `latex`. Tests: `compile_defers_upgrades_until_prefetch`,
  `install_upgrades_dependencies_and_the_kernel`.
