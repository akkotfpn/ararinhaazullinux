# ADR 0014: Ninhada v0 — sem dependências externas, com trava de fontes

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
A Ninhada constrói o sistema inteiro. Ela precisa ser confiável, simples de
entender e reproduzível.

## Decisões

1. **Rust sem nenhuma crate externa.** Só a biblioteca padrão. O leitor de TOML é
   nosso (subconjunto que as receitas usam). Compactação, hashes e download usam
   ferramentas clássicas do hospedeiro (`tar`, `zstd`, `sha256sum`, `curl`).
   Motivo: a ferramenta que constrói o sistema não pode depender de baixar
   código de terceiros para ser compilada (xv6: caber na cabeça; Anderson: cadeia
   de confiança curta). Podemos rever quando a Pena precisar de assinatura Ed25519.

2. **Toda fonte tem SHA-256 conferido.** O hash vem da receita ou de
   `receitas/fontes.trava`. `ninhada fontes --travar` grava os hashes que faltam;
   **cada linha nova deve ser revisada** contra o checksum ou a assinatura GPG
   publicada pelo projeto original antes do commit. Hash diferente = erro, sem
   exceção.

3. **Construção isolada.** Cada passo roda em namespace de usuário com rede vazia
   (`unshare --user --map-current-user --net`), ambiente limpo (`env_clear`),
   `LC_ALL=POSIX` e toolchain própria primeiro no `PATH`, como no LFS.

4. **Cache por impressão digital.** SHA-256 de (receita + hashes das fontes +
   arquitetura). Nada mudou, nada recompila.

5. **Pacotes reproduzíveis.** `.pena` com ordem de arquivos fixa, datas zeradas e
   dono 0; gravado em temporário e renomeado no fim. Pacotes só podem escrever em
   `/usr` (a Ninhada recusa o resto).

6. **Dois tipos de receita.** `toolchain` instala direto na raiz em construção
   (capítulos 4–7 do LFS); `pacote` gera `.pena` e depois o instala na raiz.

7. **Dependências por pacote ou por categoria.** `construcao = ["toolchain"]`
   significa "toda a toolchain".

8. **Fidelidade ao LFS.** As receitas da toolchain seguem o livro LFS
   (versão systemd de desenvolvimento r13.1-39, conferida em 2026-11-09), com
   diferenças documentadas no topo de cada receita (ex.: compilação cruzada para
   aarch64, `/ferramentas` no lugar de `/tools`).

## Verificado
Teste de ponta a ponta com uma receita local: trava de fontes, construção
isolada (rede inacessível dentro do passo), patch ao lado do código, cache,
pacote reproduzível (mesmo hash em duas construções), recusa de hash adulterado
e recusa de arquivos fora de `/usr`.
