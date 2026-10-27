#!/bin/sh
# Packs target/release/streamerz (and streamerz-tiktok, if built) into streamerz-linux-x86_64.AppImage with
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
if [ -f target/release/streamerz-tiktok ]; then
    install -Dm755 target/release/streamerz-tiktok "$app/usr/bin/streamerz-tiktok"
fi

desktop="target/streamerz.desktop"
sed -e "s/^Exec=.*/Exec=streamerz/" assets/streamerz.desktop > "$desktop"
cat <<EOF >> "$desktop"
X-AppImage-Version=$clean_ver
X-AppImage-Name=streamerz
EOF

# Update information: AppImage updaters (AppImageUpdate, Gear Lever...) find new releases too.
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|PedroZamecki|streamerz|latest|streamerz-linux-x86_64.AppImage.zsync"
export UPDATE_INFORMATION="$LDAI_UPDATE_INFORMATION"
export LDAI_OUTPUT=streamerz-linux-x86_64.AppImage
export LINUXDEPLOY_OUTPUT_VERSION="$clean_ver"

APPIMAGE_EXTRACT_AND_RUN=1 "$ld" --appdir "$app" --executable target/release/streamerz \
    --desktop-file "$desktop" --icon-file assets/brand/streamerz.png --output appimage
