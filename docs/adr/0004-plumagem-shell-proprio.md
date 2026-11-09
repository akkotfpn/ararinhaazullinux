# ADR 0004: Plumagem — shell gráfico próprio sobre Wayland

- **Status:** Aceita (escolha do toolkit de interface: em prova de conceito)
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: shell próprio, interface bonita construída do zero, ícones em
torno da ararinha-azul, e todo o design feito pelo projeto. Precisa rodar
aplicativos de qualquer origem (Wayland, X11, Wine, Android).

## Opções consideradas
1. **Tema sobre KDE Plasma ou GNOME** — rápido, mas não é nosso.
2. **Compositor com wlroots (C)** — base madura, mas em C.
3. **Compositor com Smithay (Rust)** — biblioteca usada por compositores reais
   (COSMIC, niri), segura e na nossa linguagem principal.
4. **Compositor escrito do zero sem biblioteca** — meses de trabalho só para
   reimplementar DRM/KMS, libinput e protocolos.

## Decisão
**Opção 3.** A Plumagem é um compositor Wayland próprio em Rust, usando Smithay.

### Peças da Plumagem
| Peça | Função |
|---|---|
| `plumagem-comp` | Compositor: janelas, animações, multimonitor, HDR (futuro), VRR |
| Barra de tarefas | Inferior por padrão; botão Ararinha, apps fixados, bandeja, relógio |
| Menu Ararinha | Apps, busca universal ("mapear rede", "wifi", nome de arquivo) |
| Central de ações | Wi-Fi, Bluetooth, volume, brilho, modo Rasante, não perturbe |
| Notificações (Grito) | Pop-ups e histórico |
| Tela de bloqueio e login | Sessão com a identidade da ave |
| Encaixe de janelas | Arrastar para o canto encaixa (como Win+setas) |

- Apps X11 rodam via XWayland; apps Windows via Wine (driver Wayland quando
  estável, XWayland até lá); Android via Waydroid em janela.
- Protocolos padrão (layer-shell, xdg-decoration, fractional-scale, screencopy via
  portal) para compatibilidade com apps de terceiros.
- **Design tokens**: cores, raios, espaçamentos e sombras definidos em um único
  arquivo por tema (`arte/temas/*.toml`). Shell, apps próprios, cursores e ícones
  leem dali. Trocar tema troca tudo.
- Layout familiar para o público geral (barra embaixo, menu à esquerda), com
  opção de barra no topo e dock.

### Toolkit de interface (prova de conceito na Fase 2)
Candidatos: **Slint** (linguagem declarativa ótima para designers, licença
compatível com GPL) e **iced** (Rust puro, usado pelo COSMIC, com layer-shell).
Critérios: suporte a layer-shell, desempenho em hardware fraco, acessibilidade
(leitores de tela via AccessKit), facilidade de tematizar, entrada de texto com
acentos (IME). O vencedor vira a base de todos os apps do Ararinha.

## Fundamentação
- DeVault, *The Wayland Protocol*: o compositor é o servidor de exibição.
- SerenityOS: identidade nasce de gosto consistente aplicado em tudo.
- Raskin: atalhos e hábitos conhecidos (Win+E, Alt+Tab, Win+D).
- Raymond: conhecimento em dados (tokens de tema em TOML).

## Consequências
- É o componente mais trabalhoso do projeto. Na Fase 1 o sistema boota em
  terminal; na Fase 2 entra a primeira versão da Plumagem.
- Acessibilidade precisa ser planejada desde o início, não depois.
