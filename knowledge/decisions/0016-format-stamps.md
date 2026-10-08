---
type: Decision
title: Format staleness by stamps
description: Each format build writes a stamp (engine size and mtime, fmttriggers revisions, hyphenation hash); after every transaction mtx deletes formats whose stamp differs or is missing, and mtx doctor reports them.
tags: [decision, formats, fmtutil, staleness]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T13:55:00Z }
---

# Context

PLAN.md §5.7 asks for a stamp next to each format: engine build, revisions of its
`fmttriggers=` packages, hash of the hyphenation set; when any changes, delete the format
and let it rebuild lazily. Until now mtx deleted formats when a transaction changed one
of their trigger packages, and *all* formats whenever the hyphenation files were
rewritten, which `mtx repair` and `mtx regen` always do (every repair rebuilt every
format). A format left over from a crash between install and deletion stayed stale.

# Decision

- `formats::mkfmt` writes `<format>.stamp` next to the format it builds (for example
  `texmf-var/web2c/pdftex/pdflatex.fmt.stamp`):
  `engine pdftex <size> <mtime ns>`, one `trigger <pkg> r<rev>` line per `fmttriggers`
  package (`-` if not installed), `hyphenation <sha256 of language.dat, .def, .dat.lua>`.
- After every transaction (`install::apply_regen`, used by installs, removals, repair,
  regen and bootstrap) `formats::stale` compares each built format of the installed
  `AddFormat` packages with its current stamp; a missing or different stamp deletes the
  format with its log and stamp. kpathsea's `mktexfmt` rebuilds it on next use.
- `mtx doctor` warns about out-of-date formats; `install-binaries` still deletes all
  formats (custom ones too), now with their stamps.

# Consequences

- Rewriting identical hyphenation files keeps the formats: `mtx repair` no longer
  rebuilds every format (checked 2026-10-08 in a scratch root).
- Formats built before stamps count as stale once: the first repair or install after
  updating mtx deletes them, and they rebuild on next use (a few seconds each).
