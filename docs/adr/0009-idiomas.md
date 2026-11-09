# ADR 0009: Português e inglês

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Decisão
- **pt-BR** é o idioma principal; **inglês** é obrigatório para tudo que for
  lançado. Nenhum texto de interface vai para uma versão sem as duas traduções.
- Apps próprios usam **Project Fluent** (`fluent-rs`): frases naturais, plurais e
  gênero corretos em português. Arquivos em `idiomas/pt-BR/*.ftl` e
  `idiomas/en/*.ftl`.
- Software de terceiros usa o gettext dele, com o idioma do sistema.
- Instalador detecta o idioma e já sugere teclado **ABNT2** e fuso
  `America/Sao_Paulo` para pt-BR.
- Comandos de terminal aceitam os dois idiomas (`pena instalar` = `pena install`).
- Documentação nasce em pt-BR e é traduzida para inglês antes de cada lançamento.

## Fundamentação
- Norman/Krug: o usuário entende na língua dele.
- Fogel: inglês amplia a comunidade de colaboradores.
