# Fase 2 — "Plumagem": a primeira tela gráfica

> Objetivo: o Ararinha inicia direto numa sessão gráfica própria (a Plumagem,
> [ADR 0004](../adr/0004-plumagem-shell-proprio.md)), com rede, som e o Ninho
> mostrando "Este Computador".

## Progresso

| Etapa | Situação |
|---|---|
| 2.1 Base gráfica sem LLVM | 18 receitas em `receitas/grafico/` (comandos do BLFS): Wayland e protocolos, libdrm, Mesa (softpipe e virgl, EGL/GBM/GLES2), libinput (libevdev, mtdev), xkeyboard-config e libxkbcommon, libseat sobre o systemd-logind, libpng, pixman, FreeType, HarfBuzz, Fontconfig e Cairo. Fragmento `kernel/config/desktop.config` (evdev, drivers de vídeo Intel/AMD/NVIDIA como módulos) |
| 2.2 LLVM e drivers de placas reais | Receitas `cmake` e `llvm` (com Clang) em `receitas/desenvolvimento/`. A fazer: Mesa com radeonsi, iris, nouveau, llvmpipe e Vulkan |
| 2.3 Rust dentro do Ararinha | Receita `rust` 1.98.1: compilado do código-fonte e sem rede; o compilador inicial (stage0 1.97.1) entra como fonte travada pelos hashes oficiais |
| 2.4 Plumagem v0 | A fazer: compositor Smithay com papel de parede e o Mingau |
| 2.5 Som e rede | A fazer: PipeWire, WirePlumber, NetworkManager e Wi-Fi |
| 2.6 Ninho v0 e Primeiro Voo v1 | A fazer |

Referência: *Beyond Linux From Scratch*, versão systemd
([linuxfromscratch.org/blfs](https://www.linuxfromscratch.org/blfs/)).
