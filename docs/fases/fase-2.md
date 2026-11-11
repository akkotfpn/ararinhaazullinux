# Fase 2 — "Plumagem": a primeira tela gráfica

> Objetivo: o Ararinha inicia direto numa sessão gráfica própria (a Plumagem,
> [ADR 0004](../adr/0004-plumagem-shell-proprio.md)), com rede, som e o Ninho
> mostrando "Este Computador".

## Progresso

| Etapa | Situação |
|---|---|
| 2.1 Base gráfica sem LLVM | 18 receitas em `receitas/grafico/` (comandos do BLFS): Wayland e protocolos, libdrm, Mesa (softpipe e virgl, EGL/GBM/GLES2), libinput (libevdev, mtdev), xkeyboard-config e libxkbcommon, libseat sobre o systemd-logind, libpng, pixman, FreeType, HarfBuzz, Fontconfig e Cairo. Fragmento `kernel/config/desktop.config` (evdev, drivers de vídeo Intel/AMD/NVIDIA como módulos) |
| 2.2 LLVM e drivers de placas reais | Receitas `cmake` e `llvm` (com Clang) em `receitas/desenvolvimento/`. A fazer: Mesa com radeonsi, iris, nouveau, llvmpipe e Vulkan |
| 2.3 Rust dentro do Ararinha | Construído em 10/11/2026. Receita `rust` 1.98.1: compilado do código-fonte e sem rede; o compilador inicial (stage0 1.97.1) entra como fonte travada pelos hashes oficiais |
| 2.4 Plumagem v0 | Compositor em `plumagem/` (Smithay 0.7, baseado no Anvil): backend udev/DRM, papel de parede Spix com o Mingau, teclado ABNT2, Super+Enter abre o Alacritty. Receitas `plumagem`, `alacritty`, `fonte-inter` e `fonte-jetbrains-mono`; crates do Cargo.lock baixadas e travadas pela Ninhada. Primeira construção completa: 10/11/2026, 11h01. Primeira tela gráfica (Mingau no papel de parede, cursor) às 11h53; primeira janela (Alacritty, Ctrl+Alt+T) às 12h06. **Plumagem v1** (10/11, 13h28): barra de tarefas com o Mingau, botões das janelas e relógio; menu iniciar com aplicativos, Sair, Reiniciar e Desligar (também Ctrl+Esc); barra de título com Minimizar/Maximizar/Fechar; maximizar sem cobrir a barra; interface automática no tty1. A fazer: ícones reais, Ninho (login gráfico), arrastar e redimensionar pelas bordas |
| 2.5 Som e rede | Receitas `receitas/multimidia/` (alsa-lib, pcre2, glib, lua, pipewire, wireplumber), iniciadas pela `plumagem-sessao` com D-Bus de sessão; o ALSA sai pelo PipeWire. Bandeja da barra com volume (wpctl) e estado da rede (cabo/Wi-Fi, IP). **Wi-Fi** (11/11): `receitas/rede/` com iwd 3.12 e o firmware de Wi-Fi/rede/vídeo (linux-firmware 20260622, só o necessário, em zstd), drivers Intel/Realtek/Atheros/MediaTek/Broadcom no Spix Kernel; Configurações › Rede e Internet lista as redes e conecta. Sons do sistema (entrar, notificação, desligar) pelo `ararinha-som`. A fazer: Bluetooth |
| 2.6 Ninho v0 e Primeiro Voo v1 | Ninho (`plumagem --ninho`, `ninho.service` no tty1): relógio, cartão de vidro com o Mingau, senha conferida no /etc/shadow; sem contas comuns, cria a primeira conta (grupos seat, video, audio, input). Sessões dos usuários via seatd. **Primeiro Voo gráfico** (idioma, fuso e teclado no próprio Ninho, gravados em /etc/locale.conf, /etc/localtime e /etc/plumagem/teclado); a raiz cresce sozinha no primeiro boot (systemd-repart + x-systemd.growfs) |
| 2.7 Flatpak e Flathub | Construído em 11/11/2026. Receitas em `receitas/flatpak/`: ca-certificates, libxml2, curl, a família GnuPG (libgpg-error, libassuan, libgcrypt, libksba, npth, gnupg, gpgme), libarchive, libseccomp, json-glib, fuse3, libfyaml, libxmlb, appstream, shared-mime-info, gdk-pixbuf, bubblewrap, xdg-dbus-proxy, ostree e flatpak 1.18.4, com o Flathub configurado. A sessão adiciona o Flathub do usuário e põe os apps exportados no menu Iniciar. **Portais** (11/11): xdg-desktop-portal 1.18.4 com o backend próprio `xdg-desktop-portal-ararinha` (Abrir/Salvar pelo Explorador, tema claro/escuro, notificações; também é o servidor org.freedesktop.Notifications da sessão) |
| 2.8 Unidades de rede | `cifs-utils` (com `talloc`) e o ajudante setuid `ararinha-unidade`: Mapear unidade de rede no Explorador (letra, \\servidor\pasta, usuário e senha), reconexão ao entrar e desconexão pelo botão direito. As unidades aparecem em Este Computador > Locais de rede |

