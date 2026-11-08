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

    if not all([client_id, api_key, tenant_id, product_id]):
        print("  [-] Microsoft Edge: API credentials not set (EDGE_CLIENT_ID, etc.).")
        print("      Package ready for manual upload at:", zip_path)
        return False

    print("  [*] Uploading to Microsoft Edge Partner Center...")
    try:
        # 1. Fetch Azure AD token
        token_url = f"https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"
        token_data = urllib.parse.urlencode({
            "client_id": client_id,
            "client_secret": api_key,
            "grant_type": "client_credentials",
            "scope": "https://api.addons.microsoftedge.microsoft.com/.default",
        }).encode("utf-8")
        req = urllib.request.Request(token_url, data=token_data)
        with urllib.request.urlopen(req) as resp:
            token = json.loads(resp.read())["access_token"]

        # 2. Upload zip
        upload_url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions/draft/package"
        req = urllib.request.Request(
            upload_url,
            data=zip_path.read_bytes(),
            headers={"Authorization": f"Bearer {token}", "Content-Type": "application/zip"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Microsoft Edge: Successfully uploaded package for", product_id)

        # 3. Publish submission
        pub_url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions"
        req = urllib.request.Request(
            pub_url,
            headers={"Authorization": f"Bearer {token}"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            print("  [+] Microsoft Edge: Successfully submitted for review!")
            return True
    except Exception as err:
        print(f"      Microsoft Edge Add-ons API error: {err}")
        return False


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
        import base64
        import hmac

        with open(EXT_DIR / "manifest.firefox.json") as f:
            m = json.load(f)
        addon_id = m["browser_specific_settings"]["gecko"]["id"]
        version = m["version"]

        def make_token():
            payload = {
                "iss": issuer,
                "jti": hashlib.sha256(str(time.time()).encode("utf-8")).hexdigest(),
                "iat": int(time.time()),
                "exp": int(time.time()) + 60,
            }
            header = {"alg": "HS256", "typ": "JWT"}
            def b64url(data: bytes) -> str:
                return base64.urlsafe_b64encode(data).decode('ascii').rstrip('=')
            segments = [
                b64url(json.dumps(header, separators=(',', ':')).encode('utf-8')),
                b64url(json.dumps(payload, separators=(',', ':')).encode('utf-8')),
            ]
            sig = hmac.new(secret.encode('utf-8'), ".".join(segments).encode('ascii'), hashlib.sha256).digest()
            return f"{segments[0]}.{segments[1]}.{b64url(sig)}"

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
        return 0

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
