#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desenha o ícone 64x64 do Gerenciador de Tarefas (supersample 4x)."""
import sys
from PIL import Image, ImageDraw

S = 4
im = Image.new("RGBA", (64 * S, 64 * S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle([6 * S, 10 * S, 58 * S, 54 * S], 7 * S, fill=(30, 64, 104, 255))
d.rounded_rectangle([8 * S, 12 * S, 56 * S, 52 * S], 6 * S, fill=(245, 249, 252, 255))
pts = [(10, 42), (18, 38), (24, 40), (30, 24), (36, 34), (42, 20), (48, 30), (54, 26)]
area = [(10 * S, 50 * S)] + [(x * S, y * S) for x, y in pts] + [(54 * S, 50 * S)]
d.polygon(area, fill=(43, 123, 196, 70))
d.line([(x * S, y * S) for x, y in pts], fill=(43, 123, 196, 255), width=3 * S, joint="curve")
im = im.resize((64, 64), Image.LANCZOS)
im.save(sys.argv[1])
