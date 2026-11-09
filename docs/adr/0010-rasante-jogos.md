# ADR 0010: Rasante — jogos e desempenho

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: jogos têm que rodar muito bem, otimizados, com monitoramento de
CPU e GPU e opção de boost.

## Decisão

### Base otimizada (sempre ligada)
- Spix Kernel com `PREEMPT_DYNAMIC` em modo `full`, alta frequência de timer e
  suporte a `sched_ext` para escalonadores voltados a jogos.
- Mesa mais recente (Intel/AMD, RADV, Zink, NVK) e driver NVIDIA proprietário como
  extensão instalada automaticamente pelo Olho de Spix.
- Bibliotecas de 32 bits completas (Steam e Wine dependem delas).
- Steam, Proton, DXVK, VKD3D-Proton, Wine e gamescope prontos.
- `zram`, ajuste de memória e `vm.max_map_count` alto (exigido por vários jogos).

### Modo Rasante (o "boost")
Um botão na Central de ações, ou automático ao abrir um jogo:
- CPU em perfil de desempenho (governor/EPP), GPU em perfil de alto desempenho
  quando o driver permite;
- jogo com prioridade maior via cgroups; serviços em segundo plano (indexação do
  Ninho, atualizações da Muda) pausados;
- notificações silenciadas, efeitos do compositor reduzidos, VRR ligado;
- volta tudo ao normal quando o jogo fecha.
- Em notebook na bateria, avisa que o Rasante aumenta o consumo.

### Monitoramento
- **Overlay no jogo**: FPS, tempo de quadro, uso e temperatura de CPU e GPU,
  VRAM, RAM. Primeira versão sobre MangoHud; versão própria com o visual da
  Plumagem depois.
- **App Rasante**: gráficos em tempo real e histórico por jogo (CPU, GPU, clocks,
  temperaturas, ventoinhas quando o hardware expõe), no método USE de Brendan Gregg.
- Coleta com eBPF e sensores (`hwmon`), custo quase zero.

### Modo console (opcional)
Sessão em tela cheia via gamescope, para TV e controle, estilo SteamOS.

## Limites
- Overclock e controle de ventoinha dependem do fabricante; fazemos o que o
  hardware expuser, sempre com aviso de risco.
- Jogos com anti-cheat de kernel não rodam (ver [limites.md](../limites.md)).

## Fundamentação
- Silberschatz: escalonamento prioriza latência no desktop.
- Gregg: medir com o método USE e eBPF.
- SteamOS: imutável + Proton + modo console.
