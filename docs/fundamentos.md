# Fundamentos: o que os grandes livros ensinam ao Ararinha

Este documento registra as ideias extraídas da biblioteca de referência do projeto
e mostra **onde cada ideia vai parar no sistema**. Toda decisão de arquitetura
(ver [ADRs](adr/)) deve apontar para pelo menos um item daqui.

> Nota de honestidade: este é um estudo dos princípios dessas obras, aplicados ao
> nosso projeto. Não reproduz trechos dos livros. Quem quiser se aprofundar deve
> ler os originais; todos valem cada página.

Legenda dos componentes: **Spix** (kernel), **Ninhada** (build), **Pena** (pacotes),
**Muda** (atualizações), **Revoada** (serviços), **Olho** (hardware), **Bando**
(compatibilidade), **Ninho** (arquivos), **Plumagem** (shell gráfico),
**Caatinga** (configurações), **Primeiro Voo** (instalador), **Asa Guardiã**
(firewall), **Rasante** (jogos).

---

## 1. Sistemas operacionais: os clássicos

**Tanenbaum & Bos, *Modern Operating Systems* / Tanenbaum & Woodhull, *OS: Design and Implementation* (MINIX)**
1. *Separar mecanismo de política.* O kernel oferece o mecanismo; quem decide é o
   espaço do usuário. → Olho, Rasante e Asa Guardiã são **políticas** em userspace
   sobre mecanismos do kernel (udev, cpufreq, nftables). Nada de patch de kernel
   para regra de negócio.
2. *Componentes pequenos e isolados falham sozinhos.* A lição do MINIX vale fora do
   kernel: cada serviço nosso roda como processo separado, com privilégio mínimo,
   reiniciado automaticamente se cair. → Revoada.
3. *Abstração é o produto.* Um SO existe para esconder a bagunça do hardware atrás
   de abstrações limpas (processo, arquivo, espaço de endereçamento). → As
   "unidades C: D:" do Ninho são exatamente isso: uma abstração amigável sobre
   partições, subvolumes e compartilhamentos de rede.

**Silberschatz, Galvin & Gagne, *Operating System Concepts***
1. *Escalonamento é troca entre vazão e latência.* Desktop e jogos pedem latência.
   → Spix usa preempção total e o escalonador ajustado para interatividade;
   Rasante prioriza o processo do jogo.
2. *Memória virtual e paginação definem a sensação de fluidez.* → zram como swap
   padrão, ajuste de `swappiness` e prevenção de travamento por falta de memória
   (OOM em userspace) na Revoada.
3. *Deadlock se evita por desenho, não por sorte.* → Ordem fixa de bloqueios no
   índice do Ninho; nenhuma operação de arquivo segura dois locks globais.

**Remzi & Andrea Arpaci-Dusseau, *Operating Systems: Three Easy Pieces* (OSTEP)**
1. *Os três pilares: virtualização, concorrência, persistência.* Usamos isso como
   mapa do próprio projeto: Bando (virtualização de ambientes), Revoada e Plumagem
   (concorrência), Ninho e Muda (persistência).
2. *Crash consistency.* Todo arquivo pode ser cortado no meio por uma queda de
   energia. → Muda nunca sobrescreve o sistema: escreve uma nova versão e troca um
   ponteiro atômico. Configurações são gravadas com "escreve em temporário, `fsync`,
   renomeia".
3. *Medir antes de otimizar.* → Todo ajuste de desempenho do Rasante precisa de
   benchmark reproduzível no repositório.

**Cox, Kaashoek & Morris, *xv6***
1. *Um sistema pequeno o bastante para caber na cabeça.* → O núcleo do Ninhada deve
   ser lido em uma tarde. Complexidade vai para receitas, não para o motor.
2. *Código como documentação.* → Comentários explicam o "porquê"; o "como" fica
   legível no próprio código.

**Maurice J. Bach, *The Design of the UNIX Operating System* / *Lions' Commentary on UNIX 6th Edition***
1. *Poucas chamadas de sistema, muito bem escolhidas.* → As APIs dos nossos serviços
   (via D-Bus/Varlink) começam mínimas; cada método novo exige justificativa.
2. *O buffer cache e o inode separam nome de dado.* → O Ninho separa **onde o
   arquivo aparece** (unidades, pastas inteligentes) de **o que o arquivo é**
   (conteúdo + metadados). Um arquivo pode aparecer em vários lugares sem ser copiado.
