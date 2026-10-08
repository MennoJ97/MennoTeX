#!/bin/sh
# Unit tests for the kpathsea on-demand resolver
# (kpathsea-ondemand/mtx-ondemand.c), compiled against the kpathsea library
# of a texlive-source build made by build/build-texlive.sh:
#   tests/run_c_tests.sh <texlive-source checkout>
# The tests read kpathsea-ondemand/tests/fixture.idx (written by mtx's Rust
# code; `cargo test` checks it is current) and use a fake mtx that records
# its arguments.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=$(cd "$1" && pwd)
lib=$src/Work/texk/kpathsea/.libs/libkpathsea.a
[ -f "$lib" ] || { echo "no $lib: build texlive-source first" >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/mtx-ctest.XXXXXX")
trap 'rm -rf "$work"' EXIT

# MAKE_KPSE_DLL: the resolver is part of kpathsea and uses its internal
# declarations (as kpathsea's own standalone tests are built).
cc -std=gnu17 -Wall -Wno-unused-function -DMAKE_KPSE_DLL -I"$src/Work/texk" -I"$src/texk" \
  -o "$work/ondemand-test" "$here/kpathsea-ondemand/tests/ondemand-test.c" "$lib"

root=$work/root
mkdir -p "$root/tlpkg/mtx/journal" "$root/texmf-dist" "$work/bin" "$work/cnf"
cp "$here/kpathsea-ondemand/tests/fixture.idx" "$root/tlpkg/mtx/files.idx"
# TeX Live 2026's search orders for the formats under test.
cat > "$work/cnf/texmf.cnf" <<'EOF'
TEXMF = $TEXMFROOT/texmf-dist
TEXMFDBS = $TEXMF
TEXINPUTS = .;$TEXMF/tex/{latex,generic,}//
TEXINPUTS.pdflatex = .;$TEXMF/tex/{latex,generic,}//
TEXINPUTS.xelatex = .;$TEXMF/tex/{xelatex,latex,xetex,generic,}//
TFMFONTS = .;$TEXMF/fonts/tfm//
EOF
# mtx's font-map table: fonts a package's map covers, sorted bytewise.
printf 'a-first\tx\nfonty10\tfonty\nfonty10x\tother\nnothing-to-do\tz\nzz-last\ty\n' \
  > "$root/tlpkg/mtx/fontmaps.tsv"
cat > "$work/bin/mtx" <<'EOF'
#!/bin/sh
# Fake mtx for `ensure --package P --path REL --siblings -- NAME`: makes the
# file (and a sibling for alpha), prints the paths, finishes the package.
# `broken` fails with $MTX_TEST_EXIT (default 4, the install failed). For
# `ensure --font-map --siblings FONT`: fonty10's map package is installed
# (prints a file), other fonts need nothing (exit 1).
echo "$*" >> "$MTX_TEST_CALLS"
if [ "$2" = --font-map ]; then
  [ "$4" = fonty10 ] || exit 1
  echo "$TEXMFROOT/texmf-dist/fonts/map/dvips/fonty/fonty.map"
  exit 0
fi
[ "$3" = broken ] && exit "${MTX_TEST_EXIT:-4}"
mkdir -p "$(dirname "$TEXMFROOT/$5")" && : > "$TEXMFROOT/$5"
echo "$TEXMFROOT/$5"
if [ "$3" = alpha ]; then
  : > "$TEXMFROOT/texmf-dist/tex/latex/alpha/alpha.lua"
  echo "$TEXMFROOT/texmf-dist/tex/latex/alpha/alpha.lua"
fi
rm -f "$TEXMFROOT/tlpkg/mtx/journal/$3"
EOF
chmod +x "$work/bin/mtx"

TEXMFCNF=$work/cnf TEXMFROOT=$root MTX_TEST_CALLS=$work/calls MTX_TEST_BIN=$work/bin \
  "$work/ondemand-test"
