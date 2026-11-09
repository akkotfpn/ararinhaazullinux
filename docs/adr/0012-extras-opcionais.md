# ADR 0012: Extras opcionais

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Davi quer que backup automático, integração com Vaultwarden, acesso remoto,
controle parental e assistente de IA local sejam possíveis, mas **opcionais**.

## Decisão
Todos desligados por padrão. No primeiro boot, a tela **"Monte o seu Ararinha"**
pergunta quais ativar; podem ser ligados depois na Caatinga. Nenhum ocupa espaço
nem roda em segundo plano se estiver desligado (são extensões ou Flatpaks).

| Extra | Como |
|---|---|
| **Backup automático** | Para servidor (SMB, SFTP, NAS) ou disco externo. Usa snapshots Btrfs para copiar um estado consistente, com criptografia e versões. Restaurar arquivo a arquivo pelo Ninho. |
| **Cofre de senhas** | Cliente compatível com Bitwarden/Vaultwarden (servidor próprio do usuário), integrado ao chaveiro e ao preenchimento no navegador. |
| **Acesso remoto** | Servidor RDP próprio da Plumagem (abre no Conexão de Área de Trabalho Remota do Windows) e opção de acesso via Tailscale. Sempre com aviso visível na tela quando alguém está conectado. |
| **Controle parental** | Contas infantis com limite de tempo, filtro de sites (DNS), aprovação de instalação de apps e relatório para o responsável. |
| **Assistente de IA local** | Modelo rodando 100% na máquina (motor compatível com llama.cpp, usa a GPU se houver). Ajuda a achar arquivos ("o PDF da nota fiscal de outubro"), explica erros e mexe em configurações com confirmação. Nada vai para a nuvem. |

## Fundamentação
- Oberon/Wirth: cortar o supérfluo do caminho padrão.
- Anderson: o que não está ligado não pode ser atacado.
- Norman: o usuário escolhe o que quer ver.
