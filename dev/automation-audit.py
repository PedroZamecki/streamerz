#!/usr/bin/env python3
"""Inventory authoring gaps from the current compiled type dump and source tables.

First run: cargo test --profile fast docs::tests::dump_types -- --ignored --exact
Then run: python3 -P dev/automation-audit.py
This reads project files only. Flags are review candidates, not live-service evidence.
"""

import csv
import json
import re
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def references():
    entries = {}
    for path in sorted((ROOT / "docs/reference").rglob("*.md")):
        if path.name == "README.md":
            continue
        for section in path.read_text().split("\n## ")[1:]:
            kind = re.search(r"^- Kind: (trigger|sub-action)$", section, re.M)
            code = re.search(r"^- Code: `?(\d+)`?$", section, re.M)
            if kind and code:
                key = kind[1], int(code[1])
                assert key not in entries, f"duplicate reference: {key}"
                entries[key] = (path.relative_to(ROOT).as_posix(), section)
    return entries


def main():
    data = json.loads((ROOT / "target/types.json").read_text())
    types = (ROOT / "src/types.rs").read_text()
    for collection, table in [("triggers", "TRIGGERS"), ("subactions", "SUB_ACTIONS")]:
        body = types.split(f"pub const {table}:", 1)[1].split("\n];", 1)[0]
        current = {(int(code), name) for code, name in re.findall(r'\(\s*(\d+),\s*"([^"\n]+)"\s*\)', body)}
        dumped = {(item["code"], item["name"]) for item in data[collection]}
        assert current == dumped, "Type dump is stale. Run the dump_types test first."
    docs = references()
    rows = []
    for collection, kind in [("triggers", "trigger"), ("subactions", "sub-action")]:
        for item in data[collection]:
            code = item["code"]
            state = item["formState"]
            path, section = docs.get((kind, code), ("", ""))
            headings = ["Fields", "Arguments"] + (["When"] if kind == "trigger" else [])
            missing = [h for h in headings if f"### {h}" not in section]
            fields = item["fields"]
            keys = {field["key"] for field in fields}
            hidden = sorted(set(item.get("stored", [])) - keys - {"subActions"})
            declared = "not classified" if kind == "trigger" else ("yes" if item["available"] else "no")
            ideas = ["H1", "H2"]
            if state == "JSON fallback":
                ideas.append("E1")
            if declared == "no":
                ideas.append("A1")
            if len(fields) >= 6:
                ideas.append("E2")
            if any(f["holds"] in ["whole number", "number", "index into []"] for f in fields):
                ideas.append("E3")
            if any(f["holds"].startswith("index into") for f in fields):
                ideas.append("E4")
            if any(f["holds"].startswith("text") for f in fields):
                ideas.append("V1")
            if any("id" in f["key"].lower() or "name" in f["key"].lower() for f in fields):
                ideas.append("C1")
            if kind == "trigger":
                ideas.extend(["T1", "T2"])
            else:
                ideas.extend(["R1", "R2"])
            rows.append({
                "kind": kind, "code": code, "name": item["name"], "title": item["title"],
                "category": item["category"] or "Core/General", "form_state": state,
                "picker_available": "yes" if item["available"] else "no",
                "engine_declares_supported": declared, "field_count": len(fields),
                "fields": "; ".join(f'{f["key"]}: {f["label"]} ({f["holds"]})' for f in fields),
                "stored_keys_not_in_form": "; ".join(hidden), "reference": path,
                "missing_section_headings": "; ".join(missing),
                "dedicated_example_heading": "yes" if "### Example" in section else "no",
                "review_ideas": "; ".join(ideas),
            })
    keys = {(row["kind"], row["code"]) for row in rows}
    assert len(keys) == len(rows), "duplicate type"
    assert keys == set(docs), f"catalog/reference mismatch: {keys ^ set(docs)}"
    out = ROOT / "docs/AUTOMATION-INVENTORY.csv"
    with out.open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    for kind in ["trigger", "sub-action"]:
        items = [row for row in rows if row["kind"] == kind]
        print(kind, len(items), dict(Counter(row["form_state"] for row in items)))
    print("Sub-actions declared supported:", sum(row["engine_declares_supported"] == "yes" for row in rows))
    print("Wrote", out.relative_to(ROOT))


if __name__ == "__main__":
    main()
