#!/bin/sh
# Crash recovery: kill mtx in the middle of an install, then check that the
# next use of the root finishes it. Usage:
#   tests/run_crash.sh <root>
# The root needs MennoTeX's binaries (kpathsea hook mode) and network access.
# Each case installs a font package that is not installed yet:
#   unpacked     killed after unpacking a package, before the database records it
#   recorded     killed after the database records it, before ls-R is updated
#   listed       killed after ls-R, before configuration and font maps
#   regenerated  killed after the font maps, before the journal is cleared
#   kill-9       killed from outside with kill -9 while unpacking cm-super (62 MiB)
# The first four use MTX_CRASH_AT, where mtx sends itself SIGKILL.
# Recovery is tried in order: a lookup of a file of the package (what the next
# TeX run does), then an on-demand install of another package; the report
# says which one finished the install. Afterwards mtx doctor must report no
# interrupted install, leftover staging or problem, the package's font map
# must be in pdftex.map, and pdfLaTeX must compile a document using the font.
set -eu
root=$(cd "$1" && pwd)
export PATH="$root/bin/universal-darwin:/usr/bin:/bin"
journal=$root/tlpkg/mtx/journal
pdfmap=$root/texmf-var/fonts/map/pdftex/updmap/pdftex.map
work=$(mktemp -d "${TMPDIR:-/tmp}/mtx-crash.XXXXXX")
fail=0

installed() { mtx --root "$root" list 2>/dev/null | awk '{print $1}' | grep -qx "$1"; }
journaled() { [ -n "$(ls -A "$journal" 2>/dev/null)" ]; }

run_case() { # point package file-to-look-up map-or-- other-file preamble
  point=$1 pkg=$2 file=$3 map=$4 other=$5 preamble=$6
  if installed "$pkg"; then
    printf '%-12s %-17s skipped (already installed)\n' "$point" "$pkg"
    return
  fi
  status=0
  if [ "$point" = kill-9 ]; then
    mtx --root "$root" install "$pkg" >"$work/$point.install" 2>&1 &
    pid=$!
    while ! journaled && kill -0 $pid 2>/dev/null; do sleep 0.01; done
    kill -9 $pid 2>/dev/null || true
    wait $pid || status=$?
  else
    MTX_CRASH_AT=$point mtx --root "$root" install "$pkg" >"$work/$point.install" 2>&1 || status=$?
  fi
  problems=""
  [ "$status" = 137 ] || problems="$problems not-killed(exit=$status)"
  journaled || problems="$problems no-journal-entry"
  interrupted=$(ls "$journal" 2>/dev/null | tr '\n' ' ')
  mtx --root "$root" doctor 2>&1 | grep -q "interrupted installs" || problems="$problems doctor-silent"

  # The next use: a lookup of the package's own file, then any other install.
  how=none
  kpsewhich "$file" >"$work/$point.lookup" 2>&1 || true
  if ! journaled; then
    how="lookup of $file"
  else
    kpsewhich "$other" >>"$work/$point.lookup" 2>&1 || true
    journaled || how="install of $other"
  fi
  [ "$how" != none ] || problems="$problems not-finished"

  mtx --root "$root" doctor >"$work/$point.doctor" 2>&1 || true
  if grep -qiE "^ *problem|interrupted|staging" "$work/$point.doctor"; then
    problems="$problems doctor:$(grep -iE '^ *problem|interrupted|staging' "$work/$point.doctor" | head -1 | tr -s ' ')"
  fi
  if [ "$map" != -- ] && ! grep -qx "% $map" "$pdfmap"; then
    problems="$problems $map-not-in-pdftex.map"
  fi
  printf '\\documentclass{article}\n%s\n\\begin{document}\nText and math $x^2+\\alpha$.\n\\end{document}\n' \
    "$preamble" >"$work/$point.tex"
  if ! (cd "$work" && pdflatex -interaction=nonstopmode -halt-on-error "$point.tex") >"$work/$point.run" 2>&1; then
    problems="$problems compile-failed"
  elif grep -qiE "map file .* not found|cannot open Type 1|font .* not found|pk font" "$work/$point.log"; then
    problems="$problems font-warning"
  fi

  if [ -z "$problems" ]; then result=ok; else result="FAILED:$problems"; fail=$((fail + 1)); fi
  printf '%-12s %-17s interrupted: %-30s finished by: %-34s %s\n' "$point" "$pkg" "$interrupted" "$how" "$result"
}

# First: later cases' T1 documents install cm-super as a font-map package.
run_case kill-9      cm-super         sfrm1000.pfb         --              mdframed.sty  '\usepackage[T1]{fontenc}'
run_case unpacked    fourier          fourier.sty          fourier.map     lipsum.sty    '\usepackage{fourier}'
run_case recorded    kpfonts          kpfonts.sty          kpfonts.map     blindtext.sty '\usepackage{kpfonts}'
run_case listed      libertinus-type1 libertinus-type1.sty libertinus.map  enumitem.sty  '\usepackage[T1]{fontenc}\usepackage{libertinus-type1}'
run_case regenerated tex-gyre         tgpagella.sty        qpl.map         csquotes.sty  '\usepackage[T1]{fontenc}\usepackage{tgpagella}'
echo "crash cases: $fail failed; logs: $work"
[ $fail -eq 0 ]
