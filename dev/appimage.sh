#!/bin/sh
# Packs target/release/streamerz (and streamerz-tiktok, if built) into streamerz-x86_64.AppImage with
# the libraries it needs (linuxdeploy; base system ones like glibc stay the system's). The TikTok window
# is copied as is: it uses the system's WebKitGTK, too big and too tied to the desktop to bundle.
# Usage: dev/appimage.sh [version]
set -eu
ver=${1:-dev}
tools=${TOOLS:-/tmp/appimage-tools}
mkdir -p "$tools"
ld="$tools/linuxdeploy-x86_64.AppImage"
[ -x "$ld" ] || {
    curl -fsSL -o "$ld" https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
    chmod +x "$ld"
}
app=target/AppDir
rm -rf "$app"
if [ -f target/release/streamerz-tiktok ]; then
    install -Dm755 target/release/streamerz-tiktok "$app/usr/bin/streamerz-tiktok"
fi
# Update information: AppImage updaters (AppImageUpdate, Gear Lever...) find new releases too.
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|PedroZamecki|streamerz|latest|streamerz-x86_64.AppImage.zsync"
export LDAI_OUTPUT=streamerz-x86_64.AppImage LINUXDEPLOY_OUTPUT_VERSION="$ver"
APPIMAGE_EXTRACT_AND_RUN=1 "$ld" --appdir "$app" --executable target/release/streamerz \
    --desktop-file assets/streamerz.desktop --icon-file assets/brand/streamerz.png --output appimage
