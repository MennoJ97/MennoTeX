---
type: Handoff
title: Handoff to the next session
description: Where MennoTeX stands, how to get a working setup again, decisions the user made, and the prioritized next steps.
tags: [handoff, next-steps, roadmap]
status: stable
stale_after: 2026-11-08T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T18:40:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T18:20:00Z }
  - { by: process:tests/run_documents.sh, at: 2026-10-07T23:20:00Z }
---

# Where things stand (2026-10-08)

- **Released and installed as `mennotex-2026-50cd1119c` (2026-10-08, signed by the user;
  the real installation runs mtx `50cd1119c`, `texmf.cnf` has `texmf-ctan`). Asked for by
  the user after the release before it:** the upgrade rule of PLAN.md §4.3 ([decision 0018](/decisions/0018-upgrades-around-compiles.md):
  dependencies and the kernel upgraded outside compiles, deferred during them, caught up
  by prefetch); the on-the-fly overhead measured (median 87 ms per package with cached
  archives, 457 ms cold; [status](/project/status.md)); the settings `freshness_ttl`,
  `prefetch_depth` and `docs`; and `mtx install --from-ctan` ([decision 0019](/decisions/0019-ctan-overlay.md)). Also `mtx list --auto|--explicit` and `ls-R` compaction in `mtx update`.
  All in mtx, no C changes, so a release run can reuse the programs (`9c5675bd`, about
  2½ minutes). The new `texmf.cnf` line (`texmf-ctan` in `TEXMFAUXTREES`) reaches an
  installation through the `repair` that `self-update` runs. 100 `cargo test` tests. Run 37800275954 (3 min, programs reused).
- **Released and installed (2026-10-08):** `mennotex-2026-675989434` (run 37794123819,
  programs `mennotex-bin-2026-6a3001880.9c5675bd`, reused from `cf6183267`'s run
  37791928013) carries PLAN.md items 1–5 the user asked for: probe installs measured, no
  deny list ([decision 0013](/decisions/0013-no-probe-deny-list.md)); `Package mtx Warning`
  lines in TeX's log ([decision 0014](/decisions/0014-install-warnings-in-tex-log.md), patch
  0004); map packages installed during the compile ([decision 0015](/decisions/0015-font-map-on-miss.md));
  format stamps ([decision 0016](/decisions/0016-format-stamps.md)); mirror-failover fixes
  from fault injection ([decision 0017](/decisions/0017-mirror-faults.md)); `mtx search`; the
  package-list install prompt; the dialog-click fix; and `self-update` ordering releases
  by `published_at`. The user signed both releases and updated the real installation
  (`mtx --version` → `675989434`, `fontmaps.tsv` present, old unstamped formats removed;
  they rebuild on next use). From here plain `mtx self-update` finds new releases.

- **Phase 2's exit criteria are met (2026-10-08):** the corpus matches a full TeX Live
  2026 (pdfLaTeX, LuaLaTeX; XeLaTeX except TeX-tree fonts by name) and crashes
  mid-install recover ([status](/project/status.md)).
- **Phases 0 and 1 work; Phase 1's exit criterion is met.** A 30 MB bootstrap plus our
  patched TeX Live 2026.1 arm64 build compiles the **51-document corpus** on the first
  run from a fresh root, and the second run installs nothing: pdfLaTeX 40/40 (11 are
  XeTeX/LuaTeX-only), XeLaTeX 43/43, LuaLaTeX 46/46 (2026-10-08, binaries of CI run
  37750229150).
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
- **Repository:** public `MennoJ97/MennoTeX` (the user, 2026-10-08), branch `main`, all work committed and
  pushed. `README.md`, `LICENSING.md` and `LICENSE-MIT`/`LICENSE-APACHE` added
  2026-10-08 ([decision 0008](/decisions/0008-licensing.md)). 100 `cargo test` tests and 57 C checks (2026-10-08); the OKF bundle checks clean.

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
  **manual only** (decided while the repository was private and macOS minutes were
  billed at 10×; it is public now, where standard runners are free). The user approved
  runs on 2026-10-07 (artifact only), 2026-10-08 (`release: true`, the first Release)
  and 2026-10-08 (run 37765924689, artifact only, with the `mtx_interrupted` C change);
  ask before any further run.
