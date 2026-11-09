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
DISCO_GB="${DISCO_GB:-}"   # vazio = calcula pelo espaço livre (máx. 150 GB)
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

# Espaço realmente livre no armazenamento (KiB, coluna "Available").
# Em LVM-thin os discos são "promessas": se a soma passar do tamanho real e o
# pool encher, TODAS as VMs podem travar. Por isso o disco nunca passa de 70%
# do que está livre de verdade.
LIVRE_GB=$(pvesm status --storage "$ARMAZENAMENTO" | awk 'NR==2 {print int($6/1024/1024)}')
LIMITE_GB=$(( LIVRE_GB * 70 / 100 ))
if [[ -z "$DISCO_GB" ]]; then
  DISCO_GB=$(( LIMITE_GB < 150 ? LIMITE_GB : 150 ))
fi
if (( DISCO_GB < 80 )); then
  echo "Pouco espaço livre em $ARMAZENAMENTO (${LIVRE_GB} GB). O build precisa de pelo menos 80 GB." >&2
  exit 1
fi
if (( DISCO_GB > LIMITE_GB )); then
  echo "DISCO_GB=$DISCO_GB passa de 70% do livre em $ARMAZENAMENTO (${LIVRE_GB} GB livres)." >&2
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
# A imagem cloud do Debian não traz o qemu-guest-agent. Um "vendor-data" do
# cloud-init instala o agente no primeiro boot, para o Proxmox mostrar o IP.
# Snippets precisam estar habilitados no armazenamento "local".
CONTEUDO_LOCAL=$(pvesh get /storage/local --output-format json | grep -oP '"content"\s*:\s*"\K[^"]+')
if [[ ",$CONTEUDO_LOCAL," != *",snippets,"* ]]; then
  echo "== Habilitando 'snippets' no armazenamento local (mantendo: $CONTEUDO_LOCAL)"
  pvesm set local --content "$CONTEUDO_LOCAL,snippets"
fi
mkdir -p /var/lib/vz/snippets
cat > /var/lib/vz/snippets/ninhada-build.yaml <<'YAML'
#cloud-config
packages:
  - qemu-guest-agent
  - git
  - tmux
runcmd:
  - systemctl enable --now qemu-guest-agent
YAML
qm set "$VMID" --cicustom "vendor=local:snippets/ninhada-build.yaml"

qm resize "$VMID" scsi0 "${DISCO_GB}G"
qm start "$VMID"

echo
echo "== Aguardando a VM instalar o agente e ganhar IP (até 5 minutos)..."
IP=""
for _ in $(seq 60); do
  IP=$(qm guest cmd "$VMID" network-get-interfaces 2>/dev/null \
       | grep -oP '"ip-address"\s*:\s*"\K[0-9.]+' | grep -v '^127\.' | head -n1 || true)
  [[ -n "$IP" ]] && break
  sleep 5
done

echo
if [[ -z "$IP" ]]; then
  echo "== VM criada, mas o IP ainda não apareceu. Tente daqui a pouco:"
  echo "   qm guest cmd $VMID network-get-interfaces"
  exit 1
fi
echo "== IP da VM: $IP"

if [[ "${PREPARAR:-1}" != 1 ]]; then
  echo "Entre com: ssh $USUARIO@$IP"
  exit 0
fi

# Daqui em diante tudo roda DENTRO da VM, via ssh a partir do Proxmox.
SSH=(ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 "$USUARIO@$IP")
echo "== Esperando o primeiro boot (cloud-init) terminar"
for _ in $(seq 30); do "${SSH[@]}" true 2>/dev/null && break; sleep 5; done
"${SSH[@]}" "cloud-init status --wait >/dev/null; echo ok"

echo "== Preparando a VM (pacotes + Ninhada). Leva alguns minutos."
"${SSH[@]}" "git clone -q https://github.com/akkotfpn/ararinhaazullinux 2>/dev/null || git -C ararinhaazullinux pull -q; \
             sudo ararinhaazullinux/ferramentas/preparar-hospedeiro.sh"

echo "== Iniciando a compilação da toolchain dentro de um tmux (continua se a conexão cair)"
"${SSH[@]}" "tmux new-session -d -s ninhada 'cd ~/ararinhaazullinux && \
  { ninhada fontes --travar && ninhada construir toolchain && ninhada construir zlib; } 2>&1 | tee ~/toolchain.log; \
  echo; echo FIM: \$(date); exec bash'"

cat <<FIM

================================================================
 Tudo rodando na VM $NOME ($IP).

 Acompanhar ao vivo (Ctrl+B depois D para sair sem parar nada):
   ssh $USUARIO@$IP -t tmux attach -t ninhada

 Ver só o final do log:
   ssh $USUARIO@$IP tail -n 40 toolchain.log
================================================================
FIM
