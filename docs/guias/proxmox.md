# Guia: máquina de build no Proxmox

A Ninhada compila o Ararinha inteiro a partir do código-fonte. Isso leva horas,
então usamos uma VM dedicada no Proxmox do Davi.

## Recursos
- **CPU:** todos os núcleos do servidor, mas com **prioridade baixa**
  (`cpuunits 50`, o padrão é 100). As outras VMs e containers (câmeras,
  Vaultwarden etc.) sempre ganham a disputa; o build usa só o que sobra.
- **RAM:** metade da memória do servidor, no máximo 32 GB, sem balloon. A Ninhada
  limita as tarefas paralelas a 1 por GB de RAM para não faltar memória.
- **Disco:** até 150 GB, nunca mais que 70% do espaço **realmente livre** no
  armazenamento. Em LVM-thin os discos são reservas "prometidas"; se o pool
  encher de verdade, todas as VMs podem travar.
- O agente do QEMU é instalado no primeiro boot (cloud-init), e o script espera
  a VM ganhar IP e já mostra o comando `ssh`.
- Não inicia sozinha com o servidor (`onboot 0`).

## 1. Criar a VM e começar a compilar (tudo no shell do Proxmox)
```sh
git clone https://github.com/akkotfpn/ararinhaazullinux   # ou: git -C ararinhaazullinux pull
bash ararinhaazullinux/ferramentas/proxmox/criar-vm-build.sh
```
O script cria a VM, espera ela subir, entra nela por ssh, instala o necessário,
compila a Ninhada e deixa a toolchain compilando dentro de um `tmux`.
Você não precisa entrar na VM.

Ajustes opcionais: `VMID=9101 RAM_GB=8 DISCO_GB=120 ARMAZENAMENTO=local-zfs bash …`
e `PREPARAR=0` para só criar a VM.

> Use o `git clone`/`git pull` em vez de baixar o script pelo
> raw.githubusercontent.com: aquele endereço guarda versões antigas em cache por
> alguns minutos depois de cada atualização.

## 2. Acompanhar
```sh
ssh davi@<ip-da-vm> -t tmux attach -t ninhada   # ao vivo; Ctrl+B depois D para sair
ssh davi@<ip-da-vm> tail -n 40 toolchain.log      # só o final
```

## 3. Construir manualmente (dentro da VM)
```sh
cd ~/ararinhaazullinux
ninhada fontes --travar
ninhada construir toolchain
ninhada construir toolchain --arq aarch64
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
