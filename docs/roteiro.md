# Roteiro do Ararinha Azul Linux

Onde estamos e o que vem a seguir. Atualizado a cada bloco de trabalho;
o detalhe de cada entrega fica em `docs/fases/` e a parte de segurança em
`docs/seguranca.md`.

Legenda: **[feito]** entregue numa imagem, **[em teste]** na imagem mais
nova e esperando o teste do Davi, **[a fazer]** ainda não começou.

## Onde estamos (11/11/2026)

O sistema instala (com o disco criptografado), entra, roda Chromium e os
apps do Flathub, e tem área de trabalho, menu Iniciar, bandeja,
Explorador, Configurações, Painel de Controle, Central de Apps,
Gerenciador de Tarefas, instalador e tela de bloqueio próprios.

Imagem: ~630 MB (qcow2). Boot até a tela de entrada: ~3,4 s.

## Parte 1 — Correções do teste de 11/11 (tarde)

| # | Item | Estado |
|---|------|--------|
| 1 | Botões de energia (menu Iniciar e instalador) | [em teste] — causa achada: o serviço checava /run/user, escondido pelo ProtectHome; agora pelo grupo seat |
| 2 | Instalação com disco criptografado | [feito] |
| 3 | Gerenciador de Tarefas agrupado por app | [feito] |
| 4 | Chromium: atalho na área de trabalho e botões de minimizar/maximizar | [em teste] — e fixado na barra |
| 5 | Desligar o firewall, com confirmação | [em teste] |
| 6 | Seletor de arquivos: "Abrir" com um clique | [em teste] |
| 7 | Wi-Fi: aviso claro sem adaptador | [em teste] |
| 8 | Tema claro/escuro e cor de destaque no Primeiro Voo | [em teste] |
| 9 | Central: progresso real e Cancelar | [em teste] |
| 10 | Speedtest (Ookla) no Chromium | [feito] — 780 Mbps |
| 11 | Steam abrir | [em teste] — abre; título/maximizar de janelas X11 corrigidos; controles também pelo grupo input |

## Parte 2 — O que um sistema completo tem

### Base que os apps esperam
- [em teste] Chaveiro de senhas (Secret Service): ararinha-chaveiro, arquivo 0600 na pasta da conta (ainda sem cifra própria; o disco criptografado protege)
- [em teste] Pedido de administrador no estilo UAC (ararinha-admin + ararinha-sistema)
- [em teste] "Abrir com…" e app padrão por tipo de arquivo (mimeapps.list)
- [a fazer] Página Apps padrão nas Configurações
- [a fazer] Impressoras (CUPS) e scanner
- [a fazer] Bluetooth (BlueZ)

### Configurações
- [em teste] Tela: monitores, resolução e escala (100–200%, aplicada na hora; apps das Penas nítidos via viewporter)
- [a fazer] Tela: escolher resolução, posição dos monitores, luz noturna
- [em teste] Som: saída/entrada padrão, volume de cada uma e de cada app, mudo, testar
- [em teste] Data e hora: fuso, hora pela internet (NTP), relógio de 24 h
- [em teste] Idioma e teclado: teclado por conta (na hora), idioma do sistema (próximo login)
- [em teste] Outros usuários: criar, administrador/padrão, apagar (pelo ararinha-sistema + systemd-run)
- [a fazer] Conta de convidado
- [em teste] Atualizações: apps do Flathub sozinhos uma vez por dia (dá para desligar), procurar agora
- [a fazer] Atualização do sistema com volta para a versão anterior
- [em teste] Privacidade: histórico do Win+V, arquivos na busca, relatórios de falhas
- [a fazer] Privacidade: câmera, microfone, localização por app
- [a fazer] Acessibilidade
- [em teste] Armazenamento: uso do disco, o que ocupa, esvaziar Lixeira, remover partes de apps sem uso

### Área de trabalho e produtividade
- [em teste] Alt+Tab com alternador na tela (ícones, ordem de uso, inclui minimizadas)
- [a fazer] Miniaturas no Alt+Tab, Visão de Tarefas, várias áreas de trabalho
- [em teste] Encaixe em metades e quartos (Win+setas, arrastar para bordas e cantos)
- [em teste] Menu de layouts ao parar o mouse no maximizar
- [em teste] Barra estilo Windows 11: no meio, um botão por app, fixar/desafixar, prévia das janelas, opção com nomes
- [em teste] Efeitos visuais liga/desliga (Personalização)
- [em teste] Atalhos: Ctrl+A/C/X/V/Z nos campos e no Bloco de Notas; Win+I, Win+M, Win+A/N, Win+R, Ctrl+Shift+Esc
- [em teste] Central de notificações (clique no relógio): histórico, Não perturbe, ações rápidas
- [em teste] Captura com recorte (Win+Shift+S ou Shift+Print)
- [a fazer] Gravação de tela
- [em teste] Histórico da área de transferência (Win+V): texto, cola com um clique
- [a fazer] Emoji (Win+.) — precisa de fonte de emoji colorida
- [em teste] Busca no menu Iniciar: apps, páginas das Configurações, arquivos da pasta pessoal e contas (2+3*4)
- [em teste] Teclas de volume, mudo e brilho com indicador na tela
- [a fazer] Gestos de touchpad
- [em teste] Hibernar (aparece no menu de energia quando há área de troca do tamanho da memória)
- [a fazer] Trocar de usuário

