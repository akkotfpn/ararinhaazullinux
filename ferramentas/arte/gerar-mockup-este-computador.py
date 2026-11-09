# SPDX-License-Identifier: GPL-3.0-or-later
# Gera arte/mockups/este-computador.svg a partir dos ícones e do logo.
# Uso: python3 ferramentas/arte/gerar-mockup-este-computador.py .
import re, sys
base = sys.argv[1]
def icon(name, x, y, size, n=[0]):
    n[0]+=1
    s=open(f"{base}/arte/icones/{name}.svg").read()
    s=re.sub(r'<\?xml.*?\?>|<!--.*?-->|<title>.*?</title>','',s,flags=re.S)
    inner=re.search(r'<svg[^>]*>(.*)</svg>',s,re.S).group(1)
    p=f"i{n[0]}_"
    inner=re.sub(r'id="(\w+)"',lambda m:f'id="{p}{m.group(1)}"',inner)
    inner=re.sub(r'url\(#(\w+)\)',lambda m:f'url(#{p}{m.group(1)})',inner)
    return f'<svg x="{x}" y="{y}" width="{size}" height="{size}" viewBox="0 0 64 64">{inner}</svg>'

def logo(x,y,size):
    s=open(f"{base}/arte/logo/ararinha-logo.svg").read()
    s=re.sub(r'<\?xml.*?\?>|<!--.*?-->|<title>.*?</title>','',s,flags=re.S)
    inner=re.search(r'<svg[^>]*>(.*)</svg>',s,re.S).group(1)
    inner=re.sub(r'id="(\w+)"',lambda m:f'id="lg_{m.group(1)}"',inner)
    inner=re.sub(r'url\(#(\w+)\)',lambda m:f'url(#lg_{m.group(1)})',inner)
    return f'<svg x="{x}" y="{y}" width="{size}" height="{size}" viewBox="0 0 256 256">{inner}</svg>'

