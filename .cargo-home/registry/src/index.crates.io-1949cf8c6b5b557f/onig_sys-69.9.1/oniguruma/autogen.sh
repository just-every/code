#!/bin/sh
# autogen.sh for Oniguruma

echo "Generating autotools files."
#autoreconf --install --force --symlink || exit 1
autoreconf --install --force || exit 1

echo ""
echo "Run ./configure, make, and make install."
