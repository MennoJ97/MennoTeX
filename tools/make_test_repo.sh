#!/bin/sh
# Generate crates/mtx-core/testdata/tlnet: a tiny fake TeX Live repository
# (plus tlnet-next and historic/, for release transitions)
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
# foo's documentation, as a separate container like tlnet's <pkg>.doc.tar.xz
d="$work/doc-foo"; mkdir -p "$d/doc/latex/foo"; echo "foo manual" > "$d/doc/latex/foo/foo-manual.pdf"
(cd "$d" && COPYFILE_DISABLE=1 tar --uid 0 --gid 0 -cf - doc/latex/foo/foo-manual.pdf) | xz -9e > "$out/archive/foo.doc.tar.xz"
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
  doc="$out/archive/$name.doc.tar.xz"
  if [ -f "$doc" ]; then
    echo "doccontainersize $(stat -f %z "$doc")"
    echo "doccontainerchecksum $(shasum -a 512 "$doc" | cut -d' ' -f1)"
    echo "docfiles size=1"
    xz -dc "$doc" | tar -tf - | sed 's|^| RELOC/|'
  fi
  echo "runfiles size=1"
  xz -dc "$a" | tar -tf - | grep -v '^tlpkg/' | sed 's|^| RELOC/|'
  echo
}
# write_repo DIR RELEASE REVISION FROZEN: the packages above as a signed
# repository declaring that release
write_repo() {
  dir=$1; mkdir -p "$dir/archive" "$dir/tlpkg"
  [ "$dir" = "$out" ] || cp "$out"/archive/*.tar.xz "$dir/archive/"
  {
    printf 'name 00texlive.config\ncategory TLCore\nrevision 1\n'
    printf 'depend frozen/%s\ndepend release/%s\ndepend revision/%s\n\n' "$4" "$2" "$3"
    entry bar 1
    entry fonts-x 1
    entry foo 1 bar
  } > "$dir/tlpkg/texlive.tlpdb"
  (cd "$dir/tlpkg" && shasum -a 512 texlive.tlpdb > texlive.tlpdb.sha512 && xz -9e texlive.tlpdb)
  gpg --batch --quiet --armor --detach-sign -o "$dir/tlpkg/texlive.tlpdb.sha512.asc" "$dir/tlpkg/texlive.tlpdb.sha512"
}
# tlnet today; tlnet after the next release; this release's frozen final
# repository as TeX Live's historic archive lays it out
next="$here/crates/mtx-core/testdata/tlnet-next"
historic="$here/crates/mtx-core/testdata/historic"
rm -rf "$next" "$historic"
write_repo "$out" 2026 4242 0
write_repo "$next" 2027 5000 0
write_repo "$historic/systems/texlive/2026/tlnet-final" 2026 4300 1
echo "test repositories written to $out, $next, $historic (key $fpr)"
