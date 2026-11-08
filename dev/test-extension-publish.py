#!/usr/bin/env python3
"""
Test Extension Publishing API Credentials & Connectivity.
Loads .env and tests Chrome Web Store, Microsoft Edge, and Mozilla AMO APIs individually.
"""

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

# Prevent dev/locale.py shadowing
if sys.path and sys.path[0] == str(Path(__file__).resolve().parent):
    sys.path.pop(0)

import argparse
import hashlib
import time

REPO_ROOT = Path(__file__).resolve().parent.parent
ENV_FILE = REPO_ROOT / ".env"
DIST_DIR = REPO_ROOT / "dist"
EXT_DIR = REPO_ROOT / "assets" / "tiktok-extension"


def load_env():
    """Loads .env into os.environ if it exists."""
    if not ENV_FILE.exists():
        return
    for line in ENV_FILE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        k = k.strip()
        v = v.strip().strip("'\"")
        if k and v and k not in os.environ:
            os.environ[k] = v


def test_chrome(upload: bool = False):
    print("\n" + "=" * 60)
    print(" [1/3] Testing Google Chrome Web Store API")
    print("=" * 60)
    client_id = os.environ.get("CHROME_CLIENT_ID")
    client_secret = os.environ.get("CHROME_CLIENT_SECRET")
    refresh_token = os.environ.get("CHROME_REFRESH_TOKEN")
    ext_id = os.environ.get("CHROME_EXTENSION_ID", "anckdakmhognoancpeefjlgicihplcjc")

    missing = []
    if not client_id: missing.append("CHROME_CLIENT_ID")
    if not client_secret: missing.append("CHROME_CLIENT_SECRET")
    if not refresh_token: missing.append("CHROME_REFRESH_TOKEN")
    if missing:
        print(f"  ❌ Missing credentials: {', '.join(missing)}")
        print("     Please set them in your .env file.")
        return False

    print(f"  Item ID: {ext_id}")
    print("  [*] Exchanging refresh token for access token...")
    try:
        token_url = "https://oauth2.googleapis.com/token"
        token_data = urllib.parse.urlencode({
            "client_id": client_id,
            "client_secret": client_secret,
            "refresh_token": refresh_token,
            "grant_type": "refresh_token",
        }).encode("utf-8")
        req = urllib.request.Request(token_url, data=token_data)
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read())
            access_token = data.get("access_token")
            if not access_token:
                print("  ❌ Token exchange failed:", data)
                return False
            print("  ✔ Access token obtained successfully!")
    except urllib.error.HTTPError as e:
        print(f"  ❌ Token request error {e.code}: {e.read().decode('utf-8', errors='replace')}")
        return False
    except Exception as e:
        print(f"  ❌ Connection error: {e}")
        return False

    # Check item status
    print(f"  [*] Fetching item details for {ext_id}...")
    try:
        url = f"https://www.googleapis.com/chromewebstore/v1.1/items/{ext_id}?projection=DRAFT"
        req = urllib.request.Request(
            url,
            headers={"Authorization": f"Bearer {access_token}", "x-goog-api-version": "2"},
        )
        with urllib.request.urlopen(req) as resp:
            item_info = json.loads(resp.read())
            print(f"  ✔ Item verified in Chrome Web Store!")
            print(f"     Kind: {item_info.get('kind', 'item')}, ID: {item_info.get('id')}")
            print(f"     Upload State: {item_info.get('uploadState', 'N/A')}")
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        print(f"  ⚠ Chrome Web Store status check returned HTTP {e.code}: {body}")
        if e.code == 404:
            print("     Hint: Ensure the initial draft item was created in the Developer Dashboard.")
        return False

    if upload:
        zip_path = DIST_DIR / "streamerz-tiktok-chrome.zip"
        if not zip_path.exists():
            print(f"  [-] Package not found at {zip_path}, run dev/package-extensions.py first.")
            return False
        print(f"  [*] Uploading package ({zip_path.stat().st_size / 1024:.1f} KB)...")
        upload_url = f"https://www.googleapis.com/upload/chromewebstore/v1.1/items/{ext_id}"
        req = urllib.request.Request(
            upload_url,
            data=zip_path.read_bytes(),
            headers={"Authorization": f"Bearer {access_token}", "x-goog-api-version": "2"},
            method="PUT",
        )
        try:
            with urllib.request.urlopen(req) as resp:
                res = json.loads(resp.read())
                print("  ✔ Upload response:", res)
        except urllib.error.HTTPError as e:
            print(f"  ❌ Upload failed HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}")
            return False

    print("  🎉 Chrome Web Store API test passed!")
    return True


