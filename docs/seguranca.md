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

## Secure Boot com chave própria

O kernel do Ararinha já é um único `.efi` (EFI stub com a linha de
comando e, no disco criptografado, a initramfs embutidas), então assinar
esse arquivo protege a cadeia inteira até a raiz.

No instalador, quando o firmware está em modo de configuração (Setup
Mode), aparece "Ativar o Secure Boot com a chave deste computador":

- três chaves RSA-3072 (PK, KEK e db) são geradas no próprio computador e
  ficam em `/etc/ararinha/secureboot` (pasta 0700 do root, dentro do disco
  criptografado quando ele é criptografado). Ninguém mais tem essas chaves;
- o kernel da ESP é assinado com a chave db (`systemd-sbsign`);
- as chaves são cadastradas no firmware pelo efivarfs, como variáveis
  autenticadas da UEFI: db e KEK são acrescentados aos que já existirem
  (placas de vídeo com ROM assinada pela Microsoft continuam funcionando)
  e o PK vai por último, o que liga o Secure Boot;
- se algo falhar, a instalação termina sem Secure Boot e avisa.

Com o Secure Boot ligado o kernel entra em lockdown "integrity" e só
carrega módulos assinados (seção abaixo).

Como conferir: Configurações > Sobre mostra "Secure Boot: Ligado, com a
chave deste computador". No terminal, `od -An -tx1
/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c`
termina em `01`, e `sudo openssl x509 -noout -subject -in
/etc/ararinha/secureboot/db.crt` mostra o certificado deste computador.

Para pôr o firmware em Setup Mode: na configuração da UEFI (F2/Del ao
ligar), em Secure Boot, "Clear/Delete all keys" ou "Reset to Setup Mode".
Numa VM do Proxmox: OVMF com "Pre-Enroll keys" desmarcado.

Ainda falta: assinar de novo o kernel quando ele for atualizado (entra com
as atualizações do sistema).

## Módulos do kernel assinados

Cada módulo do Spix Kernel é assinado (SHA-512) na construção, com uma
chave que o próprio kernel gera e que a receita apaga em seguida: nem quem
constrói o Ararinha guarda a chave privada. Com o Secure Boot ligado, o
kernel entra sozinho em lockdown "integrity" e recusa módulos sem essa
assinatura.

## Serviço do sistema (no lugar de setuid)

`ararinha-sistema` é um serviço root pequeno, ativado pelo systemd só
quando alguém pede (socket `/run/ararinha/sistema.sock`) e encerrado
depois de 60 s parado. Cada pedido é uma linha; o serviço sabe quem pediu
pelo `SO_PEERCRED` (uid e pid informados pelo kernel, impossíveis de
falsificar), exige o grupo wheel para ações de administrador e registra
cada pedido no journal (`journalctl -u ararinha-sistema`). Ele mesmo roda
confinado: `ProtectSystem=strict` (só escreve em `/etc/ararinha`), sem
dispositivos, só `CAP_NET_ADMIN`, só sockets Unix e netlink.

Hoje cuida do firewall (perfis, portas, ligar/desligar), do plano de
energia (o `ararinha-energia` deixou de ser setuid), de desligar,
reiniciar, dormir e hibernar (`CAP_SYS_BOOT`; qualquer um com sessão
aberta pode), do brilho da tela, das contas (criar, apagar,
administrador/padrão: só administradores; a senha nova chega em hex e
some do log), do fuso e da hora pela internet, e de abrir e ejetar
pendrives (`/run/media/<usuário>`, `nosuid,nodev`, arquivos FAT/exFAT/NTFS
só do dono; o disco do sistema nunca aparece como removível).

Conta padrão pedindo algo de administrador: abre a janela "Permitir esta
alteração?" (`ararinha-admin`, como o UAC), que mostra a ação e os
administradores e pede a senha de um deles. Quem confere é o serviço
(`/etc/shadow`, por isso `CAP_DAC_READ_SEARCH`), e cada senha errada
atrasa a resposta (2 s, dobrando até 30 s). O journal registra
`[como <login>]` junto do pedido, nunca a senha.

O que precisa escrever fora de `/etc/ararinha` (useradd, timedatectl,
mount, blkid) o serviço não faz: ele confere o pedido e entrega o comando
exato ao systemd numa unidade passageira (`systemd-run --wait --pipe`,
sem rede), que aparece no journal. Senhas vão pela entrada padrão, nunca
na linha de comando. Os ajudantes setuid que ainda existem (`ararinha-senha`,
`ararinha-unidade`, `ararinha-instalar`) passam para ele aos poucos.

## Firewall com perfis

Configurações > Firewall mostra se o firewall está ligado e escolhe o tipo
de rede: "pública" (padrão; não responde nem ao ping) ou "de casa"
(responde ao ping). Dá para liberar uma porta TCP/UDP para um programa e
bloqueá-la de novo. O estado fica em `/etc/ararinha/firewall.conf` e as
regras extras em `/etc/ararinha/firewall.nft`, incluído pelo
`/etc/nftables.conf`.

## Serviços endurecidos e sem core dumps

- `nftables.service` e `ararinha-energia.service` rodam com
  `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, `NoNewPrivileges`,
  `SystemCallFilter=@system-service`, `MemoryDenyWriteExecute` e só as
  capacidades de que precisam (o firewall: `CAP_NET_ADMIN`; o de energia:
  nenhuma, e sem rede).
