#!/usr/bin/env python3
"""
Automated Extension Publisher for Chrome Web Store, Edge Add-ons, and Mozilla AMO.
Uses SHA-256 content caching to only upload/publish when extension files have actually changed.
"""

import json
import os
import sys
from pathlib import Path

# Prevent dev/locale.py from shadowing the standard library locale module
if sys.path and sys.path[0] == str(Path(__file__).resolve().parent):
    sys.path.pop(0)

import argparse
import hashlib
import urllib.error
import urllib.parse
import urllib.request

REPO_ROOT = Path(__file__).resolve().parent.parent
EXT_DIR = REPO_ROOT / "assets" / "tiktok-extension"
CACHE_FILE = REPO_ROOT / ".cache" / "tiktok-extension-cache.json"
DIST_DIR = REPO_ROOT / "dist"

WATCH_FILES = [
    "bridge.js",
    "frame.js",
    "manifest.json",
    "manifest.firefox.json",
    "offscreen.html",
    "offscreen.js",
    "icon.png",
]


def compute_hash() -> tuple[str, dict[str, str]]:
    hasher = hashlib.sha256()
    file_hashes = {}
    for name in sorted(WATCH_FILES):
        p = EXT_DIR / name
        if p.exists():
            data = p.read_bytes()
            h = hashlib.sha256(data).hexdigest()
            file_hashes[name] = h
            hasher.update(name.encode("utf-8"))
            hasher.update(data)
    return hasher.hexdigest(), file_hashes


def load_cache() -> dict:
    if CACHE_FILE.exists():
        try:
            return json.loads(CACHE_FILE.read_text(encoding="utf-8"))
        except Exception:
            return {}
    return {}


def save_cache(cache_data: dict):
    CACHE_FILE.parent.mkdir(parents=True, exist_ok=True)
    CACHE_FILE.write_text(json.dumps(cache_data, indent=2), encoding="utf-8")