3. *Ler código real é a melhor escola.* → Todo colaborador novo começa lendo um
   componente pequeno de ponta a ponta.

**McKusick, Neville-Neil & Watson, *Design and Implementation of FreeBSD***
1. *Base system coeso.* O FreeBSD entrega kernel e userland como um conjunto
   versionado junto. → A imagem base do Ararinha é um artefato único e versionado,
   não um monte de pacotes soltos. (Essa é a semente do ADR 0006.)
2. *Jails como isolamento leve.* → Inspiração para os ambientes isolados do Bando.

**Wirth & Gutknecht, *Project Oberon***
1. *Duas pessoas podem construir um sistema inteiro se cortarem o supérfluo.*
   → Regra do projeto: cada recurso precisa responder "quem usa isso?". Se ninguém,
   sai do roteiro.
2. *Texto como interface.* Comandos clicáveis no próprio texto. → Inspiração para a
   busca universal da Plumagem (digitar "mapear rede" executa a ação).

**Rob Pike et al., *Plan 9 from Bell Labs***
1. *Tudo é arquivo, inclusive recursos remotos.* → Unidade de rede é só mais uma
   unidade no Ninho; o usuário não precisa saber se é SMB, NFS, SFTP ou WebDAV.
2. *Namespaces por processo.* Cada processo pode ver uma árvore diferente.
   → É exatamente o que o Bando faz: cada app enxerga só as pastas que lhe foram
   dadas (via namespaces de montagem do Linux).
3. *Protocolo único e simples (9P).* → Nossos serviços conversam por um único
   barramento padronizado, com mensagens pequenas e tipadas.

---

## 2. Linux por dentro

**Robert Love, *Linux Kernel Development***
1. *O kernel é configurável ao extremo; configure com propósito.* → A config do
   Spix é dividida em fragmentos comentados (`base`, `desktop`, `jogos`, `vm`,
   `arm64`), cada opção com motivo escrito.
2. *Preempção e granularidade de timer afetam a resposta da interface.*
   → `PREEMPT_DYNAMIC` com modo `full` por padrão no desktop.

**Bovet & Cesati, *Understanding the Linux Kernel***
1. *O VFS é a cola entre sistemas de arquivos.* → O Ninho não inventa um sistema de
   arquivos em disco; ele vive acima do VFS, usando Btrfs, NTFS (ntfs3), exFAT e
   protocolos de rede de forma uniforme.
2. *Page cache e writeback mandam na velocidade de cópia.* → Cópias grandes no
   gerenciador de arquivos usam `copy_file_range`/reflink quando possível (cópia
   instantânea no Btrfs).

**Corbet, Rubini & Kroah-Hartman, *Linux Device Drivers***
1. *O driver anuncia o que suporta (tabelas de IDs) e o kernel casa com o hardware.*
   → O Olho de Spix usa os `modalias` para saber, **antes** de instalar, qual driver
   e firmware cada peça precisa.
2. *Firmware é carregado separado do driver.* → Pacotes de firmware são divididos por
   fabricante para a ISO caber num pendrive e ainda trazer o essencial offline.

**Michael Kerrisk, *The Linux Programming Interface***
1. *Conheça as primitivas: namespaces, cgroups, seccomp, inotify/fanotify.*
   → Bando (isolamento), Rasante (cgroups para priorizar jogos), Ninho
   (fanotify para indexar mudanças em tempo real).
2. *Tratamento de erro não é opcional.* → Em Rust, todo `Result` tratado; nenhum
   `unwrap()` em código de sistema fora de testes.

**Brian Ward, *How Linux Works***
1. *O caminho do boot é uma cadeia: firmware → bootloader → kernel → initramfs →
   init → sessão.* → A Fase 1 constrói exatamente essa cadeia, elo por elo, e
   documenta cada um.
2. *Usuário comum não deveria ver essa cadeia.* → Boot silencioso com animação da
   ave; detalhes só com uma tecla.

**Beekmans et al., *Linux From Scratch* / *Beyond Linux From Scratch***
1. *Toolchain temporária isolada do sistema hospedeiro.* → A Ninhada constrói
   primeiro uma toolchain cruzada própria, para o resultado não depender da
   máquina de build. É isso que nos torna "não baseados em distro".
2. *Ordem de build importa (dependências circulares de glibc/gcc).* → A Ninhada
   resolve a ordem pelas dependências declaradas nas receitas.
