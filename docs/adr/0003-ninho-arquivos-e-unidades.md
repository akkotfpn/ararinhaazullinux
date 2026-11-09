# ADR 0003: Ninho — arquivos inteligentes com unidades estilo Windows

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: o gerenciamento de arquivos tem que ser inteligente, **mas o
usuário final vê algo parecido com o Windows**: `C:`, `D:`, *Este Computador*,
*Mapear unidade de rede*. O sistema também precisa da sua própria base de
arquivos, e os apps Windows (via Bando) devem enxergar os mesmos caminhos.

## Opções consideradas
1. **Mostrar a árvore Linux** (`/home`, `/mnt`, `/media`) — familiar só para quem
   já usa Linux. Contraria o público geral.
2. **Criar um sistema de arquivos em disco novo** — enorme risco de perda de dados
   e anos de maturação. Giampaolo levou anos para o BeFS.
3. **Camada de nomes estilo Windows sobre sistemas de arquivos maduros, com
   índice inteligente separado** — familiar, seguro e evolutivo.

## Decisão
Opção 3. O Ninho tem **duas faces**:

### Face visível: igual ao que o usuário já conhece
- **Este Computador** com as unidades e barra de espaço livre.
- `C:` = disco do Ararinha. Mostra `Usuários`, `Programas` e `Ararinha`
  (sistema, somente leitura e oculto por padrão).
- `D:`, `E:`… = outros discos e partições internas (inclusive o NTFS de um
  Windows em dual boot), cada um com letra **fixa**, lembrada pelo UUID do volume,
  como o Windows faz.
- Pendrives e HDs externos recebem a próxima letra livre, lembrada por UUID.
- **Mapear unidade de rede**: o usuário digita `\\servidor\pasta`, `smb://`,
  `sftp://`, `nfs://` ou um endereço WebDAV, escolhe a letra e marca
  "Reconectar ao entrar". Senha guardada no chaveiro do sistema.
  **Desconectar unidade de rede** faz o inverso.
- **Rede**: lista computadores e compartilhamentos descobertos na rede local
  (WS-Discovery e mDNS).
- Nomes localizados como no Windows: em disco a pasta chama `Users/davi/Documents`;
  na tela aparece `Usuários › davi › Documentos` (ou `Users › davi › Documents`
  em inglês).
- Barra de endereço aceita `C:\Users\davi`, `C:/Usuários/davi` e `/home/davi`.
- Atalhos do Windows: Win+E abre o Ninho, F2 renomeia, Alt+Enter abre
  Propriedades, Ctrl+Shift+N cria pasta.
- Propriedades de arquivo e pasta com a aba **Versões anteriores**.

### Face inteligente: por baixo
- **ninhod** (serviço em Rust) acompanha mudanças em tempo real com `fanotify` e
  mantém um índice:
  - metadados e tags em SQLite (modo WAL);
  - texto completo em Tantivy;
  - atributos também gravados como xattrs `user.ninho.*`, para sobreviverem a
    cópias entre discos Btrfs/ext4.
- **Tipo real** detectado pelo conteúdo, não pela extensão.
- **Pastas inteligentes**: consultas salvas que se atualizam sozinhas
  (ex.: "Instaladores baixados", "Fotos de 2026", "PDFs com 'nota fiscal'").
- **Organização automática dos Downloads** por regras editáveis em TOML,
  desligada até o usuário ativar.
- **Duplicatas**: detecção por hash; no Btrfs, deduplicação por reflink sem
  apagar nada.
- **Linha do tempo**: snapshots Btrfs automáticos de `C:\Users` (de hora em hora,
  com retenção decrescente) e aba Versões anteriores.
- **Busca** por nome, conteúdo, tags e, opcionalmente, OCR de imagens e PDFs.
- **Lixeira** seguindo a especificação freedesktop, com restauração no local
  original.
- O índice é **derivado e reconstruível**: apagar o índice nunca apaga arquivos.
- Indexação em baixa prioridade; pausa com bateria fraca ou jogo aberto.
- Tudo local. Nenhum dado sai da máquina.

### Como funciona por dentro
```
Tela do Ninho            Caminho canônico        Onde está de verdade
C:\Users\davi        →   /unidades/C/Users/davi  →  /home/davi  (Btrfs @home)
D:\                  →   /unidades/D            →  partição NTFS (ntfs3), UUID fixo
Z:\ (rede)           →   /unidades/Z            →  //servidor/pasta (cifs, multiusuário)
```
- `/unidades` é montado pela Revoada na sessão do usuário; terminal e apps Linux
  também podem usar `/unidades/D/...`.
- Tabela de letras em `/var/lib/ninho/unidades.toml`, por UUID.
- SMB e NFS usam o cliente do kernel; SFTP e WebDAV usam FUSE.

### Integração com apps Windows (Bando)
- Em cada prefixo Wine, as letras `D:` a `Y:` apontam para as mesmas unidades do
  Ninho. Um `.exe` que abre `D:\Fotos` vê a mesma pasta que o usuário vê.
- O `C:` de cada app Windows é o prefixo isolado dele, mas
  `C:\Users\<usuário>\Documents`, `Desktop`, `Downloads`, `Pictures`, `Music` e
  `Videos` apontam para as pastas reais. Resultado: o caminho
  `C:\Users\davi\Documents\planilha.xlsx` é o mesmo arquivo no Ninho e no Excel.

## Fundamentação
- Norman: mapeamento natural, usar o modelo mental que o usuário já tem.
- Windows Internals: letras de unidade são uma camada de nomes, não o disco.
- Plan 9: recursos remotos como arquivos; namespaces por processo.
- Giampaolo (BeFS): atributos indexados e consultas ao vivo.
- Kleppmann: índice derivado e reconstruível.

## Consequências
- Precisamos escrever o gerenciador de arquivos (app Ninho) e o `ninhod`.
- Apps Linux de terceiros que abrem diálogos de arquivo usam o portal XDG;
  implementamos o nosso portal para que o diálogo também mostre `C:`/`D:`.
- Em disco o sistema continua padrão Linux, então ferramentas de recuperação comuns
  funcionam.