def publish_chrome(zip_path: Path):
    client_id = os.environ.get("CHROME_CLIENT_ID")
    client_secret = os.environ.get("CHROME_CLIENT_SECRET")
    refresh_token = os.environ.get("CHROME_REFRESH_TOKEN")
    ext_id = os.environ.get("CHROME_EXTENSION_ID")

    if not all([client_id, client_secret, refresh_token, ext_id]):
        print("  [-] Chrome Web Store: API credentials not set (CHROME_CLIENT_ID, etc.).")
        print("      Package ready for manual upload at:", zip_path)
        return False

    print("  [*] Uploading to Chrome Web Store...")
    try:
        # 1. Fetch OAuth access token
        token_url = "https://oauth2.googleapis.com/token"
        token_data = urllib.parse.urlencode({
            "client_id": client_id,
            "client_secret": client_secret,
            "refresh_token": refresh_token,
            "grant_type": "refresh_token",
        }).encode("utf-8")
        req = urllib.request.Request(token_url, data=token_data)
        with urllib.request.urlopen(req) as resp:
            token = json.loads(resp.read())["access_token"]

        # 2. Upload zip
        upload_url = f"https://www.googleapis.com/upload/chromewebstore/v1.1/items/{ext_id}"
        req = urllib.request.Request(
            upload_url,
            data=zip_path.read_bytes(),
            headers={"Authorization": f"Bearer {token}", "x-goog-api-version": "2"},
            method="PUT",
        )
        with urllib.request.urlopen(req) as resp:
            res = json.loads(resp.read())
            if res.get("uploadState") != "SUCCESS":
                errors = res.get("itemError", [])
                if any(e.get("error_code") == "PKG_INVALID_VERSION_NUMBER" for e in errors):
                    print("  [+] Chrome Web Store: Version already published/uploaded.")
                    return True
                print("      Chrome Web Store upload failed:", res)
                return False

        # 3. Publish as unlisted (publishTarget: default or trustedTesters)
        pub_url = f"https://www.googleapis.com/chromewebstore/v1.1/items/{ext_id}/publish"
        req = urllib.request.Request(
            pub_url,
            headers={"Authorization": f"Bearer {token}", "x-goog-api-version": "2"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Chrome Web Store: Successfully published item", ext_id)
            return True
    except Exception as err:
        print(f"      Chrome Web Store API error: {err}")
        return False


def publish_edge(zip_path: Path):
    client_id = os.environ.get("EDGE_CLIENT_ID")
    api_key = os.environ.get("EDGE_API_KEY")
    tenant_id = os.environ.get("EDGE_TENANT_ID")
    product_id = os.environ.get("EDGE_PRODUCT_ID", "98d0235c-b425-4d57-82f0-26c1f0b7dc71")

    if not all([client_id, api_key, product_id]):
        print("  [-] Microsoft Edge: API credentials not set (EDGE_CLIENT_ID, EDGE_API_KEY, etc.).")
        print("      Package ready for manual upload at:", zip_path)
        return False

    print("  [*] Uploading to Microsoft Edge Partner Center...")
    headers = {"Authorization": f"ApiKey {api_key}", "X-ClientID": client_id}
    if tenant_id:
        try:
            token_url = f"https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"
            token_data = urllib.parse.urlencode({
                "client_id": client_id,
                "client_secret": api_key,
                "grant_type": "client_credentials",
                "scope": "https://api.addons.microsoftedge.microsoft.com/.default",
            }).encode("utf-8")
            req = urllib.request.Request(token_url, data=token_data)
            with urllib.request.urlopen(req) as resp:
                token = json.loads(resp.read()).get("access_token")
                if token:
                    headers = {"Authorization": f"Bearer {token}"}
        except Exception:
            pass

    try:
        # 2. Upload zip
        upload_url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions/draft/package"
        req = urllib.request.Request(
            upload_url,
            data=zip_path.read_bytes(),
            headers=dict(headers, **{"Content-Type": "application/zip"}),
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Microsoft Edge: Successfully uploaded package for", product_id)

        # 3. Publish submission
        pub_url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions"
        req = urllib.request.Request(
            pub_url,
            headers=headers,
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Microsoft Edge: Successfully submitted for review!")
            return True
    except urllib.error.HTTPError as err:
        if err.code == 404:
            print("  ℹ Microsoft Edge returned 404 (Resource not found):")
            print("     NOTE: Microsoft Edge Add-ons API only supports UPDATES to already-submitted extensions.")
            print("     The initial draft must be manually published on Partner Center once.")
            return True
        print(f"      Microsoft Edge Add-ons API error: {err}")
        return False
    except Exception as err:
        print(f"      Microsoft Edge Add-ons API error: {err}")
        return False


def amo_token(issuer: str, secret: str) -> str:
    """A short-lived JWT for the AMO API (HS256, as its docs describe)."""
    import base64
    import hmac
    import time

    def b64url(data: bytes) -> str:
        return base64.urlsafe_b64encode(data).decode("ascii").rstrip("=")

    now = int(time.time())
    payload = {"iss": issuer, "jti": hashlib.sha256(os.urandom(16)).hexdigest(), "iat": now, "exp": now + 60}
    segments = [
        b64url(json.dumps({"alg": "HS256", "typ": "JWT"}, separators=(",", ":")).encode()),
        b64url(json.dumps(payload, separators=(",", ":")).encode()),
    ]
    sig = hmac.new(secret.encode(), ".".join(segments).encode("ascii"), hashlib.sha256).digest()
    return f"{segments[0]}.{segments[1]}.{b64url(sig)}"


def firefox_version() -> tuple[str, str]:
    m = json.loads((EXT_DIR / "manifest.firefox.json").read_text(encoding="utf-8"))
    return m["browser_specific_settings"]["gecko"]["id"], m["version"]


# Firefox installs only signed add-ons, and AMO keeps an unlisted add-on's signed file private: each
# version is attached to a release of its own, where Streamerz downloads the one it was built for
# (src/tiktok_bridge.rs, firefox_xpi_url). A prerelease, so it never becomes the app's latest release.
def release_tag(version: str) -> str:
    return f"tiktok-extension-v{version}"


def xpi_name(version: str) -> str:
    return f"streamerz-tiktok-{version}.xpi"


def gh(*args: str, check: bool = True):
    import subprocess

    return subprocess.run(["gh", *args], capture_output=True, text=True, check=check)


def released(version: str) -> bool:
    found = gh("release", "view", release_tag(version), "--json", "assets", check=False)
    if found.returncode != 0:
        return False
    return any(a.get("name") == xpi_name(version) for a in json.loads(found.stdout).get("assets", []))


def signed_xpi_ok(data: bytes, addon_id: str, version: str) -> bool:
    """A signed add-on of this id and version (Firefox refuses anything else anyway)."""
    import io
    import zipfile

    try:
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            names = set(z.namelist())
            m = json.loads(z.read("manifest.json"))
    except (zipfile.BadZipFile, KeyError, ValueError):
        return False
    signed = any(n.startswith("META-INF/") and n.endswith((".rsa", ".sig")) for n in names)
    gecko = m.get("browser_specific_settings", {}).get("gecko", {})
    return signed and gecko.get("id") == addon_id and m.get("version") == version


def signed_xpi(issuer: str, secret: str, addon_id: str, version: str, wait: int = 900) -> bytes:
    """Waits for AMO to sign the version, then downloads its file."""
    import time

    base = f"https://addons.mozilla.org/api/v5/addons/addon/{urllib.parse.quote(addon_id)}/versions/"
    deadline = time.monotonic() + wait
    while True:
        info = None
        for number in (f"v{version}", version):
            req = urllib.request.Request(base + urllib.parse.quote(number) + "/", headers={"Authorization": f"JWT {amo_token(issuer, secret)}"})
            try:
                with urllib.request.urlopen(req) as resp:
                    info = json.loads(resp.read())
                    break
            except urllib.error.HTTPError as err:
                if err.code != 404:
                    raise
        file = (info or {}).get("file") or {}
        if file.get("status") == "public" and file.get("url"):
            req = urllib.request.Request(file["url"], headers={"Authorization": f"JWT {amo_token(issuer, secret)}"})
            with urllib.request.urlopen(req) as resp:
                return resp.read()
        if time.monotonic() > deadline:
            raise RuntimeError(f"AMO has not signed {version} yet (file status {file.get('status')!r})")
        time.sleep(20)


def release_firefox() -> bool:
    """Attaches the signed Firefox add-on to its release, once."""
    issuer = os.environ.get("AMO_JWT_ISSUER")
    secret = os.environ.get("AMO_JWT_SECRET")
    if not all([issuer, secret, os.environ.get("GH_TOKEN")]):
        print("  [-] Firefox release: AMO or GitHub credentials not set.")
        return False
    addon_id, version = firefox_version()
    if released(version):
        print(f"  [+] Firefox release: {release_tag(version)} already has {xpi_name(version)}")
        return True
    try:
        data = signed_xpi(issuer, secret, addon_id, version)
    except Exception as err:
        print(f"      Firefox release: {err}")
        return False
    if not signed_xpi_ok(data, addon_id, version):
        print("      Firefox release: AMO's file is not the signed add-on it should be")
        return False
    path = DIST_DIR / xpi_name(version)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    tag = release_tag(version)
    if gh("release", "view", tag, check=False).returncode == 0:
        gh("release", "upload", tag, str(path), "--clobber")
    else:
        target = os.environ.get("GITHUB_SHA")
        gh("release", "create", tag, str(path), "--prerelease", "--latest=false",
           "--title", f"TikTok extension {version}",
           "--notes", f"The signed Firefox add-on of the Streamerz TikTok extension {version}. Streamerz installs it itself from Accounts > TikTok.",
           *(["--target", target] if target else []))
    print(f"  [+] Firefox release: {xpi_name(version)} attached to {tag}")
    return True


def publish_firefox(zip_path: Path):
    issuer = os.environ.get("AMO_JWT_ISSUER")
    secret = os.environ.get("AMO_JWT_SECRET")

    if not all([issuer, secret]):
        print("  [-] Mozilla AMO: API credentials not set (AMO_JWT_ISSUER, AMO_JWT_SECRET).")
        print("      Package ready for manual upload at:", zip_path)
        return False

    print("  [*] Uploading to Mozilla AMO Add-on Signing API...")
    try:
        import time

        addon_id, version = firefox_version()

        def make_token():
            return amo_token(issuer, secret)

        # 1. Check if version already published
        try:
            chk_req = urllib.request.Request(
                f"https://addons.mozilla.org/api/v5/addons/addon/{urllib.parse.quote(addon_id)}/",
                headers={"Authorization": f"JWT {make_token()}"},
            )
            with urllib.request.urlopen(chk_req) as resp:
                info = json.loads(resp.read())
                latest = (info.get("latest_unlisted_version") or {}).get("version")
                if latest == version:
                    print(f"  [+] Mozilla AMO: Version {version} is already published and signed!")
                    return True
        except Exception:
            pass

        # 2. Upload to validation pipeline
        boundary = "----FormBoundary" + hashlib.md5(str(time.time()).encode()).hexdigest()
        body = (
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="channel"\r\n\r\n'
            f"unlisted\r\n"
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="upload"; filename="streamerz-tiktok-firefox.zip"\r\n'
            f"Content-Type: application/zip\r\n\r\n"
        ).encode("utf-8") + zip_path.read_bytes() + f"\r\n--{boundary}--\r\n".encode("utf-8")

        req = urllib.request.Request(
            "https://addons.mozilla.org/api/v5/addons/upload/",
            data=body,
            headers={
                "Authorization": f"JWT {make_token()}",
                "Content-Type": f"multipart/form-data; boundary={boundary}",
            },
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            upload_uuid = json.loads(resp.read())["uuid"]

        # 3. Wait for validation
        for _ in range(15):
            time.sleep(2)
            poll_req = urllib.request.Request(
                f"https://addons.mozilla.org/api/v5/addons/upload/{upload_uuid}/",
                headers={"Authorization": f"JWT {make_token()}"},
            )
            with urllib.request.urlopen(poll_req) as resp:
                poll_res = json.loads(resp.read())
                if poll_res.get("processed"):
                    if not poll_res.get("valid"):
                        print(f"      Mozilla AMO validation failed: {poll_res.get('validation')}")
                        return False
                    break

        # 4. Create version
        ver_url = f"https://addons.mozilla.org/api/v5/addons/addon/{urllib.parse.quote(addon_id)}/versions/"
        req = urllib.request.Request(
            ver_url,
            data=json.dumps({"upload": upload_uuid}).encode("utf-8"),
            headers={"Authorization": f"JWT {make_token()}", "Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Mozilla AMO: Uploaded and submitted version", version)
            return True
    except Exception as err:
        print(f"      Mozilla AMO error: {err}")
        return False


def main():
    parser = argparse.ArgumentParser(description="Check for extension changes and publish updates.")
    parser.add_argument("--force", action="store_true", help="Force rebuild and publish ignoring cache")
    args = parser.parse_args()

    current_hash, file_hashes = compute_hash()
    cache = load_cache()
    cached_hash = cache.get("combined_hash")

    if not args.force and cached_hash == current_hash:
        print("[CACHE HIT] assets/tiktok-extension/ has not changed since last publish.")
        print(f"  Current SHA-256: {current_hash[:16]}...")
        print("  Skipping upload. Everything is up to date!")
        # A signed Firefox file that never made it to its release (AMO was slow) is attached now.
        return 0 if release_firefox() else 1

    print("[CHANGES DETECTED] Extension files have been updated!")
    print(f"  Old SHA-256: {str(cached_hash)[:16]}...")
    print(f"  New SHA-256: {current_hash[:16]}...")

    # Package clean zip distributions
    import subprocess
    subprocess.run([sys.executable, str(REPO_ROOT / "dev" / "package-extensions.py")], check=True)

    chrome_zip = DIST_DIR / "streamerz-tiktok-chrome.zip"
    edge_zip = DIST_DIR / "streamerz-tiktok-edge.zip"
    firefox_zip = DIST_DIR / "streamerz-tiktok-firefox.zip"

    print("\n[PUBLISHING STATUS]")
    results = [publish_chrome(chrome_zip), publish_edge(edge_zip), publish_firefox(firefox_zip)]
    results.append(results[2] and release_firefox())
    if not all(results):
        print("Publishing incomplete. Cache unchanged so the next run can retry.")
        return 1

    # Update cache
    cache["combined_hash"] = current_hash
    cache["file_hashes"] = file_hashes
    save_cache(cache)
    print("\n[+] Cache updated at .cache/tiktok-extension-cache.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
