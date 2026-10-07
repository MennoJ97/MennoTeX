---
type: Reference
title: Building texlive-source on macOS
description: What it takes to build TeX Live natively on Apple Silicon, including the C23/libgd problem.
resource: https://github.com/TeX-Live/texlive-source
tags: [texlive, build, macos, arm64, autoconf]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:20:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:20:00Z }
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
- **Trunk is next year's development version.** In October 2026 trunk reports
  "TeX Live 2027/dev". Release sources are branches `tags/texlive-2026.0`,
  `tags/texlive-2026.1` (2026-03-17, includes the post-release dvipdfmx fix) and
  `branch2026` (same content as 2026.1); trunk was 324 commits ahead. Build the
  release matching tlnet's `release/` value.
- **`make world` = all + install + texlinks + `make check`.** Run in parallel, the
  test `euptexdir/euptex-ctrlsym` races with another test for its `euptests/`
  scratch directory and fails (118 of 122 web2c tests passed, 3 skipped); the
  engine output itself matched. The build script passes `check_target=all` to skip
  the in-build test run.
- **The 2026 release needs C++17 for XeTeX.** At `tags/texlive-2026.1`, web2c's
  `configure.ac` only requires C++11 (`AX_CXX_COMPILE_STDCXX([11])`), so XeTeX's C++
  is compiled with Apple clang's default standard, but the bundled ICU headers use
  C++17 (`std::is_same_v`, `auto` template parameters) and fail to compile. Trunk
  already requires C++17. The build script sets `CXX`/`OBJCXX` to `c++ -std=gnu++17`.
- **Never run `make all install` as one parallel make:** TeX Live's makefiles must
  serialize them (concurrent `tangle`/`convert` of `upbibtex.p` fails). For
  incremental rebuilds after a kpathsea change: `make -jN all && make install` in
  `Work/texk` (about 3 minutes: everything statically links kpathsea).
- Full build time on this Mac (12 cores): about 20 minutes. Output: 487 programs and
  links in `inst/bin/aarch64-apple-darwin25.6.0/`, 154 of them Mach-O.
- `xindy` is off by default (`--enable-xindy`); `asymptote` is not part of the default build.

[^readme]: texlive-source README.1prerequisites
