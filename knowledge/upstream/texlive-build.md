---
type: Reference
title: Building texlive-source on macOS
description: What it takes to build TeX Live natively on Apple Silicon, including the C23/libgd problem.
resource: https://github.com/TeX-Live/texlive-source
tags: [texlive, build, macos, arm64, autoconf]
status: draft
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T12:30:00Z }
sources:
  - id: readme
    resource: https://github.com/TeX-Live/texlive-source/blob/trunk/README.1prerequisites
    title: texlive-source README.1prerequisites
    author: team:tex-live
---

# Requirements

C and C++11 compilers, GNU make (`/usr/bin/make` on macOS is GNU make), Python
(ICU tests only) and Perl.[^readme] All libraries are bundled in `libs/`; XeTeX
uses CoreText/ApplicationServices on macOS (no fontconfig). The build host
(macOS 26.6, Apple clang 21, 12 cores) has everything via the Command Line Tools.

# How MennoTeX builds

`build/build-texlive.sh <checkout>` runs TeX Live's `./Build --disable-xdvik --without-x`
with `MACOSX_DEPLOYMENT_TARGET=13.0`, `TL_MAKE_FLAGS=-jN`, and an environment reduced to
`/usr/bin:/bin:/usr/sbin:/sbin`, so Homebrew's `pkg-config` and libraries in
`/opt/homebrew` cannot leak into the binaries. Output goes to `<checkout>/inst/bin/<triplet>/`
(here `aarch64-apple-darwin25.6.0`). The pinned revision is in `build/texlive-source.rev`.

# Pitfalls

- **C23 breaks the bundled libgd.** Autoconf 2.72's `AC_PROG_CC` selects the newest
  C standard the compiler supports; with Apple clang 21 that is `-std=gnu23`, and C23
  removed K&R function definitions, which `libs/gd/libgd-src/src/gd_nnquant.c` uses
  (`error: unknown type name 'nnq'`). Setting the autoconf cache variable
  `ac_cv_prog_cc_c23=no` keeps the compiler default (C17). Observed 2026-10-07 at
  revision 7cd76c1.
- `xindy` is off by default (`--enable-xindy`); `asymptote` is not part of the default build.

[^readme]: texlive-source README.1prerequisites
