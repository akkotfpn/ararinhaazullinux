# Segurança do Ararinha Azul Linux

O que já vem ligado, e o que ainda vai chegar. A ideia é a mesma do resto do
sistema: seguro por padrão, sem o usuário precisar configurar nada.

## Já no sistema

| Camada | O que faz |
|---|---|
| Firewall (nftables) | Nada entra sem ter sido pedido: só respostas de conexões que o computador abriu, ping e DHCP. Tudo o que sai é liberado. Regras em `/etc/nftables.conf`, serviço `nftables.service`. |
| Kernel (Spix) | KASLR, pilha protegida, `usercopy` endurecido, memória zerada ao alocar, `slab` embaralhado, Lockdown, Landlock e Yama. Sem `/dev/mem` e sem BPF para usuários comuns. |
| sysctl | `/usr/lib/sysctl.d/50-ararinha.conf`: endereços do kernel escondidos, `dmesg` só para o administrador, `ptrace` restrito, rede sem redirecionamentos e com proteção contra *SYN flood*. |
| Senhas | Guardadas com yescrypt no `/etc/shadow`. A tela de bloqueio confere pelo ajudante `ararinha-senha`, que só confere a própria conta e espera 2 s a cada erro. |
| Tela de bloqueio | Win+L. Nada chega às janelas enquanto a tela está bloqueada. |
| Apps | Os do Flathub rodam na caixa de areia (bubblewrap + seccomp) e só tocam nos arquivos pelos portais. AppImage baixado pergunta antes de executar. |
| Ajudantes setuid | `ararinha-unidade`, `ararinha-energia`, `ararinha-instalar` e `ararinha-senha`: cada um faz uma coisa só, valida tudo o que recebe e limpa o ambiente. |
| Imagem | Sem compiladores (eles ficam no pacote `ararinha-dev`). Todas as fontes têm hash travado e, quando o projeto publica, assinatura conferida. |

## Nada da construção vai na imagem

O `ferramentas/gerar-imagem.sh` "sela" a imagem antes de montá-la:

- Compiladores, montadores e depuradores (GCC, Clang, Rust, binutils, make,
  autotools), cabeçalhos, bibliotecas estáticas, manuais e a configuração do
  kernel ficam só no pacote `ararinha-dev` (lista em `imagem/pacote-dev.txt`).
- Programas e bibliotecas vão sem símbolos de depuração (`strip`), e os
  nossos são compilados sem os caminhos da máquina de construção.
- Registros, históricos, caches e pastas de trabalho da construção são
  apagados. Identificadores (machine-id, semente aleatória) não vão na
  imagem: cada instalação gera os seus no primeiro boot.
- Clientes de rede antigos e sem criptografia (telnet, ftp, rsh) não vão.

O código-fonte do Ararinha é aberto (GPL-3.0), e isso é proposital: a
segurança não depende de esconder como o sistema funciona, e sim de não
deixar ferramentas e pistas à mão dentro do computador de quem usa.

## Privacidade na rede

- Sem telemetria: o sistema não envia nada para o projeto.
- O computador não anuncia o próprio nome na rede (DHCP anônimo, sem LLMNR
  nem mDNS) e usa endereços IPv6 temporários.
- DNS cifrado (DNS sobre TLS) sempre que o servidor da rede aceitar.
- No Wi-Fi, o endereço físico (MAC) é sorteado.

O que isso não esconde: o provedor de internet continua vendo com quais
sites o computador fala, e os sites veem o IP. Quem precisa esconder isso
usa uma VPN ou o Tor por cima.

## Criptografia do disco

O instalador oferece "Criptografar o disco (recomendado)", ligado por
padrão. Com ele:

- a raiz vira um volume LUKS2 na partição GPT `ararinha-cripto`:
  AES-XTS com chave de 512 bits, cabeçalho com SHA-512 e a senha
  derivada por Argon2id (custo de ~2 s de CPU e até 1 GB de memória por
  tentativa, o que torna adivinhar a senha à força muito caro);
- a senha do disco precisa ter pelo menos 8 caracteres; a tela mostra a
  força dela; ela vai do instalador para o ajudante pela entrada padrão,
  nunca na linha de comando, e é apagada da memória depois de usada;
- a ESP recebe o `vmlinuz-spix-cripto`: o mesmo Spix Kernel, com uma
  initramfs embutida feita pela Ninhada (receita `spix-kernel`). Ela tem só
  bash, cryptsetup, blkid, mount, switch_root, loadkeys (ABNT2) e o
  `kernel/initramfs/init`. Não tem rede nem shell de emergência: se algo
  der errado, espera e reinicia;
- cada senha errada atrasa a próxima tentativa (1 s, 2 s, … até 10 s),
  além do custo do Argon2id;
- o cryptsetup usa a criptografia do próprio kernel (AF_ALG, com AES-NI),
  sem OpenSSL na initramfs.

Sem a senha, o disco é ruído: nada do sistema, das contas ou dos arquivos
pode ser lido tirando o disco do computador.

Para conferir num sistema instalado: `sudo cryptsetup luksDump
/dev/disk/by-partlabel/ararinha-cripto` mostra `luks2`, `aes-xts-plain64`,
512 bits e `argon2id`.

Ainda falta: tela de senha gráfica (hoje é texto no console), TPM2
opcional e trocar a senha do disco pelas Configurações.

## Módulos do kernel assinados

Cada módulo do Spix Kernel é assinado (SHA-512) na construção, com uma
chave que o próprio kernel gera e que a receita apaga em seguida: nem quem
constrói o Ararinha guarda a chave privada. Com o Secure Boot ligado, o
kernel entra sozinho em lockdown "integrity" e recusa módulos sem essa
assinatura.

## Bloqueio automático

A tela bloqueia sozinha depois de 5 minutos sem teclado nem mouse
(Configurações > Energia: 1, 5, 10, 15, 30 min ou nunca) e sempre antes
de suspender (gancho em `/usr/lib/systemd/system-sleep`, que avisa a
Plumagem pelo FIFO `$XDG_RUNTIME_DIR/plumagem-ctl`, só do usuário).

## A seguir

- Atualizações automáticas dos apps do Flathub.
- Secure Boot com chave própria e kernel assinado (UKI).