def test_edge(upload: bool = False):
    print("\n" + "=" * 60)
    print(" [2/3] Testing Microsoft Edge Add-ons API")
    print("=" * 60)
    product_id = os.environ.get("EDGE_PRODUCT_ID", "98d0235c-b425-4d57-82f0-26c1f0b7dc71")
    client_id = os.environ.get("EDGE_CLIENT_ID")
    api_key = os.environ.get("EDGE_API_KEY")
    tenant_id = os.environ.get("EDGE_TENANT_ID")

    missing = []
    if not client_id: missing.append("EDGE_CLIENT_ID")
    if not api_key: missing.append("EDGE_API_KEY")
    if not tenant_id: missing.append("EDGE_TENANT_ID")
    if missing:
        print(f"  ❌ Missing credentials: {', '.join(missing)}")
        print("     Please set them in your .env file.")
        return False

    print(f"  Product ID: {product_id}")
    
    # Try Edge Publish API (ApiKey + X-ClientID) or Azure AD Bearer token
    headers = {}
    if api_key and client_id:
        # First check if api_key is direct Partner Center API Key
        headers = {
            "Authorization": f"ApiKey {api_key}",
            "X-ClientID": client_id,
        }

    access_token = None
    if tenant_id:
        print("  [*] Attempting Azure AD OAuth token exchange...")
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
                data = json.loads(resp.read())
                access_token = data.get("access_token")
                if access_token:
                    headers = {"Authorization": f"Bearer {access_token}"}
                    print("  ✔ Azure AD Bearer token obtained successfully!")
        except urllib.error.HTTPError as e:
            err_msg = e.read().decode("utf-8", errors="replace")
            print(f"  ℹ Azure AD token exchange note: {err_msg[:120]}...")
            print("  [*] Falling back to direct Partner Center ApiKey header...")

    print(f"  [*] Checking draft package status for {product_id}...")
    try:
        url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions/draft"
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as resp:
            draft_info = json.loads(resp.read())
            print(f"  ✔ Product draft found in Microsoft Partner Center!")
            print(f"     Status: {draft_info.get('status', 'OK')}")
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        if e.code == 404:
            print("  ℹ Microsoft Edge returned 404 (Resource not found):")
            print("     NOTE: Microsoft Edge Add-ons API only supports UPDATES to already-submitted extensions.")
            print("     The initial/first version MUST be submitted by clicking 'Publish' on the Partner Center web dashboard.")
            print("     Once published for review the first time, all subsequent updates work automatically via this API!")
            return True
        else:
            print(f"  ⚠ Edge Partner Center responded HTTP {e.code}: {body}")
            return False

    if upload:
        zip_path = DIST_DIR / "streamerz-tiktok-edge.zip"
        if not zip_path.exists():
            print(f"  [-] Package not found at {zip_path}, run dev/package-extensions.py first.")
            return False
        print(f"  [*] Uploading package to draft ({zip_path.stat().st_size / 1024:.1f} KB)...")
        upload_url = f"https://api.addons.microsoftedge.microsoft.com/v1/products/{product_id}/submissions/draft/package"
        upload_headers = dict(headers)
        upload_headers["Content-Type"] = "application/zip"
        req = urllib.request.Request(
            upload_url,
            data=zip_path.read_bytes(),
            headers=upload_headers,
            method="POST",
        )
        try:
            with urllib.request.urlopen(req) as resp:
                print("  ✔ Upload accepted by Microsoft Edge Add-ons API!")
        except urllib.error.HTTPError as e:
            print(f"  ❌ Upload failed HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}")
            return False

    print("  🎉 Microsoft Edge Add-ons API test passed!")
    return True


