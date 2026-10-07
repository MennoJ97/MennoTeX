---
type: Decision
title: Build on TeX Live engines and tlnet, copy MiKTeX's idea
description: MennoTeX uses TeX Live engines and the tlnet repository with a new Rust package manager instead of porting MiKTeX or packaging CTAN directly.
tags: [decision, architecture]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
verified:
  - { by: human:MennoJ97, at: 2026-10-06T19:30:00Z }
sources:
  - id: plan
    resource: https://github.com/MennoJ97/MennoTeX/blob/main/PLAN.md
    title: PLAN.md §3 and §4
---

# Decision

Use TeX Live's engines (built from texlive-source) and packages (tlnet), plus a
new package manager `mtx` and a small kpathsea patch.[^plan] Do not port MiKTeX;
do not build packages from raw CTAN.

# Reasons

- tlnet is CTAN already packaged, signed, mirrored, and about 1–2 days behind CTAN.
- Only ~546 of 7,094 CTAN packages ship installable TDS zips; 467 of those are in TeX Live anyway.
- MiKTeX's macOS build carries Intel-only assumptions and a large custom toolchain.
- The file → package index is tiny (about 0.4 MiB compressed) and can be built client-side, so no server is needed.

# Consequences

- Yearly TeX Live release transitions must be handled (`release/2026` in the tlpdb).
- Binaries with the patch must be built and signed by us (Phase 1).

[^plan]: PLAN.md §3 and §4
