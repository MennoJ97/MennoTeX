---
type: Handoff
title: Handoff to the next session
description: Where MennoTeX stands, how to get a working setup again, decisions the user made, and the prioritized next steps.
tags: [handoff, next-steps, roadmap]
status: stable
stale_after: 2026-11-08T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T12:15:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T23:00:00Z }
  - { by: process:tests/run_documents.sh, at: 2026-10-07T23:20:00Z }
---

# Where things stand (2026-10-08)

- **Phases 0 and 1 work.** A 30 MB bootstrap plus our patched TeX Live 2026.1 arm64
  build compiles the 26-document corpus on the first run from a fresh root:
  pdfLaTeX 23/23 (3 are XeTeX/LuaTeX-only), XeLaTeX 26/26, LuaLaTeX 26/26, using the
  binaries from the **first GitHub CI build** (2026-10-08, 14 min, 39 MB artifact).
  Details and numbers: [status](/project/status.md).
- **Done beyond the plan's phase 1:** command shims, `mtx prefetch`/`doctor`/`repair`/
  `update`/`remove`, an install journal, a font-map index (`ec` → `cm-super`),
  fonts by name on all engines (XeTeX patch, font-name index, and a `luaotfload-main.lua`
  overlay for LuaLaTeX's first run), concurrency-safe format builds, release
  transitions ([decision 0006](/decisions/0006-release-transitions.md)), `bootstrap --from`,
  `autoinstall yes|no|ask` with `mtx config`/`mtx log` ([decision 0007](/decisions/0007-install-consent.md)),
  `install-binaries --github`, `mtx gc`, documentation on demand (`texdoc`, `mtx docs`), and
  a latexmk integration (system rc with prefetch and a failure report; doctor check for
  rc files pointing at another TeX).
- **Real installation exists** at `~/Library/MennoTeX/2026` (CI binaries recorded as
  `mennotex-bin-2026-6a3001880-arm64-darwin`, kpathsea hook mode, overlay, texdoc; kept
  current with `mtx repair` from the newest build of mtx) and **in daily use**: the user
  put its bin directory first on PATH (`~/.zprofile` line 7, 2026-10-08) and compiles
  with it from VS Code LaTeX Workshop. In a login shell `which -a pdflatex` lists
  MennoTeX before MiKTeX's `/usr/local/bin` links; `mtx doctor` reports the bin directory
  on PATH, autoinstall `ask`, and only mirror and failed-install warnings.
- **Repository:** private `MennoJ97/MennoTeX`, branch `main`, all work committed and
  pushed. `README.md`, `LICENSING.md` and `LICENSE-MIT`/`LICENSE-APACHE` added
  2026-10-08 ([decision 0008](/decisions/0008-licensing.md)). 60 `cargo test` tests; the OKF bundle checks clean.

# Rebuilding the setup (nothing outside the repo survives a session)

The previous session's build trees and test roots lived in a session scratch
directory and are gone. Instead of building locally you can install the CI artifact
(see the [development playbook](/playbooks/development.md); run 37694133534 keeps it for
GitHub's artifact retention period). To get going again:

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
  **manual only** (private repo; macOS minutes billed at 10×). The user approved a
  run on 2026-10-07 (artifact only) and on 2026-10-08 one with `release: true`, which
  published the first Release; ask before any further run. No other workflows were added (a cheap Linux
  `cargo test` + OKF check on push would also need the user's OK).
- **Real installation on this Mac:** installed 2026-10-08 (the user chose "install, no
  PATH edit", then "finish the install"); the user then added it to PATH themselves in
  `~/.zprofile` and uses it in VS Code. Do not edit `~/.zprofile` or other shell files
  unless asked. Keep running tests in scratch roots; the real root is the user's own
  working TeX, so changes to it (repair, binaries, config) need care and, if not
  routine, the user's OK.
- **`~/.latexmkrc`:** at the user's request (2026-10-08) its MiKTeX biber override only
  applies when the first `kpsewhich` on PATH has no `mtx` next to it (i.e. MiKTeX).
  Do not touch other user dotfiles without asking.
- **Knowledge upkeep:** keep `knowledge/` current in the same commit (see `CLAUDE.md`).

# Next steps, in priority order

1. **Editors with the real installation.** VS Code LaTeX Workshop works (the user,
   2026-10-08: it picks up the login shell's PATH, so the `~/.zprofile` line suffices;
   a build at 09:55 is the one behind commit `f334f3b`). Still to smoke-test: TeXShop
   and TeXstudio, which look in `/Library/TeX/texbin` rather than the shell's PATH
   (plan §5.2: TeXDist registration needs admin once, so ask first).
2. **Release downloads:** done 2026-10-08 for TeX Live's programs:
   `mtx install-binaries --github [--run ID | --release TAG|latest]` uses `gh` (private
   repository), checks the archive's release and `SHA256SUMS`, and skips an installed
   build. Still open:
   - The artifact holds TeX Live's programs only, not `mtx`. A one-command upgrade
     (`mtx self-update`, and moving to the next release) needs the workflow to build and
     package mtx too: a workflow change plus a CI run, which needs the user's OK.
   - Releases: the first, `mennotex-bin-2026-6a3001880-arm64-darwin`, was published
     2026-10-08 by run 37750229150 (15 min 49 s) with the binary archive (37 MB), the
     source archive (95 MB) and `SHA256SUMS`. `mtx install-binaries --github --release
     latest` installed its 153 programs into a scratch root in 26 s. Release assets do
     not expire; run artifacts do (90 days by default).
   - A signed manifest (plan §5.8) matters once the repository or releases are public.
   - **Licensing before publishing a Release:** done in the workflow 2026-10-08
     ([decision 0008](/decisions/0008-licensing.md)): the archive carries the license
     texts (`LICENSING.md`, `COPYINGv2`, `COPYING.LESSERv2`, a `licenses/` tree with
     every license file of texlive-source) and `kpathsea-ondemand/`; a Release also
     gets `mennotex-src-<release>-<rev>.tar.xz` (texlive-source at the pinned revision,
     unpatched, plus the patches, `build/` and the workflow). Checked in the first
     Release: both assets match `SHA256SUMS`, `licenses/` has 109 files, and the source
     archive's `tex-make.c` is the unpatched upstream file.
3. **Install feedback:** done 2026-10-08 ([decision 0007](/decisions/0007-install-consent.md)):
   `mtx config autoinstall yes|no|ask` (`$MTX_AUTOINSTALL` overrides), `ask` via terminal,
   dialog or `ask_fallback`, one answer per compile, shims gated with `install --for`;
   failures and refusals in `mtx.log`, shown by `mtx log --problems` and `mtx doctor`.
   Still open:
   - **The dialog did not show for one real build.** On the real root (autoinstall
     `ask`), a build of the user's lecture notes on 2026-10-08 10:23–10:25 logged 18
     times "no terminal or dialog to ask …; ask_fallback is yes", 3–11 s after the
     lookup: `osascript` failed fast (a 30 s give-up or a click would look different).
     The cause is still unknown. **Diagnostics are in** (2026-10-08): every `ask`
     outcome is now logged with osascript's exit status, stderr and run time, and the
     process chain above mtx (`from pdflatex < latexmk.pl < … < Visual Studio Code`);
     `mtx doctor` warns when prompts could not be shown and quotes the last reason.
     The user ran `mtx repair` with this build on the real installation (2026-10-08,
     commit `788edd9`; Claude Code's auto mode blocks agents from doing it). Next: after
     the user's next VS Code build, read its `asked about` / `could not ask` lines in
     `~/Library/MennoTeX/2026/tlpkg/mtx/mtx.log`. A dialog test from an agent session on
     a scratch root was also blocked by auto mode; it needs the user's permission.
     Ask the user before popping dialogs.
   - Getting the reason into TeX's own `.log` (what editors show) is not possible from
     kpathsea; a C change could print a `! mtx: …` line to the terminal/log via the engine.
4. **Release transitions:** staying on a release is done ([decision 0006](/decisions/0006-release-transitions.md),
   commit `38c0bac`). When tlnet serves a newer release, `refresh` switches the
   repository to `historic:2026` (the frozen `tlnet-final`, resolved over the tug.org
   historic mirrors and pinned), lagging mirrors are avoided, and `mtx doctor` warns.
   Moving up is the next release's MennoTeX build running `mtx bootstrap --from <old
   root>` side by side, not `mtx upgrade-release` in the old mtx. Tested with the local
   test repositories only; the historic mirrors were checked by hand against 2025's
   `tlnet-final` ([tlnet](/upstream/tlnet.md)). Still open:
   - Watch the real switch (usually April 2027): confirm the old root moves to
     `historic:2026` by itself and that a historic mirror has the frozen repository.
   - Prepare the 2027 build: pin the 2027 `texlive-source` revision in `build/texlive-source.rev`,
     bump `root::RELEASE`, rebuild with the patches, rerun the corpus.
   - A one-command upgrade from the old mtx needs release downloads (step 2);
     until then `mtx doctor` names the command.
5. **Grow the corpus** from 26 toward the plan's 50 documents (e.g. arXiv sources, kept
   locally; theses, posters with real fonts, Japanese with LuaTeX-ja, music with
   musixtex (needs `musixflx` between runs), Arabic/Hebrew with bidi).
6. **Known gaps** (each has a note in the knowledge bundle). The next substantial one is
   the pdfTeX map reload below: a C patch, so it needs a local build (~20 min) to develop
   and a CI run (user's OK) to reach the real installation.
   - LuaLaTeX fonts by name: **fixed** by the overlay `texmf-mtx/…/luaotfload-main.lua`
     ([decision 0005](/decisions/0005-fonts-by-name.md)). It assumes luaotfload's
     `resolvers.name` and `config.luaotfload.db.update_live` keep their shape; if a
     luaotfload update breaks `fontspec-by-name` under LuaLaTeX, look there first.
     Anonymous requests (`\font\x="Name"`) are not wrapped.
   - Font packages first needed after pdfTeX's first `\shipout` are missing from that run's
     map (plan §5.7); a pdfTeX/LuaTeX map-reload patch would fix it.
   - Roots made before the font-map rule need `mtx repair`; a `mktexpk` fallback could
     install map packages automatically.
   - Format staleness is handled by deletion (binary install, `fmttriggers`); no stamps.
   - `mtx gc` and docs on demand (`texdoc NAME`, `mtx docs PKG`) are done. texdoc's
     viewer path (opening a PDF) was not exercised, to avoid opening windows; only
     `texdoc -l -M`.
7. **Data refresh:** `crates/mtx-core/data/fontmaps.tsv.xz` and `fontnames.tsv.xz` come
   from `tools/build_fontmap_index.py` / `tools/build_fontname_index.py` (download all
   font packages once, ~850 MiB each, mostly shared). Regenerate when font packages
   change; ideally a manual CI job.
8. **PLAN.md drift:** "as built" notes now cover the layout (§5.2), binaries (§5.8), the
   index (§5.5) and release transitions (§4.3). Other sections may still drift; the
   knowledge bundle is authoritative.

# Gotchas for whoever continues

- The shell is **zsh**: unquoted `$var` does not word-split (`for d in $list` runs once).
- Always use `PATH=<root>/bin/universal-darwin:/usr/bin:/bin`; MiKTeX's x86_64 links in
  `/usr/local/bin` shadow `pdflatex`, `kpsewhich`, …
- Claude Code's safety check blocks `rm -rf` on variables; use `mktemp -d` for fresh dirs.
- CTAN mirrors are flaky (corrupt archive, expired TLS seen); mtx fails over by itself,
  but a test run can still take longer when it happens.
- TeX Live's `make all install` must not run as one parallel make; the build script's
  `--incremental` mode does it right (about 3 minutes after a kpathsea change).
- Test binaries can be stale across clones: a session that builds a temporary clone
  with this checkout's `target/` can leave a test binary that embeds the clone's
  `CARGO_MANIFEST_DIR`, and cargo reuses it as fresh. Symptom (seen 2026-10-08):
  8 `local_repo_tests` fail with `NotFound` under `cargo test` but pass with
  `-p mtx-core`. Fix: `cargo clean -p mtx-core`; check with
  `strings target/debug/deps/mtx_core-* | grep testdata`.
- Two sessions have worked in this same checkout at once (2026-10-08): fetch and check
  `git log` before committing, and keep commits small.
- kpathsea reads `ls-R` only for `TEXMFDBS` trees: a `!!` entry outside them (such as
  an aux tree) is never searched.
- texlive.info (historic mirror) shows non-browser clients a bot challenge with HTTP 200;
  do not work around it.
- latexmk keeps a failed rule failed until a recorded input changes; mtx's rc forces a
  rerun (`$go_mode`) when its prefetch installed packages. A package prefetch cannot
  see (loaded conditionally) and that failed to install still needs an edit or
  `latexmk -g` after the network is back.
- Release binaries are stripped: check for the patch with
  `strings pdftex | grep MTX_AUTOINSTALL`.
