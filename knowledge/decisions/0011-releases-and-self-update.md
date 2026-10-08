---
type: Decision
title: Signed releases and self-update
description: Releases (mtx plus TeX Live's programs) are drafted by CI, signed with minisign on the maintainer's Mac and published; mtx self-update and upgrade-release install only releases whose SHA256SUMS signature and checksums verify, and a current symlink makes release upgrades a PATH-free switch.
tags: [decision, release, signing, minisign, self-update, upgrade]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T19:40:00Z }
sources:
  - id: minisign
    resource: https://jedisct1.github.io/minisign/
    title: Minisign, a tool to sign files and verify signatures
    author: frank-denis
---

# Context

Until 2026-10-08 a release held only TeX Live's programs
(`mennotex-bin-<release>-<rev>`), so `mtx` itself was updated by building it from
source and running `mtx repair` from the new build, and nothing could move an
installation to the next TeX Live release (PLAN.md §5.8, §5.10: `mtx self-update`,
`mtx upgrade-release`, a minisign-signed manifest). The user asked for both together
with the signed manifest, and chose to keep the signing key on their Mac.

# Decision

- **A release** is tagged `mennotex-<TeX Live release>-<commit>` and holds
  `mtx-<release>-<commit>-arm64-darwin.tar.xz` (mtx, `VERSION` with `release`,
  `commit`, `commit_time`, `binaries`, license files), TeX Live's programs
  `mennotex-bin-<release>-<rev>.<patches>-arm64-darwin.tar.xz` (`<patches>` = 8 hex
  digits of the git tree hashes of `kpathsea-ondemand/` and `build/`), the
  corresponding source `mennotex-src-…`, `SHA256SUMS` and `SHA256SUMS.minisig`.
- **CI** (`build-binaries.yml`, manual) builds mtx every run and TeX Live's programs
  only when no earlier release has an archive with the same name (`rebuild` forces
  it); with `release` it creates a **draft**.
- **Signing on the maintainer's Mac:** `tools/sign_release.sh <tag>` downloads the
  draft, checks every file against `SHA256SUMS`, signs `SHA256SUMS` with minisign[^minisign]
  (Ed25519, prehashed) with the trusted comment `mennotex release <tag>`, uploads the
  signature and publishes. The secret key (`~/.minisign/mennotex.key`, password
  protected) never leaves that Mac; mtx embeds the public key
  (`crates/mtx-core/data/release-key.pub`). Drafts are invisible to mtx.
- **Verification** (`release.rs`): before anything from a release is used, the
  signature must verify with the embedded key, its trusted comment must name the tag
  derived from the mtx archive's name, every file present must be listed in
  `SHA256SUMS` with a matching SHA-256, and `VERSION` must name that tag and a present
  programs archive.
- **`mtx self-update`** takes the newest published release for this TeX Live
  release (or `--release TAG`, or `--from DIR`), compares commit times with the
  **running** mtx (an older release needs `--force`; `--check` only reports), compares
  the release's program with the **installed** `bin/universal-darwin/mtx` (they differ
  when a local build updates an installation, as for the first self-update; found
  2026-10-08 before the first release was signed), swaps it atomically if it differs, then lets the **new** mtx install the programs (if `binaries_build`
  differs) and run `mtx repair`. Old versions therefore only need to download,
  verify and swap.
- **`mtx upgrade-release`** takes the newest release for a newer TeX Live, runs its
  mtx as `bootstrap --from <old root>` into `<parent>/<new release>`
  ([decision 0006](/decisions/0006-release-transitions.md)), installs its programs,
  and points `<parent>/current` at the new root. The old root is left alone.
- **`…/MennoTeX/current`**: bootstrap creates it (→ its release) when it does not
  exist; a PATH entry through `current/bin/universal-darwin` survives release
  upgrades.

# Consequences

- The first releases' mtx can only verify releases signed with this key; a key
  change needs a release signed with the old key that carries the new one.
- `mtx install-binaries --github` still trusts GitHub access and `SHA256SUMS` only;
  it is the expert path, self-update is the normal one.
- The repository is public (the user, 2026-10-08), so mtx downloads releases over
  plain HTTPS (`api.github.com` for the list, which never shows drafts;
  `github.com/<repo>/releases/download/<tag>/<file>` for files) and needs no `gh`;
  it falls back to `gh` if that fails (a private repository). A `.pkg` or Homebrew
  cask can use the same signed files.
- Tested 2026-10-08 with test builds signed by a throwaway key: `self-update --check`,
  update (mtx swapped, programs installed, repair), "up to date" on a second run, a
  tampered archive and an unsigned release refused; `upgrade-release` mechanics with a
  fake "2027" release (new root next to the old one with the same packages, `current`
  moved, a second run refused). Not yet tested: a real signed release from CI, and a
  real TeX Live 2027.
