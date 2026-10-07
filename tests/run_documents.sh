#!/bin/sh
# Compile test documents with a MennoTeX installation and report what was
# installed on demand. Usage:
#   tests/run_documents.sh <root> [engine] [document.tex ...]
# With no documents, compiles every tests/documents/*.tex.
# PATH is reduced to the installation plus system directories so that other
# TeX installations (e.g. MiKTeX symlinks in /usr/local/bin) cannot interfere.
set -eu
root=$(cd "$1" && pwd); shift
engine=${1:-pdflatex}; [ $# -gt 0 ] && shift
here=$(cd "$(dirname "$0")" && pwd)
[ $# -gt 0 ] || set -- "$here"/documents/*.tex
export PATH="$root/bin/universal-darwin:/usr/bin:/bin"
work=$(mktemp -d "${TMPDIR:-/tmp}/mtx-docs.XXXXXX")
status=0
for doc in "$@"; do
  name=$(basename "$doc" .tex)
  cp "$doc" "$work/"
  before=$(mtx list | wc -l | tr -d ' ')
  start=$(date +%s)
  # Two runs, like a real build, so cross-references settle.
  if (cd "$work" && "$engine" -interaction=nonstopmode -halt-on-error "$name.tex" >"$name.run1" 2>&1 &&
      "$engine" -interaction=nonstopmode -halt-on-error "$name.tex" >"$name.run2" 2>&1); then
    result=ok
  else
    result=FAILED; status=1
  fi
  end=$(date +%s)
  after=$(mtx list | wc -l | tr -d ' ')
  pages=$(grep -ho "Output written on [^ ]* ([0-9]* page" "$work/$name.run2" 2>/dev/null | grep -o "([0-9]*" | tr -d '(' || true)
  printf '%-28s %-8s %-6s pages=%-3s new-packages=%-4s seconds=%s\n' \
    "$name" "$engine" "$result" "${pages:-?}" "$((after - before))" "$((end - start))"
  if [ "$result" = FAILED ]; then
    grep -E "^!|^l\.[0-9]|^mtx:|not found" "$work/$name.run1" "$work/$name.run2" 2>/dev/null | head -20
  fi
done
echo "logs: $work"
exit $status