- **Release signing (decision 0011):** the minisign secret key stays on the user's
  Mac; CI only drafts releases, `tools/sign_release.sh` signs and publishes. No other workflows were added (a cheap Linux
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
2. **Next release:** nothing waiting. A release needs a CI run with `release: true`
   (ask first), then the user signs and runs `mtx self-update`. Open from the plan:
   the weekly launchd `mtx update` (PLAN.md §4.3, optional) and the 300-document corpus
   (PLAN.md §8), which the user has not chosen yet.
3. **Self-update and signed releases (2026-10-08, [decision 0011](/decisions/0011-releases-and-self-update.md)):**
   done. First signed release `mennotex-2026-f06b1506f` (CI run 37781813612, 2 min:
   programs reused from the run before; signed by the user with
   `tools/sign_release.sh`); the real installation updated to it with a local build's
   `self-update` and now updates with plain `mtx self-update`. The user's `~/.zprofile`
   PATH line points at `~/Library/MennoTeX/current/bin/universal-darwin` (changed at the
   user's request). A new release needs: a CI run with `release: true` (user's OK), then
   the user signs. The next release carries everything under "Ready for the next
   release" above. Earlier binary channel notes:
   `mtx install-binaries --github` (TeX Live's programs only):
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
   - Signed manifest (plan §5.8): done as minisign-signed `SHA256SUMS` (decision 0011).
   - **Licensing before publishing a Release:** done in the workflow 2026-10-08
     ([decision 0008](/decisions/0008-licensing.md)): the archive carries the license
     texts (`LICENSING.md`, `COPYINGv2`, `COPYING.LESSERv2`, a `licenses/` tree with
     every license file of texlive-source) and `kpathsea-ondemand/`; a Release also
     gets `mennotex-src-<release>-<rev>.tar.xz` (texlive-source at the pinned revision,
     unpatched, plus the patches, `build/` and the workflow). Checked in the first
     Release: both assets match `SHA256SUMS`, `licenses/` has 109 files, and the source
     archive's `tex-make.c` is the unpatched upstream file.
4. **Install feedback:** done 2026-10-08 ([decision 0007](/decisions/0007-install-consent.md)):
   `mtx config autoinstall yes|no|ask` (`$MTX_AUTOINSTALL` overrides), `ask` via terminal,
   dialog or `ask_fallback`, one answer per compile, shims gated with `install --for`;
   failures and refusals in `mtx.log`, shown by `mtx log --problems` and `mtx doctor`.
   Still open:
   - **Lost dialog clicks: found and fixed 2026-10-08.** The diagnostics caught a VS
     Code build (`from pdflatex < latexmk.pl < Visual Studio Code`): `osascript exit 1:
     … The variable result is not defined. (-2753) after 2.7 s`. The dialog did show;
     after a click, the script's `return button returned of result` failed because the
     preceding `if` statement had replaced AppleScript's `result`. Every click was lost
     and the install fell back to `ask_fallback` (yes): this morning's 18 silent installs,
     and probably the earlier "more dialogs after Install All". Fixed by storing the answer
     in a variable (commit after `f06b150`; a unit test runs the script through osascript
     without a window). Reaches the real installation with the next release.
   - Done 2026-10-08, in the next release: the reason in TeX's own `.log`
     ([decision 0014](/decisions/0014-install-warnings-in-tex-log.md)); the prompt lists
     every package with its dependencies.
5. **Release transitions:** staying on a release is done ([decision 0006](/decisions/0006-release-transitions.md),
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
6. **Corpus and Phase 2 exit:** 51 documents; compared with a full TeX Live 2026
   (2026-10-08): pdfLaTeX and LuaLaTeX identical, XeLaTeX identical except TeX-tree
   fonts by name (a MennoTeX feature); crash recovery tested ([status](/project/status.md)).
   Fixes from this work reach the real installation with `mtx repair` run by the user
   from a new build of mtx: `mf.base`, the luaotfload rescan, `fontname`, the shadow
   rule (`xetex-pstricks`), crash recovery. The C part of crash recovery
   (`mtx_interrupted` in `mtx-ondemand.c`) needs a CI binary build (user's OK).
   Next: real arXiv sources kept locally; a Linux comparison for XeLaTeX fonts by name
   (fontconfig) would close the remaining gap.
7. **Known gaps** (each has a note in the knowledge bundle). The map re-read
   ([decision 0012](/decisions/0012-map-reread.md)) shipped in release `f06b1506f`; the
   C changes of decisions 0014 and 0015 ship with the next one.
   - LuaLaTeX fonts by name: **fixed** by the overlay `texmf-mtx/…/luaotfload-main.lua`
     ([decision 0005](/decisions/0005-fonts-by-name.md)). It assumes luaotfload's
     `resolvers.name` and `config.luaotfload.db.update_live` keep their shape; if a
     luaotfload update breaks `fontspec-by-name` under LuaLaTeX, look there first.
     Anonymous requests (`\font\x="Name"`) are not wrapped.
   - Font packages first needed after page 1: **fixed** 2026-10-08 by patch 0003
     (pdfTeX and LuaTeX re-read `pdftex.map` after an install; corpus `late-font-map`).
     Before, the run failed fatally (not the warning PLAN.md §5.7 expected).
   - METAFONT-only fonts (`bbm`) work since 2026-10-08: `mktexfmt mf.base` installs
     `metafont` and `modes`, and mktexpk makes the bitmap.
   - Roots that lack a map package: pdfTeX and LuaTeX install it during the compile
     ([decision 0015](/decisions/0015-font-map-on-miss.md)); `mtx repair` still fixes them ahead.
   - Format staleness: stamps ([decision 0016](/decisions/0016-format-stamps.md)).
   - `mtx gc` and docs on demand (`texdoc NAME`, `mtx docs PKG`) are done. texdoc's
     viewer path (opening a PDF) was not exercised, to avoid opening windows; only
     `texdoc -l -M`.
8. **Data refresh:** `crates/mtx-core/data/fontmaps.tsv.xz` and `fontnames.tsv.xz` come
   from `tools/build_fontmap_index.py` / `tools/build_fontname_index.py` (download all
   font packages once, ~850 MiB each, mostly shared). Regenerate when font packages
   change; ideally a manual CI job.
9. **PLAN.md drift:** "as built" notes now cover the layout (§5.2), binaries (§5.8), the
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
