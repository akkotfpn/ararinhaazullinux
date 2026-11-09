# ADR 0006: Muda — sistema imutável com atualizações atômicas

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: atualizações do jeito **mais seguro**. Para o público geral, o
sistema não pode quebrar por uma atualização interrompida, nem por um app mal
comportado.

## Opções consideradas
1. **Tradicional (pacotes atualizados no lugar)** — flexível, mas pode quebrar.
2. **OSTree** (Fedora Silverblue) — maduro, mas grande e com modelo próprio.
3. **Snapshots Btrfs** (openSUSE MicroOS) — simples, porém o sistema continua
   gravável e a verificação de integridade é limitada.
4. **Imagem A/B somente leitura, verificada criptograficamente** (modelo de
   ChromeOS, SteamOS e GNOME OS, com `systemd-sysupdate`).

## Decisão
**Opção 4**, com dados em Btrfs.

```
Disco
├── ESP (FAT32)       systemd-boot + kernels unificados (UKI) assinados, A e B
├── usr-A  (EROFS)    sistema versão N      ─┐ protegidos por dm-verity
├── usr-B  (EROFS)    sistema versão N+1    ─┘ (hash raiz dentro do UKI assinado)
└── dados  (Btrfs, LUKS opcional)
    ├── @etc   configurações locais (sobre os padrões de fábrica)
    ├── @var   estado do sistema, Flatpaks, prefixos do Bando
    └── @home  usuários (C:\Users), com snapshots do Ninho
```

- **Atualizar**: a Muda baixa a nova imagem (com delta), grava no slot inativo,
  verifica a assinatura e marca para o próximo boot. O sistema atual nunca é tocado.
- **Rollback automático**: contagem de boot do systemd-boot. Se a nova versão não
  chegar até a área de trabalho 3 vezes, volta sozinha para a anterior.
- **Rollback manual**: escolher a versão anterior no menu de boot ou na Caatinga.
- **Cadeia verificada**: Secure Boot → UKI assinado → dm-verity do `/usr`.
  Um `/usr` adulterado simplesmente não monta.
- **Restaurar padrões de fábrica**: apagar `@etc` e `@var` volta tudo ao original,
  preservando `@home` se o usuário quiser.
- Extensões (`systemd-sysext`) são reaplicadas a cada versão; drivers NVIDIA são
  reconstruídos para o kernel novo antes da troca.
- **Modo portátil** usa a mesma imagem, com a partição de dados no pendrive.

## Fundamentação
- OSTEP: crash consistency, nunca sobrescrever no lugar.
- Anderson: defesa em profundidade e cadeia de confiança.
- FreeBSD/SteamOS: base coesa e versionada.

## Consequências
- Usuário não instala programas "no sistema"; usa Flatpak, Bando ou extensões.
  Precisamos tornar isso invisível pela loja Viveiro.
- Partições A/B ocupam espaço extra (alvo: até ~4 GB cada).
- A imagem precisa ser pequena e reprodutível, o que reforça a Ninhada.
