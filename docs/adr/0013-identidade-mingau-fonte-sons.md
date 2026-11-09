# ADR 0013: Identidade — Mingau, tipografia e sons

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Mascote: Mingau
A ararinha mascote se chama **Mingau**. Aparece no assistente de primeiro boot,
nas telas de "Por que isso não abriu?", na documentação, no site e em mensagens
de erro amigáveis (nunca em telas de trabalho, para não atrapalhar).
Arte: [`arte/mascote/mingau.svg`](../../arte/mascote/mingau.svg).

## Logo
Aprovado pelo Davi como está ([`arte/logo/ararinha-logo.svg`](../../arte/logo/ararinha-logo.svg)).

## Tipografia
- **Interface: Inter** (SIL Open Font License). Feita especificamente para telas,
  com excelente legibilidade em tamanhos pequenos, acentos completos do português,
  números tabulares (bons para monitores de CPU/GPU) e variação de peso contínua.
- **Monoespaçada: JetBrains Mono** (OFL), para terminal e código.
- Fonte própria de títulos ("Ararinha Display") fica como desejo para a Fase 3,
  se houver um designer de tipos interessado em colaborar.

Motivo para não escolher agora uma fonte brasileira: priorizamos legibilidade e
cobertura técnica comprovadas em interfaces; a identidade brasileira já vem do
logo, das cores, do Mingau e dos sons.

## Sons (Grito)
Pedido do Davi: usar gravação real de uso livre, se houver; senão, sintetizar.

- Gravações conhecidas da espécie estão no xeno-canto, que usa em geral licenças
  Creative Commons **não comerciais** (NC). NC é incompatível com a nossa licença
  CC BY-SA e com a distribuição comercial por terceiros, então **não podemos
  usá-las**. O site também bloqueia acesso automatizado, o que impediu conferir
  cada gravação.
- **Decisão:** sons **sintetizados**, inspirados no chamado da ave (piados curtos
  com varredura de frequência), gerados por
  [`ferramentas/arte/sintetizar-sons.py`](../../ferramentas/arte/sintetizar-sons.py)
  em [`arte/sons/grito-padrao/`](../../arte/sons/grito-padrao/).
- Se aparecer uma gravação real sob **CC0, CC BY ou CC BY-SA** (sem "NC"), ou com
  autorização por escrito do autor, ela substitui os sintetizados.

## Fundamentação
- SerenityOS: identidade forte vem de gosto consistente em todos os detalhes.
- Raymond, regra do silêncio: sons curtos (menos de 1 s) e só quando importam.
