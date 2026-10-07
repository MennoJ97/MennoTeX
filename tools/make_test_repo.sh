#!/bin/sh
# Generate crates/mtx-core/testdata/tlnet: a tiny fake TeX Live repository
# signed with a throw-away test key that has TeX Live's key structure
# (certification-only primary key + signing subkey). Dev-time only; needs
# gpg. The output is committed so tests run offline and without gpg.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out="$here/crates/mtx-core/testdata/tlnet"
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
export GNUPGHOME="$work/gnupg"; mkdir -m 700 "$GNUPGHOME"
rm -rf "$out"; mkdir -p "$out/archive" "$out/tlpkg"

gpg --batch --quiet --passphrase '' --quick-gen-key "MennoTeX test repository <test@invalid>" ed25519 cert never
fpr=$(gpg --batch --with-colons --list-keys | awk -F: '/^fpr/{print $10; exit}')
gpg --batch --quiet --passphrase '' --quick-add-key "$fpr" ed25519 sign never
gpg --batch --armor --export "$fpr" > "$here/crates/mtx-core/testdata/test-key.asc"
printf '%s\n' "$fpr" | tr 'A-F' 'a-f' > "$here/crates/mtx-core/testdata/test-key.fpr"

# make_pkg NAME RELOCATED FILE... : archive with the given root-relative files
make_pkg() {
  name=$1; reloc=$2; shift 2
  d="$work/pkg-$name"; mkdir -p "$d"
  for f in "$@"; do
    rel=$f; [ "$reloc" = 1 ] && rel=${f#texmf-dist/}
    mkdir -p "$d/$(dirname "$rel")"; printf '%% %s from %s\n' "$f" "$name" > "$d/$rel"
  done
  mkdir -p "$d/tlpkg/tlpobj"; echo "name $name" > "$d/tlpkg/tlpobj/$name.tlpobj"
  (cd "$d" && COPYFILE_DISABLE=1 tar --uid 0 --gid 0 -cf - $(cd "$d" && find . -type f | sed 's|^\./||' | sort) ) | xz -9e > "$out/archive/$name.tar.xz"
}
make_pkg foo 1 texmf-dist/tex/latex/foo/foo.sty texmf-dist/tex/latex/foo/foo.lua
make_pkg bar 1 texmf-dist/tex/latex/bar/bar.sty
make_pkg fonts-x 1 texmf-dist/fonts/tfm/public/x/x10.tfm

entry() { # name reloc depends... ; prints a tlpdb record
  name=$1; reloc=$2; shift 2
  a="$out/archive/$name.tar.xz"
  echo "name $name"; echo "category Package"; echo "revision 7"
  [ "$reloc" = 1 ] && echo "relocated 1"
  for d in "$@"; do echo "depend $d"; done
  echo "containersize $(stat -f %z "$a")"
  echo "containerchecksum $(shasum -a 512 "$a" | cut -d' ' -f1)"
  echo "runfiles size=1"
  xz -dc "$a" | tar -tf - | grep -v '^tlpkg/' | sed 's|^| RELOC/|'
  echo
}
{
  printf 'name 00texlive.config\ncategory TLCore\nrevision 1\ndepend release/2026\ndepend revision/4242\n\n'
  entry bar 1
  entry fonts-x 1
  entry foo 1 bar
} > "$out/tlpkg/texlive.tlpdb"
(cd "$out/tlpkg" && shasum -a 512 texlive.tlpdb > texlive.tlpdb.sha512 && xz -9e texlive.tlpdb)
gpg --batch --quiet --armor --detach-sign -o "$out/tlpkg/texlive.tlpdb.sha512.asc" "$out/tlpkg/texlive.tlpdb.sha512"
echo "test repository written to $out (key $fpr)"
