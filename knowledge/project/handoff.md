---
type: Handoff
title: Handoff to the next session
description: Where MennoTeX stands, how to get a working setup again, decisions the user made, and the prioritized next steps.
tags: [handoff, next-steps, roadmap]
status: stable
stale_after: 2026-11-08T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T07:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T13:00:00Z }
  - { by: process:tests/run_documents.sh, at: 2026-10-07T13:00:00Z }
---

# Where things stand (2026-10-08)

- **Phases 0 and 1 work.** A 30 MB bootstrap plus our patched TeX Live 2026.1 arm64
  build compiles the 18-document corpus on the first run from a fresh root:
  pdfLaTeX 16/16 (2 are XeTeX/LuaTeX-only), XeLaTeX 18/18, LuaLaTeX 18/18.
  Details and numbers: [status](/project/status.md).
- **Done beyond the plan's phase 1:** command shims, `mtx prefetch`/`doctor`/`repair`/
  `update`/`remove`, an install journal, a font-map index (`ec` → `cm-super`),
  fonts by name (XeTeX patch + font-name index), concurrency-safe format builds.
- **Repository:** private `MennoJ97/MennoTeX`, branch `main`, all work committed and
  pushed. 39 `cargo test` tests; the OKF bundle checks clean.

# Rebuilding the setup (nothing outside the repo survives a session)

The previous session's build trees and test roots lived in a session scratch
directory and are gone. To get going again:

```bash
cargo build --release && cargo test
git clone --depth 1 --branch tags/texlive-2026.1 https://github.com/TeX-Live/texlive-source.git /tmp/tl2026
build/build-texlive.sh /tmp/tl2026            # ~20 min; applies kpathsea-ondemand/patches/*
./target/release/mtx --root /tmp/r bootstrap
/tmp/r/bin/universal-darwin/mtx install-binaries /tmp/tl2026/inst/bin/aarch64-apple-darwin*/
tests/run_documents.sh /tmp/r pdflatex          # also xelatex, lualatex
tests/run_concurrent.sh /tmp/r2 pdflatex 8      # needs its own fresh root
```

Set `MTX_CACHE` to one directory for all scratch roots to avoid re-downloading
archives. See the [development playbook](/playbooks/development.md).

# Decisions the user made

- **CI:** the binary build workflow (`.github/workflows/build-binaries.yml`) is
  **manual only** (private repo; macOS minutes billed at 10×). It has never been run;
  ask before triggering it. No other workflows were added (a cheap Linux
  `cargo test` + OKF check on push would also need the user's OK).
- **Real installation on this Mac** (`~/Library/MennoTeX/2026`, PATH ahead of MiKTeX):
  the user said **"not yet"**. Keep testing in scratch roots until asked.
- **Knowledge upkeep:** keep `knowledge/` current in the same commit (see `CLAUDE.md`).

# Next steps, in priority order

1. **Ask the user about installing for real** and, if yes, do it: bootstrap
   `~/Library/MennoTeX/2026`, install the binaries, add the bin directory to `PATH`
   (`~/.zprofile`, ahead of MiKTeX's `/usr/local/bin` links; `mtx doctor` must come out
   clean). Then smoke-test editors: TeXShop, VS Code LaTeX Workshop, TeXstudio
   (they look in `/Library/TeX/texbin`; plan §5.2 describes TeXDist registration).
2. **Run the manual CI workflow once** (with the user's OK) to prove the GitHub build;
   then teach `mtx install-binaries` to fetch a release (private repo → needs `gh`
   auth or a token; consider signing the manifest, plan §5.8).
3. **Release transitions** (plan §4.3): when tlnet moves to `release/2027`, pin to
   `historic/systems/texlive/2026/tlnet-final` and add `mtx upgrade-release`
   (side-by-side `~/Library/MennoTeX/2027`). Today `ctx.rs` just refuses other releases.
4. **Grow the corpus** toward the plan's 50 documents (e.g. arXiv sources, kept locally;
   theses, CVs, letters, exams, music/chess/linguistics packages, CJK).
5. **Known gaps** (each has a note in the knowledge bundle):
   - LuaLaTeX + fontspec font *names* need `mtx prefetch` for the first run (luaotfload
     resolves `name:` before kpathsea; [decision 0005](/decisions/0005-fonts-by-name.md)).
     Option: a luaotfload overlay or a callback hook.
   - Font packages first needed after pdfTeX's first `\shipout` are missing from that run's
     map (plan §5.7); a pdfTeX/LuaTeX map-reload patch would fix it.
   - Roots made before the font-map rule need `mtx repair`; a `mktexpk` fallback could
     install map packages automatically.
   - Format staleness is handled by deletion (binary install, `fmttriggers`); no stamps.
   - `ask` policy and UI for installs (plan §5.9) are not implemented; installs are automatic.
   - `mtx gc` (unused auto-installed packages) and docs on demand (`texdoc`) are not done.
6. **Data refresh:** `crates/mtx-core/data/fontmaps.tsv.xz` and `fontnames.tsv.xz` come
   from `tools/build_fontmap_index.py` / `tools/build_fontname_index.py` (download all
   font packages once, ~850 MiB each, mostly shared). Regenerate when font packages
   change; ideally a manual CI job.
7. **PLAN.md drift:** the archive cache lives in `<root>/tlpkg/mtx/cache` (not
   `~/Library/Caches`), binaries are pinned to the 2026.1 release branch, and the
   resolver also handles font names. Update PLAN.md or point it at the knowledge bundle.

# Gotchas for whoever continues

- The shell is **zsh**: unquoted `$var` does not word-split (`for d in $list` runs once).
- Always use `PATH=<root>/bin/universal-darwin:/usr/bin:/bin`; MiKTeX's x86_64 links in
  `/usr/local/bin` shadow `pdflatex`, `kpsewhich`, …
- Claude Code's safety check blocks `rm -rf` on variables; use `mktemp -d` for fresh dirs.
- CTAN mirrors are flaky (corrupt archive, expired TLS seen); mtx fails over by itself,
  but a test run can still take longer when it happens.
- TeX Live's `make all install` must not run as one parallel make; the build script's
  `--incremental` mode does it right (about 3 minutes after a kpathsea change).
- Release binaries are stripped: check for the patch with
  `strings pdftex | grep MTX_AUTOINSTALL`.
