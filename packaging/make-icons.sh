#!/bin/sh
# Regenerates the app icons from icon.svg. Needs rsvg-convert and ImageMagick.
set -eu
cd "$(dirname "$0")"
rsvg-convert -w 1024 -h 1024 icon.svg -o icon-1024.png
rsvg-convert -w 256 -h 256 icon.svg -o icon-256.png
for s in 16 24 32 48 64 256; do rsvg-convert -w $s -h $s icon.svg -o "ico-$s.png"; done
magick ico-16.png ico-24.png ico-32.png ico-48.png ico-64.png ico-256.png icon.ico
# The window icon, as raw RGBA pixels (64×64) so the app needs no image decoder.
magick ico-64.png -depth 8 rgba:icon-64.rgba
rm ico-*.png
