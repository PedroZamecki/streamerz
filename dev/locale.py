#!/usr/bin/env python3
"""Languages of Streamerz: each is one file, assets/locales/<code>.json (flat: key -> text).

  dev/locale.py new es_ES "Español (España)"   make es_ES.json from the English one, ready to translate
  dev/locale.py check [code ...]               list what's wrong (all languages when none is named)

Nothing else to do: a file in assets/locales is found when Streamerz is built. `locale.name` is the
language's own name, shown in Settings. Keep every {placeholder} as it is.
"""
import json, re, sys
from pathlib import Path

DIR = Path(__file__).resolve().parent.parent / "assets" / "locales"
DEFAULT = "en_US"
README = (DIR.parent.parent / "README.md").read_text(encoding="utf-8")


def load(code):
    return json.loads((DIR / f"{code}.json").read_text(encoding="utf-8"))


def holes(text):
    return sorted(re.findall(r"\{(\w+)\}", text))


def new(code, name):
    if not re.fullmatch(r"[a-z]{2,3}_[A-Z]{2}", code):
        sys.exit(f"'{code}' should look like es_ES (language_COUNTRY)")
    path = DIR / f"{code}.json"
    if path.exists():
        sys.exit(f"{path} exists already")
    texts = load(DEFAULT)
    texts["locale.name"] = name
    path.write_text(json.dumps(dict(sorted(texts.items())), ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"made {path}: translate the values (not the keys), then run: dev/locale.py check {code}")


def check(codes):
    en = load(DEFAULT)
    problems = 0
    for code in codes or sorted(p.stem for p in DIR.glob("*.json")):
        try:
            tr = load(code)
        except (OSError, json.JSONDecodeError) as e:
            print(f"{code}: can't read it: {e}")
            problems += 1
            continue
        found = []
        if f"`{code}`" not in README:
            found.append("no row in the README's language table (add yours: language, code, who translated it)")
        if not re.fullmatch(r"[a-z]{2,3}_[A-Z]{2}", code):
            found.append("the file's name should look like es_ES.json")
        if not isinstance(tr, dict) or not all(isinstance(v, str) for v in tr.values()):
            found.append("it should be a flat object of texts")
        else:
            found += [f"missing: {k}" for k in sorted(en.keys() - tr.keys())]
            found += [f"not in English (remove): {k}" for k in sorted(tr.keys() - en.keys())]
            if not tr.get("locale.name", "").strip():
                found.append("locale.name is empty")
            for k in sorted(en.keys() & tr.keys()):
                if holes(en[k]) != holes(tr[k]):
                    found.append(f"{k}: {holes(en[k])} in English, {holes(tr[k])} here")
            if code != DEFAULT:
                same = [k for k in en if k in tr and tr[k] == en[k] and re.search("[A-Za-z]{4}", en[k])]
                print(f"{code}: {len(same)} of {len(en)} texts are still the English ones")
        for f in found:
            print(f"{code}: {f}")
        problems += len(found)
    print("ok" if not problems else f"{problems} problem(s)")
    return problems


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[:1] == ["new"] and len(a) == 3:
        new(a[1], a[2])
    elif a[:1] == ["check"]:
        sys.exit(1 if check(a[1:]) else 0)
    else:
        sys.exit(__doc__)
