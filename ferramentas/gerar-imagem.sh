#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Gera a imagem de disco do Ararinha a partir da raiz construída pela Ninhada.
#   ferramentas/gerar-imagem.sh <saida> [x86_64|aarch64]
# Resultado: <saida>/imagens/ararinha-<arq>.raw e .qcow2
#
# Roda sem root: o systemd-repart trabalha "offline" (sem loop devices) dentro
# de um namespace de usuário, onde os arquivos da raiz aparecem como do root,
# que é o dono que eles precisam ter dentro da imagem.

set -euo pipefail
SAIDA="${1:?uso: $0 <saida> [arq]}"
ARQ="${2:-x86_64}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
RAIZ="$SAIDA/$ARQ/raiz"
DESTINO="$SAIDA/imagens"
case "$ARQ" in
  x86_64) ARQ_REPART=x86-64 ;;
  aarch64) ARQ_REPART=arm64 ;;
  *) echo "arquitetura desconhecida: $ARQ" >&2; exit 1 ;;
esac

[[ -f "$RAIZ/boot/vmlinuz-spix" ]] || { echo "Falta o kernel em $RAIZ/boot (construa spix-kernel)." >&2; exit 1; }
mkdir -p "$DESTINO"
RAW="$DESTINO/ararinha-$ARQ.raw"
rm -f "$RAW"

echo "== Montando a imagem ($ARQ)"
unshare --user --map-root-user -- systemd-repart \
  --empty=create --size=auto --offline=yes --dry-run=no \
  --architecture="$ARQ_REPART" \
  --sector-size=512 \
  --definitions="$REPO/imagem/repart.d" \
  --copy-source="$RAIZ" \
  "$RAW"

# O firmware UEFI só lê a FAT se o setor dela for igual ao do disco (512).
# Confere direto no setor de boot da FAT (bytes 11-12) da partição EFI.
INICIO=$(sfdisk -J "$RAW" | python3 -c 'import json,sys; print(json.load(sys.stdin)["partitiontable"]["partitions"][0]["start"])')
SETOR_FAT=$(od -An -tu2 -j $(( INICIO * 512 + 11 )) -N2 "$RAW" | tr -d ' ')
echo "== Partição EFI começa no setor $INICIO; setor da FAT: $SETOR_FAT bytes"
[[ "$SETOR_FAT" == 512 ]] || { echo "FAT com setor de $SETOR_FAT bytes: o UEFI não lê." >&2; exit 1; }

echo "== Convertendo para qcow2 (Proxmox)"
qemu-img convert -O qcow2 -c "$RAW" "$DESTINO/ararinha-$ARQ.qcow2"
ls -lh "$DESTINO"/ararinha-"$ARQ".*
