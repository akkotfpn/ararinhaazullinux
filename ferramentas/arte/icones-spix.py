#!/usr/bin/env python3
# SPDX-License-Identifier: CC-BY-SA-4.0
"""Ícones Spix: um conjunto coerente para os apps do Ararinha.

Mesma grade (azulejo arredondado de 88% do quadro, raio 22%), mesma luz
(gradiente de cima para baixo, brilho "Aero" na metade de cima, sombra
suave embaixo) e o mesmo traço (símbolo branco com sombra discreta).
Tudo desenhado em 8x e reduzido, para bordas lisas.

    python3 ferramentas/arte/icones-spix.py            # grava em apps/*/recursos
    python3 ferramentas/arte/icones-spix.py --mockup saida.png
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 8  # super-amostragem
BASE = 128 * S

BRANCO = (255, 255, 255, 255)


def hexa(c, a=255):
    c = c.lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4)) + (a,)


def misturar(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(4))


def clarear(c, t):
    return misturar(c, (255, 255, 255, c[3]), t)


def escurecer(c, t):
    return misturar(c, (0, 0, 0, c[3]), t)


def mascara_azulejo(n=BASE):
    m = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(m)
    mg = round(n * 0.06)
    d.rounded_rectangle((mg, mg, n - mg, n - mg), radius=round(n * 0.22), fill=255)
    return m


def azulejo(cor):
    """Fundo: gradiente vertical, brilho em cima, borda interna clara."""
    n = BASE
    cima, baixo = clarear(cor, 0.18), escurecer(cor, 0.22)
    grad = Image.new("RGBA", (n, n))
    px = grad.load()
    for y in range(n):
        c = misturar(cima, baixo, y / n)
        for x in range(n):
            px[x, y] = c
    m = mascara_azulejo()
    im = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    # Sombra suave embaixo do azulejo.
    sombra = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    sm = m.filter(ImageFilter.GaussianBlur(n * 0.025))
    sombra.putalpha(sm.point(lambda v: v * 0.35))
    im.alpha_composite(sombra, (0, round(n * 0.02)))
    fundo = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    fundo.paste(grad, (0, 0), m)
    im.alpha_composite(fundo)
    # Brilho "Aero": elipse branca translúcida na metade de cima.
    brilho = Image.new("L", (n, n), 0)
    ImageDraw.Draw(brilho).ellipse((-n * 0.3, -n * 0.75, n * 1.3, n * 0.5), fill=70)
    brilho = ImageChops.multiply(brilho, m)
    camada = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    camada.putalpha(brilho)
    im.alpha_composite(camada)
    # Contorno interno claro (1 px no tamanho final).
    borda = Image.new("L", (n, n), 0)
    mg = round(n * 0.06)
    ImageDraw.Draw(borda).rounded_rectangle((mg, mg, n - mg, n - mg), radius=round(n * 0.22), outline=60, width=S)
    camada = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    camada.putalpha(borda)
    im.alpha_composite(camada)
    return im


def com_sombra(simbolo):
    """Símbolo com sombra discreta para baixo (a mesma luz em todos)."""
    n = BASE
    a = simbolo.getchannel("A")
    s = Image.new("RGBA", (n, n), (0, 30, 60, 0))
    s.putalpha(a.filter(ImageFilter.GaussianBlur(n * 0.012)).point(lambda v: v * 0.45))
    im = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    im.alpha_composite(s, (0, round(n * 0.012)))
    im.alpha_composite(simbolo)
    return im


def tela():
    return Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))


def u(v):
    """Unidade da grade: 128 px no ícone grande."""
    return v * S


def linha_grossa(d, pts, larg, cor=BRANCO):
    d.line(pts, fill=cor, width=round(larg), joint="curve")
    r = larg / 2
    for x, y in (pts[0], pts[-1]):
        d.ellipse((x - r, y - r, x + r, y + r), fill=cor)


# ---------------------------------------------------------------- símbolos

def s_pasta(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(30), u(38), u(66), u(52)), radius=u(5), fill=BRANCO)
    d.rounded_rectangle((u(30), u(46), u(98), u(92)), radius=u(7), fill=BRANCO)
    d.rounded_rectangle((u(30), u(56), u(98), u(92)), radius=u(7), fill=clarear(cor, 0.82))
    return im


def s_engrenagem(cor):
    im = tela(); d = ImageDraw.Draw(im)
    cx = cy = u(64)
    dentes = 8
    for i in range(dentes):
        a = i * 2 * math.pi / dentes
        dente = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
        ImageDraw.Draw(dente).rounded_rectangle((cx - u(7), cy - u(38), cx + u(7), cy - u(20)), radius=u(3), fill=BRANCO)
        im.alpha_composite(dente.rotate(math.degrees(a), center=(cx, cy), resample=Image.BICUBIC))
    d.ellipse((cx - u(29), cy - u(29), cx + u(29), cy + u(29)), fill=BRANCO)
    d.ellipse((cx - u(11), cy - u(11), cx + u(11), cy + u(11)), fill=(0, 0, 0, 0))
    return im


def s_controles(cor):
    im = tela(); d = ImageDraw.Draw(im)
    for i, pos in enumerate((0.35, 0.7, 0.5)):
        x = u(42 + i * 22)
        linha_grossa(d, [(x, u(34)), (x, u(94))], u(6))
        y = u(34) + pos * u(60)
        d.ellipse((x - u(10), y - u(10), x + u(10), y + u(10)), fill=BRANCO)
        d.ellipse((x - u(5), y - u(5), x + u(5), y + u(5)), fill=escurecer(cor, 0.1))
    return im


def s_sacola(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.arc((u(46), u(26), u(82), u(62)), 180, 360, fill=BRANCO, width=u(6))
    d.rounded_rectangle((u(32), u(44), u(96), u(98)), radius=u(8), fill=BRANCO)
    # seta para baixo (baixar app)
    c = escurecer(cor, 0.05)
    linha_grossa(d, [(u(64), u(56)), (u(64), u(84))], u(6), c)
    d.polygon([(u(52), u(74)), (u(76), u(74)), (u(64), u(88))], fill=c)
    return im


def s_calculadora(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(36), u(26), u(92), u(102)), radius=u(8), fill=BRANCO)
    c = escurecer(cor, 0.05)
    d.rounded_rectangle((u(43), u(33), u(85), u(49)), radius=u(3), fill=clarear(cor, 0.55))
    for li in range(3):
        for co in range(3):
            x, y = u(44 + co * 14), u(56 + li * 14)
            cc = hexa("#E2553B") if (co == 2 and li == 2) else c
            d.rounded_rectangle((x, y, x + u(10), y + u(10)), radius=u(2.5), fill=cc)
    return im


def s_pulso(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(28), u(34), u(100), u(94)), radius=u(8), outline=BRANCO, width=u(5))
    pts = [(u(36), u(68)), (u(50), u(68)), (u(57), u(50)), (u(67), u(84)), (u(76), u(60)), (u(82), u(68)), (u(92), u(68))]
    linha_grossa(d, pts, u(6))
    return im


def s_nota(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.polygon([(u(52), u(36)), (u(92), u(28)), (u(92), u(40)), (u(52), u(48))], fill=BRANCO)
    linha_grossa(d, [(u(55), u(42)), (u(55), u(86))], u(6))
    linha_grossa(d, [(u(89), u(34)), (u(89), u(78))], u(6))
    d.ellipse((u(36), u(76), u(58), u(96)), fill=BRANCO)
    d.ellipse((u(70), u(68), u(92), u(88)), fill=BRANCO)
    return im


def s_cartas(cor):
    im = tela(); d = ImageDraw.Draw(im)
    trás = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    dt = ImageDraw.Draw(trás)
    dt.rounded_rectangle((u(34), u(34), u(76), u(94)), radius=u(6), fill=clarear(cor, 0.6))
    trás = trás.rotate(14, center=(u(55), u(64)), resample=Image.BICUBIC)
    im.alpha_composite(trás)
    d.rounded_rectangle((u(50), u(30), u(94), u(96)), radius=u(6), fill=BRANCO)
    c = escurecer(cor, 0.2)
    # espadas
    cx, cy = u(72), u(60)
    d.ellipse((cx - u(13), cy - u(4), cx + u(1), cy + u(10)), fill=c)
    d.ellipse((cx - u(1), cy - u(4), cx + u(13), cy + u(10)), fill=c)
    d.polygon([(cx - u(13), cy + u(2)), (cx, cy - u(18)), (cx + u(13), cy + u(2))], fill=c)
    d.polygon([(cx - u(5), cy + u(18)), (cx, cy + u(4)), (cx + u(5), cy + u(18))], fill=c)
    return im


def s_instalar(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(28), u(70), u(100), u(98)), radius=u(7), fill=BRANCO)
    d.ellipse((u(84), u(81), u(91), u(88)), fill=hexa("#3FBF6B"))
    linha_grossa(d, [(u(64), u(26)), (u(64), u(58))], u(8))
    d.polygon([(u(48), u(50)), (u(80), u(50)), (u(64), u(66))], fill=BRANCO)
    return im


def s_imagem(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(28), u(34), u(100), u(94)), radius=u(8), fill=BRANCO)
    d.rounded_rectangle((u(34), u(40), u(94), u(88)), radius=u(5), fill=clarear(cor, 0.8))
    d.polygon([(u(34), u(88)), (u(56), u(58)), (u(72), u(78)), (u(80), u(68)), (u(94), u(88))], fill=escurecer(cor, 0.05))
    d.ellipse((u(76), u(46), u(88), u(58)), fill=hexa("#F5B53D"))
    return im


def s_notas(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(36), u(26), u(92), u(102)), radius=u(7), fill=BRANCO)
    c = escurecer(cor, 0.35)
    for i in range(5):
        y = u(44 + i * 11)
        fim = u(82) if i < 4 else u(66)
        linha_grossa(d, [(u(46), y), (fim, y)], u(4), c)
    return im


def s_computador(cor):
    im = tela(); d = ImageDraw.Draw(im)
    d.rounded_rectangle((u(24), u(30), u(104), u(84)), radius=u(6), fill=BRANCO)
    d.rounded_rectangle((u(30), u(36), u(98), u(78)), radius=u(3), fill=clarear(cor, 0.25))
    d.polygon([(u(56), u(84)), (u(72), u(84)), (u(76), u(96)), (u(52), u(96))], fill=BRANCO)
    d.rounded_rectangle((u(44), u(94), u(84), u(100)), radius=u(3), fill=BRANCO)
    return im


APPS = [
    # (arquivo de saída relativo a apps/, cor do azulejo, símbolo, rótulo)
    ("explorador/recursos/explorador", "#E8A317", s_pasta, "Explorador"),
    ("configuracoes/recursos/configuracoes", "#5B6B7F", s_engrenagem, "Configurações"),
    ("painel/recursos/painel-de-controle", "#0F8B8D", s_controles, "Painel de Controle"),
    ("central/recursos/central", "#2B7BC4", s_sacola, "Central de Apps"),
    ("calculadora/recursos/calculadora", "#3A4A5C", s_calculadora, "Calculadora"),
    ("tarefas/recursos/tarefas", "#2E9E5B", s_pulso, "Tarefas"),
    ("musicas/recursos/musicas", "#E2553B", s_nota, "Músicas"),
    ("paciencia/recursos/paciencia", "#1F7A4D", s_cartas, "Paciência"),
    ("instalador/recursos/instalador", "#1E5FA3", s_instalar, "Instalador"),
    ("imagens/recursos/imagens", "#7B4FC9", s_imagem, "Imagens"),
    ("notas/recursos/notas", "#E9B826", s_notas, "Notas"),
    ("explorador/recursos/este-computador", "#2B7BC4", s_computador, "Este Computador"),
]


def icone(cor, simbolo):
    c = hexa(cor)
    im = azulejo(c)
    im.alpha_composite(com_sombra(simbolo(c)))
    return im


def reduzir(im, lado):
    return im.resize((lado, lado), Image.LANCZOS)


def mockup(saida):
    from PIL import ImageFont
    try:
        fonte = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 13)
    except OSError:
        fonte = ImageFont.load_default()
    col = 6
    cel = 150
    linhas = math.ceil(len(APPS) / col)
    img = Image.new("RGBA", (col * cel * 2 + 80, linhas * 190 + 20), (0, 0, 0, 0))
    for k, (fundo, texto) in enumerate((((244, 247, 250, 255), (11, 26, 46, 255)), ((11, 26, 46, 255), (234, 242, 249, 255)))):
        x0 = k * (col * cel + 40)
        ImageDraw.Draw(img).rectangle((x0, 0, x0 + col * cel + 40, img.height), fill=fundo)
        for i, (_, cor, simb, rot) in enumerate(APPS):
            ic = icone(cor, simb)
            x = x0 + 20 + (i % col) * cel
            y = 20 + (i // col) * 190
            img.alpha_composite(reduzir(ic, 128), (x + 11, y))
            img.alpha_composite(reduzir(ic, 32), (x + 59, y + 132))
            d = ImageDraw.Draw(img)
            w = d.textlength(rot, font=fonte)
            d.text((x + 75 - w / 2, y + 170), rot, fill=texto, font=fonte)
    img.save(saida)


def main():
    if len(sys.argv) > 2 and sys.argv[1] == "--mockup":
        mockup(sys.argv[2])
        return
    raiz = Path(__file__).resolve().parents[2] / "apps"
    for caminho, cor, simb, _ in APPS:
        ic = icone(cor, simb)
        reduzir(ic, 64).save(raiz / f"{caminho}.png", optimize=True)
        reduzir(ic, 128).save(raiz / f"{caminho}-128.png", optimize=True)
        print("gravado", caminho)


if __name__ == "__main__":
    main()
