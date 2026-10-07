#!/bin/sh
# Compile test documents with a MennoTeX installation and report what was
# installed on demand. Usage:
#   tests/run_documents.sh <root> [engine] [document.tex ...]
# With no documents, compiles every tests/documents/*.tex.
#
# Header comments in a document control the run:
#   % engines: pdflatex xelatex lualatex   (skipped for other engines)
#   % tools: biber | bibtex | makeindex | makeglossaries
#   % prefetch: lualatex     (run `mtx prefetch` first for these engines)
# Each document is compiled, its tools run, then compiled again (a third
# time when tools ran), like a real build.
#
# PATH is reduced to the installation plus system directories so that other
# TeX installations (e.g. MiKTeX symlinks in /usr/local/bin) cannot interfere.
set -eu
root=$(cd "$1" && pwd); shift
engine=${1:-pdflatex}; [ $# -gt 0 ] && shift
here=$(cd "$(dirname "$0")" && pwd)
[ $# -gt 0 ] || set -- "$here"/documents/*.tex
export PATH="$root/bin/universal-darwin:/usr/bin:/bin"
work=$(mktemp -d "${TMPDIR:-/tmp}/mtx-docs.XXXXXX")
pass=0; fail=0; skip=0

tex() { (cd "$work" && "$engine" -interaction=nonstopmode -halt-on-error "$1.tex"); }

for doc in "$@"; do
  name=$(basename "$doc" .tex)
  engines=$(sed -n 's/^% engines: *//p' "$doc" | head -1)
  tools=$(sed -n 's/^% tools: *//p' "$doc" | head -1)
  prefetch=$(sed -n 's/^% prefetch: *//p' "$doc" | head -1)
  if [ -n "$engines" ] && ! echo " $engines " | grep -q " $engine "; then
    printf '%-26s %-8s skip   (engines: %s)\n' "$name" "$engine" "$engines"
    skip=$((skip + 1)); continue
  fi
  cp "$doc" "$work/"
  before=$(mtx list | wc -l | tr -d ' ')
  start=$(date +%s)
  result=ok
  if echo " $prefetch " | grep -q " $engine "; then
    mtx prefetch "$work/$name.tex" >"$work/$name.prefetch" 2>&1 || result="FAILED(prefetch)"
  fi
  if [ "$result" != ok ]; then
    :
  elif ! tex "$name" >"$work/$name.run1" 2>&1; then
    result=FAILED
  else
    for tool in $tools; do
      case $tool in
        makeindex) cmd="makeindex $name.idx" ;;
        *) cmd="$tool $name" ;;
      esac
      if ! (cd "$work" && $cmd) >"$work/$name.$tool" 2>&1; then
        result="FAILED($tool)"; break
      fi
    done
    if [ "$result" = ok ]; then
      tex "$name" >"$work/$name.run2" 2>&1 || result=FAILED
      if [ "$result" = ok ] && [ -n "$tools" ]; then
        tex "$name" >"$work/$name.run3" 2>&1 || result=FAILED
      fi
    fi
  fi
  end=$(date +%s)
  after=$(mtx list | wc -l | tr -d ' ')
  last=$(ls "$work/$name".run* | tail -1)
  pages=$(grep -ho "Output written on [^ ]* ([0-9]* page" "$last" 2>/dev/null | grep -o "([0-9]*" | tr -d '(' || true)
  printf '%-26s %-8s %-14s pages=%-3s new-packages=%-4s seconds=%s\n' \
    "$name" "$engine" "$result" "${pages:-?}" "$((after - before))" "$((end - start))"
  if [ "$result" = ok ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    grep -hE "^!|^l\.[0-9]|^mtx:.*(error|cannot)|not found|ERROR" "$work/$name".* 2>/dev/null | head -8 | sed 's/^/    /'
  fi
done
echo "$engine: $pass ok, $fail failed, $skip skipped; logs: $work"
[ $fail -eq 0 ]