def test_firefox(upload: bool = False):
    print("\n" + "=" * 60)
    print(" [3/3] Testing Mozilla Firefox Add-ons (AMO) API")
    print("=" * 60)
    issuer = os.environ.get("AMO_JWT_ISSUER")
    secret = os.environ.get("AMO_JWT_SECRET")

    missing = []
    if not issuer: missing.append("AMO_JWT_ISSUER")
    if not secret: missing.append("AMO_JWT_SECRET")
    if missing:
        print(f"  ❌ Missing credentials: {', '.join(missing)}")
        print("     Please set them in your .env file.")
        return False

    with open(EXT_DIR / "manifest.firefox.json") as f:
        m = json.load(f)
    addon_id = m["browser_specific_settings"]["gecko"]["id"]
    version = m["version"]

    print(f"  Gecko Add-on ID: {addon_id}")
    print(f"  Version: {version}")
    print("  [*] Generating AMO JWT Token...")

    payload = {
        "iss": issuer,
        "jti": hashlib.sha256(str(time.time()).encode("utf-8")).hexdigest(),
        "iat": int(time.time()),
        "exp": int(time.time()) + 60,
    }
    
    # Pure Python stdlib JWT implementation (no external dependencies needed)
    import base64
    import hmac
    header = {"alg": "HS256", "typ": "JWT"}
    def b64url(data: bytes) -> str:
        return base64.urlsafe_b64encode(data).decode('ascii').rstrip('=')
    segments = [
        b64url(json.dumps(header, separators=(',', ':')).encode('utf-8')),
        b64url(json.dumps(payload, separators=(',', ':')).encode('utf-8')),
    ]
    sig = hmac.new(secret.encode('utf-8'), ".".join(segments).encode('ascii'), hashlib.sha256).digest()
    token = f"{segments[0]}.{segments[1]}.{b64url(sig)}"
    print("  ✔ JWT Token generated!")

    existing_addon = None
    print(f"  [*] Checking add-on detail on Mozilla AMO...")
    try:
        url = f"https://addons.mozilla.org/api/v5/addons/addon/{urllib.parse.quote(addon_id)}/"
        req = urllib.request.Request(
            url,
            headers={"Authorization": f"JWT {token}"},
        )
        with urllib.request.urlopen(req) as resp:
            existing_addon = json.loads(resp.read())
            print(f"  ✔ Add-on found on AMO! Status: {existing_addon.get('status', 'OK')}")
            latest_unlisted = existing_addon.get("latest_unlisted_version") or {}
            if latest_unlisted.get("version") == version:
                file_info = latest_unlisted.get("file") or {}
                print(f"  ✔ Version {version} is ALREADY signed and published on Mozilla AMO!")
                print(f"     Status: {file_info.get('status', 'N/A')}")
                if file_info.get("url"):
                    print(f"     Direct Download XPI: {file_info.get('url')}")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f"  ℹ Add-on {addon_id} not yet submitted on AMO (will be registered upon first upload).")
        else:
            print(f"  ⚠ AMO returned HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}")

    if upload:
        latest_unlisted = (existing_addon or {}).get("latest_unlisted_version") or {}
        if latest_unlisted.get("version") == version:
            print(f"  ℹ Version {version} already exists on AMO. Bump version in manifest.json to upload a new release.")
            print("  🎉 Mozilla AMO API test passed!")
            return True

        zip_path = DIST_DIR / "streamerz-tiktok-firefox.zip"
        if not zip_path.exists():
            print(f"  [-] Package not found at {zip_path}, run dev/package-extensions.py first.")
            return False

        print(f"  [*] Uploading package ({zip_path.stat().st_size / 1024:.1f} KB) to AMO validation pipeline...")
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
                "Authorization": f"JWT {token}",
                "Content-Type": f"multipart/form-data; boundary={boundary}",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(req) as resp:
                upload_res = json.loads(resp.read())
                upload_uuid = upload_res["uuid"]
        except urllib.error.HTTPError as e:
            print(f"  ❌ AMO Upload failed HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}")
            return False

        print("  [*] Waiting for AMO validation...")
        for _ in range(15):
            time.sleep(2)
            poll_req = urllib.request.Request(
                f"https://addons.mozilla.org/api/v5/addons/upload/{upload_uuid}/",
                headers={"Authorization": f"JWT {token}"},
            )
            try:
                with urllib.request.urlopen(poll_req) as resp:
                    poll_res = json.loads(resp.read())
                    if poll_res.get("processed"):
                        if not poll_res.get("valid"):
                            print(f"  ❌ Validation failed: {poll_res.get('validation')}")
                            return False
                        print("  ✔ Validation passed!")
                        break
            except Exception as e:
                print(f"  ⚠ Poll error: {e}")

        print(f"  [*] Submitting version {version}...")
        ver_url = f"https://addons.mozilla.org/api/v5/addons/addon/{urllib.parse.quote(addon_id)}/versions/"
        req = urllib.request.Request(
            ver_url,
            data=json.dumps({"upload": upload_uuid}).encode("utf-8"),
            headers={"Authorization": f"JWT {token}", "Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req) as resp:
                print("  ✔ Package uploaded and version created successfully on Mozilla AMO!")
        except urllib.error.HTTPError as e:
            print(f"  ❌ Version creation failed HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}")
            return False

    print("  🎉 Mozilla AMO API test passed!")
    return True


def main():
    parser = argparse.ArgumentParser(description="Test Extension Publishing API Credentials & Connectivity")
    parser.add_argument(
        "--platform",
        choices=["chrome", "edge", "firefox", "all"],
        default="all",
        help="Which platform to test (default: all)",
    )
    parser.add_argument(
        "--upload",
        action="store_true",
        help="Actually upload the package (default: test authentication and draft presence only)",
    )
    args = parser.parse_args()

    load_env()

    success = True
    if args.platform in ("chrome", "all"):
        if not test_chrome(upload=args.upload):
            success = False
    if args.platform in ("edge", "all"):
        if not test_edge(upload=args.upload):
            success = False
    if args.platform in ("firefox", "all"):
        if not test_firefox(upload=args.upload):
            success = False

    print("\n" + "=" * 60)
    if success:
        print("  🚀 All tested platforms PASSED successfully!")
    else:
        print("  ⚠️  Some platforms reported missing credentials or errors above.")
    print("=" * 60)
    return 0 if success else 1


if __name__ == "__main__":
    sys.exit(main())