Referência: *Beyond Linux From Scratch*, versão systemd
([linuxfromscratch.org/blfs](https://www.linuxfromscratch.org/blfs/)).

## Apps (adiantado da Fase 3)

Em `apps/` (ADR 0015), feitos do zero com o kit **Penas** (janela Wayland
pelo smithay-client-toolkit e o mesmo desenho da Plumagem):

- **Explorador de Arquivos**: Este Computador (pastas do usuário, Disco
  Local C: e unidades D:, E:... com espaço livre), navegação com Voltar,
  Avançar e Subir, barra de endereço clicável, menu do botão direito (Nova
  pasta, Renomear, Excluir para a Lixeira, Copiar, Recortar, Colar) e
  atalhos (F2, Delete, Ctrl+C/X/V, F5, Ctrl+H). Fixado na barra de tarefas
  e nos ícones da área de trabalho (Este Computador, Pasta pessoal,
  Lixeira).
- **Visualizador de Imagens** e **Bloco de Notas**, abertos pelo Explorador.
  **v2** (11/11): ícones e coluna Tipo por extensão, seleção múltipla
  (Ctrl/Shift+clique, Ctrl+A), busca (filtra na hora; Enter procura nas
  subpastas), arrastar para pastas (Ctrl copia), Propriedades (Alt+Enter)
  e Lixeira com Restaurar e Esvaziar.
- **Configurações**: tamanho da barra de tarefas, modo claro/escuro, papel
  de parede (Spix, Noite no Sertão, Velho Chico, Aero Azul, Caatinga em
  Flor), plano de energia (Padrão, Econômico, Desempenho máximo, pelo
  ajudante `ararinha-energia`) e Sobre com créditos. Tudo vale na hora.
- **Central de Apps**: instala, abre, remove e atualiza apps do Flathub.
- **Calculadora** (padrão, com teclado) e **Gerenciador de Tarefas**
  (processos com Finalizar tarefa; gráficos de processador e memória).

Mais apps: **Músicas** (MP3, FLAC, OGG, WAV, M4A pelo Symphonia), **Paciência
dos Pássaros** (Klondike com pássaros nas figuras) e **Instalar o Ararinha**
(do pendrive para o disco, com o ajudante `ararinha-instalar`).

Na Plumagem: **notificações** no canto (`ararinha-notificar "Título" "Texto"`),
**captura de tela** com Print Screen (Imagens › Capturas de Tela) e menus do
**botão direito** na área de trabalho e na barra de tarefas. O tempo até a
tela de entrada aparece em Configurações › Sobre.

Imagem: compiladores, cabeçalhos e bibliotecas estáticas saem da imagem e vão
para o pacote `ararinha-dev-<arq>.tar.zst` (lista em `imagem/pacote-dev.txt`).

Atalhos do Windows na Plumagem: Alt+Tab, Alt+F4, Win+D / Ctrl+Alt+D,
Win+E / Ctrl+Alt+E, Ctrl+Esc (menu), Ctrl+Alt+T (terminal) e Print Screen
(captura de tela).


### Segurança, design e otimização (11/11, tarde)

- **Disco criptografado** no instalador (LUKS2, AES-XTS 512, Argon2id),
  com o kernel `vmlinuz-spix-cripto` que traz uma initramfs mínima feita
  pela Ninhada para pedir a senha no boot.
- **Secure Boot com chave própria**: com o firmware em Setup Mode, o
  instalador gera as chaves do computador, assina o kernel e as cadastra.
  Módulos do kernel assinados; lockdown "integrity" com Secure Boot.
- **Bloqueio automático** (Configurações › Energia; padrão 5 min) e antes
  de suspender; atraso progressivo depois de 3 senhas erradas.
- **ararinha-sistema**: serviço root ativado por socket (SO_PEERCRED),
  no lugar de ajudantes setuid. Já cuida do firewall e da energia.
- **Configurações › Firewall**: rede pública ou de casa e portas liberadas.
- Serviços endurecidos e sem core dumps. CI com clippy, cargo-audit e a
  contagem de `unsafe`.
- **Menu Iniciar**: busca ao digitar, lista com rolagem, teclado, avatar e
  nome da conta, botões de Bloquear, Sair, Reiniciar e Desligar.
- **Penas**: componentes únicos (botão, campo, caixa de marcar, chave,
  aviso, medidor de senha) e medidas do sistema de design.
- **Otimização**: as Penas não acordam mais a cada 100 ms (inotify); a
  barra não cria processos a cada poucos segundos (getifaddrs,
  localtime_r); a Central não fica num laço de redesenho durante as
  instalações; Plumagem sem avisos de compilação.
- Configurações › Sobre mostra a criptografia do disco, o Secure Boot e
  que o sistema não tem telemetria.

### Pedidos do teste de 11/11 (manhã)

- **Gerenciador de Tarefas**: modelo do processador; memória instalada e
  slots em uso/livres (tabela SMBIOS, lida pelo `ararinha-sistema`); nova
  aba **Hardware** (computador, processador, memória por slot, vídeo,
  discos, rede, som e bateria).
- **Central de Apps** com o ícone de cada app (catálogo do Flathub).
- **Ícones Spix** em todos os apps (64 e 128 px).
- **Texto mais nítido** (gama na cobertura dos glifos) e título das janelas
  em branco sobre o vidro.
- **Foto da conta** (Configurações › Conta), no Ninho, no menu e no
  bloqueio.
- **Bandeja do sistema**: `ararinha-bandeja` (StatusNotifierWatcher) e os
  ícones dos apps ao lado de rede e som, com clique e menu.
