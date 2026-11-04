#!/usr/bin/env -S python3 -P
# -P: dev/locale.py would shadow the standard library.
"""Takes the codes scenarios now cover off the todo list in tests/excluded.json, and prints what's left.

Runs the ratchet test (`every_code_is_tested_or_excluded`) and reads its complaints.
"""
import json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "tests/excluded.json"


def codes(text):
    out = set()
    for part in filter(None, (p.strip() for p in text.split(","))):
        a, _, b = part.partition("-")
        out.update(range(int(a), int(b or a) + 1))
    return out


def ranges(xs):
    xs, out, i = sorted(xs), [], 0
    while i < len(xs):
        j = i
        while j + 1 < len(xs) and xs[j + 1] == xs[j] + 1:
            j += 1
        out.append(str(xs[i]) if i == j else f"{xs[i]}-{xs[j]}")
        i = j + 1
    return ", ".join(out)


run = subprocess.run(
    ["cargo", "test", "--profile", "fast", "every_code_is_tested_or_excluded"],
    cwd=ROOT, capture_output=True, text=True,
)
done = {"trigger": set(), "sub-action": set()}
for kind, code in re.findall(r"^(trigger|sub-action) (\d+) \S+: tested now", run.stdout, re.M):
    done[kind].add(int(code))
missing = re.findall(r"^((?:trigger|sub-action) \d+ \S+): no scenario uses it", run.stdout, re.M)
if missing:
    print("not in a scenario and not on todo:\n  " + "\n  ".join(missing))
text = FILE.read_text()
data = json.loads(text)
todo = data["todo"]
left_t = codes(todo["triggers"]) - done["trigger"] - {c for e in data["triggers"] for c in codes(e["codes"])}
left_s = codes(todo["subActions"]) - done["sub-action"]
new = '"todo": {\n    "triggers": ' + json.dumps(ranges(left_t)) + ',\n    "subActions": ' + json.dumps(ranges(left_s)) + "\n  }"
FILE.write_text(re.sub(r'"todo": \{.*?\n  \}', new, text, flags=re.S))
print(f"took off {len(done['trigger'])} triggers and {len(done['sub-action'])} sub-actions; "
      f"left: {len(left_t)} triggers, {len(left_s)} sub-actions")
sys.exit(1 if missing else 0)
