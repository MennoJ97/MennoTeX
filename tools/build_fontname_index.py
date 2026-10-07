#!/usr/bin/env python3
"""Build crates/mtx-core/data/fontnames.tsv.xz: OpenType/TrueType font names
-> (package, root-relative font file), so that `\\setmainfont{TeX Gyre Pagella}`
can install the package that ships the font.

Rows: <normalized name>\t<kind>\t<package>\t<path>. The normalized name is the
name lowercased with everything but letters and digits removed. Kinds:
  family  typographic or legacy family name -> the regular face of the family
  full    full font name ("TeX Gyre Pagella Bold") -> that face
  ps      PostScript name ("TeXGyrePagella-Bold") -> that face

Usage: tools/build_fontname_index.py <texlive.tlpdb> [cache-dir]
Downloads every package with OpenType/TrueType fonts (about 850 MiB in 2026),
verified against the tlpdb, and parses the 'name' tables directly.
"""
import concurrent.futures as cf
import io
import lzma
import os
import re
import struct
import sys
import tarfile

sys.path.insert(0, os.path.dirname(__file__))
import tlnetlib  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "mtx-core", "data", "fontnames.tsv.xz")
FONT_RE = re.compile(r"/fonts/(opentype|truetype)/.+\.(otf|ttf|ttc)$", re.I)
REGULAR = ["regular", "book", "roman", "normal", "text", "medium"]


def normalize(name):
    return re.sub(r"[^0-9a-z]", "", name.lower())


def decode(platform, encoding, raw):
    if platform in (0, 3):
        return raw.decode("utf-16-be", "replace")
    if platform == 1 and encoding == 0:
        return raw.decode("mac_roman", "replace")
    return None


def font_names(data, offset=0):
    """{nameID: name} of the sfnt at `offset`, preferring Windows English."""
    num_tables = struct.unpack_from(">H", data, offset + 4)[0]
    for i in range(num_tables):
        tag, _, toff, _ = struct.unpack_from(">4sIII", data, offset + 12 + 16 * i)
        if tag != b"name":
            continue
        _, count, string_off = struct.unpack_from(">HHH", data, toff)
        names, rank = {}, {}
        for r in range(count):
            pid, eid, lid, nid, length, off = struct.unpack_from(">HHHHHH", data, toff + 6 + 12 * r)
            if nid not in (1, 2, 4, 6, 16, 17):
                continue
            start = toff + string_off + off
            text = decode(pid, eid, data[start:start + length])
            if not text:
                continue
            score = 3 if (pid == 3 and lid == 0x409) else 2 if pid == 3 else 1
            if score > rank.get(nid, 0):
                names[nid], rank[nid] = text.strip(), score
        return names
    return {}


def faces(data):
    """Names of each face in a font file (several for a .ttc collection)."""
    if data[:4] == b"ttcf":
        count = struct.unpack_from(">I", data, 8)[0]
        offsets = struct.unpack_from(f">{count}I", data, 12)
        return [font_names(data, o) for o in offsets]
    return [font_names(data)]


def scan(archive, package, relocated):
    rows = []
    with tarfile.open(fileobj=io.BytesIO(lzma.decompress(open(archive, "rb").read()))) as tar:
        for member in tar.getmembers():
            path = ("texmf-dist/" + member.name) if relocated else member.name
            if not member.isfile() or not FONT_RE.search(path):
                continue
            try:
                for names in faces(tar.extractfile(member).read()):
                    if names:
                        rows.append((names, package, path))
            except (struct.error, IndexError):
                print(f"  unreadable font {path} in {package}", file=sys.stderr)
    return rows


def main():
    tlpdb = sys.argv[1]
    cache = sys.argv[2] if len(sys.argv) > 2 else os.path.expanduser("~/Library/Caches/MennoTeX/fontname-build")
    os.makedirs(cache, exist_ok=True)
    pkgs = {n: p for n, p in tlnetlib.parse_tlpdb(tlpdb).items() if any(FONT_RE.search(f) for f in p["runfiles"])}
    mirror = tlnetlib.pinned_mirror()
    print(f"{len(pkgs)} packages with OpenType/TrueType fonts, {sum(p['size'] for p in pkgs.values()) / 2**20:.0f} MiB, mirror {mirror}")
    faces_all = []
    with cf.ThreadPoolExecutor(8) as pool:
        futs = {pool.submit(tlnetlib.fetch, mirror, cache, n, p["size"], p["sha"]): n for n, p in pkgs.items()}
        for i, fut in enumerate(cf.as_completed(futs), 1):
            name = futs[fut]
            try:
                faces_all.extend(scan(fut.result(), name, pkgs[name]["relocated"]))
            except Exception as e:  # noqa: BLE001 - report and continue
                print(f"  skipped {name}: {e}", file=sys.stderr)
            if i % 50 == 0:
                print(f"  {i}/{len(pkgs)} packages, {len(faces_all)} faces")

    table = {}  # (normalized, kind) -> (package, path)
    families = {}  # normalized family -> [(rank, path, package)]
    for names, pkg, path in faces_all:
        for nid, kind in ((4, "full"), (6, "ps")):
            if names.get(nid):
                table.setdefault((normalize(names[nid]), kind), (pkg, path))
        style = (names.get(17) or names.get(2) or "").lower()
        rank = REGULAR.index(style) if style in REGULAR else len(REGULAR)
        for nid in (16, 1):
            if names.get(nid):
                families.setdefault(normalize(names[nid]), []).append((rank, path, pkg))
    for fam, cands in families.items():
        _, path, pkg = min(cands)
        table[(fam, "family")] = (pkg, path)
    rows = sorted(f"{n}\t{k}\t{p}\t{f}\n" for (n, k), (p, f) in table.items() if n)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "wb") as f:
        f.write(lzma.compress("".join(rows).encode(), preset=9))
    print(f"{len(faces_all)} faces, {len(rows)} names -> {os.path.normpath(OUT)} ({os.path.getsize(OUT) / 1024:.0f} KiB)")


if __name__ == "__main__":
    main()
