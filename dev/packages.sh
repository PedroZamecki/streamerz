#!/bin/sh
# Packs target/release/Streamerz for Linux, every way a release ships it (into dist/), beside the
# AppImage (dev/appimage.sh). Nothing is compiled: each one holds the same program.
#   Streamerz-linux-x86_64              the program alone; it updates itself, like the Windows exe
#   Streamerz-linux-x86_64.deb          packages (nfpm) that add the Streamerz repository, so the
#   Streamerz-linux-x86_64.rpm            system's updater updates Streamerz (Arch: pacman.conf, by hand)
#   Streamerz-linux-x86_64.pkg.tar.zst
#   streamerz-flatpak-repo.tar          the Flatpak as an OSTree repository, unsigned: the release
#                                       signs it, puts it on GitHub Pages and makes the .flatpak from it
#                                       (dev/publish-repos.sh)
# Usage: dev/packages.sh [version]   (needs flatpak; nfpm is downloaded when missing; STREAMERZ_BIN
# names another program to pack)
set -eu
bin=${STREAMERZ_BIN:-target/release/Streamerz}
ver=${1:-dev}
clean=${ver#v}
case $clean in [0-9]*) ;; *) clean=0.0.0 ;; esac
id=io.github.pedrozamecki.streamerz
day=$(git log -1 --format=%cs 2>/dev/null || date +%F)
tools=${TOOLS:-/tmp/appimage-tools}
nfpm_version=2.48.0

mkdir -p dist target/packaging "$tools"
nfpm=$(command -v nfpm || echo "$tools/nfpm")
[ -x "$nfpm" ] || curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v$nfpm_version/nfpm_${nfpm_version}_Linux_x86_64.tar.gz" \
    | tar -xz -C "$tools" nfpm

# The program alone.
install -m755 "$bin" dist/Streamerz-linux-x86_64
install -m755 "$bin" target/packaging/Streamerz

# What the packages install beside it: the menu entry and what software centers show.
grep -v '^X-AppImage' assets/streamerz.desktop > target/packaging/streamerz.desktop
metainfo() {
    sed -e "s/@VERSION@/$clean/" -e "s/@DATE@/$day/" -e "s/@DESKTOP_ID@/$1/" \
        packaging/io.github.pedrozamecki.streamerz.metainfo.xml
}
metainfo streamerz.desktop > target/packaging/streamerz.metainfo.xml

export STREAMERZ_PACKAGE_VERSION="$clean"
for kind in deb rpm archlinux; do
    case $kind in archlinux) ext=pkg.tar.zst ;; *) ext=$kind ;; esac
    "$nfpm" package --config packaging/nfpm.yaml --packager "$kind" --target "dist/Streamerz-linux-x86_64.$ext"
done

# The Flatpak, made from the program as it is (no SDK needed): the folder flatpak-builder would make.
build=target/flatpak/build
repo=target/flatpak/repo
rm -rf target/flatpak
install -Dm755 "$bin" "$build/files/bin/Streamerz"
sed -e "s/^Icon=.*/Icon=$id/" target/packaging/streamerz.desktop > target/packaging/flatpak.desktop
install -Dm644 target/packaging/flatpak.desktop "$build/files/share/applications/$id.desktop"
install -Dm644 assets/brand/streamerz.png "$build/files/share/icons/hicolor/256x256/apps/$id.png"
install -Dm644 assets/brand/streamerz.svg "$build/files/share/icons/hicolor/scalable/apps/$id.svg"
metainfo "$id.desktop" > target/packaging/flatpak.metainfo.xml
install -Dm644 target/packaging/flatpak.metainfo.xml "$build/files/share/metainfo/$id.metainfo.xml"
printf '[Application]\nname=%s\nruntime=org.freedesktop.Platform/x86_64/26.08\nsdk=org.freedesktop.Sdk/x86_64/26.08\n' "$id" \
    > "$build/metadata"
# What software centers show of the remote (name, summary, version, icon): AppStream data, when
# appstreamcli's compose is here (CI installs it); without it the Flatpak installs and updates the same.
if appstreamcli compose --help > /dev/null 2>&1; then
    # PNG icons, which every client reads (newer appstreamcli makes JPEG XL unless told).
    png=$(appstreamcli compose --help | grep -q -- --image-format && echo --image-format=png || true)
    appstreamcli compose $png --origin="$id" --result-root="$build/files" \
        --data-dir="$build/files/share/app-info/xmls" --icons-dir="$build/files/share/app-info/icons/flatpak" \
        --prefix=/ "$build/files" > /dev/null
fi
# What the sandbox lets through. Streamerz drives programs on the host (browsers for TikTok and
# overlay pictures, terminals for AI agents, Run a Program): flatpak-spawn --host, looked up under
# /run/host. Settings stay in the host's ~/.config/streamerz, shared with every other copy.
flatpak build-finish "$build" --command=Streamerz \
    --share=network --share=ipc \
    --socket=wayland --socket=x11 --socket=pulseaudio \
    --device=all \
    --filesystem=home --filesystem=host-os:ro --filesystem=/opt:ro \
    --talk-name=org.freedesktop.Flatpak \
    --talk-name=org.freedesktop.Notifications \
    --talk-name=org.freedesktop.secrets \
    --talk-name=org.kde.StatusNotifierWatcher \
    --talk-name='org.mpris.MediaPlayer2.*'
flatpak build-export --subject="Streamerz $clean" "$repo" "$build" stable
tar -C target/flatpak -cf dist/streamerz-flatpak-repo.tar repo
ls -l dist
