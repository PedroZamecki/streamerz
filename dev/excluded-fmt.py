#!/usr/bin/env -S python3 -P
"""Writes tests/excluded.json back in its one-entry-per-line layout: `excluded-fmt.py` (reads stdin JSON
when given `-`, else the file itself)."""
import json, sys
from pathlib import Path

FILE = Path(__file__).resolve().parent.parent / "tests/excluded.json"


def dump(d):
    lines = ["{", '  "about": ' + json.dumps(d["about"]) + ",", '  "triggers": [']
    t = d["triggers"]
    lines += ["    " + json.dumps(e) + ("," if i < len(t) - 1 else "") for i, e in enumerate(t)]
    lines += ["  ],", '  "todo": {', '    "triggers": ' + json.dumps(d["todo"]["triggers"]) + ",",
              '    "subActions": ' + json.dumps(d["todo"]["subActions"]), "  }", "}"]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    data = json.load(sys.stdin) if sys.argv[1:] == ["-"] else json.loads(FILE.read_text())
    FILE.write_text(dump(data))
