<p align="center">
  <img src="arte/logo/ararinha-logo.svg" width="160" alt="Logo do Ararinha Azul Linux">
</p>

<h1 align="center">Ararinha Azul Linux</h1>

<p align="center">
  <b>Um sistema operacional brasileiro, construído do zero, que abre de tudo.</b><br>
  <i>A Brazilian operating system, built from scratch, that opens anything.</i>
</p>

<p align="center">
  Codinome <code>Spix</code> · Fase atual: <b>2 — Plumagem</b> (com os apps da Fase 3 adiantados) · x86_64
</p>

---

## Português

O **Ararinha Azul Linux** é um sistema operacional independente: não é baseado em
Ubuntu, Debian, Arch, Fedora nem em nenhuma outra distribuição. Usamos o kernel
Linux (é onde estão os drivers de praticamente todo o hardware do mundo), e todo o
resto é nosso: o sistema de build, o gerenciador de pacotes, o instalador, a
interface gráfica, o gerenciador de arquivos e a identidade visual.

O nome homenageia a ararinha-azul (*Cyanopsitta spixii*), ave da Caatinga baiana
que desapareceu da natureza e foi reintroduzida em Curaçá (BA) em 2022.
O sistema carrega essa ideia: **renascer, voar livre e ser genuinamente brasileiro.**

### Onde estamos (novembro de 2026)

A **Fase 1 — Primeiro Pio** (sistema base construído do zero pela Ninhada,
com o Spix Kernel e o systemd) está concluída. Estamos na **Fase 2 —
Plumagem**: o Ararinha já é um sistema de desktop que dá para usar no dia a
dia, testado em máquina virtual (Proxmox) e em pendrive.

O que já funciona numa imagem de ~630 MB que liga em ~3,5 s:

- **Instalação de verdade**: roda direto do pendrive ou instala no disco
  (com criptografia), pergunta "Experimentar ou instalar?" ao ligar e
  configura a inicialização pelo disco sozinha.
- **Plumagem**, a interface gráfica feita do zero: barra de tarefas no estilo
  Windows 11 com ícones dos apps, prévias ao passar o mouse e apps fixados;
  menu Iniciar com busca; área de trabalho com atalhos, pastas e arquivos
  (copiar, colar, nova pasta); encaixe de janelas (layouts, Win+Z, arrastar
  para as bordas); Alt+Tab; Win+V (histórico da área de transferência);
  central de notificações com "Não perturbe"; tela de bloqueio; efeitos de
  vidro que podem ser desligados.
- **Explorador de Arquivos** com abas, barra de comandos, painel de
  detalhes, seleção com o mouse, pendrives com Ejetar, **Mapear unidade de
  rede** (SMB) e "Abrir com…".
- **Configurações** completas: tela e escala, rede (Wi-Fi, DNS próprio),
  som, energia, idioma, privacidade, contas, atualizações, armazenamento e
  personalização (papel de parede de qualquer imagem).
- **Apps**: Central de Apps (Flathub), Chromium instalado no primeiro
  acesso, Gerenciador de Tarefas, Bloco de Notas, Visualizador de Imagens,
  Músicas, Steam e jogos com controle.
- **Segurança**: pedido de senha de administrador (como o UAC), serviço do
  sistema sem setuid, apps Flatpak isolados, relatórios de falha só no
  computador.

Próximos passos: captura de tela para OBS/Discord, áreas de trabalho
virtuais, impressoras e Bluetooth, atualizações do sistema com rollback e
modo de recuperação. O detalhe está no [roteiro](docs/roteiro.md) e no
[plano da Fase 2](docs/fases/fase-2.md).

### Os pilares

