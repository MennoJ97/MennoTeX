"""Quantitative analysis of texlive.tlpdb for designing an on-demand package manager."""
import collections, gzip, lzma, os, re, statistics, sys

DB = sys.argv[1] if len(sys.argv) > 1 else "texlive.tlpdb"  # unpacked tlnet/tlpkg/texlive.tlpdb
ARCH = "universal-darwin"

pkgs = {}
cur = None
section = None
for line in open(DB, encoding="utf-8"):
    line = line.rstrip("\n")
    if not line:
        cur = None; section = None; continue
    if line.startswith(" "):
        if cur is not None and section:
            path = line.strip().split(" ", 1)[0]
            cur[section].append(path)
        continue
    key, _, val = line.partition(" ")
    if key == "name":
        cur = pkgs[val] = collections.defaultdict(list); cur["name"] = val; section = None
        continue
    if key in ("runfiles", "docfiles", "srcfiles", "binfiles"):
        attrs = dict(re.findall(r"(\w+)=(\S+)", val))
        section = key if key != "binfiles" else "binfiles:" + attrs.get("arch", "?")
        cur.setdefault(section, [])
        cur[section + "_size"] = int(attrs.get("size", 0))
        continue
    section = None
    if key in ("depend", "execute", "postaction"):
        cur[key].append(val)
    else:
        cur[key] = val

print(f"entries: {len(pkgs)}")
cats = collections.Counter(p.get("category") for p in pkgs.values())
print("categories:", dict(cats))

# Arch-split packages: foo.ARCH -> foo.universal-darwin etc.
archs = collections.Counter(n.rsplit(".", 1)[1] for n in pkgs if "." in n and not n.startswith("00"))
print("arch suffixes:", dict(archs.most_common(20)))

real = {n: p for n, p in pkgs.items() if "." not in n and p.get("category") in ("Package", "TLCore", "ConTeXt")}
print(f"real (non-arch, non-collection) packages: {len(real)}")

# Container sizes (runtime container only, xz-compressed)
cs = sorted(int(p["containersize"]) for p in real.values() if "containersize" in p)
def pct(a, q): return a[min(len(a) - 1, int(q * len(a)))]
print(f"runtime container size (xz): n={len(cs)} median={pct(cs,.5)/1024:.0f}KiB p90={pct(cs,.9)/1024:.0f}KiB "
      f"p99={pct(cs,.99)/1024/1024:.1f}MiB max={cs[-1]/1024/1024:.1f}MiB total={sum(cs)/1024**3:.2f}GiB")
dcs = [int(p["doccontainersize"]) for p in real.values() if "doccontainersize" in p]
print(f"doc containers total (xz): {sum(dcs)/1024**3:.2f}GiB  (n={len(dcs)})")
biggest = sorted(((int(p.get("containersize", 0)), n) for n, p in real.items()), reverse=True)[:15]
print("largest runtime containers:", [(n, f"{s/1024/1024:.0f}MiB") for s, n in biggest])

# Runfiles + reverse index basename -> packages
base2pkgs = collections.defaultdict(set)
nrun = 0
for n, p in real.items():
    for f in p.get("runfiles", []):
        nrun += 1
        base2pkgs[os.path.basename(f)].add(n)
print(f"runfiles: {nrun}, distinct basenames: {len(base2pkgs)}")
dup = {b: s for b, s in base2pkgs.items() if len(s) > 1}
print(f"basenames provided by >1 package: {len(dup)} ({100*len(dup)/len(base2pkgs):.2f}%)")
by_ext = collections.Counter(os.path.splitext(b)[1] for b in dup)
print("  ...by extension:", by_ext.most_common(12))
important_ext = {".sty", ".cls", ".def", ".cfg", ".ldf", ".fd", ".tfm", ".pfb", ".otf", ".ttf", ".enc", ".map", ".vf", ".lua", ".bst", ".clo"}
dup_imp = {b: s for b, s in dup.items() if os.path.splitext(b)[1] in important_ext}
print(f"  ...of which TeX-lookup-relevant extensions: {len(dup_imp)}")
for b in sorted(dup_imp)[:25]:
    print("     ", b, sorted(dup_imp[b])[:5])

