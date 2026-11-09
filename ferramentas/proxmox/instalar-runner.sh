#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Registra a VM ninhada-build como runner do GitHub Actions (rótulo "ninhada").
# Rodar DENTRO da VM, com sudo, passando o token de registro do GitHub:
#
#   sudo ferramentas/proxmox/instalar-runner.sh <TOKEN>
#
# O token aparece em: repositório → Settings → Actions → Runners →
# New self-hosted runner, na linha "./config.sh --url ... --token XXXXX".
# Ele vale por 1 hora e só serve para registrar.
#
# Segurança: o runner roda como o usuário "ninhada", SEM sudo. O workflow que
# usa este runner só dispara por push no main ou manualmente; pull requests
# de fora nunca rodam aqui.

set -euo pipefail

TOKEN="${1:-}"
REPO_URL="https://github.com/akkotfpn/ararinhaazullinux"
USUARIO=ninhada
CASA="/home/$USUARIO"
PASTA="$CASA/runner"

if [[ -d /etc/pve ]]; then
  echo "Isto é o Proxmox (host). Rode dentro da VM: ssh davi@<ip-da-vm>" >&2; exit 1
fi
[[ $EUID -eq 0 ]] || { echo "Rode com sudo." >&2; exit 1; }
[[ -n "$TOKEN" ]] || { echo "Uso: sudo $0 <TOKEN>" >&2; exit 1; }

echo "== Usuário '$USUARIO' sem privilégios de administrador"
id "$USUARIO" &>/dev/null || useradd --create-home --shell /bin/bash "$USUARIO"
mkdir -p "$CASA/saida"
chown "$USUARIO:$USUARIO" "$CASA/saida"

echo "== Baixando a versão mais recente do runner"
VERSAO=$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest \
         | grep -oP '"tag_name":\s*"v\K[^"]+')
case "$(uname -m)" in
  x86_64) ARQ=x64 ;;
  aarch64) ARQ=arm64 ;;
  *) echo "Arquitetura não suportada pelo runner: $(uname -m)" >&2; exit 1 ;;
esac
mkdir -p "$PASTA"
curl -fsSL "https://github.com/actions/runner/releases/download/v$VERSAO/actions-runner-linux-$ARQ-$VERSAO.tar.gz" \
  | tar -xz -C "$PASTA"
chown -R "$USUARIO:$USUARIO" "$PASTA"
"$PASTA/bin/installdependencies.sh" >/dev/null

echo "== Registrando no GitHub (versão $VERSAO)"
sudo -u "$USUARIO" bash -c "cd '$PASTA' && ./config.sh --unattended --replace \
  --url '$REPO_URL' --token '$TOKEN' --name ninhada-build --labels ninhada --work _trabalho"

echo "== Instalando como serviço (sobe junto com a VM)"
cd "$PASTA"
./svc.sh install "$USUARIO" >/dev/null
./svc.sh start >/dev/null
./svc.sh status | grep -E "Active:" || true

cat <<FIM

Pronto. A VM aparece como "ninhada-build" em:
  $REPO_URL/settings/actions/runners

As compilações feitas pelo GitHub ficam em $CASA/saida.
FIM
