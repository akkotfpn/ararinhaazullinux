#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 da Calculadora (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
def r(x, y, w, h, rad, cor):
    d.rounded_rectangle([x * S, y * S, (x + w) * S, (y + h) * S], rad * S, fill=cor)
r(10, 4, 44, 56, 7, (30, 64, 104, 255))
r(12, 6, 40, 52, 6, (52, 98, 150, 255))
r(16, 10, 32, 12, 3, (214, 232, 247, 255))
for li in range(3):
    for ci in range(3):
        cor = (43, 123, 196, 255) if (li, ci) == (2, 2) else (238, 242, 246, 255)
        r(16 + ci * 11, 26 + li * 10, 9, 8, 2, cor)
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
