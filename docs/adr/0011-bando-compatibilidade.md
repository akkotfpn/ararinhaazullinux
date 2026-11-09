# ADR 0011: Bando — camada universal de apps

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
O pilar "abre de tudo": o usuário dá dois cliques e o sistema resolve.

## Decisão

### Detecção pelo conteúdo
| Primeiros bytes / estrutura | Tipo | Ambiente |
|---|---|---|
| `MZ` + cabeçalho PE | Windows `.exe`, `.msi` | Wine/Proton (x86) ou FEX/Box64 + Wine (ARM) |
| ZIP com `AndroidManifest.xml` | Android `.apk` | Waydroid |
| ELF | Linux nativo | Direto, com sandbox opcional |
| AppImage (ELF + assinatura AI) | AppImage | Direto, isolado |
| `.flatpakref` / `.flatpak` | Flatpak | Flatpak |
| `.deb` / `.rpm` | Pacote de outra distro | Container isolado (estilo Distrobox) |
| Mach-O, `.dmg`, `.app` | macOS | Darling (experimental) |
| `#!` scripts | Script | Pergunta antes de executar |

- Registro via `binfmt_misc`: até no terminal, `./instalador.exe` funciona.

### Isolamento
- Cada app Windows ganha um **prefixo próprio**; cada app tem só as permissões que
  precisa (pastas, rede, câmera), pedidas em português na primeira vez.
- Letras de unidade iguais às do Ninho (ver ADR 0003).

### Perfis de compatibilidade
- Arquivos TOML por app (versão do Proton, DLLs, variáveis, correções conhecidas),
  num repositório comunitário. O Bando aplica o perfil certo pelo hash do
  instalador.

### Integração
- Ícone no menu e na área de trabalho como app nativo; desinstalação pelo Viveiro.
- Painel **"Por que isso não abriu?"**: explica o erro em português e sugere saída
  (outro Proton, versão Android, versão web, alternativa nativa).

## Fundamentação
- Levine, *Linkers and Loaders*: formato antes da extensão; binfmt_misc.
- Windows Internals: subsistemas sobre um núcleo comum.
- Plan 9 e Kerrisk: namespaces por processo para isolamento.
- Raymond: regra da reparação, falhar alto e explicar.
