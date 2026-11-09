#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cria a VM "ninhada-build" no Proxmox, onde o Ararinha é compilado.
# Rodar no SHELL DO PROXMOX (nó PVE), como root.
#
# A VM usa todos os núcleos do servidor, mas com PRIORIDADE BAIXA de CPU
# (cpuunits): quando o Frigate, o Vaultwarden ou outra VM precisar de CPU,
# eles ganham. A compilação só usa o que sobrar.
#
# Uso:
#   ./criar-vm-build.sh                       # padrões abaixo
#   VMID=9100 RAM_GB=24 DISCO_GB=250 ARMAZENAMENTO=local-zfs ./criar-vm-build.sh

set -euo pipefail

VMID="${VMID:-9100}"
NOME="${NOME:-ninhada-build}"
NUCLEOS="${NUCLEOS:-$(nproc)}"
# Padrão: metade da RAM do servidor, no máximo 32 GB (deixa espaço para o resto).
RAM_TOTAL_MB=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo)
RAM_PADRAO_GB=$(( RAM_TOTAL_MB / 2048 )); (( RAM_PADRAO_GB > 32 )) && RAM_PADRAO_GB=32
RAM_GB="${RAM_GB:-$RAM_PADRAO_GB}"
DISCO_GB="${DISCO_GB:-250}"
ARMAZENAMENTO="${ARMAZENAMENTO:-local-lvm}"   # LVM-thin ou ZFS (não "local", que é diretório)
PONTE="${PONTE:-vmbr0}"
USUARIO="${USUARIO:-davi}"
CHAVES_SSH="${CHAVES_SSH:-/root/.ssh/authorized_keys}"
IMAGEM_URL="https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2"
IMAGEM="/var/lib/vz/template/iso/debian-13-genericcloud-amd64.qcow2"

if qm status "$VMID" &>/dev/null; then
  echo "Já existe uma VM com ID $VMID. Escolha outro: VMID=9101 $0" >&2
  exit 1
fi
if ! pvesm status | awk 'NR>1 {print $1}' | grep -qx "$ARMAZENAMENTO"; then
  echo "Armazenamento '$ARMAZENAMENTO' não existe. Disponíveis:" >&2
  pvesm status | awk 'NR>1 {print "  " $1}' >&2
  exit 1
fi

echo "== Criando $NOME (ID $VMID): $NUCLEOS núcleos, ${RAM_GB} GB RAM, ${DISCO_GB} GB disco em $ARMAZENAMENTO"

if [[ ! -f "$IMAGEM" ]]; then
  echo "== Baixando imagem do Debian 13 (só para dar a partida no build)"
  wget -q --show-progress -O "$IMAGEM.tmp" "$IMAGEM_URL"
  mv "$IMAGEM.tmp" "$IMAGEM"
fi

qm create "$VMID" \
  --name "$NOME" \
  --description "Máquina de build do Ararinha Azul Linux (Ninhada)" \
  --ostype l26 \
  --machine q35 \
  --cpu host \
  --cores "$NUCLEOS" \
  --cpuunits 50 \
  --memory $(( RAM_GB * 1024 )) \
  --balloon 0 \
  --net0 "virtio,bridge=$PONTE" \
  --scsihw virtio-scsi-single \
  --agent enabled=1 \
  --serial0 socket \
  --onboot 0 \
  --tags ararinha

qm disk import "$VMID" "$IMAGEM" "$ARMAZENAMENTO"
qm set "$VMID" \
  --scsi0 "$ARMAZENAMENTO:vm-$VMID-disk-0,iothread=1,discard=on,ssd=1" \
  --boot order=scsi0 \
  --ide2 "$ARMAZENAMENTO:cloudinit" \
  --ciuser "$USUARIO" \
  --ipconfig0 ip=dhcp
if [[ -f "$CHAVES_SSH" ]]; then
  qm set "$VMID" --sshkeys "$CHAVES_SSH"
else
  echo "Aviso: $CHAVES_SSH não existe; defina uma senha com: qm set $VMID --cipassword" >&2
fi
qm resize "$VMID" scsi0 "${DISCO_GB}G"
qm start "$VMID"

echo
echo "== Pronto. Em ~1 minuto a VM terá IP. Para descobrir:"
echo "   qm guest cmd $VMID network-get-interfaces   (depois que o agente subir)"
echo "Depois, dentro da VM:"
echo "   git clone https://github.com/akkotfpn/ararinhaazullinux"
echo "   sudo ararinhaazullinux/ferramentas/preparar-hospedeiro.sh"
