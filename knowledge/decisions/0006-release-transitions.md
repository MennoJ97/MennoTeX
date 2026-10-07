---
type: Decision
title: Release transitions
description: When tlnet moves to the next TeX Live release, an installation stays on its own release via the frozen historic repository; moving up is a new MennoTeX build plus `mtx bootstrap --from`.
tags: [decision, release, tlnet, historic, upgrade]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T22:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T22:30:00Z }
---

# Context

tlnet serves one release at a time (`release/2026` in `00texlive.config`). In April it
switches to the next one, whose packages may need that release's engines and formats.
MennoTeX's engines are built from one release branch (`build/texlive-source.rev`) and
`mtx` manages one release (`root::RELEASE`). PLAN.md §4.3 asked for: detect the new
release, pin to `historic/systems/texlive/<release>/tlnet-final`, offer `mtx upgrade-release`.
Facts about the historic archive are in [tlnet](/upstream/tlnet.md).

# Decision

- **Staying.** `Ctx::refresh` checks the release of every database *after*
  verifying it. If tlnet serves a newer release, mtx records `newer_release`, sets the
  `repository` setting to `historic:<RELEASE>` and refreshes again. Installs and
  `mtx update` keep working, from the frozen final state. `mtx doctor` warns.
  This happens inside `refresh`, so `ensure` (compiles), `install`, `update` and
  `bootstrap` all get it.
- `historic:<release>` is resolved like a redirector: the historic mirrors are tried
  in order (setting `historic_mirrors`, else `repo::HISTORIC_MIRRORS`), avoided mirrors
  last, and the result is pinned like any mirror. Each candidate's checksum file must
  look like one, because a bot wall can answer 200 for everything.
- A repository from `$MTX_REPOSITORY`, or one that is already historic, is not
  switched; the release mismatch stays an error.
- **Lagging mirrors.** A mirror serving an *older* release is avoided for 24 h, like
  one serving bad data, and refresh retries once with another mirror.
- **Moving up** is not `mtx upgrade-release` in the old mtx. The old mtx cannot run the
  new release: it has neither the new engines nor the new `RELEASE`. Instead, the
  MennoTeX build for the new release bootstraps its own root side by side
  (`~/Library/MennoTeX/<new>`) with `mtx bootstrap --from <old root>`. That installs the
  old root's explicitly requested packages as explicit, and its on-demand ones as auto.
  Bootstrap and dependency packages are not carried over (the new release has its own
  core and closures). Names the new release no longer has are listed and skipped.

# Consequences

- Nothing breaks on the day tlnet switches: the next freshness check (TTL 1 h) moves
  the installation to the frozen repository. The frozen database usually differs from
  the last tlnet one, so the index is rebuilt once.
- A one-command upgrade from the old mtx needs a way to fetch a new MennoTeX build.
  That comes with release downloads (handoff item 2). `mtx doctor` names the
  command meanwhile.
- If no historic mirror has the frozen repository yet, refresh fails with "no historic
  mirror serves TeX Live <release>'s final repository". It is not treated as offline,
  and the setting stays, so mtx retries on the next refresh.
