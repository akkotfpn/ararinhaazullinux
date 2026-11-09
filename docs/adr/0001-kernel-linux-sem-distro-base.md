# ADR 0001: Kernel Linux, sem distribuição base

- **Status:** Aceita
- **Data:** 2026-11-09
- **Decisor:** Davi Souza

## Contexto
Queremos um sistema "nosso", sem herdar Ubuntu, Debian, Arch ou Fedora, mas que
funcione em qualquer notebook, desktop ou VM, de qualquer fabricante.

## Opções consideradas
1. **Kernel próprio do zero** — identidade máxima, mas sem drivers: levaria
   décadas para suportar o hardware que o Linux já suporta. Inviável.
2. **Derivar de uma distro** (remasterizar Arch/Debian) — rápido, mas não seria
   nosso; dependeríamos das decisões e do calendário deles.
3. **Kernel Linux + todo o resto construído por nós (método Linux From Scratch)** —
   drivers do mundo inteiro, e controle total sobre build, pacotes e experiência.

## Decisão
Opção 3. O **Spix Kernel** é o kernel Linux oficial (linha estável e LTS),
configurado e compilado pela Ninhada. Toolchain, bibliotecas, init, pacotes,
instalador e interface são construídos a partir do código-fonte original de cada
projeto (upstream), nunca a partir de pacotes binários de outra distro.

## Fundamentação
- Tanenbaum: mecanismo (kernel) separado de política (nosso userspace).
- Corbet et al.: os drivers vivem no kernel; é o único caminho real para "todos os
  fabricantes".
- Linux From Scratch: toolchain cruzada isolada garante independência do hospedeiro.

## Detalhes
- Seguir a série **estável** no desktop e oferecer **LTS** como alternativa.
- Configuração em fragmentos: `base`, `desktop`, `jogos`, `vm`, `x86_64`, `arm64`.
- Opções essenciais: `PREEMPT_DYNAMIC`, Btrfs, ntfs3, exFAT, `binder` (Android),
  namespaces e cgroups v2, eBPF, KVM, módulos de VM convidada (virtio, Hyper-V,
  VMware), WireGuard.
- Patches próprios só quando indispensáveis, sempre documentados e enviados ao
  upstream quando fizer sentido.

## Consequências
- O build inicial é longo (horas). Precisamos de cache e de uma máquina de build
  dedicada (VM no Proxmox).
- Cada vulnerabilidade de pacote vira responsabilidade nossa: precisamos
  acompanhar avisos de segurança.
