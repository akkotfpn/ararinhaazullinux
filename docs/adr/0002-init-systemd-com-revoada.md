# ADR 0002: systemd como init, com a camada Revoada

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: "compatível com tudo que seja possível". O init define que
serviços, apps e ferramentas de terceiros funcionam sem adaptação.

## Opções consideradas
1. **systemd** — padrão de fato. Steam, Flatpak, Waydroid, NetworkManager,
   Tailscale, ferramentas de fabricantes, logind (sessões, energia, suspender) e
   quase todo software de terceiros esperam por ele. Mais pesado e grande.
2. **dinit / s6** — leves e elegantes, mas exigiriam reescrever ou adaptar
   dezenas de integrações (logind via elogind, unidades de serviço etc.).
3. **Init próprio do zero** — identidade, mas quebra compatibilidade, contrário
   ao requisito.

## Decisão
**systemd** como init, gerenciador de serviços e sessão. A **Revoada** é a nossa
camada por cima:
- unidades `.service` padrão do Ararinha, com endurecimento por padrão
  (`ProtectSystem`, `NoNewPrivileges`, filtros de chamadas de sistema);
- ferramenta `revoada` em pt-BR/inglês (`revoada status`, `revoada reiniciar rede`);
- painel "Serviços" na Caatinga;
- boot silencioso com a animação da ave.

Usamos só os módulos do systemd que agregam valor (systemd-boot, systemd-homed é
opcional, systemd-resolved, systemd-oomd, logind). Para Wi-Fi e rede, o padrão é
NetworkManager (ver Canto), não o systemd-networkd, por compatibilidade com VPNs.

## Fundamentação
- Raymond: regra da menor surpresa — software de terceiros deve "só funcionar".
- Tanenbaum/MINIX: serviços isolados e reiniciáveis, que o systemd já oferece.
- Brooks: não gastar energia reinventando o que não diferencia o produto.

## Consequências
- Ganhamos compatibilidade imediata com Steam, Waydroid, Tailscale etc.
- Se um dia o requisito mudar, a Revoada isola o resto do sistema do systemd.
