#!/usr/bin/env bash
# Check the Linux packages produced by dev/appimage.sh and dev/packages.sh.
set -euo pipefail

test -s dist/Streamerz-linux-x86_64.AppImage
test -s dist/Streamerz-linux-x86_64.AppImage.zsync
deb=$(dpkg-deb --contents dist/Streamerz-linux-x86_64.deb)
grep -q '/usr/bin/Streamerz$' <<< "$deb"
grep -q '/etc/apt/sources.list.d/streamerz.sources$' <<< "$deb"
tar --zstd -tf dist/Streamerz-linux-x86_64.pkg.tar.zst | grep -x 'usr/bin/Streamerz' > /dev/null
rpm -qlp dist/Streamerz-linux-x86_64.rpm | grep -x '/etc/yum.repos.d/streamerz.repo' > /dev/null
cmp "${STREAMERZ_BIN:-target/release/Streamerz}" dist/Streamerz-linux-x86_64
flatpak repo target/flatpak/repo