3. *O LFS é manual; o nosso precisa ser automático e reproduzível.* → Receitas
   declarativas, hashes de fontes fixados, builds em ambiente limpo.

**Gentoo Handbook e Arch Wiki**
1. *Documentação prática é parte do produto.* → Nossa wiki em pt-BR e inglês é
   entregável de cada fase.
2. *Flags de compilação por perfil (USE flags).* → A Ninhada tem perfis
   (`x86_64-v3`, `arm64-generico`) para otimizar sem perder compatibilidade.

---

## 3. Unix: filosofia e programação

**Kernighan & Pike, *The UNIX Programming Environment* / Eric S. Raymond, *The Art of Unix Programming***
1. *Faça uma coisa bem feita.* → Pena instala pacotes; Muda aplica atualizações;
   Olho detecta hardware. Nenhum faz o trabalho do outro.
2. *Regra da representação: coloque o conhecimento nos dados.* → Regras de drivers,
   perfis de compatibilidade do Bando e temas são **arquivos de dados** (TOML),
   não código.
3. *Regra do silêncio: quando nada de interessante acontece, não diga nada.*
   → Notificações (Grito) só para o que exige ação.
4. *Regra da reparação: quando falhar, falhe alto e cedo.* → O painel "Por que isso
   não abriu?" do Bando.

**W. Richard Stevens & Stephen Rago, *Advanced Programming in the UNIX Environment***
1. *Processos, sinais e descritores são a base de tudo.* → Serviços nossos tratam
   `SIGTERM` corretamente e liberam recursos; desligar o PC nunca corrompe estado.

**Kernighan & Ritchie, *The C Programming Language* / Klabnik & Nichols, *The Rust Programming Language***
1. *C é a língua franca do sistema.* → Integração com kernel e bibliotecas
   existentes continua em C quando necessário.
2. *Segurança de memória elimina classes inteiras de falhas.* → Todo componente novo
   em Rust.

**William Shotts, *The Linux Command Line***
1. *O terminal é poderoso, mas não pode ser obrigatório.* → Toda tarefa comum tem
   caminho gráfico. O terminal é opção, nunca requisito.

---

## 4. Os "outros mundos" que vamos executar

**Russinovich, Solomon, Ionescu et al., *Windows Internals***
1. *O Windows é organizado em subsistemas sobre um núcleo.* → O Bando é a nossa
   versão disso: "subsistemas" Windows (Wine/Proton), Android (Waydroid) e Mac
   (Darling) sobre o kernel Linux.
2. *Letras de unidade são uma camada de nomes (o Object Manager), não o disco.*
   → Confirma o desenho do Ninho: `C:` e `D:` são **nomes**, mapeados para
   volumes reais. Mapear unidade de rede é apenas registrar mais um nome.
3. *Registro e perfis de usuário.* → Cada app Windows ganha um prefixo próprio com
   pastas do usuário redirecionadas para as pastas reais do Ararinha.

**Jonathan Levin, **OS Internals* / Amit Singh, *Mac OS X Internals***
1. *Apps de Mac dependem de frameworks da Apple (Cocoa, Metal).* → Explica por que
   suporte a apps gráficos de Mac é limitado: não podemos distribuir esses
   frameworks e as reimplementações livres ainda estão no começo. Marcado como
   experimental.

**Karim Yaghmour, *Embedded Android***
1. *Android é um userspace inteiro (init, binder, ART) sobre um kernel Linux.*
   → Waydroid funciona porque roda esse userspace num container, compartilhando o
   nosso kernel. Precisamos dos módulos `binder` no Spix.

**Documentação do Wine, Proton/DXVK/VKD3D, Waydroid e Darling**
1. *Tradução de API, não emulação de CPU.* → Desempenho próximo do nativo em x86_64.
2. *No ARM64, executáveis x86 precisam de tradução de CPU (FEX-Emu/Box64).*
   → Jogos Windows em ARM terão desempenho menor; documentado em
   [limites.md](limites.md).

---

## 5. Hardware, compiladores e baixo nível

**Bryant & O'Hallaron, *Computer Systems: A Programmer's Perspective***
1. *A hierarquia de memória domina o desempenho.* → Índice do Ninho projetado para
   caber em cache: estruturas compactas, acesso sequencial.
2. *Linkagem e carregamento definem o que um binário precisa.* → Pena calcula
   dependências lendo os binários (bibliotecas dinâmicas), não só o que a receita
   declara.

