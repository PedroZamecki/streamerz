#!/usr/bin/env bash
# Builds a Wine 11.20 with the small patches the GPUI frontend needs, for local testing only.
#
# Upstream Wine stubs IDWriteFontSet::GetMatchingFonts (GPUI cannot resolve any font) and the Direct
# Manipulation classes (GPUI treats the failure as fatal). gpui-support.patch fills both in.
# Needs clang, lld, flex, bison and the usual Wine build libraries; no root and no mingw-w64.
# Usage: dev/wine/build.sh [work dir]   (default /tmp/streamerz-wine)
# Then: dev/ui-smoke.py <Streamerz.exe> --wine --wine-bin <work dir>/full/wine \
#         --wine-prefix <prefix> with WINESERVER=<work dir>/full/server/wineserver
set -euo pipefail

version=11.20
work=${1:-/tmp/streamerz-wine}
here=$(cd "$(dirname "$0")" && pwd)

mkdir -p "$work"
cd "$work"
if [ ! -d "wine-wine-$version" ]; then
    curl -fsSL "https://gitlab.winehq.org/wine/wine/-/archive/wine-$version/wine-wine-$version.tar.gz" | tar xz
    patch -d "wine-wine-$version" -p1 <"$here/gpui-support.patch"
fi
mkdir -p full
cd full
[ -f Makefile ] || "../wine-wine-$version/configure" --enable-win64 --with-mingw=clang --disable-tests
make -j"$(nproc)"
echo "Wine: $work/full/wine  server: $work/full/server/wineserver"
