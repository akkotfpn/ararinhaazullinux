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
      [[ -e "$p" || -L "$p" ]] && printf '%s\n' "$p"
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
# "Selar" a imagem: nada da construção fica para quem usa (ou para quem
# invade) encontrar.
echo "== Selando a imagem (símbolos, registros e sobras da construção)"
# 1) Registros, históricos, caches e pastas de trabalho.
( cd "$PODADA"
  rm -rf ninhada-obra fontes tools root/.cache root/.cargo root/.bash_history root/.lesshst \
         root/.python_history root/.wget-hsts
  for d in var/log var/tmp tmp var/cache; do
    [[ -d "$d" ]] && find "$d" -mindepth 1 -delete 2>/dev/null || true
  done
  # Cada instalação gera os seus no primeiro boot.
  rm -f etc/machine-id var/lib/systemd/random-seed var/lib/dbus/machine-id etc/ssh/ssh_host_* )
# 2) Símbolos de depuração e nomes internos fora dos programas. O strip
#    grava num arquivo novo e o mv troca, para não mexer na raiz original
#    (a cópia é feita de hardlinks).
if command -v strip >/dev/null; then
  n=0
  while IFS= read -r -d '' f; do
    cab="$(head -c 4 "$f" 2>/dev/null | od -An -c | tr -d ' ')"
    [[ "$cab" == '177ELF' ]] || continue
    case "$f" in
      *.ko) modo=--strip-debug ;;
      *.so|*.so.*) modo=--strip-unneeded ;;
      *) modo=--strip-all ;;
    esac
    if strip $modo -o "$f.selo" "$f" 2>/dev/null; then
      chmod --reference="$f" "$f.selo" && chown --reference="$f" "$f.selo" 2>/dev/null || true
      mv -f "$f.selo" "$f"; n=$((n+1))
    else
      rm -f "$f.selo"
    fi
  done < <(find "$PODADA/usr/bin" "$PODADA/usr/sbin" "$PODADA/usr/lib" "$PODADA/usr/libexec" \
             -xdev -type f \( -perm -u+x -o -name '*.so' -o -name '*.so.*' -o -name '*.ko' \) -print0 2>/dev/null)
  echo "== Programas e bibliotecas sem símbolos: $n"
fi
# 3) Confere: nenhum caminho da construção pode sobrar nos nossos programas.
for b in usr/bin/plumagem usr/bin/explorador usr/bin/configuracoes; do
  if [[ -f "$PODADA/$b" ]] && grep -qa -e '/ninhada-obra/' -e '/home/' "$PODADA/$b"; then
    echo "aviso: $b ainda guarda caminhos da construção"
  fi
done
echo "== Pastas na raiz da imagem: $(ls "$PODADA" | tr '\n' ' ')"

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