**Patterson & Hennessy, *Computer Organization and Design***
1. *Arquiteturas diferentes, mesmos princípios.* → O sistema inteiro nasce para
   x86_64 **e** ARM64 desde o primeiro commit (ADR 0007).

**John R. Levine, *Linkers and Loaders***
1. *Um executável é um formato (ELF, PE, Mach-O) que um carregador interpreta.*
   → O Bando identifica o formato pelos primeiros bytes (`MZ`/PE, ELF, Mach-O, ZIP
   com `AndroidManifest.xml`) antes de olhar a extensão.
2. *binfmt_misc permite ao kernel delegar formatos a um interpretador.* → Executar
   um `.exe` pelo terminal chama o Bando de forma transparente.

**Aho, Lam, Sethi & Ullman, *Compilers* / Igor Zhirkov, *Low-Level Programming***
1. *Otimização é guiada pelo alvo.* → Perfis `x86_64-v3` para máquinas novas e
   genérico para antigas, escolhidos na instalação.

**Especificações UEFI, ACPI e PCI**
1. *DMI/SMBIOS diz o fabricante e modelo; PCI/USB dizem as peças.* → Base do Olho de
   Spix para identificar "Lenovo ThinkPad T14 Gen 3" e aplicar ajustes específicos.

---

## 6. Sistemas de arquivos e dados (o coração do Ninho)

**Dominic Giampaolo, *Practical File System Design with the Be File System***
1. *Atributos estendidos indexados transformam o sistema de arquivos em banco de
   dados.* → Cada arquivo no Ninho tem atributos (tipo real, origem, tags, projeto)
   em xattrs **e** num índice consultável.
2. *Consultas ao vivo (live queries).* → **Pastas inteligentes** que se atualizam
   sozinhas ("Instaladores baixados", "Fotos de 2026").
3. *Journaling de metadados.* → Índice com log de escrita antecipada; nunca
   corrompe numa queda de energia.

**Martin Kleppmann, *Designing Data-Intensive Applications***
1. *O log é a fonte da verdade; índices são derivados e reconstruíveis.* → Se o
   índice do Ninho se perder, ele é reconstruído do disco. Nunca é o único lugar
   onde um dado existe.
2. *Consistência eventual com o usuário no centro.* → A busca pode atrasar alguns
   segundos após uma cópia, mas a pasta mostra o arquivo na hora.

**Btrfs e ZFS**
1. *Copy-on-write dá snapshots baratos.* → Linha do tempo de pastas e rollback do
   sistema (Muda).
2. *Checksums detectam corrupção silenciosa.* → Btrfs com verificação periódica
   (scrub) agendada.
3. *Reflinks permitem cópia instantânea e deduplicação.*

**Haiku, Tracker (GNOME) e Baloo (KDE)**
1. *Indexação precisa ser invisível: baixa prioridade, pausa com bateria fraca ou
   jogo aberto.* → Ninho usa cgroups de baixa prioridade e pausa quando o Rasante
   está ativo.
2. *O erro clássico é indexar tudo.* → Por padrão só as pastas do usuário; sistema
   e prefixos de apps ficam de fora.

---

## 7. Redes e segurança

**Kurose & Ross, *Computer Networking* / Stevens, *TCP/IP Illustrated***
1. *Camadas bem separadas.* → Canto (enlace, Wi-Fi, cabo), Asa Guardiã (filtro),
   Rota Migratória (túnel) são componentes separados que se comunicam.
2. *Diagnóstico de rede deve ser legível.* → Botão "Testar minha internet" que
   checa link, DHCP, DNS e rota, e explica em português o que falhou.

**Ross Anderson, *Security Engineering***
1. *Segurança que atrapalha o usuário é desligada pelo usuário.* → Firewall com
   perfis simples (Casa, Rede pública, Trabalho), sem jargão.
2. *Defesa em profundidade.* → Secure Boot + sistema imutável + sandbox de apps +
   firewall + criptografia de disco.
3. *O atacante procura o elo mais fraco, muitas vezes a atualização.* → Pacotes e
   imagens assinados; Muda recusa qualquer coisa sem assinatura válida.

**Jason Donenfeld, *WireGuard***
1. *Pouco código é mais seguro.* → WireGuard é o padrão da Rota Migratória.

---

## 8. Desempenho e observabilidade

**Brendan Gregg, *Systems Performance* / *BPF Performance Tools***
1. *Método USE: Utilização, Saturação, Erros para cada recurso.* → O monitor do
   Rasante mostra CPU, GPU, RAM, disco e rede nesses três termos, traduzidos para
   linguagem simples.