### Apps essenciais
- [em teste] Compactador no Explorador: "Extrair aqui" e "Compactar em .zip"
- [a fazer] PDF, vídeo, câmera
- [em teste] Explorador: painel de visualização (imagens e textos), pendrives e HDs externos (abrir e Ejetar)
- [a fazer] Explorador: abas, miniaturas na lista

### Segurança e confiabilidade
- [em teste] Desligar o firewall com aviso
- [em teste] Tela de bloqueio com espera crescente depois de 3 senhas erradas
- [a fazer] Backup e pontos de restauração
- [a fazer] Modo de recuperação no boot
- [a fazer] Senha do disco em tela gráfica; TPM2
- [a fazer] Kernel reassinado nas atualizações
- [em teste] Relatório de falhas local (apps das Penas, Plumagem e coredump), aviso e página em Privacidade

### Pedidos de 12/11
- [em teste] Chromium travando ao abrir download: portal AppChooser
- [em teste] zram (troca comprimida) para máquinas com pouca RAM
- [em teste] Janelas abrem no meio e cabem na tela (escala 125%+)
- [em teste] Pendrive pergunta: experimentar ou instalar

### Pedidos de 13/11
- [em teste] Instalador: Reiniciar agora (além de Desligar)
- [em teste] Senha sem Enter na entrada e no bloqueio (confere ao parar de digitar)
- [em teste] Campos de texto com cursor (clique, setas, Home/End, Ctrl+setas)
- [em teste] Relógio: botão direito, 12/24 h na hora, Configurações cai em Data e hora
- [em teste] Mixer de volume no quadro do som; botão direito no som e na rede
- [em teste] Lixeira que enche e "Esvaziar a Lixeira" no botão direito
- [em teste] Mover apps fixados na barra (botão direito)
- [em teste] Papel de parede com imagem própria
- [em teste] Win+Z: layouts de encaixe em qualquer app
- [em teste] DNS personalizado na Rede (AdGuard)
- [feito] Alt+Tab e Win+Z (confirmados no teste)
- [a fazer] Arrastar os ícones da barra para reordenar
- [em teste] Instalador grava a entrada de boot UEFI do disco e põe em primeiro
- [em teste] Explorador: colar/cursor em todos os campos, seleção por retângulo
- [em teste] DNS: até 4 servidores (o 1º é o preferencial)
- [em teste] "Definir como papel de parede" no botão direito da imagem
- [a fazer] Papel de parede animado (vídeo/GIF), colar na busca do Iniciar

### Pedidos de 13/11 (segunda rodada)
- [em teste] Chromium instalado depois de instalar no disco (espera a rede, tenta de novo)
- [em teste] Mapear unidade: "conectado com sucesso" e o motivo da falha em português
- [em teste] Área de trabalho livre: mostra qualquer arquivo, Colar, Nova pasta, Novo documento, Copiar/Recortar/Excluir
- [em teste] Área de transferência de arquivos entre Explorador, área de trabalho e apps
- [em teste] Explorador com abas (Ctrl+T, Ctrl+W, Ctrl+Tab, Abrir em nova aba)
- [em teste] Copiar do servidor sem travar o Explorador (progresso na barra de baixo)
- [em teste] Explorador no estilo Windows 11: barra de comandos, Classificar, Visualizar, Detalhes, barra de status, unidades e Rede na lateral
- [em teste] Papel de parede grande não sai mais borrado (reduzido para 1920 px)
- [em teste] DNS próprio vale também no Wi-Fi (a placa deixa de ser rota padrão)
- [feito] README na Fase 2
- [em teste] Arrastar arquivos do Explorador para a área de trabalho e posicionar ícones livremente
- [feito] Itens 1 a 8 desta rodada confirmados no teste

### Fechamento da Fase 2 (13/11)
- [em teste] Bluetooth: BlueZ, áudio Bluetooth no PipeWire, firmware e kernel; Configurações › Bluetooth e dispositivos
- [em teste] Áreas de trabalho virtuais e Visão de Tarefas (Win+Tab, Win+Ctrl+D/setas/F4, botão na barra)
- [em teste] Área de trabalho: ícones em qualquer lugar, soltar em pasta/Lixeira, renomear, Delete/Enter/Ctrl+C/X/V
- [a fazer] Primeira versão publicada nas Releases do GitHub ("Fase 2")
- [a fazer] Impressoras (CUPS; impressoras de rede IPP primeiro)
- [a fazer] Captura de tela para OBS/Discord (portal ScreenCast com PipeWire)
- [a fazer] Atualizações do sistema com volta atrás (Muda, Fase 5; slots usr-A/usr-B)

### Qualidade
- [a fazer] Medir o consumo da Plumagem parada
- [a fazer] Testes automáticos com prints numa VM no CI
