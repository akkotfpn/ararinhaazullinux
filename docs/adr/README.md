# Decisões de Arquitetura (ADRs)

Cada decisão importante do Ararinha fica registrada aqui: o contexto, as opções,
o que escolhemos e por quê. Assim, daqui a dois anos, ninguém precisa adivinhar.

| Nº | Decisão | Status |
|---|---|---|
| [0001](0001-kernel-linux-sem-distro-base.md) | Kernel Linux, sem distribuição base | Aceita |
| [0002](0002-init-systemd-com-revoada.md) | systemd como init, com a camada Revoada | Aceita |
| [0003](0003-ninho-arquivos-e-unidades.md) | Ninho: arquivos inteligentes com unidades estilo Windows | Aceita |
| [0004](0004-plumagem-shell-proprio.md) | Plumagem: shell gráfico próprio sobre Wayland | Aceita |
| [0005](0005-pena-formato-de-pacote.md) | Pena: formato de pacote e repositórios | Aceita |
| [0006](0006-muda-sistema-imutavel.md) | Muda: sistema imutável com atualizações atômicas | Aceita |
| [0007](0007-arquiteturas-x86_64-e-arm64.md) | x86_64 e ARM64 desde o início | Aceita |
| [0008](0008-licenciamento.md) | Licenciamento | Aceita |
| [0009](0009-idiomas.md) | Português e inglês | Aceita |
| [0010](0010-rasante-jogos.md) | Rasante: jogos e desempenho | Aceita |
| [0011](0011-bando-compatibilidade.md) | Bando: camada universal de apps | Aceita |
| [0012](0012-extras-opcionais.md) | Extras opcionais (backup, Vaultwarden, acesso remoto, controle parental, IA local) | Aceita |

## Modelo
Copie [`modelo.md`](modelo.md) para criar um novo ADR.
