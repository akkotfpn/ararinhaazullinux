# ADR 0007: x86_64 e ARM64 desde o início

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Requisito do Davi: rodar em PCs comuns e em ARM (Raspberry Pi, notebooks ARM).

## Decisão
A Ninhada constrói para as duas arquiteturas desde o primeiro commit. Nenhuma
receita pode assumir x86.

| Arquitetura | Alvo | Observações |
|---|---|---|
| **x86_64** | PCs, notebooks, VMs | Base `x86-64-v2` (roda em PCs de ~2009 em diante). Bibliotecas críticas também em `x86-64-v3`, escolhidas automaticamente pelo glibc-hwcaps em CPUs novas. Inclui bibliotecas de 32 bits (multilib), obrigatórias para Steam e Wine. |
| **aarch64 (UEFI)** | VMs ARM, servidores e notebooks compatíveis com UEFI/ACPI | Mesma imagem genérica. Notebooks Snapdragon dependem do suporte do kernel, que ainda é parcial em alguns modelos. |
| **aarch64 (Raspberry Pi 4/5)** | Raspberry Pi | Imagem própria: o Pi inicia pelo firmware dele e device tree, não por UEFI. |

## Limites honestos
- Apps Windows x86 em ARM precisam de tradução de CPU (FEX-Emu ou Box64):
  funcionam, com desempenho menor.
- Drivers NVIDIA proprietários não existem para a maioria das placas em ARM.
- Steam em ARM depende de tradução de CPU.

## Fundamentação
- Patterson & Hennessy: mesmos princípios em arquiteturas diferentes.
- Brooks: decidir cedo é barato; adaptar depois é caro.

## Consequências
- CI precisa compilar e testar as duas (QEMU para ARM).
- Testes em hardware real ARM na Fase 3.
