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

## A seguir

- Criptografia do disco no instalador. Precisa de uma initramfs para pedir
  a senha antes da raiz montar: cryptsetup (LUKS2 + Argon2id) e uma
  initramfs mínima feita pela Ninhada.
- Bloquear sozinho depois de um tempo parado e ao suspender.
- Atualizações automáticas dos apps do Flathub.
- Secure Boot com chave própria e kernel assinado (UKI).
