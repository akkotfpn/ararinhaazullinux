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
- [em teste] Chaveiro de senhas (Secret Service): ararinha-chaveiro, arquivo 0600 na pasta da conta (ainda sem cifra própria; o disco criptografado protege)
- [em teste] Pedido de administrador no estilo UAC (ararinha-admin + ararinha-sistema)
- [em teste] "Abrir com…" e app padrão por tipo de arquivo (mimeapps.list)
- [a fazer] Página Apps padrão nas Configurações
- [a fazer] Impressoras (CUPS) e scanner
- [a fazer] Bluetooth (BlueZ)

### Configurações
- [em teste] Tela: monitores, resolução e escala (100–200%, aplicada na hora)
- [a fazer] Tela: escolher resolução, posição dos monitores, luz noturna
- [em teste] Som: saída/entrada padrão, volume de cada uma e de cada app, mudo, testar
- [em teste] Data e hora: fuso, hora pela internet (NTP), relógio de 24 h
- [em teste] Idioma e teclado: teclado por conta (na hora), idioma do sistema (próximo login)
- [em teste] Outros usuários: criar, administrador/padrão, apagar (pelo ararinha-sistema + systemd-run)
- [a fazer] Conta de convidado
- [a fazer] Atualizações (sistema e Flatpak) com histórico e volta
- [a fazer] Privacidade: câmera, microfone, localização por app
- [a fazer] Acessibilidade
- [em teste] Armazenamento: uso do disco, o que ocupa, esvaziar Lixeira, remover partes de apps sem uso

### Área de trabalho e produtividade
- [em teste] Alt+Tab com alternador na tela (ícones, ordem de uso, inclui minimizadas)
- [a fazer] Miniaturas no Alt+Tab, Visão de Tarefas, várias áreas de trabalho
- [em teste] Encaixe em metades e quartos (Win+setas, arrastar para bordas e cantos)
- [a fazer] Menu de layouts ao passar o mouse no maximizar
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
- [a fazer] Relatório de falhas local

### Qualidade
- [a fazer] Medir o consumo da Plumagem parada
- [a fazer] Testes automáticos com prints numa VM no CI
