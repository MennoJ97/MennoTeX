#!/bin/sh
# Build TeX Live's programs natively for Apple Silicon from texlive-source,
# with MennoTeX's kpathsea on-demand patch applied.
#
#   build/build-texlive.sh <texlive-source checkout> [--no-patch] [--incremental]
#
# The checkout should be at the revision in build/texlive-source.rev: the
# TeX Live release branch matching the tlnet release mtx manages (trunk is
# already next year's development version), e.g.
#   git clone --depth 1 --branch tags/texlive-2026.1 \
#     https://github.com/TeX-Live/texlive-source.git
# Output: <checkout>/inst/bin/<triplet>/ (binaries), <checkout>/Work/build.log.
#
# The environment is reduced to system tools on purpose: with Homebrew's
# pkg-config on PATH, configure could link Homebrew dylibs instead of the
# libraries bundled in texlive-source, and the binaries would not be
# relocatable.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=$(cd "$1" && pwd); shift
patch=yes; clean=yes
for a in "$@"; do
  case $a in
    --no-patch) patch=no ;;
    --incremental) clean=no ;;
    *) echo "unknown option $a" >&2; exit 2 ;;
  esac
done

want=$(cat "$here/build/texlive-source.rev")
have=$(git -C "$src" rev-parse HEAD 2>/dev/null || echo unknown)
[ "$want" = "$have" ] || echo "warning: $src is at $have, expected $want" >&2

if [ $patch = yes ]; then
  # The resolver lives in its own file, included by the patched tex-make.c,
  # so TeX Live's automake files need no changes.
  # Copy only on change: a new timestamp alone relinks every program.
  cmp -s "$here/kpathsea-ondemand/mtx-ondemand.c" "$src/texk/kpathsea/mtx-ondemand.c" \
    || cp "$here/kpathsea-ondemand/mtx-ondemand.c" "$src/texk/kpathsea/mtx-ondemand.c"
  for p in "$here"/kpathsea-ondemand/patches/*.patch; do
    [ -e "$p" ] || continue
    if git -C "$src" apply --reverse --check "$p" 2>/dev/null; then
      echo "already applied: $(basename "$p")"
    else
      git -C "$src" apply "$p"
      echo "applied: $(basename "$p")"
    fi
  done
fi

jobs=$(sysctl -n hw.ncpu)
# Autoconf 2.72 switches to the newest C standard the compiler supports;
# with Apple clang 21 that is C23 (-std=gnu23), which rejects the K&R
# function definitions in the bundled libgd (gd_nnquant.c). Staying on the
# compiler's default (C17) builds everything unchanged.
c23_off="ac_cv_prog_cc_c23=no"
# The 2026 release's web2c only asks for C++11, so XeTeX is compiled with
# the compiler's default C++ standard, but the bundled ICU's headers need
# C++17 (std::is_same_v, auto template parameters). Trunk has since raised
# web2c to C++17 (AX_CXX_COMPILE_STDCXX([17])); do the same here.
cxx="CXX=c++ -std=gnu++17"
objcxx="OBJCXX=c++ -std=gnu++17"
cd "$src"
build_args="--disable-xdvik --without-x"
# `make world` ends with the full test suite. Run in parallel, one e-upTeX
# test (euptex-ctrlsym) races with another for its scratch directory and
# fails the build, so skip it here; run `make check` separately.
make_flags="-j$jobs check_target=all"
if [ $clean = yes ]; then
  exec env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C \
    MACOSX_DEPLOYMENT_TARGET=13.0 TL_MAKE_FLAGS="$make_flags" "$c23_off" "$cxx" "$objcxx" \
    ./Build $build_args
else
  # Rebuild after source changes without reconfiguring (kpathsea is linked
  # statically, so every program is relinked). `all` and `install` must not
  # run in one parallel make (they race on generated sources).
  cd Work
  env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C \
    MACOSX_DEPLOYMENT_TARGET=13.0 "$c23_off" "$cxx" "$objcxx" make -j"$jobs" all \
    && env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C make install
fi
