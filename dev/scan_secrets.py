#!/usr/bin/env python3
"""Looks for secrets and personal data in what is about to be public.

  dev/scan_secrets.py             the staged changes (the pre-commit hook runs this)
  dev/scan_secrets.py --history   every line ever committed, on every branch (CI runs this)

Fails (exit 1) and says where when a line looks like a token, a key, a login cookie or a path with a
person's name. Fake values in tests are fine: put a regex for them in dev/secrets-allow.txt.
"""
import sys
from pathlib import Path

# Not this folder first: dev/locale.py would stand in for the standard library's locale.
sys.path[:] = [p for p in sys.path if Path(p or ".").resolve() != Path(__file__).resolve().parent]

import re, subprocess  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# What a real secret looks like. Streamerz's own settings (streamerz.json) hold the logins, and are
# gitignored; this catches one pasted into a file, a log or a doc.
RULES = {
    "GitHub token": r"\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}",
    "Google API key or secret": r"\bAIza[0-9A-Za-z_-]{35}|GOCSPX-[A-Za-z0-9_-]{20,}",
    "Slack or API token": r"\bxox[baprs]-[A-Za-z0-9-]{10,}|\bsk-[A-Za-z0-9]{32,}",
    "Private key": r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    "Twitch chat token": r"\boauth:[a-z0-9]{30}\b",
    "Token in a settings value": r"\"(access_?[tT]oken|refresh_?[tT]oken|client_?[sS]ecret|sessionCookie|authPassword)\"\s*:\s*\"[A-Za-z0-9_./+=-]{16,}\"",
    "Login cookie": r"\b(sessionid|sid_tt|sessionid_ss)=[A-Za-z0-9]{16,}",
    "Bearer token": r"[Bb]earer\s+[A-Za-z0-9_.-]{30,}",
    "A person's home folder": r"/home/(?!someone\b|user\b|runner\b|you\b|name\b)[a-z][a-z0-9_-]{2,}/|C:\\\\Users\\\\(?!Public|Default|you|user|name)[A-Za-z][A-Za-z0-9_-]+",
}
SKIP_FILES = ("Cargo.lock", ".png", ".jpg", ".svg", ".ico", ".gz", ".wasm")


def allowed():
    f = ROOT / "dev" / "secrets-allow.txt"
    if not f.exists():
        return []
    return [re.compile(l.strip()) for l in f.read_text().splitlines() if l.strip() and not l.startswith("#")]


def git(*a):
    return subprocess.run(["git", "-C", str(ROOT), *a], capture_output=True, text=True, errors="replace").stdout


def scan(diff, allow):
    """Added lines of a `git diff` or `git log -p`: (where, rule, line)."""
    found, file, commit = [], "?", ""
    for line in diff.splitlines():
        if line.startswith("commit "):
            commit = line[7:15]
        elif line.startswith("+++ b/"):
            file = line[6:]
        elif line.startswith("+") and not line.startswith("+++") and not file.endswith(SKIP_FILES):
            text = line[1:]
            if any(a.search(text) for a in allow):
                continue
            for name, rx in RULES.items():
                if re.search(rx, text):
                    found.append((f"{commit} {file}".strip(), name, text.strip()[:90]))
    return found


def main():
    allow = allowed()
    history = "--history" in sys.argv
    diff = git("log", "--all", "-p", "--no-color", "--format=commit %H") if history else git("diff", "--cached", "--no-color")
    found = scan(diff, allow)
    # A settings file or a database must never be tracked, whatever its content.
    names = git("log", "--all", "--name-only", "--format=").split() if history else git("diff", "--cached", "--name-only").split()
    for n in sorted(set(names)):
        if re.search(r"(^|/)(streamerz\.json(\.bak)?|community\.json|\.env)$", n):
            found.append((n, "A private file", n))
    for where, rule, text in found:
        print(f"{rule}: {where}: {text}")
    if found:
        print(f"\n{len(found)} finding(s). Remove them, or if they are fake, add a regex to dev/secrets-allow.txt.")
        sys.exit(1)
    print("no secrets found" + (" in the history" if history else " in the staged changes"))


if __name__ == "__main__":
    main()
