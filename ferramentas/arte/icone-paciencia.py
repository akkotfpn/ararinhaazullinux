#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 da Paciência dos Pássaros (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
def r(x, y, w, h, rad, cor, contorno=None):
    d.rounded_rectangle([x * S, y * S, (x + w) * S, (y + h) * S], rad * S, fill=cor, outline=contorno, width=S)
# Carta de trás (verso azul), inclinada para a esquerda
verso = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
dv = ImageDraw.Draw(verso)
dv.rounded_rectangle([14 * S, 8 * S, 44 * S, 50 * S], 4 * S, fill=(255, 255, 255, 255))
dv.rounded_rectangle([16 * S, 10 * S, 42 * S, 48 * S], 3 * S, fill=(30, 95, 163, 255))
verso = verso.rotate(14, resample=Image.BICUBIC, center=(29 * S, 50 * S))
im.alpha_composite(verso)
# Carta da frente
r(24, 12, 30, 44, 4, (255, 255, 255, 255), (184, 196, 207, 255))
# Pássaro (ararinha-azul)
d.ellipse([30 * S, 30 * S, 46 * S, 46 * S], fill=(59, 127, 196, 255))
d.ellipse([36 * S, 34 * S, 44 * S, 44 * S], fill=(143, 193, 236, 255))
d.ellipse([38 * S, 22 * S, 48 * S, 32 * S], fill=(59, 127, 196, 255))
d.ellipse([43 * S, 25 * S, 46 * S, 28 * S], fill=(255, 255, 255, 255))
d.polygon([(47 * S, 27 * S), (51 * S, 29 * S), (47 * S, 30 * S)], fill=(42, 42, 42, 255))
# "K" vermelho no canto
d.text((27 * S, 13 * S), "K", fill=(200, 53, 46, 255), font_size=10 * S) if hasattr(d, "text") else None
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
