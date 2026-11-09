# ADR 0008: Licenciamento

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza (delegou a recomendação)

## Contexto
O projeto será público e Davi quer créditos. Precisamos proteger o sistema de
virar produto fechado de terceiros, sem afastar quem quer criar apps para ele.

## Opções consideradas
1. **MIT/Apache em tudo** — máxima liberdade, mas qualquer empresa pode fechar o
   sistema e vender.
2. **GPL em tudo** — protege, mas assusta desenvolvedores de apps proprietários se
   o SDK também for GPL.
3. **Mista** — GPL no sistema, Apache no SDK, Creative Commons na arte.

## Decisão
**Opção 3**, detalhada em [LICENSE.md](../../LICENSE.md):
- Sistema: **GPL-3.0-or-later**.
- SDK e bibliotecas para apps: **Apache-2.0**.
- Arte e documentação: **CC BY-SA 4.0**.
- O nome e o logo identificam a versão oficial; forks usam outro nome.

## Fundamentação
- Fogel: licença clara desde o primeiro commit atrai colaboradores.
- Mesma estratégia usada por projetos como o Linux (GPL) com bibliotecas mais
  permissivas para quem desenvolve em cima.

## Consequências
- Todo arquivo de código novo começa com o identificador SPDX
  (`// SPDX-License-Identifier: GPL-3.0-or-later`).
- Contribuições futuras: avaliar um DCO (assinatura `Signed-off-by`).
