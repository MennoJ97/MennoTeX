#!/bin/sh
# Regenerate crates/mtx-core/testdata/release: a tiny MennoTeX release signed
# with a throwaway minisign key, for the release.rs tests. The secret key is
# deleted afterwards; only the public key is kept.
#   tools/make_test_release.sh        (needs minisign: brew install minisign)
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/crates/mtx-core/testdata/release
tag=mennotex-2026-0123456ab
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rm -rf "$out"
mkdir -p "$out" "$work/mtx-2026-0123456ab-arm64-darwin" "$work/mennotex-bin-2026-6a3001880.0f0f0f0f-arm64-darwin"
cd "$work"
printf '#!/bin/sh\necho "fake mtx $*"\n' > mtx-2026-0123456ab-arm64-darwin/mtx
chmod 755 mtx-2026-0123456ab-arm64-darwin/mtx
cat > mtx-2026-0123456ab-arm64-darwin/VERSION <<EOF
release=2026
commit=0123456ab
commit_time=4102444800
binaries=mennotex-bin-2026-6a3001880.0f0f0f0f-arm64-darwin
EOF
printf 'not a program\n' > mennotex-bin-2026-6a3001880.0f0f0f0f-arm64-darwin/README
# Fixed owner and times keep the archives stable.
for d in mtx-2026-0123456ab-arm64-darwin mennotex-bin-2026-6a3001880.0f0f0f0f-arm64-darwin; do
  tar --uid 0 --gid 0 --uname root --gname wheel -cJf "$out/$d.tar.xz" "$d"
done
cd "$out"
shasum -a 256 mtx-*.tar.xz mennotex-bin-*.tar.xz > SHA256SUMS
minisign -G -W -p "$out/test-release.pub" -s "$work/test.key" >/dev/null
minisign -S -s "$work/test.key" -m SHA256SUMS -t "mennotex release $tag" >/dev/null
ls -l "$out"
