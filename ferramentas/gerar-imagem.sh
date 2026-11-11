#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Gera a imagem de disco do Ararinha a partir da raiz construída pela Ninhada.
#   ferramentas/gerar-imagem.sh <saida> [x86_64|aarch64]
# Resultado: <saida>/imagens/ararinha-<arq>.raw e .qcow2, e o pacote de
# desenvolvimento ararinha-dev-<arq>.tar.zst (ver imagem/pacote-dev.txt)
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

# Raiz "podada": cópia com hardlinks (rápida, sem gastar disco) sem o que
# está em imagem/pacote-dev.txt. O que sai vai para o pacote de
# desenvolvimento, gerado ao lado da imagem.
PODADA="$SAIDA/$ARQ/raiz-imagem"
DEV="$DESTINO/ararinha-dev-$ARQ.tar.zst"
rm -rf "$PODADA"
echo "== Podando a raiz (pacote de desenvolvimento fica fora da imagem)"
cp -al "$RAIZ" "$PODADA"
LISTA="$(mktemp)"
shopt -s nullglob
( cd "$PODADA"
  while IFS= read -r padrao; do
    [[ -z "$padrao" || "$padrao" == \#* ]] && continue
    for p in $padrao; do
      printf '%s\n' "$p"
    done
  done < "$REPO/imagem/pacote-dev.txt" ) | sort -u > "$LISTA"
shopt -u nullglob
if [[ -s "$LISTA" ]]; then
  if command -v zstd >/dev/null; then
    tar -C "$RAIZ" --numeric-owner -cf - -T "$LISTA" | zstd -q -T0 -10 -f -o "$DEV"
  else
    DEV="${DEV%.zst}.gz"
    tar -C "$RAIZ" --numeric-owner -czf "$DEV" -T "$LISTA"
  fi
  ( cd "$PODADA" && xargs -d '\n' rm -rf -- < "$LISTA" )
fi
echo "== Fora da imagem: $(wc -l < "$LISTA") itens; pacote dev: $(du -h "$DEV" 2>/dev/null | cut -f1)"
echo "== Tamanho da raiz: $(du -sh "$RAIZ" | cut -f1) -> $(du -sh "$PODADA" | cut -f1)"
rm -f "$LISTA"

echo "== Montando a imagem ($ARQ)"
unshare --user --map-root-user -- systemd-repart \
  --empty=create --size=auto --offline=yes --dry-run=no \
  --architecture="$ARQ_REPART" \
  --sector-size=512 \
  --definitions="$REPO/imagem/repart.d" \
  --copy-source="$PODADA" \
  "$RAW"
rm -rf "$PODADA"

# O firmware UEFI só lê a FAT se o setor dela for igual ao do disco (512).
# Confere direto no setor de boot da FAT (bytes 11-12) da partição EFI.
INICIO=$(sfdisk -J "$RAW" | python3 -c 'import json,sys; print(json.load(sys.stdin)["partitiontable"]["partitions"][0]["start"])')
SETOR_FAT=$(od -An -tu2 -j $(( INICIO * 512 + 11 )) -N2 "$RAW" | tr -d ' ')
echo "== Partição EFI começa no setor $INICIO; setor da FAT: $SETOR_FAT bytes"
[[ "$SETOR_FAT" == 512 ]] || { echo "FAT com setor de $SETOR_FAT bytes: o UEFI não lê." >&2; exit 1; }

echo "== Convertendo para qcow2 (Proxmox)"
qemu-img convert -O qcow2 -c "$RAW" "$DESTINO/ararinha-$ARQ.qcow2"
ls -lh "$DESTINO"/ararinha-"$ARQ".*
