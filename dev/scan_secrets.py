#!/usr/bin/env python3
"""Looks for secrets and personal data in what is about to be public.

  dev/scan_secrets.py             the staged changes (the pre-commit hook runs this)
  dev/scan_secrets.py --history   every line ever committed, on every branch (CI runs this)

Fails (exit 1) and says where when a line looks like a token, a key, a login cookie or a path with a
person's name. Fake values in tests are fine: put a regex for them in dev/secrets-allow.txt.
Exact historical home paths whose current file is redacted are recorded in dev/secrets-history.json.
That history-only baseline never exempts credentials, current files or paths added in new commits.
"""
import sys
from pathlib import Path

# Not this folder first: dev/locale.py would stand in for the standard library's locale.
sys.path[:] = [p for p in sys.path if Path(p or ".").resolve() != Path(__file__).resolve().parent]

import hashlib, json, re, subprocess  # noqa: E402

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
PATTERNS = {name: re.compile(pattern) for name, pattern in RULES.items()}
SKIP_FILES = ("Cargo.lock", ".png", ".jpg", ".svg", ".ico", ".gz", ".wasm")


def allowed():
    f = ROOT / "dev" / "secrets-allow.txt"
    if not f.exists():
        return []
    return [re.compile(l.strip()) for l in f.read_text().splitlines() if l.strip() and not l.startswith("#")]


def git_command(*args):
    return ["git", "-C", str(ROOT), *args]


def git(*args):
    return subprocess.run(git_command(*args), capture_output=True, text=True, errors="replace", check=True).stdout


def history_baseline():
    """Exact legacy home-path findings, usable only after the current file is redacted.

    Credentials cannot be baselined. The commit, path, rule and full-line digest must all match;
    adding the same path in a new commit is still a failure. No personal paths are stored here.
    """
    path = ROOT / "dev" / "secrets-history.json"
    if not path.exists():
        return set()
    data = json.loads(path.read_text())
    if data.get("version") != 1 or not isinstance(data.get("findings"), list):
        raise ValueError("Invalid history baseline format")
    records = set()
    for entry in data["findings"]:
        if entry.get("rule") != "A person's home folder":
            raise ValueError("Credentials cannot be exempted by the history baseline")
        if not re.fullmatch(r"[0-9a-f]{40}", entry.get("commit", "")) or not re.fullmatch(r"[0-9a-f]{64}", entry.get("sha256", "")):
            raise ValueError("History entries require an exact commit and full-line SHA-256")
        relative = Path(entry.get("path", ""))
        current = (ROOT / relative).resolve()
        if not entry.get("reason") or relative.is_absolute() or not relative.parts or ".." in relative.parts or not current.is_relative_to(ROOT.resolve()):
            raise ValueError("History entries require a relative file and a review reason")
        if current.exists() and re.search(RULES[entry["rule"]], current.read_text()):
            raise ValueError(f"Current file must be redacted before recording legacy findings: {relative}")
        records.add((entry["commit"], relative.as_posix(), entry["rule"], entry["sha256"]))
    return records


def scan(lines, allow, baseline=frozenset(), reviewed=None):
    """Report locations and rules for added lines without printing their contents."""
    found, file, commit, in_hunk = [], "?", "", False
    for line in lines:
        line = line.removesuffix("\n")
        if line.startswith("commit "):
            commit = line[7:].strip()
            in_hunk = False
        elif line.startswith("diff --git "):
            file, in_hunk = "?", False
        elif line.startswith("+++ ") and not in_hunk:
            file = line[4:].partition("\t")[0].removeprefix("b/")
        elif line.startswith("@@ "):
            in_hunk = True
        elif in_hunk and line.startswith("+") and not file.rstrip('"').endswith(SKIP_FILES):
            text = line[1:]
            approved = [m.span() for rx in allow for m in rx.finditer(text)]
            for name, rx in PATTERNS.items():
                for match in rx.finditer(text):
                    if any(start < match.end() and match.start() < end for start, end in approved):
                        continue
                    fingerprint = (commit, file, name, hashlib.sha256(text.encode()).hexdigest())
                    if name == "A person's home folder" and fingerprint in baseline:
                        if reviewed is not None:
                            reviewed.append(fingerprint)
                        continue
                    found.append((f"{commit[:8]} {file}".strip(), name))
                    break
    return found


def main():
    allow = allowed()
    history = "--history" in sys.argv
    scope = ("log", "--all") if history else ("diff", "--cached")
    diff = git_command(*scope, "-p", "--no-color", "--no-ext-diff", "--no-textconv",
                       "--src-prefix=a/", "--dst-prefix=b/", *(("--format=commit %H",) if history else ()))
    reviewed = []
    baseline = history_baseline() if history else frozenset()
    with subprocess.Popen(diff, stdout=subprocess.PIPE, text=True, errors="replace") as process:
        found = scan(process.stdout, allow, baseline, reviewed)
    if process.returncode:
        raise subprocess.CalledProcessError(process.returncode, diff)
    # A settings file or a database must never be tracked, whatever its content.
    names = git(*scope, "--name-only", "--diff-filter=ACMRT", "-z", *(("--format=",) if history else ()))
    for name in sorted(set(names.split("\0"))):
        if Path(name).name in {"streamerz.json", "streamerz.json.bak", "community.json", ".env"}:
            found.append((repr(name), "A private file"))
    for where, rule in found:
        print(f"{rule}: {where}")
    if found:
        print(f"\n{len(found)} finding(s). Remove them, or if they are fake, add a regex to dev/secrets-allow.txt.")
        sys.exit(1)
    if reviewed:
        print(f"history scan passed: {len(reviewed)} recorded legacy home-path occurrences; current files are redacted")
    else:
        print("no secrets found" + (" in the history" if history else " in the staged changes"))


if __name__ == "__main__":
    main()
