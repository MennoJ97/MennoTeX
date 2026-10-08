# MennoTeX

A TeX distribution for Apple Silicon Macs, based on TeX Live 2026, that installs
packages on the fly. Setup downloads about 30 MB. The first time a document
needs a package (`\usepackage{tcolorbox}`, a font, a Lua module, a BibTeX style),
MennoTeX downloads it from TeX Live's repository, verifies it, and the compile
carries on. You get MiKTeX's convenience with TeX Live's engines, packages and
results, built natively for arm64.

**Status:** working, not yet released. pdfLaTeX, XeLaTeX and LuaLaTeX compile the
52-document test corpus on the first run from an empty installation, with the same
output as a full TeX Live 2026 (XeLaTeX differs only where MennoTeX finds TeX fonts by
name and a stock TeX Live on macOS does not).
[knowledge/project/status.md](knowledge/project/status.md) has the measurements.

## How it works

- **`mtx`**, a package manager written in Rust, keeps a verified copy of TeX Live's
  package database and builds a memory-mapped index from every file name to the
  package that ships it.
- **A small kpathsea patch** hooks TeX's file lookup. When a file is missing and the
  index knows a package that provides it, kpathsea runs `mtx ensure`. That installs
  the package and its dependencies and returns the path, so the running TeX finds
  the file. A miss costs a single binary search.
- **Packages come straight from tlnet**, unmodified. Each archive's size and SHA-512
  checksum must match TeX Live's GPG-signed package database. A bad mirror means
  switching to another mirror, never weaker checks.
- **Formats, font maps and hyphenation** are regenerated as packages arrive, the same
  way `tlmgr` would. Concurrent compiles share the installation safely.

The design is in [PLAN.md](PLAN.md). The [knowledge bundle](knowledge/index.md)
holds the up-to-date architecture, decisions and upstream facts.

## Getting started

You need Rust (`brew install rust`).

```bash
cargo build --release
./target/release/mtx bootstrap                 # creates ~/Library/MennoTeX/2026
~/Library/MennoTeX/current/bin/universal-darwin/mtx self-update
```

`bootstrap` installs a minimal TeX Live and points `~/Library/MennoTeX/current` at it.
`self-update` installs the newest signed MennoTeX release: its `mtx` and the patched
arm64 TeX Live programs built by CI. Before that step, on-demand installs go through
TeX Live's stock `mktex*` hooks, which cover less. (Until the first signed release is
published, `mtx install-binaries --github` fetches the programs instead; it needs
`gh`.) Then put the bin directory first on your `PATH`, through `current`, so moving
to the next TeX Live release later needs no PATH change:

```bash
export PATH="$HOME/Library/MennoTeX/current/bin/universal-darwin:$PATH"
pdflatex paper.tex
```

Releases are signed with minisign on the maintainer's Mac; `mtx` only installs a
release whose signature and checksums verify.

Use `--root DIR` (or `$MTX_ROOT`) to put an installation somewhere else, for example
a throw-away root for testing.

## Everyday commands

| Command | What it does |
|---|---|
| `mtx doctor` / `mtx repair` | Check the installation and environment; fix what doctor finds |
| `mtx self-update [--check]` | Install the newest signed release of MennoTeX itself (mtx and the TeX programs) |
| `mtx upgrade-release` | Install MennoTeX for the next TeX Live release next to this one, with the same packages, and switch `current` to it |
| `mtx install PKG…` / `mtx remove PKG…` | Install or remove packages by hand |
| `mtx prefetch FILE.tex` | Install everything a document statically needs in one go |
| `mtx search TEXT` / `mtx search --file TEXT` | Find packages by name or description, or by the files they ship, installed or not |
| `mtx which FILE` / `mtx info PKG` / `mtx list` | Find a file's package, show a package, list what is installed |
| `mtx update` / `mtx refresh` | Upgrade installed packages / check for a newer package database |
| `mtx config autoinstall yes\|no\|ask` | Choose whether missing packages are installed silently, never, or after asking (the prompt lists every package and what it brings along) |
| `mtx log [--problems]` | See what was installed, and why an install failed or was declined (TeX's log also gets a `Package mtx Warning` line) |
| `mtx gc [--days N]` | Remove on-demand packages no document has used for N days |
| `texdoc NAME` / `mtx docs PKG` | Documentation, also installed on demand |

`mtx help COMMAND` gives the details.

## How MennoTeX differs from TeX Live

MennoTeX is not TeX Live and is not made by the TeX Live team, so please report
problems here, not to TeX Live. The changes:

- **kpathsea** is patched to install missing files on demand
  ([`kpathsea-ondemand/`](kpathsea-ondemand/)). Every engine links it, so the change
  applies to all of them.
- **XeTeX** on macOS is patched to find fonts in the TeX tree by name, not only
  fonts registered with CoreText.
- **luaotfload** gets a small overlay (`luaotfload-main.lua`) so a font requested by
  name can be installed during a LuaLaTeX run.
- **Package management** is `mtx` instead of `tlmgr`. The configuration files it
  writes (`fmtutil.cnf`, `updmap.cfg`, `language.*`) follow tlmgr's rules.
- **Only arm64 macOS**, and only the packages your documents actually use.

Packages are not changed. Each one is installed exactly as TeX Live publishes it.

## Repository layout

| Path | Contents |
|---|---|
| `crates/mtx-core/` | Library: package database, file index, repository client, verification, installer, config generation |
| `crates/mtx/` | The `mtx` command-line tool |
| `kpathsea-ondemand/` | The kpathsea resolver and patches to TeX Live's sources |
| `build/` | `build-texlive.sh` and the pinned texlive-source revision |
| `.github/workflows/` | Manual CI build of the patched TeX Live binaries |
| `tests/` | Test documents and scripts that compile them on fresh installations |
| `tools/` | Index generators, tlpdb analysis, the knowledge-bundle checker |
| `knowledge/` | Curated project knowledge in [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) |

## Development

```bash
cargo build --release && cargo test
tests/run_documents.sh /tmp/root pdflatex      # after bootstrapping /tmp/root
python3 tools/check_okf.py knowledge
```

Building the TeX Live binaries locally takes about 20 minutes (`build/build-texlive.sh`).
The [development playbook](knowledge/playbooks/development.md) covers this, test
roots, and the pitfalls (such as other TeX installations shadowing yours on `PATH`).
Contributors, human or agent, start at [CLAUDE.md](CLAUDE.md).

## Use of AI

MennoTeX was written with an AI model: the code, patches and documentation (including
`knowledge/`) were largely written by Anthropic's Claude through Claude Code, under the
maintainer's direction, and every commit says so in a `Co-Authored-By` line. Changes are
checked with the unit tests and the document corpus, but they can still contain
mistakes; please report what you find.

## License

MennoTeX's own code is licensed under either of [Apache License 2.0](LICENSE-APACHE)
or [MIT](LICENSE-MIT), at your option. The patches to TeX Live programs keep the
licenses of the code they modify (LGPL-2.1-or-later for kpathsea, MIT/X11 for XeTeX).
The TeX Live programs and packages MennoTeX builds or installs keep their own
licenses. [LICENSING.md](LICENSING.md) has the details, including what
redistributing the binaries requires.

## Acknowledgements

MennoTeX is built on [TeX Live](https://tug.org/texlive/), a joint effort of the TeX
user groups; please consider [joining one](https://tug.org/usergroups.html).
Installing packages on the fly is [MiKTeX](https://miktex.org/)'s idea. MennoTeX
contains no MiKTeX code and is not affiliated with either project.
