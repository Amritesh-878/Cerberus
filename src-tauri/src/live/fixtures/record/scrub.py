"""Turns the inputs recorded under ROOT into one fixture: fake paths and ids, in the order Claude Code ran them.

Usage: python scrub.py <recording folder> <fixture.jsonl>
"""
import glob
import json
import os
import re
import sys

ROOT = os.path.abspath(sys.argv[1])
OUT = sys.argv[2]
FAKE_ROOT = "C:\\fake"
FAKE_SESSION = "00000000-0000-4000-8000-000000000001"

ids = {}

def fake_id(kind, value):
    key = (kind, value)
    if key not in ids:
        ids[key] = f"{kind}_fake_{len([k for k in ids if k[0] == kind]) + 1}"
    return ids[key]

def scrub(value, key=None):
    if isinstance(value, dict):
        return {k: scrub(v, k) for k, v in value.items()}
    if isinstance(value, list):
        return [scrub(v, key) for v in value]
    if not isinstance(value, str):
        return value
    if key == "session_id":
        return FAKE_SESSION
    if key == "prompt_id":
        return fake_id("prompt", value)
    if key == "tool_use_id":
        return fake_id("toolu", value)
    if key == "agent_id":
        return "a000000000000001"
    for root in (ROOT, ROOT.replace("\\", "/")):
        if root in value:
            sep = "/" if "/" in root else "\\"
            value = value.replace(root, FAKE_ROOT if sep == "\\" else FAKE_ROOT.replace("\\", "/"))
    slug = "".join(c if c.isalnum() else "-" for c in ROOT)
    value = value.replace(slug, "C--fake")
    home = os.path.expanduser("~")
    for h in (home, home.replace("\\", "/")):
        value = value.replace(h, "C:\\Users\\someone" if "\\" in h else "C:/Users/someone")
    value = re.sub(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", FAKE_SESSION, value)
    value = re.sub(r"agent-[0-9a-f]+", "agent-a000000000000001", value)
    if os.path.basename(home).lower() in value.lower():
        raise SystemExit(f"left a real name in {key}: {value[:60]}")
    return value

lines = []
for f in sorted(glob.glob(os.path.join(ROOT, "recorded", "*.json"))):
    kind = os.path.basename(f).split("-", 1)[1][: -len(".json")]
    with open(f, encoding="utf-8") as fh:
        data = json.load(fh)
    source = "status" if kind == "status" else "hook"
    lines.append(json.dumps({"source": source, "input": scrub(data)}, ensure_ascii=False))

with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write("\n".join(lines) + "\n")
print(len(lines), "lines to", OUT)
