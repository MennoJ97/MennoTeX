#!/usr/bin/env python3
"""Check a knowledge bundle against the OKF v0.2 conformance rules (§11).

Every non-reserved .md file must start with a YAML frontmatter block that has a
non-empty `type`; index.md files carry no frontmatter except `okf_version` at
the bundle root. Also reports bundle-relative links whose target is missing
(allowed by the spec, but usually a mistake here).
"""
import os, re, sys

root = sys.argv[1] if len(sys.argv) > 1 else "knowledge"
errors, warnings = [], []
for dirpath, _, files in os.walk(root):
    for f in sorted(files):
        if not f.endswith(".md"):
            continue
        path = os.path.join(dirpath, f)
        rel = os.path.relpath(path, root)
        text = open(path, encoding="utf-8").read()
        fm = re.match(r"---\n(.*?)\n---\n", text, re.S)
        if f == "index.md":
            if fm and (rel != "index.md" or set(re.findall(r"^(\w+):", fm.group(1), re.M)) - {"okf_version"}):
                errors.append(f"{rel}: index.md may only carry okf_version, at the bundle root")
        elif f == "log.md":
            for h in re.findall(r"^## (.+)$", text, re.M):
                if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", h.strip()):
                    errors.append(f"{rel}: log heading '{h}' is not YYYY-MM-DD")
        elif not fm or not re.search(r"^type:\s*\S", fm.group(1), re.M):
            errors.append(f"{rel}: missing frontmatter with a non-empty `type`")
        for target in re.findall(r"\]\((/[^)#\s]+)", text):
            if not os.path.exists(os.path.join(root, target.lstrip("/"))):
                warnings.append(f"{rel}: link target {target} does not exist")
for w in warnings:
    print("warning:", w)
for e in errors:
    print("error:", e)
print(f"{'FAIL' if errors else 'OK'}: {root} ({len(errors)} errors, {len(warnings)} warnings)")
sys.exit(1 if errors else 0)
