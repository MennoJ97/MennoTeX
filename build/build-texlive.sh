#!/bin/sh
# Build TeX Live's programs natively for Apple Silicon from texlive-source,
# with MennoTeX's kpathsea on-demand patch applied.
#
#   build/build-texlive.sh <texlive-source checkout> [--no-patch] [--incremental]
#
# The checkout should be at the revision in build/texlive-source.rev.
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
  cp "$here/kpathsea-ondemand/mtx-ondemand.c" "$src/texk/kpathsea/mtx-ondemand.c"
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
cd "$src"
build_args="--disable-xdvik --without-x"
if [ $clean = yes ]; then
  exec env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C \
    MACOSX_DEPLOYMENT_TARGET=13.0 TL_MAKE_FLAGS="-j$jobs" "$c23_off" \
    ./Build $build_args
else
  # Rebuild after source changes without reconfiguring (kpathsea is linked
  # statically, so every program is relinked).
  cd Work
  exec env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C \
    MACOSX_DEPLOYMENT_TARGET=13.0 "$c23_off" make -j"$jobs" world
fi
