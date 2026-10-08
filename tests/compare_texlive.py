#!/usr/bin/env python3
"""Compare the corpus built by MennoTeX with the same corpus built by a
reference TeX installation (PLAN.md Phase 2 exit: "the corpus compiles
identically to a full TeX Live 2026 install").

Usage:
    tests/compare_texlive.py <mennotex-out> <reference-out> <engine>

Both directories come from tests/run_documents.sh with $DOCS_OUT set, one
run on a fresh MennoTeX root (packages installed on demand during the run),
one on the reference (everything installed beforehand). For every corpus
document built for <engine> it compares:

  pages     page count (pypdf)
  text      pdftotext output, whitespace-normalized
  fonts     embedded fonts from pdffonts: name (without the subset
            prefix), type and whether embedded
  warnings  font warnings in the TeX log (substituted shapes, missing
            characters, fonts not found), line numbers removed
  inputs    the texmf-dist files TeX read (from the .fls of -recorder,
            last TeX run), relative to texmf-dist: a file read from
            another directory means a package that was not installed
            would have shadowed it (pstricks.con from xetex-pstricks)

A difference means an on-demand install changed the document, e.g. a font
that was not there when TeX looked for it and was quietly replaced.
Needs pdftotext and pdffonts (poppler) and pypdf.
"""

import difflib
import re
import subprocess
import sys
from pathlib import Path

from pypdf import PdfReader

DOCS = Path(__file__).resolve().parent / "documents"
WARNING = re.compile(
    r"Font shape .* undefined|using .* instead|Missing character|"
    r"Font .* not found|not loadable|Some font shapes were not available",
    re.IGNORECASE,
)


def engines_of(tex: Path) -> list[str]:
    for line in tex.read_text(encoding="utf-8").splitlines():
        if line.startswith("% engines:"):
            return line.split(":", 1)[1].split()
    return ["pdflatex", "xelatex", "lualatex"]


def text_of(pdf: Path) -> str:
    out = subprocess.run(["pdftotext", "-enc", "UTF-8", str(pdf), "-"], capture_output=True, text=True, check=True)
    return " ".join(out.stdout.split())


def fonts_of(pdf: Path) -> set[tuple[str, str, str]]:
    out = subprocess.run(["pdffonts", str(pdf)], capture_output=True, text=True, check=True).stdout.splitlines()
    fonts = set()
    for line in out[2:]:  # header and ruler
        # name type... encoding emb sub uni object ID; types contain spaces
        # ("Type 1C"), so read the fixed tail from the right.
        parts = line.split()
        if len(parts) < 7:
            continue
        name = re.sub(r"^[A-Z]{6}\+", "", parts[0])
        emb = parts[-5]
        kind = " ".join(parts[1:-6])
        fonts.add((name, kind, emb))
    return fonts


def warnings_of(log: Path) -> set[str]:
    if not log.exists():
        return set()
    text = log.read_text(encoding="utf-8", errors="replace")
    # TeX wraps log lines at 79 characters; join continuation lines of
    # package warnings ("(Font)   ...").
    text = re.sub(r"\n\(Font\)\s+", " ", text)
    found = set()
    for line in text.splitlines():
        if WARNING.search(line):
            found.add(re.sub(r" on input line \d+", "", line).strip())
    return found


# MennoTeX's own overlay (texmf-mtx) replaces this one on purpose.
OWN_OVERLAY = {"tex/luatex/luaotfload/luaotfload-main.lua"}


def inputs_of(fls: Path) -> set[str]:
    if not fls.exists():
        return set()
    found = set()
    for line in fls.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("INPUT ") and "/texmf-dist/" in line:
            rel = line.split("/texmf-dist/", 1)[1]
            if rel not in OWN_OVERLAY:
                found.add(rel)
    return found


def compare(name: str, a: Path, b: Path) -> list[str]:
    pa, pb = a / f"{name}.pdf", b / f"{name}.pdf"
    if not pa.exists() or not pb.exists():
        return [f"pdf missing in {'MennoTeX' if not pa.exists() else 'reference'}"]
    diffs = []
    try:
        na = len(PdfReader(pa).pages)
    except Exception as e:  # a PDF the driver did not finish
        return [f"unreadable pdf from MennoTeX: {e}"]
    try:
        nb = len(PdfReader(pb).pages)
    except Exception as e:
        return [f"unreadable pdf from the reference: {e}"]
    if na != nb:
        diffs.append(f"pages {na} vs {nb}")
    ta, tb = text_of(pa), text_of(pb)
    if ta != tb:
        sm = difflib.SequenceMatcher(None, ta, tb, autojunk=False)
        first = next(op for op in sm.get_opcodes() if op[0] != "equal")
        _, i1, i2, j1, j2 = first
        diffs.append(f"text differs at char {i1}: {ta[max(0, i1 - 30):i2 + 30]!r} vs {tb[max(0, j1 - 30):j2 + 30]!r}")
    fa, fb = fonts_of(pa), fonts_of(pb)
    if fa != fb:
        diffs.append(f"fonts only in MennoTeX {sorted(fa - fb)}, only in reference {sorted(fb - fa)}")
    ia, ib = inputs_of(a / f"{name}.fls"), inputs_of(b / f"{name}.fls")
    if ia != ib:
        diffs.append(f"inputs only in MennoTeX {sorted(ia - ib)}, only in reference {sorted(ib - ia)}")
    wa, wb = warnings_of(a / f"{name}.log"), warnings_of(b / f"{name}.log")
    if wa != wb:
        diffs.append(f"warnings only in MennoTeX {sorted(wa - wb)}, only in reference {sorted(wb - wa)}")
    return diffs


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    a, b, engine = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    same = differ = 0
    for tex in sorted(DOCS.glob("*.tex")):
        if engine not in engines_of(tex):
            continue
        diffs = compare(tex.stem, a, b)
        if diffs:
            differ += 1
            print(f"{tex.stem:26} {engine:8} DIFFERENT")
            for d in diffs:
                print(f"    {d}")
        else:
            same += 1
            print(f"{tex.stem:26} {engine:8} same")
    print(f"{engine}: {same} same, {differ} different")
    return 1 if differ else 0


if __name__ == "__main__":
    sys.exit(main())
