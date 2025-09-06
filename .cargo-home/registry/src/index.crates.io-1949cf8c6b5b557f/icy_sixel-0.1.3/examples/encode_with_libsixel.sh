#!/bin/sh
#
# Prerequisites: img2sixel
#
# Installation:
# $ sudo apt install libsixel-bin
#
# Tested using:
# $ img2sixel --version
# img2sixel 1.10.3
#
# configured with:
#   libcurl: no
#   libpng: no
#   libjpeg: no
#   gdk-pixbuf2: no
#   GD: no

img2sixel -o map8_libsixel.six map8.png
img2sixel -o snake_libsixel.six snake.png
