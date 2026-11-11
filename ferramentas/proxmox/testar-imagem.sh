#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cria (ou recria) a VM "ararinha-teste" no Proxmox com a imagem mais recente
# gerada na VM de build. Rodar no SHELL DO PROXMOX:
#   bash ferramentas/proxmox/testar-imagem.sh
# Depois: abra a VM no Proxmox → Console. Ou: qm terminal 9200 (serial).

set -euo pipefail
VMID="${VMID:-9200}"
BUILD="${BUILD:-davi@192.168.1.63}"
ARMAZENAMENTO="${ARMAZENAMENTO:-local-lvm}"
IMAGEM_REMOTA=/home/ninhada/saida/imagens/ararinha-x86_64.qcow2
LOCAL=/var/lib/vz/template/iso/ararinha-x86_64.qcow2

echo "== Copiando a imagem da VM de build"
ssh -n "$BUILD" "sudo -n cat $IMAGEM_REMOTA" > "$LOCAL.tmp"
mv "$LOCAL.tmp" "$LOCAL"
ls -lh "$LOCAL"

if qm status "$VMID" &>/dev/null; then
  echo "== Removendo a VM de teste anterior ($VMID)"
  qm stop "$VMID" &>/dev/null || true
  qm destroy "$VMID" --purge
fi

echo "== Criando a VM ararinha-teste ($VMID), UEFI sem Secure Boot"
qm create "$VMID" --name ararinha-teste --ostype l26 --machine q35 \
  --bios ovmf --cores 2 --memory 2048 --net0 virtio,bridge=vmbr0 \
  --scsihw virtio-scsi-single --serial0 socket --vga std --tags ararinha
qm set "$VMID" --efidisk0 "$ARMAZENAMENTO:1,efitype=4m,pre-enrolled-keys=0"
qm disk import "$VMID" "$LOCAL" "$ARMAZENAMENTO"
qm set "$VMID" --scsi0 "$ARMAZENAMENTO:vm-$VMID-disk-1" --boot order=scsi0
# Espaço para apps do Flatpak: o disco cresce e o Ararinha aumenta a raiz
# sozinho no boot.
qm disk resize "$VMID" scsi0 +24G
qm start "$VMID"

cat <<FIM

A VM ararinha-teste ($VMID) está ligando.
  • Tela: Proxmox → ararinha-teste → Console
  • Terminal serial aqui mesmo: qm terminal $VMID   (sair: Ctrl+O)
No primeiro boot o systemd pergunta idioma, teclado, fuso e senha do root.
FIM
