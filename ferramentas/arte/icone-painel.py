#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 do Painel de Controle (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
cores = [(43, 123, 196, 255), (46, 158, 106, 255), (217, 115, 43, 255), (139, 63, 184, 255)]
for i, c in enumerate(cores):
    x = 6 + (i % 2) * 28
    y = 6 + (i // 2) * 28
    d.rounded_rectangle([x * S, y * S, (x + 24) * S, (y + 24) * S], 6 * S, fill=c)
d.ellipse([22 * S, 22 * S, 42 * S, 42 * S], fill=(255, 255, 255, 255))
d.ellipse([28 * S, 28 * S, 36 * S, 36 * S], fill=(107, 124, 143, 255))
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
