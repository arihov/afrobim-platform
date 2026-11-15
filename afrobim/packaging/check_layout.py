#!/usr/bin/env python3
"""Checks packaging consistency without Windows or macOS.

  1. windows/main.wxs is well-formed XML and every File Source under $(var.StageDir) exists in a
     tree staged by stage.sh, and every staged file is listed in main.wxs (so a pack file added
     to packs/ cannot be silently left out of the .msi).
  2. macos/Info.plist.in, once filled in, parses as a property list.
  3. Component, File and Directory ids in main.wxs are unique.

Usage: check_layout.py STAGED_DIR      (a directory produced by stage.sh, fonts included)
"""
import os
import plistlib
import sys
import xml.etree.ElementTree as ET

here = os.path.dirname(os.path.abspath(__file__))
NS = "{http://schemas.microsoft.com/wix/2006/wi}"
problems = []

stage = sys.argv[1]
tree = ET.parse(os.path.join(here, "windows", "main.wxs"))

listed = set()
ids = {}
for el in tree.iter():
    tag = el.tag.replace(NS, "")
    if tag in ("Component", "File", "Directory", "Feature") and el.get("Id"):
        key = (tag, el.get("Id"))
        if key in ids:
            problems.append("duplicate %s id %s" % key)
        ids[key] = True
    if tag == "File":
        src = el.get("Source", "")
        prefix = "$(var.StageDir)\\"
        if not src.startswith(prefix):
            problems.append("File %s does not start with $(var.StageDir)" % el.get("Id"))
            continue
        rel = src[len(prefix):].replace("\\", "/")
        listed.add(rel)
        exists = os.path.isfile(os.path.join(stage, rel)) or (rel == "afrobim.exe" and os.path.isfile(os.path.join(stage, "afrobim")))
        if not exists:
            problems.append("main.wxs lists %s, which stage.sh did not produce" % rel)

# Components referenced by a feature must exist, and every component must be referenced.
comps = {el.get("Id") for el in tree.iter(NS + "Component")}
refs = {el.get("Id") for el in tree.iter(NS + "ComponentRef")}
for c in comps - refs:
    problems.append("component %s is in no feature" % c)
for c in refs - comps:
    problems.append("ComponentRef %s has no component" % c)

staged = set()
for root, _, files in os.walk(stage):
    for f in files:
        rel = os.path.relpath(os.path.join(root, f), stage).replace(os.sep, "/")
        if rel in ("README.txt",) or rel.startswith("afrobim") and "/" not in rel and rel != "afrobim.exe":
            continue  # the Linux README and the unix binary are not part of the msi
        staged.add(rel)
wix_expected = listed | {"afrobim.exe"}
for rel in sorted(staged - wix_expected):
    problems.append("staged file %s is not in main.wxs" % rel)

with open(os.path.join(here, "macos", "Info.plist.in"), "rb") as f:
    raw = f.read().replace(b"@VERSION@", b"1.2.3").replace(b"@BUNDLE_ID@", b"com.example.x").replace(b"@MIN_MACOS@", b"11.0")
plist = plistlib.loads(raw)
for key in ("CFBundleIdentifier", "CFBundleExecutable", "CFBundleShortVersionString", "LSMinimumSystemVersion", "NSHighResolutionCapable"):
    if key not in plist:
        problems.append("Info.plist.in lacks " + key)
if plist.get("CFBundleExecutable") != "afrobim":
    problems.append("CFBundleExecutable must be afrobim")

if problems:
    print("\n".join("PROBLEM: " + p for p in problems))
    sys.exit(1)
print("layout ok: %d files in main.wxs, %d staged, Info.plist.in parses" % (len(listed), len(staged)))
