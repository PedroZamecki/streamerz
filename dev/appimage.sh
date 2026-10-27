#!/bin/sh
# Packs target/release/Streamerz (and Streamerz-tiktok, if built) into Streamerz-linux-x86_64.AppImage with
# the libraries it needs (linuxdeploy; base system ones like glibc stay the system's). The TikTok window
# is copied as is: it uses the system's WebKitGTK, too big and too tied to the desktop to bundle.
# Usage: dev/appimage.sh [version]
set -eu
ver=${1:-dev}
clean_ver="${ver#v}"
tools=${TOOLS:-/tmp/appimage-tools}
mkdir -p "$tools"
ld="$tools/linuxdeploy-x86_64.AppImage"
[ -x "$ld" ] || {
    curl -fsSL -o "$ld" https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
    chmod +x "$ld"
}
app=target/AppDir
rm -rf "$app"
if [ -f target/release/Streamerz-tiktok ]; then
    install -Dm755 target/release/Streamerz-tiktok "$app/usr/bin/Streamerz-tiktok"
fi

install -Dm644 assets/brand/streamerz.png "$app/usr/share/icons/hicolor/256x256/apps/streamerz.png"
install -Dm644 assets/brand/streamerz.svg "$app/usr/share/icons/hicolor/scalable/apps/streamerz.svg"
install -Dm644 assets/brand/streamerz-simple.svg "$app/usr/share/icons/hicolor/scalable/apps/streamerz-symbolic.svg"

desktop="target/streamerz.desktop"
sed -e "s/^Exec=.*/Exec=Streamerz/" assets/streamerz.desktop > "$desktop"
echo "X-AppImage-Version=$clean_ver" >> "$desktop"

# Update information: AppImage updaters (AppImageUpdate, Gear Lever...) find new releases too.
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|PedroZamecki|streamerz|latest|Streamerz-linux-x86_64.AppImage.zsync"
export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
export LDAI_OUTPUT=Streamerz-linux-x86_64.AppImage
export LINUXDEPLOY_OUTPUT_VERSION="$clean_ver"

APPIMAGE_EXTRACT_AND_RUN=1 "$ld" --appdir "$app" --executable target/release/Streamerz \
    --desktop-file "$desktop" --icon-file assets/brand/streamerz.png --output appimage
