// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-instalar: instala o sistema em uso (o do pendrive) num disco.
//!
//! Instalado com setuid root e só obedece a quem é do grupo wheel. Não
//! aceita partições nem o disco onde o sistema está rodando, e exige a
//! palavra de confirmação.
//!
//!   ararinha-instalar sda APAGAR-TUDO [--cripto] [--secureboot]
//!
//! Com `--cripto`, a senha do disco chega pela entrada padrão; a raiz vira
//! um volume LUKS2 (AES-XTS de 512 bits, Argon2id) chamado
//! "ararinha-cripto" e a ESP recebe o kernel com a initramfs que pede a
//! senha no boot.
//!
//! Com `--secureboot` e o firmware em modo de configuração, gera as chaves
//! deste computador, assina o kernel da ESP e cadastra as chaves (ver
//! secureboot.rs). Se não der, avisa ("AVISO ...") e segue sem Secure Boot.
//!
//! Passos: tabela GPT nova (ESP de 256 MiB + raiz "ararinha-raiz"), cópia da
//! ESP do pendrive, ext4 novo na raiz e cópia do sistema com `cp -ax`.
//! Imprime "ETAPA <texto>", "PROGRESSO <0-100>" e, no fim, "PRONTO".

#[allow(dead_code)]
mod discos;
mod secureboot;

use std::{
    fs,
    io::Write,
    process::{exit, Command, Stdio},
    thread,
    time::Duration,
};

extern "C" {
    fn getuid() -> u32;
    fn geteuid() -> u32;
    fn getgroups(n: i32, lista: *mut u32) -> i32;
    fn setresuid(r: u32, e: u32, s: u32) -> i32;
    fn setresgid(r: u32, e: u32, s: u32) -> i32;
    fn sync();
}

#[repr(C)]
struct Statvfs {
    f_bsize: u64,
    f_frsize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_favail: u64,
    f_fsid: u64,
    f_flag: u64,
    f_namemax: u64,
    _reservado: [i32; 6],
}

extern "C" {
    fn statvfs(caminho: *const std::os::raw::c_char, buf: *mut Statvfs) -> i32;
}

fn usado(caminho: &str) -> u64 {
    let c = std::ffi::CString::new(caminho).unwrap();
    let mut s: Statvfs = unsafe { std::mem::zeroed() };
    if unsafe { statvfs(c.as_ptr(), &mut s) } != 0 {
        return 0;
    }
    (s.f_blocks - s.f_bfree) * s.f_frsize
}

fn falhar(msg: &str) -> ! {
    println!("ERRO {msg}");
    let _ = std::io::stdout().flush();
    exit(1);
}

fn etapa(t: &str) {
    println!("ETAPA {t}");
    let _ = std::io::stdout().flush();
}

fn progresso(p: u32) {
    println!("PROGRESSO {}", p.min(100));
    let _ = std::io::stdout().flush();
}

fn rodar(cmd: &mut Command, oque: &str) {
    match cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => falhar(&format!("{oque}: {}", String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => falhar(&format!("{oque}: {e}")),
    }
}

const KERNEL_CRIPTO: &str = "/usr/lib/ararinha/vmlinuz-spix-cripto";
const MAPA: &str = "ararinha-alvo";

/// Roda um comando do cryptsetup passando a senha pela entrada padrão.
fn com_senha(cmd: &mut Command, senha: &[u8], oque: &str) {
    let mut f = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| falhar(&format!("{oque}: {e}")));
    if let Some(mut e) = f.stdin.take() {
        let _ = e.write_all(senha);
    }
    let saida = f.wait_with_output().unwrap_or_else(|e| falhar(&format!("{oque}: {e}")));
    if !saida.status.success() {
        falhar(&format!("{oque}: {}", String::from_utf8_lossy(&saida.stderr).trim()));
    }
}

fn fechar_cripto(cripto: bool) {
    if cripto {
        let _ = Command::new("cryptsetup").args(["close", MAPA]).status();
    }
}

/// Põe na ESP nova o kernel com a initramfs que pede a senha do disco.
fn trocar_kernel_da_esp(esp: &str) {
    let dir = "/run/ararinha-instalador/esp";
    let _ = fs::create_dir_all(dir);
    rodar(Command::new("mount").args([esp, dir]), "mount da ESP");
    let nome = if cfg!(target_arch = "aarch64") { "BOOTAA64.EFI" } else { "BOOTX64.EFI" };
    let destino = format!("{dir}/EFI/BOOT/{nome}");
    let r = fs::copy(KERNEL_CRIPTO, &destino);
    unsafe { sync() };
    let _ = Command::new("umount").arg(dir).status();
    if let Err(e) = r {
        falhar(&format!("kernel na ESP: {e}"));
    }
}

