# Ninhada

O sistema de build do Ararinha Azul Linux. Lê as receitas em `receitas/`,
baixa e confere as fontes, constrói tudo em ordem, isolado e reproduzível.

## Compilar
```sh
cargo build --release --manifest-path ninhada/Cargo.toml
cp ninhada/target/release/ninhada ~/.local/bin/
```
Sem dependências externas: compila sem internet.

## Uso
```sh
ninhada hospedeiro                 # confere se a máquina tem o necessário
ninhada verificar                  # valida as receitas
ninhada listar
ninhada plano zlib --arq aarch64   # ordem de construção
ninhada fontes --travar            # baixa e grava hashes novos (revise!)
ninhada construir toolchain        # constrói a toolchain cruzada
ninhada construir zlib --arq aarch64
```

Saídas em `saida/`:
```
saida/
├── fontes/                 cache de downloads (todas as arquiteturas)
└── <arq>/
    ├── raiz/               sistema em construção ($RAIZ)
    │   └── ferramentas/    toolchain cruzada ($FERRAMENTAS)
    ├── pacotes/            arquivos .pena
    ├── registros/          log de cada construção
    └── estado/             impressões digitais (cache)
```

## Formato da receita
```toml
[pacote]
nome      = "zlib"            # igual ao nome da pasta
versao    = "1.3.1"
revisao   = 1                 # aumente ao mudar a receita sem mudar a versão
resumo    = "…"
licenca   = "Zlib"            # identificador SPDX
categoria = "base"
tipo      = "pacote"          # ou "toolchain" (instala direto na raiz)

[[fonte]]                     # uma principal; extras com `destino` ou `extrair = false`
url = "https://…/zlib-1.3.1.tar.gz"
# sha256 = "…"                # opcional; senão vem de receitas/fontes.trava

[dependencias]
construcao = ["toolchain"]    # pacotes ou categorias
execucao   = []

[construcao]
construir_em = "build"        # opcional: subpasta onde os passos rodam
passos   = ["CHOST=$ALVO ./configure --prefix=$PREFIXO", "make"]
testar   = ["make check"]     # só com --testar
instalar = ["make DESTDIR=$DESTINO install"]

[arquiteturas]
suportadas = ["x86_64", "aarch64"]
```

Variáveis disponíveis nos passos: `ARQ`, `ARQ_KERNEL`, `ALVO`, `CARREGADOR`,
`PREFIXO`, `RAIZ`, `FERRAMENTAS`, `DESTINO`, `FONTES`, `NUCLEOS`, `MAKEFLAGS`.
Cada passo roda num `bash -euo pipefail` próprio, a partir da pasta do código
(ou de `construir_em`). Strings `"""multilinha"""` aceitam `\` no fim da linha.

Decisões de projeto: [ADR 0014](../docs/adr/0014-ninhada-v0.md).
