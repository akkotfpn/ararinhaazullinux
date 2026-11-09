# Estrutura do repositório

```
ararinhaazullinux/
├── README.md, LICENSE, LICENSE.md, CREDITOS.md, CONTRIBUINDO.md
├── docs/
│   ├── fundamentos.md        lições da biblioteca de referência
│   ├── limites.md            o que não dá para fazer (ainda)
│   ├── roteiro.md            fases do projeto
│   ├── estrutura.md          este arquivo
│   ├── adr/                  decisões de arquitetura
│   ├── fases/                plano detalhado de cada fase
│   └── guias/                passo a passo (Proxmox, build, contribuição)
├── ninhada/                  sistema de build (Rust)            [Fase 1]
├── receitas/                 uma receita TOML por pacote
│   ├── toolchain/            binutils, gcc, glibc (toolchain cruzada)
│   ├── base/                 sistema base
│   ├── kernel/               Spix Kernel e firmwares
│   └── …                     desktop, jogos, rede (fases seguintes)
├── kernel/
│   ├── config/               fragmentos de configuração do kernel
│   └── patches/              patches próprios (o mínimo possível)
├── imagem/
│   └── repart.d/             layout de partições da imagem
├── componentes/              código dos componentes próprios (Rust)
│   ├── pena/                 gerenciador de pacotes          [Fase 2]
│   ├── muda/                 atualizações                     [Fase 5]
│   ├── revoada/              ferramenta de serviços           [Fase 2]
│   ├── olho-de-spix/         detecção de hardware            [Fase 3]
│   ├── bando/                compatibilidade de apps          [Fase 4]
│   ├── ninho/                ninhod + gerenciador de arquivos [Fase 2–3]
│   ├── plumagem/             compositor e shell               [Fase 2]
│   ├── caatinga/             configurações                    [Fase 2]
│   ├── primeiro-voo/         instalador                       [Fase 2]
│   └── rasante/              jogos e desempenho               [Fase 4]
├── sdk/                      bibliotecas para apps (Apache-2.0)
├── idiomas/                  traduções Fluent (pt-BR, en)
├── arte/                     logo, ícones, temas, mockups, sons (CC BY-SA)
├── licencas/                 textos completos das licenças
└── ferramentas/              scripts auxiliares (testes, geração de arte)
```

Pastas marcadas com fase são criadas quando a fase começar.
