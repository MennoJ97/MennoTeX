#!/usr/bin/env python3
"""Build crates/mtx-core/data/fontmaps.tsv.xz: TeX font name -> packages whose
font map files (addMap/addMixedMap) cover it.

Why: with T1 encoding and the default fonts, pdfTeX needs a map entry
(ecrm1000 -> sfrm1000.pfb) that lives in cm-super. No file lookup names
cm-super, so on-demand installation cannot find it from file names alone.
With this table, installing a TFM can also install the package that maps it.

Usage: tools/build_fontmap_index.py <texlive.tlpdb> [cache-dir]
Downloads every package with font maps (about 830 MiB in 2026) from one
mirror, verifies each archive's SHA-512 against the tlpdb, and reads only
the map files named in its addMap/addMixedMap directives. The tlpdb should
come from a verified source, e.g. an mtx root's tlpkg/mtx/texlive.tlpdb.
"""
import concurrent.futures as cf
import io
import lzma
import os
import sys
import tarfile

sys.path.insert(0, os.path.dirname(__file__))
import tlnetlib  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "mtx-core", "data", "fontmaps.tsv.xz")


def map_packages(tlpdb):
    """name -> (map files from addMap/addMixedMap, size, sha)."""
    out = {}
    for name, p in tlnetlib.parse_tlpdb(tlpdb).items():
        maps = {e.split()[1] for e in p["executes"] if e.split()[0] in ("addMap", "addMixedMap")}
        if maps:
            out[name] = (maps, p["size"], p["sha"])
    return out


def map_fonts(archive, wanted_maps):
    """TeX font names from the map files in `archive` named in `wanted_maps`."""
    fonts = set()
    with tarfile.open(fileobj=io.BytesIO(lzma.decompress(open(archive, "rb").read()))) as tar:
        for member in tar.getmembers():
            if not member.isfile() or os.path.basename(member.name) not in wanted_maps:
                continue
            text = tar.extractfile(member).read().decode("latin-1")
            for line in text.splitlines():
                line = line.strip()
                if not line or line[0] in "%#*;":
                    continue
                fonts.add(line.split()[0])
    return fonts


def main():
    tlpdb = sys.argv[1]
    cache = sys.argv[2] if len(sys.argv) > 2 else os.path.expanduser("~/Library/Caches/MennoTeX/fontmap-build")
    os.makedirs(cache, exist_ok=True)
    pkgs = map_packages(tlpdb)
    mirror = tlnetlib.pinned_mirror()
    print(f"{len(pkgs)} packages with font maps, {sum(s for _, s, _ in pkgs.values()) / 2**20:.0f} MiB, mirror {mirror}")
    table = {}
    with cf.ThreadPoolExecutor(8) as pool:
        futures = {pool.submit(tlnetlib.fetch, mirror, cache, n, s, h): n for n, (_, s, h) in pkgs.items()}
        for i, fut in enumerate(cf.as_completed(futures), 1):
            name = futures[fut]
            try:
                fonts = map_fonts(fut.result(), pkgs[name][0])
            except Exception as e:  # noqa: BLE001 - report and continue
                print(f"  skipped {name}: {e}", file=sys.stderr)
                continue
            for font in fonts:
                table.setdefault(font, set()).add(name)
            if i % 50 == 0:
                print(f"  {i}/{len(pkgs)} packages, {len(table)} fonts")
    lines = "".join(f"{font}\t{','.join(sorted(p))}\n" for font, p in sorted(table.items()))
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "wb") as f:
        f.write(lzma.compress(lines.encode(), preset=9))
    print(f"{len(table)} fonts -> {os.path.normpath(OUT)} ({os.path.getsize(OUT) / 1024:.0f} KiB)")


if __name__ == "__main__":
    main()
