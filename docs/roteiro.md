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
| 1 | Botões de energia (menu Iniciar e instalador) | [em teste] — faltava `CAP_SYS_BOOT` no ararinha-sistema |
| 2 | Instalação com disco criptografado | [feito] |
| 3 | Gerenciador de Tarefas agrupado por app | [feito] |
| 4 | Chromium: atalho na área de trabalho e botões de minimizar/maximizar | [em teste] |
| 5 | Desligar o firewall, com confirmação | [em teste] |
| 6 | Seletor de arquivos: "Abrir" com um clique | [em teste] |
| 7 | Wi-Fi: aviso claro sem adaptador | [em teste] |
| 8 | Tema claro/escuro e cor de destaque no Primeiro Voo | [em teste] |
| 9 | Central: progresso real e Cancelar | [em teste] |
| 10 | Speedtest (Ookla) no Chromium | [a fazer] — investigar |
| 11 | Steam abrir | [em teste] — GLX no Mesa/Xwayland, steam-devices, controles |

## Parte 2 — O que um sistema completo tem

### Base que os apps esperam
- [a fazer] Chaveiro de senhas (Secret Service), destravado no login
- [a fazer] Pedido de administrador no estilo UAC (ararinha-sistema)
- [a fazer] Apps padrão e "Abrir com…"
- [a fazer] Impressoras (CUPS) e scanner
- [a fazer] Bluetooth (BlueZ)

### Configurações
- [a fazer] Tela: resolução, escala, monitores, luz noturna
- [a fazer] Som: saída/entrada, volume por app, teste de microfone
- [a fazer] Data e hora: fuso, NTP, formato
- [a fazer] Idioma e teclado
- [a fazer] Contas: criar usuário, administrador/padrão, convidado
- [a fazer] Atualizações (sistema e Flatpak) com histórico e volta
- [a fazer] Privacidade: câmera, microfone, localização por app
- [a fazer] Acessibilidade
- [a fazer] Armazenamento

### Área de trabalho e produtividade
- [a fazer] Alt+Tab com miniaturas, Visão de Tarefas, várias áreas de trabalho
- [a fazer] Layouts de encaixe
- [a fazer] Central de notificações e ações rápidas
- [a fazer] Captura com recorte (Win+Shift+S) e gravação de tela
- [a fazer] Histórico da área de transferência (Win+V) e emoji (Win+.)
- [a fazer] Busca de arquivos, configurações e cálculos no menu Iniciar
- [a fazer] Teclas de mídia com indicador na tela; gestos de touchpad
- [a fazer] Trocar de usuário; hibernar

### Apps essenciais
- [a fazer] PDF, vídeo, compactador, câmera
- [a fazer] Explorador: abas, miniaturas, visualização, USB com Ejetar

### Segurança e confiabilidade
- [em teste] Desligar o firewall com aviso
- [a fazer] Backup e pontos de restauração
- [a fazer] Modo de recuperação no boot
- [a fazer] Senha do disco em tela gráfica; TPM2
- [a fazer] Kernel reassinado nas atualizações
- [a fazer] Relatório de falhas local

### Qualidade
- [a fazer] Medir o consumo da Plumagem parada
- [a fazer] Testes automáticos com prints numa VM no CI