fn do_grupo_wheel() -> bool {
    let wheel = fs::read_to_string("/etc/group")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split(':').collect();
            (c.first() == Some(&"wheel")).then(|| c.get(2).and_then(|g| g.parse::<u32>().ok())).flatten()
        });
    let Some(wheel) = wheel else { return false };
    let mut grupos = [0u32; 256];
    let n = unsafe { getgroups(256, grupos.as_mut_ptr()) };
    n > 0 && grupos[..n as usize].contains(&wheel)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opcoes = args.get(3..).unwrap_or(&[]);
    let cripto = opcoes.iter().any(|o| o == "--cripto");
    let secure_boot = opcoes.iter().any(|o| o == "--secureboot");
    if args.len() < 3 || args[2] != "APAGAR-TUDO" || opcoes.iter().any(|o| o != "--cripto" && o != "--secureboot") {
        eprintln!("uso: ararinha-instalar <disco> APAGAR-TUDO [--cripto] [--secureboot]");
        exit(2);
    }
    let uid = unsafe { getuid() };
    if uid != 0 && !do_grupo_wheel() {
        falhar("Só administradores (grupo wheel) podem instalar o sistema.");
    }
    if unsafe { geteuid() } != 0 {
        falhar("O ajudante precisa ser setuid root.");
    }
    if unsafe { setresgid(0, 0, 0) } != 0 || unsafe { setresuid(0, 0, 0) } != 0 {
        falhar("Não deu para virar root.");
    }
    // Ambiente limpo: nada do usuário passa para os comandos.
    for (k, _) in std::env::vars_os() {
        std::env::remove_var(k);
    }
    std::env::set_var("PATH", "/usr/sbin:/usr/bin:/sbin:/bin");

    // Senha do disco: lida antes de qualquer mudança no disco.
    let mut senha = Vec::new();
    if cripto {
        use std::io::Read;
        let _ = std::io::stdin().take(1024).read_to_end(&mut senha);
        while senha.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            senha.pop();
        }
        if senha.len() < 8 {
            falhar("A senha do disco precisa ter pelo menos 8 caracteres.");
        }
        if !discos::existe(KERNEL_CRIPTO) {
            falhar("Este pendrive não tem o kernel para discos criptografados.");
        }
    }

    let disco = args[1].trim_start_matches("/dev/").to_string();
    if disco.is_empty() || !disco.chars().all(|c| c.is_ascii_alphanumeric()) {
        falhar("Nome de disco inválido.");
    }
    let candidatos = discos::listar();
    let Some(alvo) = candidatos.iter().find(|d| d.nome == disco) else {
        falhar("Esse disco não pode receber o sistema (não existe ou é o disco em uso).");
    };
    if alvo.bytes < 12_000_000_000 {
        falhar("O disco precisa ter pelo menos 12 GB.");
    }
    let Some(origem) = discos::disco_do_sistema() else { falhar("Não achei o disco do sistema em uso.") };
    let esp_origem = format!("/dev/{}", discos::particao(&origem, 1));
    if !discos::existe(&esp_origem) {
        falhar("Não achei a partição EFI do pendrive.");
    }
    // Nada do disco de destino pode estar montado.
    let montagens = fs::read_to_string("/proc/mounts").unwrap_or_default();
    if montagens.lines().any(|l| l.starts_with(&format!("/dev/{disco}"))) {
        falhar("Há partições desse disco em uso. Desconecte-as antes.");
    }

    etapa("Criando as partições");
    progresso(1);
    let tipo_raiz = if cfg!(target_arch = "aarch64") {
        "B921B045-1DF0-41C3-AF44-4C6F280D3FAE"
    } else {
        "4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709"
    };
    let nome_raiz = if cripto { "ararinha-cripto" } else { "ararinha-raiz" };
    let tabela = format!(
        "label: gpt\nsize=256MiB, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name=\"ESP\"\ntype={tipo_raiz}, name=\"{nome_raiz}\"\n"
    );
    let dev = format!("/dev/{disco}");
    let _ = Command::new("wipefs").args(["-a", "-q", &dev]).status();
    let mut sfdisk = Command::new("sfdisk")
        .args(["--quiet", "--wipe", "always", &dev])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| falhar(&format!("sfdisk: {e}")));
    let _ = sfdisk.stdin.take().map(|mut s| s.write_all(tabela.as_bytes()));
    let saida = sfdisk.wait_with_output().unwrap_or_else(|e| falhar(&format!("sfdisk: {e}")));
    if !saida.status.success() {
        falhar(&format!("sfdisk: {}", String::from_utf8_lossy(&saida.stderr).trim()));
    }
    let _ = Command::new("udevadm").arg("settle").status();
    let (p1, p2) = (format!("/dev/{}", discos::particao(&disco, 1)), format!("/dev/{}", discos::particao(&disco, 2)));
    for _ in 0..50 {
        if discos::existe(&p1) && discos::existe(&p2) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !discos::existe(&p2) {
        falhar("As partições novas não apareceram.");
    }

    etapa("Copiando a partição de inicialização (EFI)");
    progresso(3);
    {
        let mut de = fs::File::open(&esp_origem).unwrap_or_else(|e| falhar(&format!("ESP do pendrive: {e}")));
        let mut para = fs::OpenOptions::new().write(true).open(&p1).unwrap_or_else(|e| falhar(&format!("ESP nova: {e}")));
        std::io::copy(&mut de, &mut para).unwrap_or_else(|e| falhar(&format!("cópia da ESP: {e}")));
        let _ = para.sync_all();
    }

    if cripto {
        etapa("Instalando o kernel que abre o disco criptografado");
        trocar_kernel_da_esp(&p1);
        let _ = Command::new("cryptsetup").args(["close", MAPA]).status();
        etapa("Criptografando o disco (LUKS2)");
        progresso(4);
        com_senha(
            Command::new("cryptsetup").args([
                "luksFormat", "--batch-mode", "--type", "luks2",
                "--cipher", "aes-xts-plain64", "--key-size", "512", "--hash", "sha512",
                "--pbkdf", "argon2id", "--iter-time", "2000",
                "--use-urandom", "--label", "ararinha-cripto", "--key-file=-", &p2,
            ]),
            &senha,
            "cryptsetup luksFormat",
        );
        com_senha(
            Command::new("cryptsetup").args(["open", "--allow-discards", "--key-file=-", &p2, MAPA]),
            &senha,
            "cryptsetup open",
        );
        senha.fill(0);
    }
    let raiz = if cripto { format!("/dev/mapper/{MAPA}") } else { p2.clone() };

    etapa("Formatando o disco");
    progresso(5);
    rodar(Command::new("mkfs.ext4").args(["-F", "-q", "-L", "ararinha", &raiz]), "mkfs.ext4");

    etapa("Copiando o sistema (pode levar alguns minutos)");
    let alvo_dir = "/run/ararinha-instalador/alvo";
    let _ = fs::create_dir_all(alvo_dir);
    rodar(Command::new("mount").args([&raiz, alvo_dir]), "mount");
    let total = usado("/").max(1);
    let registro = "/run/ararinha-instalador/copia.log";
    let erros = fs::File::create(registro).unwrap_or_else(|e| falhar(&format!("registro: {e}")));
    let mut cp = Command::new("cp")
        .args(["-ax", "--sparse=always", "/.", alvo_dir])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(erros))
        .spawn()
        .unwrap_or_else(|e| falhar(&format!("cp: {e}")));
    let estado = loop {
        match cp.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                let feito = usado(alvo_dir);
                progresso(8 + (feito as f64 / total as f64 * 88.0) as u32);
                thread::sleep(Duration::from_millis(700));
            }
            Err(e) => falhar(&format!("cp: {e}")),
        }
    };
    if !estado.success() {
        let erro = fs::read_to_string(registro).unwrap_or_default();
        let _ = Command::new("umount").arg(alvo_dir).status();
        fechar_cripto(cripto);
        falhar(&format!("cópia do sistema: {}", erro.lines().last().unwrap_or("").trim()));
    }

    etapa("Terminando");
    progresso(97);
    // Pastas que são de sistema de arquivos virtuais ficam vazias; limpa o
    // que é só do pendrive.
    for d in ["tmp", "run", "var/tmp"] {
        let p = format!("{alvo_dir}/{d}");
        if let Ok(rd) = fs::read_dir(&p) {
            for e in rd.flatten() {
                let _ = if e.path().is_dir() { fs::remove_dir_all(e.path()) } else { fs::remove_file(e.path()) };
            }
        }
    }
    if cripto {
        // A raiz agora é o volume aberto pela initramfs.
        let _ = fs::write(
            format!("{alvo_dir}/etc/fstab"),
            "# Ararinha Azul Linux — raiz criptografada (LUKS2), aberta no boot\n/dev/mapper/raiz  /  ext4  defaults  1 1\n",
        );
        // A partição já ocupa o disco todo; o repart não tem o que crescer.
        let _ = fs::remove_file(format!("{alvo_dir}/usr/lib/repart.d/50-raiz.conf"));
    }
    if secure_boot {
        etapa("Ativando o Secure Boot com a chave deste computador");
        progresso(98);
        if !secureboot::modo_configuracao() {
            println!("AVISO O firmware não está em modo de configuração (Setup Mode); o Secure Boot ficou desligado.");
        } else {
            let dir = "/run/ararinha-instalador/esp";
            let _ = fs::create_dir_all(dir);
            rodar(Command::new("mount").args([&p1, dir]), "mount da ESP");
            let nome = if cfg!(target_arch = "aarch64") { "BOOTAA64.EFI" } else { "BOOTX64.EFI" };
            let r = secureboot::configurar(
                std::path::Path::new(&format!("{alvo_dir}/etc/ararinha/secureboot")),
                &[std::path::PathBuf::from(format!("{dir}/EFI/BOOT/{nome}"))],
            );
            unsafe { sync() };
            let _ = Command::new("umount").arg(dir).status();
            if let Err(e) = r {
                println!("AVISO Secure Boot não ativado: {e}");
            }
        }
    }
    unsafe { sync() };
    rodar(Command::new("umount").arg(alvo_dir), "umount");
    fechar_cripto(cripto);
    progresso(100);
    println!("PRONTO");
}
