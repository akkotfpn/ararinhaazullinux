# Contribuindo com o Ararinha Azul Linux

Obrigado pelo interesse! O projeto está na **Fase 0 — Fundamentos**.

## Como as decisões são tomadas
- Mudanças de arquitetura viram um ADR em `docs/adr/` (use o [modelo](docs/adr/modelo.md)).
- O mantenedor (Davi Souza) decide o produto; a discussão é pública.

## Regras de código
- Componentes novos em **Rust**; C apenas quando necessário.
- Todo arquivo começa com o identificador SPDX da licença.
- Nada de `unwrap()` em código de sistema fora de testes.
- Toda mensagem para o usuário em **pt-BR e inglês** (Fluent).
- Receitas sempre com URL e SHA-256 fixos.

## Commits
- Mensagens em português, no imperativo: `ninhada: adiciona cache por hash`.
- Assine seus commits com `git commit -s` (Developer Certificate of Origin).

## Arte
Toda arte deve ser original e publicada sob CC BY-SA 4.0. Não use imagens,
fontes ou sons de terceiros sem licença compatível.