ext_all = collections.Counter(os.path.splitext(b)[1] for b in base2pkgs)
print("runfile basenames by extension:", ext_all.most_common(25))

# Index size estimate: "basename\tpkgid" lines, sorted
pkg_ids = {n: i for i, n in enumerate(sorted(real))}
lines = "".join(f"{b}\t{','.join(str(pkg_ids[x]) for x in sorted(s))}\n" for b, s in sorted(base2pkgs.items())).encode()
print(f"reverse index raw: {len(lines)/1024/1024:.1f}MiB gzip: {len(gzip.compress(lines, 9))/1024/1024:.2f}MiB "
      f"xz: {len(lzma.compress(lines))/1024/1024:.2f}MiB")

# Full-path index (needed when the same basename lives in different dirs, e.g. README)
paths = "".join(f"{f}\t{pkg_ids[n]}\n" for n, p in real.items() for f in p.get("runfiles", [])).encode()
print(f"full-path index raw: {len(paths)/1024/1024:.1f}MiB xz: {len(lzma.compress(paths))/1024/1024:.2f}MiB")

# execute directives = post-install actions
ex = collections.Counter(e.split()[0] for p in pkgs.values() for e in p.get("execute", []))
print("execute directives:", dict(ex))
pa = collections.Counter(e.split()[0] for p in pkgs.values() for e in p.get("postaction", []))
print("postaction directives:", dict(pa))
fmts = [e for p in pkgs.values() for e in p.get("execute", []) if e.startswith("AddFormat")]
print(f"formats declared: {len(fmts)}")

# Dependencies among real packages (not collections)
deps_real = {n: [d for d in p.get("depend", []) if not d.startswith("collection-")] for n, p in real.items()}
with_deps = {n: d for n, d in deps_real.items() if d}
print(f"real packages with 'depend' lines: {len(with_deps)} of {len(real)}")
print("  examples:", list(with_deps.items())[:8])

# Arch-specific binaries for our target
darwin = {n: p for n, p in pkgs.items() if n.endswith("." + ARCH)}
bsize = sum(int(p.get("containersize", 0)) for p in darwin.values())
print(f"{ARCH} binary packages: {len(darwin)}, total xz {bsize/1024/1024:.0f}MiB")
bigbins = sorted(((int(p.get("containersize", 0)), n) for n, p in darwin.items()), reverse=True)[:12]
print("  largest:", [(n, f"{s/1024/1024:.1f}MiB") for s, n in bigbins])

# Scheme closure sizes
def closure(root):
    seen, stack = set(), [root]
    while stack:
        n = stack.pop()
        if n in seen: continue
        seen.add(n)
        for d in pkgs.get(n, {}).get("depend", []):
            d = d.replace(".ARCH", "." + ARCH)
            if d in pkgs: stack.append(d)
    return seen
for scheme in ("scheme-infraonly", "scheme-minimal", "scheme-basic", "scheme-small", "scheme-medium", "scheme-full"):
    c = closure(scheme)
    rs = sum(int(pkgs[n].get("containersize", 0)) for n in c)
    files = sum(len(pkgs[n].get("runfiles", [])) for n in c)
    print(f"{scheme:18s} pkgs={len(c):5d} runfiles={files:7d} runtime-xz={rs/1024/1024:7.0f}MiB")

# Unpacked size of runfiles (size= is in 4k blocks per TLPOBJ docs)
rsz = sum(p.get("runfiles_size", 0) for p in real.values())
print(f"unpacked runfiles, all real pkgs: ~{rsz*4096/1024**3:.1f}GiB (size= counts 4KiB blocks)")