2. *eBPF permite observar sem atrapalhar.* → Monitoramento com custo quase zero.
3. *Flame graphs explicam onde o tempo vai.* → Ferramenta de diagnóstico para
   desenvolvedores do projeto.

---

## 9. Interface e experiência do usuário

**Don Norman, *The Design of Everyday Things***
1. *Affordances e mapeamentos naturais.* → O gerenciador de arquivos imita o que o
   usuário já conhece (Este Computador, C:, D:) em vez de ensinar algo novo.
2. *Feedback imediato.* → Toda ação mostra resposta em menos de 100 ms, mesmo que
   seja só "trabalhando nisso".
3. *Erros são culpa do design, não do usuário.* → Desfazer em tudo que for possível
   (lixeira com versões, snapshots).

**Jef Raskin, *The Humane Interface***
1. *Modos confundem.* → Nenhum diálogo que bloqueia a tela inteira, exceto
   confirmação de ações destrutivas.
2. *Hábitos são poderosos.* → Atalhos de teclado do Windows funcionam (Win+E,
   Win+D, Alt+Tab, Ctrl+Shift+Esc).

**Steve Krug, *Don't Make Me Think***
1. *Se precisa de explicação, está errado.* → Teste de usabilidade com pessoas
   não técnicas (família) antes de cada versão.

**Drew DeVault, *The Wayland Protocol***
1. *No Wayland, o compositor é o servidor de exibição.* → A Plumagem é um
   compositor próprio, não um tema sobre o de outro projeto.
2. *Protocolos de extensão (layer-shell, xdg-decoration).* → Painel, menu e
   notificações usam protocolos padrão para apps de terceiros funcionarem.

**Especificações freedesktop.org**
1. *Padrões compartilhados garantem compatibilidade.* → Ícones, `.desktop`, pastas
   XDG e temas seguem os padrões; apps de outros sistemas Linux aparecem certinhos.

---

## 10. Gestão de um projeto deste tamanho

**Frederick Brooks, *The Mythical Man-Month***
1. *Integridade conceitual vem de poucas cabeças.* → Davi decide o produto; ADRs
   registram o porquê.
2. *Planeje jogar um fora.* → A Fase 1 é deliberadamente simples; vamos reescrever
   partes.
3. *Não existe bala de prata.* → Cronograma em fases com entregas testáveis.

**Karl Fogel, *Producing Open Source Software***
1. *Documentação e processo de contribuição antes de pedir contribuição.*
   → `CONTRIBUINDO.md`, código de conduta e issues bem descritas.
2. *Decisões em público.* → ADRs e discussões no próprio repositório.

---

## 11. Autores independentes

**Philipp Oppermann, *Writing an OS in Rust*** — Rust consegue ir até o metal; o
ecossistema de crates de baixo nível é maduro. → Confiança para usar Rust em
compositor, init helpers e serviços.

**Nick Blundell, *Writing a Simple OS from Scratch* / Tu Do, *Operating Systems: From 0 to 1* / James Molloy / OSDev Wiki**
→ Entender boot, modo protegido e interrupções nos dá base para depurar problemas
de boot em hardware real e escrever o nosso próprio carregador de boot no futuro,
se fizer sentido.

**SerenityOS (Andreas Kling)** — Um sistema com identidade forte nasce de **gosto
consistente**: um estilo visual, aplicado em tudo, do boot ao relógio. → Guia de
identidade da Plumagem e regra de que nenhum app padrão destoa.

**Haiku, elementary OS e SteamOS**
- Haiku: velocidade percebida e coerência.
- elementary: guia de interface humana escrito antes do código.
- SteamOS: imutável + Proton + modo console = jogos sem dor. → Muda e Rasante.

---

## Resumo: os 10 mandamentos do Ararinha

1. O kernel oferece mecanismo; o Ararinha oferece política.
2. O sistema nunca se corrompe: atualizações atômicas, sempre com volta.
3. O usuário vê o que já conhece; a inteligência fica por baixo.
4. Todo app abre ou explica claramente por que não abriu.
5. O hardware se configura sozinho.
6. Conhecimento mora em dados (TOML), não em código.
7. Cada serviço faz uma coisa, com privilégio mínimo.
8. Medir antes de otimizar.
9. Português e inglês, desde o primeiro dia.
10. Nenhuma promessa falsa.
