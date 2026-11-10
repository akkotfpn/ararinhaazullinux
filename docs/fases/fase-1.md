# Fase 1 — "Primeiro Pio": um sistema que boota

> Objetivo: gerar, **a partir apenas de código-fonte**, uma imagem do Ararinha que
> inicia numa VM (Proxmox e QEMU), em x86_64 e ARM64, e chega a um login em
> terminal com rede cabeada funcionando.

Sem interface gráfica nesta fase. A Plumagem começa na Fase 2.

---

## Progresso

> **9 de novembro de 2026, 21h46: o Ararinha Azul Linux bootou pela primeira vez** no Proxmox
> (`ararinha login:` no console serial), entrou na rede por DHCP com o nome
> "ararinha" e, às 22h04, aconteceu o primeiro login como root, em pt_BR.

| Etapa | Situação |
|---|---|
| 1.1 Máquina de build | ✅ VM `ninhada-build` rodando no Proxmox. Scripts: [`criar-vm-build.sh`](../../ferramentas/proxmox/criar-vm-build.sh) e [`preparar-hospedeiro.sh`](../../ferramentas/preparar-hospedeiro.sh). Guia em [guias/proxmox.md](../guias/proxmox.md) |
| 1.2 Ninhada v0 | ✅ Pronta e testada ([ADR 0014](../adr/0014-ninhada-v0.md)) |
| 1.3 Toolchain cruzada | ✅ x86_64 compilada na VM (capítulo 5 do LFS: binutils 2.47, GCC 16.2.0, Linux 7.1.8, glibc 2.44, libstdc++) e primeiro `.pena` (zlib). Hashes travados e conferidos: 7 fontes com assinatura GPG oficial válida, 3 sem assinatura publicada (patches do LFS e zlib, conferida pelo hash oficial). Ferramenta: [`verificar-assinaturas.sh`](../../ferramentas/verificar-assinaturas.sh). aarch64 também compilada (com ajuste lib64→lib que o LFS não cobre) |
| 1.3 Ferramentas temporárias | Capítulo 6 do LFS: 17 receitas em `receitas/temporarias/` (m4, ncurses, bash, coreutils, diffutils, file, findutils, gawk, grep, gzip, make, patch, sed, tar, xz, binutils e GCC passo 2). Capítulo 7: modo isolado da Ninhada (namespaces de usuário no lugar do chroot com root) e 9 receitas em `receitas/isoladas/`. ✅ Capítulos 6 e 7 compilados nas duas arquiteturas (aarch64 via qemu-user) |
| 1.4 Sistema base | Capítulo 8 do LFS, bloco 1: 23 receitas em `receitas/sistema/` (glibc, zlib, bzip2, xz, lz4, zstd, file, readline, m4, bc, flex, ninja, pkgconf, binutils, gmp, mpfr, mpc, attr, acl, libcap, libxcrypt, shadow, gcc). Bloco 2: mais 40 receitas até o systemd (ncurses, bash, perl, openssl, python, meson, kmod, coreutils com suporte a acentos, kbd, util-linux, systemd, dbus, procps-ng, e2fsprogs). Comandos extraídos do código-fonte do livro (repositório lfs-book/lfs, edição systemd) |
| 1.5 Spix Kernel | Receita `spix-kernel` (Linux 7.1.8, defconfig + fragmentos `base`, `vm` e `primeiro-boot`), kernel como aplicativo UEFI |
| 1.6–1.8 Boot, imagem, primeiro boot | `configurar-sistema` (os-release do Ararinha, pt_BR, ABNT2, DHCP, tela de login com o Mingau, primeiro boot do systemd), `imagem/repart.d` + `ferramentas/gerar-imagem.sh` (systemd-repart sem root) e `ferramentas/proxmox/testar-imagem.sh` |
| 1.9 Testes | CI leve no GitHub + construção completa no runner próprio `ninhada-build` (workflow Construir) |

