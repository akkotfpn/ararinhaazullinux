#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 do Instalador (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
def r(x, y, w, h, rad, cor):
    d.rounded_rectangle([x * S, y * S, (x + w) * S, (y + h) * S], rad * S, fill=cor)
# Disco
r(6, 36, 52, 22, 5, (107, 124, 143, 255))
r(9, 39, 46, 12, 3, (201, 211, 221, 255))
r(44, 52, 7, 3, 1, (63, 191, 107, 255))
# Seta azul descendo para o disco
azul = (43, 123, 196, 255)
r(27, 4, 10, 20, 2, azul)
d.polygon([(18 * S, 22 * S), (46 * S, 22 * S), (32 * S, 36 * S)], fill=azul)
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
