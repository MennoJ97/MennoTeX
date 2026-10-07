#!/bin/sh
# Concurrency test: compile N documents at the same time against one
# installation, so that the runs race to install the same packages and to
# build the same format. Use a fresh root (bootstrap + install-binaries).
#   tests/run_concurrent.sh <root> [engine] [copies]
# Passes when every run exits 0 and `mtx doctor` reports no problem.
set -eu
root=$(cd "$1" && pwd)
engine=${2:-pdflatex}
copies=${3:-8}
here=$(cd "$(dirname "$0")" && pwd)
export PATH="$root/bin/universal-darwin:/usr/bin:/bin"
work=$(mktemp -d "${TMPDIR:-/tmp}/mtx-concurrent.XXXXXX")
docs="article-tcolorbox tables pgfplots-figures math-theorems chemistry beamer-talk"
i=0
start=$(date +%s)
while [ $i -lt "$copies" ]; do
  set -- $docs
  shift $((i % $#))
  doc=$1
  i=$((i + 1))
  mkdir -p "$work/$i"
  cp "$here/documents/$doc.tex" "$work/$i/"
  (cd "$work/$i" && "$engine" -interaction=nonstopmode -halt-on-error "$doc.tex" >run.out 2>&1; echo $? >status) &
done
wait
failed=0
for d in "$work"/*/; do
  [ "$(cat "$d/status")" = 0 ] || { failed=$((failed + 1)); echo "failed: $d"; grep '^!' "$d/run.out" | head -3; }
done
waited=$(grep -l 'waiting for another mtx' "$work"/*/run.out 2>/dev/null | wc -l | tr -d ' ')
echo "$copies parallel $engine runs in $(( $(date +%s) - start )) s: $failed failed, $waited waited for the install lock"
mtx doctor | grep -E 'PROBLEM|packages,' || true
[ "$failed" -eq 0 ] && ! mtx doctor | grep -q PROBLEM