- Nenhum programa gera core dump (`Storage=none` no systemd-coredump e
  `DefaultLimitCORE=0`): a memória de um programa que travou pode ter
  senhas ou chaves.

## Chaveiro e área de transferência

- `ararinha-chaveiro` implementa o Secret Service (o que Chromium, Steam e
  outros usam para guardar senhas). Os segredos ficam num arquivo 0600 na
  pasta da conta; ainda sem cifra própria — quem protege é o disco
  criptografado e a sessão bloqueada. Cifrar com a senha da conta está na
  lista.
- O histórico do Win+V guarda só texto, até 25 itens, só na memória da
  Plumagem: some ao sair da sessão e nunca vai para o disco.

## Firewall desligado

Desligar o firewall pede confirmação com aviso de risco; o estado fica no
marcador `/etc/ararinha/firewall-desligado` (o `nftables.service` não sobe
enquanto ele existir) e as Configurações mostram o aviso em vermelho.

## Tela de bloqueio

Depois de 3 senhas erradas, espera crescente: 5 s, dobrando a cada erro
até 5 minutos.

## Bloqueio automático

A tela bloqueia sozinha depois de 5 minutos sem teclado nem mouse
(Configurações > Energia: 1, 5, 10, 15, 30 min ou nunca) e sempre antes
de suspender (gancho em `/usr/lib/systemd/system-sleep`, que avisa a
Plumagem pelo FIFO `$XDG_RUNTIME_DIR/plumagem-ctl`, só do usuário).

## Auditoria das crates

Os workflows Apps e Plumagem rodam clippy, `cargo audit` (base RustSec) e
contam os blocos `unsafe`; o resumo fica em cada run do CI. Primeira
rodada (11/11):

- `memmap2` 0.8 (RUSTSEC-2026-0186), puxada pela crate `xkbcommon` que o
  smithay-client-toolkit usa para mapear o mapa de teclado que a Plumagem
  envia. Sai quando o smithay-client-toolkit subir de versão; o dado
  mapeado vem só do nosso compositor.
- `cgmath` (RUSTSEC-2026-0197, `swap_columns` com índices iguais): a
  Plumagem não chama essa função.
- Crates sem manutenção, sem falha conhecida: `ttf-parser`, `paste`,
  `cgmath`, `zbus_names`, `zvariant`.

## A seguir

- Atualizações automáticas dos apps do Flathub.
- Secure Boot com chave própria e kernel assinado (UKI).
