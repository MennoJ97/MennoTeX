"""Shared helpers for tools that read package archives from tlnet."""
import hashlib
import os
import re
import urllib.error
import urllib.request

REDIRECTOR = "https://mirror.ctan.org/systems/texlive/tlnet/"


def parse_tlpdb(path):
    """name -> {size, sha, runfiles, executes} for real (non-arch) packages."""
    pkgs = {}
    for block in open(path, encoding="utf-8").read().split("\n\n"):
        m = re.match(r"name (\S+)", block)
        if not m or "." in m.group(1):
            continue
        runfiles, section = [], None
        for line in block.splitlines():
            if line.startswith(" "):
                if section == "run":
                    runfiles.append(line.strip().split(" ")[0].replace("RELOC/", "texmf-dist/", 1))
            else:
                section = "run" if line.startswith("runfiles") else None
        pkgs[m.group(1)] = {
            "size": int((re.search(r"^containersize (\d+)", block, re.M) or [0, 0])[1]),
            "sha": (re.search(r"^containerchecksum (\S+)", block, re.M) or [0, ""])[1],
            "relocated": bool(re.search(r"^relocated 1", block, re.M)),
            "runfiles": runfiles,
            "executes": re.findall(r"^execute (.*)$", block, re.M),
        }
    return pkgs


def pinned_mirror():
    """Resolve mirror.ctan.org once, so all archives come from one mirror."""

    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *a, **k):
            return None

    req = urllib.request.Request(REDIRECTOR + "tlpkg/texlive.tlpdb.sha512", method="HEAD")
    try:
        urllib.request.build_opener(NoRedirect).open(req)
        return REDIRECTOR
    except urllib.error.HTTPError as e:
        loc = e.headers.get("Location", "")
        return loc[: -len("tlpkg/texlive.tlpdb.sha512")] if loc else REDIRECTOR


def fetch(mirror, cache, name, size, sha):
    """Download archive/<name>.tar.xz into `cache`, verified against the tlpdb."""
    path = os.path.join(cache, f"{name}.tar.xz")
    if os.path.exists(path) and hashlib.sha512(open(path, "rb").read()).hexdigest() == sha:
        return path
    data = urllib.request.urlopen(f"{mirror}archive/{name}.tar.xz", timeout=180).read()
    if len(data) != size or hashlib.sha512(data).hexdigest() != sha:
        raise RuntimeError(f"{name}: archive does not match the tlpdb")
    with open(path + ".part", "wb") as f:
        f.write(data)
    os.replace(path + ".part", path)
    return path
