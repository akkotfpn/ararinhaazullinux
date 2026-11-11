#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Confere as fontes travadas em receitas/fontes.trava contra as assinaturas
# GPG oficiais de cada projeto. Rodar na máquina de build, depois de
# `ninhada fontes --travar`.
#
#   1. o arquivo baixado tem o SHA-256 que está na trava?
#   2. a assinatura oficial do projeto confirma esse arquivo?
#
# Resultado por fonte: ASSINADO, SEM ASSINATURA (o projeto não publica) ou FALHOU.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
TRAVA="$REPO/receitas/fontes.trava"
FONTES="${FONTES:-$REPO/saida/fontes}"

command -v gpg >/dev/null || { echo "Falta o gpg: sudo apt-get install -y gnupg" >&2; exit 1; }

# Chaveiro temporário, só para esta conferência.
GNUPGHOME="$(mktemp -d)"
export GNUPGHOME
trap 'rm -rf "$GNUPGHOME"' EXIT
TMP="$GNUPGHOME/tmp"; mkdir -p "$TMP"

echo "== Importando chaves oficiais"
# Chaveiro dos mantenedores GNU (gcc, glibc, gmp, mpfr, mpc, binutils).
curl -fsSL https://ftp.gnu.org/gnu/gnu-keyring.gpg -o "$TMP/gnu-keyring.gpg"
gpg --quiet --batch --import "$TMP/gnu-keyring.gpg" 2>/dev/null || true
# Kernel: chaves publicadas pelo próprio kernel.org (WKD).
gpg --quiet --batch --auto-key-locate clear,wkd \
    --locate-keys torvalds@kernel.org gregkh@kernel.org >/dev/null 2>&1 || true
# linux-firmware: assinado pelos mantenedores do firmware.
gpg --quiet --batch --auto-key-locate clear,wkd \
    --locate-keys jwboyer@kernel.org superm1@kernel.org >/dev/null 2>&1 || true

# 0 = assinatura válida de uma chave do chaveiro.
assinatura_ok() {  # $1 = assinatura, $2 = arquivo ("-" para stdin)
  gpg --batch --status-fd 1 --verify "$1" "$2" 2>/dev/null | grep -q '^\[GNUPG:\] VALIDSIG'
}

ok=0; sem=0; falhou=0
while read -r hash url; do
  [[ -z "${hash:-}" || "$hash" == \#* ]] && continue
  nome="${url##*/}"
  arq="$FONTES/$nome"
  printf '%-45s ' "$nome"

  if [[ ! -f "$arq" ]]; then
    echo "FALHOU (arquivo não está em $FONTES)"; falhou=$((falhou+1)); continue
  fi
  if [[ "$(sha256sum "$arq" | cut -d' ' -f1)" != "$hash" ]]; then
    echo "FALHOU (hash diferente da trava)"; falhou=$((falhou+1)); continue
  fi

  resultado="SEM ASSINATURA"
  case "$url" in
    https://ftp.gnu.org/*|https://sourceware.org/pub/binutils/*)
      if curl -fsL "$url.sig" -o "$TMP/$nome.sig"; then
        assinatura_ok "$TMP/$nome.sig" "$arq" && resultado="ASSINADO" || resultado="FALHOU"
      fi ;;
    https://cdn.kernel.org/*)
      # O kernel assina o .tar descompactado.
      sig="${url%.xz}.sign"
      if curl -fsL "$sig" -o "$TMP/$nome.sign"; then
        xz -dc "$arq" > "$TMP/kernel.tar"
        assinatura_ok "$TMP/$nome.sign" "$TMP/kernel.tar" && resultado="ASSINADO" || resultado="FALHOU"
        rm -f "$TMP/kernel.tar"
      fi ;;
    https://zlib.net/*)
      if curl -fsL "$url.asc" -o "$TMP/$nome.asc"; then
        assinatura_ok "$TMP/$nome.asc" "$arq" && resultado="ASSINADO" || resultado="SEM ASSINATURA (chave não encontrada)"
      fi ;;
    https://www.linuxfromscratch.org/*)
      resultado="SEM ASSINATURA (o LFS publica só MD5)" ;;
  esac

  echo "$resultado"
  case "$resultado" in
    ASSINADO) ok=$((ok+1)) ;;
    FALHOU*) falhou=$((falhou+1)) ;;
    *) sem=$((sem+1)) ;;
  esac
done < "$TRAVA"

echo
echo "Assinadas: $ok   Sem assinatura publicada: $sem   Falharam: $falhou"
(( falhou == 0 ))