Referência: livro *Linux From Scratch*, versão systemd
([linuxfromscratch.org](https://www.linuxfromscratch.org/lfs/read.html)).

## Critérios de pronto

- [ ] `ninhada imagem --arq x86_64` gera `ararinha-x86_64.raw` e `.qcow2` sem
      usar nenhum pacote binário de outra distro.
- [ ] A imagem inicia em UEFI no QEMU e na VM do Proxmox, mostra o logo no boot e
      chega ao login em menos de 15 s (SSD).
- [ ] O mesmo vale para `--arq aarch64` no QEMU (`virt`, UEFI).
- [ ] Rede cabeada por DHCP, `ping` e resolução de nomes funcionando.
- [ ] Layout de disco final (ESP, usr-A/usr-B somente leitura, dados Btrfs),
      ainda sem atualização automática.
- [ ] Primeiro boot pergunta usuário e senha; idioma pt-BR e teclado ABNT2 por padrão.
- [ ] Teste automático de boot no CI (lê o console serial até aparecer o login).
- [ ] Documentação de como reproduzir o build do zero.

---

## Etapas

### 1.1 Máquina de build (VM no Proxmox)
- VM `ninhada-build`, sugestão: 8–16 vCPUs, 16–32 GB de RAM, 250 GB de disco.
- Qualquer Linux recente serve de hospedeiro **só para dar a partida**: a
  toolchain própria isola o resultado (método LFS). Recomendação: um sistema
  mínimo, sem nada além de compilador, Rust e QEMU.
- Também roda um *runner* do GitHub Actions para o CI, pois os builds levam horas
  e não cabem nos runners gratuitos.

### 1.2 Ninhada v0 (Rust)
Ferramenta de linha de comando que:
1. lê as receitas TOML (`receitas/**/receita.toml`);
2. monta o grafo de dependências e a ordem de build;
3. baixa os fontes e **confere o SHA-256** (falha se não bater);
4. compila cada pacote em ambiente isolado (namespaces, sem rede, raiz limpa);
5. gera pacotes `.pena` num repositório local;
6. monta a imagem de disco.

Comandos: `ninhada fontes`, `ninhada toolchain`, `ninhada construir <pacote>`,
`ninhada imagem`, `ninhada limpar`. Cache por hash da receita + hash dos fontes:
receita não mudou, pacote não recompila.

### 1.3 Toolchain cruzada
Equivalente aos capítulos de toolchain temporária do LFS, automatizado:
`binutils` → `gcc` (passo 1) → cabeçalhos do kernel → `glibc` → `libstdc++` →
ferramentas temporárias → ambiente isolado → `binutils`/`gcc` finais.
Alvos: `x86_64-ararinha-linux-gnu` e `aarch64-ararinha-linux-gnu`
(compilação cruzada a partir de x86_64).

### 1.4 Sistema base (~60 pacotes)
- **Núcleo**: glibc, gcc-libs, zlib, xz, zstd, bzip2, lz4, openssl, libffi,
  ncurses, readline, ca-certificates.
- **Utilitários**: coreutils, util-linux, bash, findutils, grep, sed, gawk, diffutils,
  tar, gzip, procps-ng, shadow, kmod, less, nano, file, which.
- **Discos**: e2fsprogs, btrfs-progs, dosfstools, erofs-utils, cryptsetup, lvm2
  (para dm-verity), parted.
- **Sistema**: systemd (com systemd-boot, ukify, repart, firstboot), dbus,
  libseccomp, libcap, acl, attr.
- **Rede**: iproute2, iputils, openssh. Nesta fase a rede cabeada usa
  `systemd-networkd`, de forma provisória; o NetworkManager (Canto) entra na
  Fase 2 junto com o Wi-Fi.
- **Kernel e firmware**: Spix Kernel, `linux-firmware` (subconjunto para VMs
  nesta fase), microcode Intel/AMD.
- Bibliotecas de 32 bits ficam para a Fase 4 (Steam/Wine).

### 1.5 Spix Kernel
- Configuração em fragmentos em [`kernel/config/`](../../kernel/config/):
  `base`, `desktop`, `vm`, `x86_64`, `arm64`. Fase 1 usa `base + vm + arquitetura`.
- Módulos necessários para Proxmox (virtio-blk, virtio-net, virtio-gpu,
  qemu-guest-agent em userspace) e QEMU ARM.

### 1.6 Boot
- **UEFI** com `systemd-boot` e **kernel unificado (UKI)**: kernel + initramfs +
  linha de comando num só arquivo, pronto para ser assinado na Fase 5.
- initramfs gerado pela Ninhada a partir dos próprios pacotes (systemd no initrd).
- BIOS legado fica para a Fase 5.
- Splash de boot animado (ave levantando voo) fica para a Fase 2 com Plymouth ou
  solução própria; nesta fase, logo no menu do `systemd-boot` e terminal colorido.

### 1.7 Imagem de disco
Gerada com `systemd-repart` a partir de definições em `imagem/repart.d/`:
```
ESP     512 MB  FAT32   systemd-boot + UKI
usr-A   ~2 GB   EROFS   /usr somente leitura (+ partição dm-verity)
usr-B   ~2 GB   vazio   (slot de atualização; usado pela Muda na Fase 5)
dados   resto   Btrfs   criada e expandida no primeiro boot (@etc, @var, @home)
```
Saídas: `.raw` (pendrive/QEMU) e `.qcow2` (Proxmox).

### 1.8 Primeiro boot
- `systemd-firstboot`: idioma pt-BR, teclado `br-abnt2`, fuso `America/Sao_Paulo`
  (todos alteráveis), nome do computador `ararinha`, criação do usuário.
- Mensagem de boas-vindas com a ararinha em arte ASCII e a versão.

### 1.9 Testes
- `ferramentas/testar-qemu.sh <imagem>`: inicia no QEMU com OVMF/AAVMF e console
  serial, espera o prompt de login, valida rede e encerra.
- Guia para importar no Proxmox (`qm importdisk`) em `docs/guias/proxmox.md`.
- CI: build incremental + teste de boot nas duas arquiteturas a cada mudança em
  `receitas/`, `kernel/` ou `imagem/`.

---

## Riscos e como lidamos

| Risco | Mitigação |
|---|---|
| Build demorado (glibc, gcc, kernel, systemd) | Cache por hash; build paralelo; máquina dedicada |
| Compilação cruzada para ARM quebra pacotes | Testar cedo; patches mínimos e documentados |
| Erros sutis de toolchain | Seguir a ordem do LFS e rodar os testes dos pacotes críticos (glibc, gcc) |
| Fontes upstream mudam ou somem | Espelho próprio dos tarballs, sempre conferidos por hash |

## Depois da Fase 1
Fase 2: NetworkManager e Wi-Fi, Pena com extensões, primeira Plumagem (compositor,
barra de tarefas, menu), app Ninho v0 com Este Computador, instalador Primeiro Voo v1.
Veja o [roteiro completo](../roteiro.md).
