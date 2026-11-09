#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Prepara a VM de build (Debian 13) para rodar a Ninhada.
# Rodar DENTRO da VM ninhada-build, com sudo.
#
# O Debian só dá a partida: a toolchain cruzada da Ninhada isola o resultado,
# e nada do Debian vai para o Ararinha.

set -euo pipefail

if [[ -d /etc/pve ]]; then
  echo "Isto é o Proxmox (host). Este script roda DENTRO da VM ninhada-build:" >&2
  echo "  ssh davi@<ip-da-vm>" >&2
  exit 1
fi
if [[ $EUID -ne 0 ]]; then
  echo "Rode com sudo: sudo $0" >&2
  exit 1
fi
USUARIO="${SUDO_USER:-davi}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

echo "== Pacotes do hospedeiro (equivalente ao capítulo 2 do LFS)"
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y \
  build-essential bison gawk texinfo m4 perl python3 patch \
  tar xz-utils gzip bzip2 zstd curl ca-certificates git file \
  util-linux rsync cargo rustc gnupg \
  qemu-guest-agent \
  qemu-system-x86 qemu-system-arm qemu-utils ovmf qemu-efi-aarch64 qemu-user-binfmt \
  systemd-repart systemd-boot-efi erofs-utils dosfstools mtools \
  cryptsetup-bin btrfs-progs

systemctl enable --now qemu-guest-agent || true

echo "== /bin/sh → bash (exigência do LFS; esta VM é dedicada ao build)"
ln -sfn bash /usr/bin/sh

echo "== Compilando a Ninhada"
sudo -u "$USUARIO" cargo build --release --manifest-path "$REPO/ninhada/Cargo.toml"
install -Dm755 "$REPO/ninhada/target/release/ninhada" /usr/local/bin/ninhada

echo "== Conferindo"
cd "$REPO"
sudo -u "$USUARIO" ninhada hospedeiro
sudo -u "$USUARIO" ninhada verificar

cat <<FIM

Tudo pronto. Próximos passos (como $USUARIO, sem sudo):
  cd $REPO
  ninhada fontes --travar      # baixa e grava os hashes; confira com os oficiais
  ninhada construir toolchain  # x86_64
  ninhada construir toolchain --arq aarch64
FIM
