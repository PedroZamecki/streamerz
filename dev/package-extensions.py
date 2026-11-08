#!/usr/bin/env python3
"""
Streamerz TikTok Extension Packager
Produces ready-to-upload zip archives for Chrome Web Store, Microsoft Edge Add-ons,
and Mozilla Firefox Add-ons (AMO), tailored for UNLISTED publishing.
"""

import json
import shutil
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXT_DIR = REPO_ROOT / "assets" / "tiktok-extension"
DIST_DIR = REPO_ROOT / "dist"

def build_chrome_edge():
    """Chrome and Edge require MV3 without the local development 'key' attribute."""
    with open(EXT_DIR / "manifest.json", "r", encoding="utf-8") as f:
        manifest = json.load(f)
    
    # Strip key (forbidden on Chrome Web Store and Edge Add-ons upload)
    manifest.pop("key", None)
    
    files = {
        "manifest.json": json.dumps(manifest, indent=2).encode("utf-8"),
        "bridge.js": (EXT_DIR / "bridge.js").read_bytes(),
        "frame.js": (EXT_DIR / "frame.js").read_bytes(),
        "offscreen.html": (EXT_DIR / "offscreen.html").read_bytes(),
        "offscreen.js": (EXT_DIR / "offscreen.js").read_bytes(),
        "icon.png": (EXT_DIR / "icon.png").read_bytes(),
    }
    
    chrome_zip = DIST_DIR / "streamerz-tiktok-chrome.zip"
    with zipfile.ZipFile(chrome_zip, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name, data)
    
    edge_zip = DIST_DIR / "streamerz-tiktok-edge.zip"
    shutil.copyfile(chrome_zip, edge_zip)
    
    return chrome_zip, edge_zip

def build_firefox():
    """Firefox uses gecko ID in browser_specific_settings, background scripts, and no offscreen API."""
    with open(EXT_DIR / "manifest.firefox.json", "r", encoding="utf-8") as f:
        manifest = json.load(f)
        
    manifest.pop("key", None)
    
    files = {
        "manifest.json": json.dumps(manifest, indent=2).encode("utf-8"),
        "bridge.js": (EXT_DIR / "bridge.js").read_bytes(),
        "frame.js": (EXT_DIR / "frame.js").read_bytes(),
        "icon.png": (EXT_DIR / "icon.png").read_bytes(),
    }
    
    firefox_zip = DIST_DIR / "streamerz-tiktok-firefox.zip"
    with zipfile.ZipFile(firefox_zip, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name, data)
            
    return firefox_zip

def main():
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    chrome_zip, edge_zip = build_chrome_edge()
    firefox_zip = build_firefox()
    
    print("=" * 60)
    print("Streamerz TikTok Extension Packages Created Successfully!")
    print("=" * 60)
    for p in [chrome_zip, edge_zip, firefox_zip]:
        size_kb = p.stat().st_size / 1024
        print(f"  [+] {p.name:<32} {size_kb:6.1f} KB  ({p})")
    print("=" * 60)

if __name__ == "__main__":
    main()
