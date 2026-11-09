# Guia: máquina de build no Proxmox

A Ninhada compila o Ararinha inteiro a partir do código-fonte. Isso leva horas,
então usamos uma VM dedicada no Proxmox do Davi.

## Recursos
- **CPU:** todos os núcleos do servidor, mas com **prioridade baixa**
  (`cpuunits 50`, o padrão é 100). As outras VMs e containers (câmeras,
  Vaultwarden etc.) sempre ganham a disputa; o build usa só o que sobra.
- **RAM:** metade da memória do servidor, no máximo 32 GB, sem balloon.
- **Disco:** 250 GB (fontes, toolchains das duas arquiteturas, pacotes, imagens).
- Não inicia sozinha com o servidor (`onboot 0`).

## 1. Criar a VM (no shell do Proxmox)
```sh
curl -fsSLO https://raw.githubusercontent.com/akkotfpn/ararinhaazullinux/main/ferramentas/proxmox/criar-vm-build.sh
bash criar-vm-build.sh
# ou ajustando: VMID=9100 RAM_GB=24 ARMAZENAMENTO=local-zfs bash criar-vm-build.sh
```

## 2. Preparar a VM
```sh
ssh davi@<ip-da-vm>
git clone https://github.com/akkotfpn/ararinhaazullinux
sudo ararinhaazullinux/ferramentas/preparar-hospedeiro.sh
```

## 3. Construir a toolchain
```sh
cd ararinhaazullinux
ninhada fontes --travar
ninhada construir toolchain
ninhada construir toolchain --arq aarch64
ninhada construir zlib            # primeiro pacote .pena
```

### Conferindo os hashes travados
`fontes --travar` grava em `receitas/fontes.trava` o hash do que foi baixado.
Antes do commit, compare cada linha com o checksum oficial:
- GNU (binutils, gcc, glibc, gmp, mpfr, mpc): baixe também o `.sig` e confira com
  `gpg --verify` usando o chaveiro GNU.
- Kernel: `sha256sums.asc` em cdn.kernel.org.
- LFS (patches e pacotes): página "All Packages" do livro traz os MD5.

## 4. Testar imagens (a partir do fim da Fase 1)
`ferramentas/proxmox/importar-imagem.sh` (a escrever junto com a geração de
imagem) cria uma VM UEFI de teste a partir do `.qcow2` gerado.

## 5. CI (opcional)
Para o GitHub rodar os builds pesados nesta VM, registre um runner próprio:
GitHub → repositório → Settings → Actions → Runners → *New self-hosted runner*,
com o rótulo `ninhada`. O CI leve (testes da Ninhada e validação das receitas)
já roda nos servidores do GitHub.
