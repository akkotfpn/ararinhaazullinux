#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 do app Músicas (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle([6 * S, 6 * S, 58 * S, 58 * S], 14 * S, fill=(43, 123, 196, 255))
d.rounded_rectangle([6 * S, 6 * S, 58 * S, 30 * S], 14 * S, fill=(73, 148, 214, 255))
d.rectangle([6 * S, 22 * S, 58 * S, 32 * S], fill=(43, 123, 196, 255))
branco = (255, 255, 255, 255)
d.ellipse([16 * S, 38 * S, 28 * S, 48 * S], fill=branco)
d.ellipse([34 * S, 34 * S, 46 * S, 44 * S], fill=branco)
d.rectangle([25 * S, 16 * S, 28 * S, 43 * S], fill=branco)
d.rectangle([43 * S, 12 * S, 46 * S, 39 * S], fill=branco)
d.polygon([(25 * S, 16 * S), (46 * S, 12 * S), (46 * S, 18 * S), (25 * S, 22 * S)], fill=branco)
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
