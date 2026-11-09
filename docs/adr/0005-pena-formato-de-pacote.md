# ADR 0005: Pena — formato de pacote e repositórios

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Precisamos construir o sistema a partir de pacotes nossos, distribuir apps e
drivers extras, tudo com segurança, num sistema imutável (ADR 0006).

## Decisão
Três camadas, cada uma com uma função:

| Camada | Para quê | Como |
|---|---|---|
| **Imagem base** | O sistema em si (kernel, bibliotecas, Plumagem, serviços) | A Ninhada compila **pacotes Pena** e os junta numa imagem única assinada |
| **Extensões de sistema** | Drivers proprietários (NVIDIA), firmwares extras, ferramentas de sistema opcionais | Imagens `systemd-sysext` geradas a partir de pacotes Pena, assinadas |
| **Aplicativos** | Navegador, Office, jogos, apps do usuário | Flatpak (Flathub + repositório Ararinha), AppImage e Bando |

### O pacote `.pena`
```
firefox-146.0-1-x86_64.pena      (tar + zstd)
├── .PENA/manifesto.toml         nome, versão, arquitetura, dependências, licença,
│                                 origem, hash dos fontes, tamanho
├── .PENA/arquivos.lista         todos os arquivos com hash BLAKE3
└── usr/…                        conteúdo (só em /usr; nada em /etc ou /var)
```
- Assinatura Ed25519 do índice do repositório e de cada pacote.
- Repositório = arquivos estáticos via HTTPS (qualquer servidor ou GitHub Releases).
- Ferramenta `pena`, bilíngue: `pena buscar`, `pena instalar` (cria extensão),
  `pena info`, `pena verificar`. Aliases em inglês: `search`, `install`, `info`.
- Pacotes só escrevem em `/usr`. Configuração padrão vem de `/usr/share/factory/etc`.

### Receitas (Ninhada)
Uma receita TOML por pacote em `receitas/<categoria>/<nome>/receita.toml`, com a
URL e o hash fixos do código-fonte, dependências e passos de build. Exemplo em
[`receitas/base/zlib/receita.toml`](../../receitas/base/zlib/receita.toml).

## Fundamentação
- Raymond: conhecimento nos dados (receitas declarativas).
- LFS: build a partir do upstream com hashes fixos.
- Anderson: assinatura em toda a cadeia de distribuição.
- FreeBSD: base coesa e versionada como um só artefato.

## Consequências
- Não reinventamos loja de apps genéricos: Flathub dá milhares de apps no dia 1.
- Precisamos manter chaves de assinatura com cuidado (chave mestra offline).
