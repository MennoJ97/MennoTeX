#!/bin/sh
# Sign a draft release made by .github/workflows/build-binaries.yml and
# publish it (decision 0011). Run on the maintainer's Mac, where the secret
# key lives; minisign asks for its password.
#   tools/sign_release.sh <tag> [secret key, default ~/.minisign/mennotex.key]
# The key pair was made with
#   minisign -G -p crates/mtx-core/data/release-key.pub -s ~/.minisign/mennotex.key
# and mtx embeds the public key, so it only installs releases signed here.
set -eu
tag=$1
key=${2:-$HOME/.minisign/mennotex.key}
repo=${MTX_REPO:-MennoJ97/MennoTeX}
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

case $tag in mennotex-20[0-9][0-9]-*) ;; *) echo "not a MennoTeX release tag: $tag" >&2; exit 2 ;; esac
gh release view "$tag" -R "$repo" --json isDraft --jq .isDraft | grep -qx true \
  || { echo "$tag is not a draft release" >&2; exit 1; }
gh release download "$tag" -R "$repo" -D "$work"
cd "$work"
rm -f SHA256SUMS.minisig
# What gets signed must be exactly what was uploaded.
shasum -a 256 -c SHA256SUMS
for f in *; do
  [ "$f" = SHA256SUMS ] || grep -q "  $f\$" SHA256SUMS || { echo "$f is not in SHA256SUMS" >&2; exit 1; }
done
# Show what is being signed.
mtx_archive=$(ls mtx-*-arm64-darwin.tar.xz)
tar -xJf "$mtx_archive"
"${mtx_archive%.tar.xz}/mtx" --version
cat "${mtx_archive%.tar.xz}/VERSION"
rm -rf "${mtx_archive%.tar.xz}"
minisign -S -s "$key" -m SHA256SUMS -t "mennotex release $tag"
minisign -V -p "$here/crates/mtx-core/data/release-key.pub" -m SHA256SUMS
gh release upload "$tag" SHA256SUMS.minisig -R "$repo" --clobber
gh release edit "$tag" -R "$repo" --draft=false --latest
echo "published $tag"
