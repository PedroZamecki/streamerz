#!/bin/sh
# Regenerates every raster of the app's logo from the SVGs in assets/brand (needs rsvg-convert and magick).
#
#   streamerz.svg              colour, for light backgrounds    (window and tray icon, light theme)
#   streamerz-dark.svg         colour, for dark backgrounds     (window icon, dark theme)
#   streamerz-simple.svg       black and white, light themes    (tray icon, light theme)
#   streamerz-simple-dark.svg  black and white, dark themes     (tray icon, dark theme)
#
# Writes next to the SVGs: streamerz.png (256 px, the desktop file and AppImage icon), streamerz.ico (the
# Windows exe) and rgba/<name>.rgba (64x64 raw pixels, built into the binary: no image decoder needed).
set -eu
cd "$(dirname "$0")/../assets/brand"
mkdir -p rgba
rsvg-convert -w 256 -h 256 streamerz.svg -o streamerz.png
magick streamerz.png -define icon:auto-resize=64,48,32,16 streamerz.ico
for svg in streamerz*.svg; do
    rsvg-convert -w 64 -h 64 "$svg" | magick png:- -depth 8 "rgba:rgba/${svg%.svg}.rgba"
done