T="#0B1A2E"; T2="#4A5A6A"; AC="#2B7BC4"; B="#D3DDE6"
F='font-family="Inter, Segoe UI, Noto Sans, sans-serif"'
o=[]
o.append(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 800" width="1280" height="800" {F}>')
o.append('<title>Mockup — Ninho, Este Computador</title>')
o.append('''<defs><linearGradient id="wp" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#5AA8EC"/><stop offset="0.6" stop-color="#2B7BC4"/><stop offset="1" stop-color="#0E3F73"/></linearGradient>
<filter id="sh" x="-5%" y="-5%" width="110%" height="115%"><feDropShadow dx="0" dy="8" stdDeviation="12" flood-color="#0B1A2E" flood-opacity="0.3"/></filter></defs>''')
# papel de parede
o.append('<rect width="1280" height="800" fill="url(#wp)"/>')
o.append('<circle cx="1080" cy="160" r="90" fill="#E8B23A" opacity="0.85"/>')
o.append('<path d="M0 640 C 300 600, 700 610, 1280 560 L1280 800 L0 800 Z" fill="#0E3F73" opacity="0.6"/>')
o.append('<path d="M760 620 C 900 590, 1050 585, 1280 560" stroke="#8A5A33" stroke-width="12" fill="none" stroke-linecap="round"/>')
# janela
wx,wy,ww,wh=150,60,980,620
o.append(f'<g filter="url(#sh)"><rect x="{wx}" y="{wy}" width="{ww}" height="{wh}" rx="12" fill="#FFFFFF"/></g>')
# título
o.append(f'<rect x="{wx}" y="{wy}" width="{ww}" height="44" rx="12" fill="#F4F7FA"/><rect x="{wx}" y="{wy+30}" width="{ww}" height="14" fill="#F4F7FA"/>')
o.append(icon("este-computador",wx+14,wy+10,24))
o.append(f'<text x="{wx+46}" y="{wy+27}" font-size="14" fill="{T}">Este Computador</text>')
for i,(c) in enumerate(["#D3DDE6","#D3DDE6","#C2413B"]):
    cx=wx+ww-90+i*30
    o.append(f'<circle cx="{cx}" cy="{wy+22}" r="7" fill="{c}"/>')
# barra de comandos
cy=wy+44
o.append(f'<rect x="{wx}" y="{cy}" width="{ww}" height="44" fill="#FFFFFF"/><line x1="{wx}" y1="{cy+44}" x2="{wx+ww}" y2="{cy+44}" stroke="{B}"/>')
cmds=["＋ Novo","Recortar","Copiar","Colar","Propriedades","Mapear unidade de rede","Desconectar","⋯"]
x=wx+16
for c in cmds:
    w=len(c)*7.4+18
    hl = c.startswith("Mapear")
    if hl: o.append(f'<rect x="{x-6}" y="{cy+9}" width="{w}" height="26" rx="6" fill="#D6E8F7"/>')
    o.append(f'<text x="{x+3}" y="{cy+27}" font-size="13" fill="{AC if hl else T}">{c}</text>')
    x+=w+6
# endereço
ay=cy+52
o.append(f'<text x="{wx+16}" y="{ay+21}" font-size="16" fill="{T2}">←  →  ↑</text>')
o.append(f'<rect x="{wx+100}" y="{ay+4}" width="600" height="30" rx="8" fill="#F4F7FA" stroke="{B}"/>')
o.append(f'<text x="{wx+114}" y="{ay+24}" font-size="13" fill="{T}">Este Computador</text>')
o.append(f'<rect x="{wx+712}" y="{ay+4}" width="252" height="30" rx="8" fill="#F4F7FA" stroke="{B}"/>')
o.append(f'<text x="{wx+726}" y="{ay+24}" font-size="13" fill="{T2}">⌕  Pesquisar em Este Computador</text>')
# painel nav
ny=ay+44
o.append(f'<rect x="{wx}" y="{ny}" width="210" height="{wy+wh-ny-12}" fill="#F4F7FA"/>')
nav=[("Acesso rápido",None,0),("Área de Trabalho","pasta",1),("Downloads","pasta",1),("Documentos","pasta",1),("Imagens","pasta",1),
     ("Este Computador","este-computador",0),("Windows (C:)",None,-1),("Ararinha (C:)","unidade-sistema",1),("Arquivos (D:)","unidade-disco",1),
     ("Pendrive (E:)","unidade-disco",1),("filmes (Z:)","unidade-rede",1),("Rede","unidade-rede",0),("Lixeira","lixeira",0)]
y=ny+16
for name,ic,lvl in nav:
    if lvl==-1: continue
    if name=="Este Computador":
        o.append(f'<rect x="{wx+8}" y="{y-4}" width="194" height="28" rx="6" fill="#D6E8F7"/>')
    xx=wx+16+(18 if lvl==1 else 0)
    if ic: o.append(icon(ic,xx,y,20))
    o.append(f'<text x="{xx+(28 if ic else 0)}" y="{y+15}" font-size="13" fill="{T}" font-weight="{600 if lvl==0 else 400}">{name}</text>')
    y+=30
# conteúdo
cx0=wx+232; y=ny+28
o.append(f'<text x="{cx0}" y="{y}" font-size="14" font-weight="600" fill="{T}">Dispositivos e unidades</text>')
drives=[("Ararinha (C:)","unidade-sistema",0.42,"118 GB livres de 476 GB"),
        ("Arquivos (D:)","unidade-disco",0.71,"271 GB livres de 931 GB"),
        ("Pendrive (E:)","unidade-disco",0.18,"48,9 GB livres de 59,6 GB")]
y+=16
for i,(n,ic,u,t) in enumerate(drives):
    col=i%2; row=i//2
    x=cx0+col*360; yy=y+row*92
    if i==0: o.append(f'<rect x="{x-8}" y="{yy-6}" width="340" height="80" rx="8" fill="#EAF0F5"/>')
    o.append(icon(ic,x,yy,64))
    o.append(f'<text x="{x+76}" y="{yy+20}" font-size="14" fill="{T}">{n}</text>')
    o.append(f'<rect x="{x+76}" y="{yy+30}" width="240" height="12" rx="3" fill="#E4ECF3" stroke="{B}"/>')
    o.append(f'<rect x="{x+76}" y="{yy+30}" width="{240*u:.0f}" height="12" rx="3" fill="{AC}"/>')
    o.append(f'<text x="{x+76}" y="{yy+60}" font-size="12" fill="{T2}">{t}</text>')
y+=2*92+20
o.append(f'<text x="{cx0}" y="{y}" font-size="14" font-weight="600" fill="{T}">Locais de rede</text>')
y+=16
o.append(icon("unidade-rede",cx0,y,64))
o.append(f'<text x="{cx0+76}" y="{y+20}" font-size="14" fill="{T}">filmes (\\\\servidor) (Z:)</text>')
o.append(f'<rect x="{cx0+76}" y="{y+30}" width="240" height="12" rx="3" fill="#E4ECF3" stroke="{B}"/>')
o.append(f'<rect x="{cx0+76}" y="{y+30}" width="156" height="12" rx="3" fill="{AC}"/>')
o.append(f'<text x="{cx0+76}" y="{y+60}" font-size="12" fill="{T2}">1,2 TB livres de 3,6 TB</text>')
# diálogo mapear
dx,dy=cx0+380,y-6
o.append(f'<g filter="url(#sh)"><rect x="{dx}" y="{dy}" width="330" height="168" rx="10" fill="#FFFFFF" stroke="{B}"/></g>')
o.append(f'<text x="{dx+16}" y="{dy+26}" font-size="14" font-weight="600" fill="{T}">Mapear unidade de rede</text>')
o.append(f'<text x="{dx+16}" y="{dy+52}" font-size="12" fill="{T2}">Unidade</text>')
o.append(f'<rect x="{dx+80}" y="{dy+38}" width="60" height="22" rx="5" fill="#F4F7FA" stroke="{B}"/><text x="{dx+90}" y="{dy+54}" font-size="12" fill="{T}">Y: ▾</text>')
o.append(f'<text x="{dx+16}" y="{dy+82}" font-size="12" fill="{T2}">Pasta</text>')
o.append(f'<rect x="{dx+80}" y="{dy+68}" width="234" height="22" rx="5" fill="#F4F7FA" stroke="{AC}"/><text x="{dx+88}" y="{dy+84}" font-size="12" fill="{T}">\\\\192.168.0.10\\backup</text>')
o.append(f'<rect x="{dx+16}" y="{dy+100}" width="12" height="12" rx="3" fill="{AC}"/><text x="{dx+36}" y="{dy+111}" font-size="12" fill="{T}">Reconectar ao entrar</text>')
o.append(f'<rect x="{dx+150}" y="{dy+128}" width="78" height="28" rx="6" fill="#EAF0F5"/><text x="{dx+163}" y="{dy+147}" font-size="12" fill="{T}">Cancelar</text>')
o.append(f'<rect x="{dx+236}" y="{dy+128}" width="78" height="28" rx="6" fill="{AC}"/><text x="{dx+250}" y="{dy+147}" font-size="12" fill="#FFFFFF">Concluir</text>')
# status
o.append(f'<text x="{cx0}" y="{wy+wh-20}" font-size="12" fill="{T2}">4 itens</text>')
# barra de tarefas
o.append('<rect x="0" y="752" width="1280" height="48" fill="#FFFFFF" opacity="0.88"/>')
o.append(logo(12,756,40))
for i,ic in enumerate(["este-computador","pasta","lixeira"]):
    o.append(icon(ic,72+i*48,760,32))
o.append('<rect x="118" y="794" width="20" height="3" rx="1.5" fill="#2B7BC4"/>')
o.append(f'<rect x="230" y="760" width="260" height="32" rx="16" fill="#F4F7FA" stroke="{B}"/><text x="246" y="781" font-size="13" fill="{T2}">⌕  Pesquisar apps, arquivos e ajustes</text>')
o.append(f'<text x="1110" y="772" font-size="12" fill="{T}">PT    Wi-Fi</text>')
o.append(f'<text x="1196" y="771" font-size="12" fill="{T}">11:32</text><text x="1186" y="788" font-size="11" fill="{T2}">09/11/2026</text>')
o.append('</svg>')
open(f"{base}/arte/mockups/este-computador.svg","w").write("\n".join(o))
