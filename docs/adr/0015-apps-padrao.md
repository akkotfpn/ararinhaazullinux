# ADR 0015: Apps padrão do Ararinha

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
O sistema precisa vir com os apps do dia a dia, com a mesma identidade da
Plumagem, em vez de um conjunto de programas de projetos diferentes, cada um com
uma aparência.

## Decisão
Os apps básicos são **feitos do zero pelo projeto**: interface, funcionamento e
integração com o sistema. Usam o mesmo toolkit e os mesmos *design tokens* da
Plumagem (ADR 0004), falam pt-BR e inglês (ADR 0009) e seguem os temas.

"Do zero" vale para o app. Bibliotecas consagradas continuam por baixo onde
reescrevê-las não traria benefício: FFmpeg para decodificar áudio e vídeo,
PipeWire para áudio, libarchive para arquivos compactados, Poppler ou PDFium
para PDF.

Na tela os nomes são simples e descritivos ("Calculadora", "Bloco de Notas"),
para quem nunca usou o sistema achar tudo na hora. Os componentes do sistema
mantêm os nomes temáticos (Ninho, Caatinga, Viveiro, Rasante…).

| App | O que faz | Base técnica | Fase |
|---|---|---|---|
| Terminal | Terminal com abas e temas | toolkit da Plumagem | 2 |
| Bloco de Notas | Editor de texto com abas, realce de código e salvamento automático | toolkit da Plumagem | 2 |
| Calculadora | Padrão, científica e conversor de unidades | toolkit da Plumagem | 2 |
| Ninho | Gerenciador de arquivos (ADR 0003) | ninhod | 2–3 |
| Caatinga | Configurações | serviços do sistema | 2 |
| Reprodutor de Mídia | Áudio e vídeo, listas, legendas | FFmpeg + PipeWire | 3 |
| Gravador de Voz | Gravar, cortar e exportar áudio | PipeWire + FFmpeg | 3 |
| Visualizador de Imagens | Ver, girar, cortar, apresentação | bibliotecas de imagem | 3 |
| Captura de Tela | Imagem e gravação de tela | portal de captura do Wayland + FFmpeg | 3 |
| Leitor de PDF | Ler, buscar, anotar | Poppler ou PDFium | 3 |
| Compactador | zip, rar, 7z, tar, integrado ao Ninho | libarchive | 3 |
| Relógio | Alarme, cronômetro, timer, fusos | toolkit da Plumagem | 3 |
| Viveiro | Loja de apps (ADR 0005) | Pena + Flatpak + Bando | 4 |
| Rasante | Monitor de desempenho e modo jogo (ADR 0010) | eBPF + hwmon | 4 |

## Fora do escopo (não fazer do zero)
- **Navegador:** um motor de navegador é um projeto do tamanho de um sistema
  operacional. Firefox ou Chromium, instaláveis pelo Viveiro.
- **Pacote Office:** LibreOffice ou OnlyOffice, pelo Viveiro.

## Fundamentação
- SerenityOS: identidade vem de apps próprios com o mesmo gosto visual.
- Norman/Krug: nomes óbvios, nada para o usuário adivinhar.
- Raymond, regra da economia: não reescrever o que já é excelente (codecs).
- Brooks: cortar o que nunca terminaria (navegador próprio).