| Pilar | O que significa |
|---|---|
| **Abre de tudo** | Dois cliques em `.exe`, `.apk`, `.AppImage`, `.deb`, `.rpm`, `.flatpak` e o sistema escolhe sozinho como executar, isolado e seguro. Apps de Mac em modo experimental. |
| **Arquivos inteligentes, cara de Windows** | Por baixo, um índice com tags, busca instantânea, snapshots e deduplicação. Na frente, o que todo mundo já conhece: **Este Computador**, unidades `C:` e `D:`, **Mapear unidade de rede**. |
| **Hardware sem dor** | Detecta o notebook ou a VM (Acer, Lenovo, Dell, HP, Asus, Positivo, Proxmox, VMware, VirtualBox…) e instala drivers e firmwares sozinho. |
| **Jogos de verdade** | Steam, Proton, monitoramento de CPU e GPU, modo de desempenho com um clique. |
| **Seguro por padrão** | Sistema imutável com atualizações atômicas e rollback, firewall, VPN, criptografia. |
| **Instalável e portátil** | Instale no disco ou leve no pendrive, com persistência criptografada. |
| **Identidade própria** | Interface gráfica desenhada do zero, ícones, sons e temas em torno da ararinha. |

### Proposta visual original

![Este Computador no Ararinha: unidades C:, D:, E: e unidade de rede Z:](arte/mockups/este-computador.png)

### Componentes

| Nome | O que é |
|---|---|
| **Spix Kernel** | Kernel Linux com configuração e otimizações próprias |
| **Ninhada** | Sistema de build que gera o SO inteiro a partir do código-fonte |
| **Pena** | Gerenciador de pacotes (`pena instalar firefox`) |
| **Muda** | Atualizações atômicas, como a ave trocando as penas |
| **Revoada** | Inicialização e serviços (camada sobre o systemd) |
| **Olho de Spix** | Detecção de hardware e drivers automáticos |
| **Bando** | Camada universal que abre qualquer aplicativo |
| **Ninho** | Base de arquivos inteligente e o gerenciador de arquivos |
| **Plumagem** | O shell gráfico: área de trabalho, barra, menu, janelas e temas |
| **Caatinga** | Central de configurações |
| **Primeiro Voo** | Instalador gráfico e criador de pendrive portátil |
| **Asa Guardiã** | Firewall |
| **Rota Migratória** | VPN (WireGuard, OpenVPN, Tailscale) |
| **Canto** | Wi-Fi, rede cabeada, Bluetooth |
| **Rasante** | Modo jogo: desempenho, overlay e monitoramento |
| **Viveiro** | Loja de aplicativos |
| **Grito** | Sons e notificações |

### Documentação

- [Fundamentos: o que aprendemos com os grandes livros](docs/fundamentos.md)
- [Decisões de arquitetura (ADRs)](docs/adr/)
- [Plano e progresso da Fase 1](docs/fases/fase-1.md)
- [Plano e progresso da Fase 2](docs/fases/fase-2.md)
- [Segurança](docs/seguranca.md)
- [Ninhada, o sistema de build](ninhada/README.md)
- [Guia da máquina de build no Proxmox](docs/guias/proxmox.md)
- [Roteiro geral](docs/roteiro.md)
- [Identidade visual](arte/README.md)
- [Limites técnicos, sem promessa falsa](docs/limites.md)

---

## English

**Ararinha Azul Linux** is an independent operating system. It is not based on any
existing distribution. We use the Linux kernel for hardware support; everything
else is ours: build system, package manager, installer, graphical shell, file
manager and visual identity.

Goals: run Windows (`.exe`) and Android (`.apk`) apps transparently, a smart
file layer presented in a familiar Windows-like way (drive letters, *Map network
drive*), automatic drivers, first-class gaming, an immutable and secure base,
installable or portable from a USB drive, in Portuguese and English.

**Current status (November 2026):** phase 1 (base system built from source)
is done; phase 2 (*Plumagem*, our own Wayland desktop) is in daily testing:
installer with disk encryption, Windows 11-style taskbar and file explorer
with tabs, network drives, settings, app center (Flathub), Chromium and
Steam, in a ~630 MB image that boots in about 3.5 s.

See [docs/](docs/) (Portuguese first, English translations are planned).

---

## Licença / License

Código do sistema sob **GPL-3.0-or-later**, bibliotecas para desenvolvedores sob
**Apache-2.0**, arte sob **CC BY-SA 4.0**. Detalhes em [LICENSE.md](LICENSE.md).

Criado por **Davi Souza**. Veja [CREDITOS.md](CREDITOS.md).
