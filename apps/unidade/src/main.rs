// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-unidade: conecta e desconecta unidades de rede (SMB/CIFS) para
//! o usuário, sem dar root a ele.
//!
//! Instalado com setuid root. Por isso é pequeno e desconfiado: aceita só
//! letras, nomes de servidor e de pasta com caracteres seguros, monta sempre
//! em /run/media/<usuário>/<LETRA>, com os arquivos do usuário que pediu e
//! com nosuid/nodev. A senha chega pela entrada padrão, nunca pela linha de
//! comando (que outros usuários poderiam ver).
//!
//!   ararinha-unidade montar Z '\\servidor\pasta' usuario [dominio] < senha
//!   ararinha-unidade desmontar Z

use std::{
    fs,
    io::{BufRead, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{exit, Command, Stdio},
};

extern "C" {
    fn getuid() -> u32;
    fn getgid() -> u32;
    fn geteuid() -> u32;
    fn setresuid(r: u32, e: u32, s: u32) -> i32;
    fn setresgid(r: u32, e: u32, s: u32) -> i32;
}

fn falhar(msg: &str) -> ! {
    eprintln!("{msg}");
    exit(1);
}

fn nome_do_usuario(uid: u32) -> String {
    let passwd = fs::read_to_string("/etc/passwd").unwrap_or_default();
    passwd
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split(':').collect();
            (c.len() > 2 && c[2].parse::<u32>().ok() == Some(uid)).then(|| c[0].to_string())
        })
        .filter(|n| n.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) && !n.is_empty())
        .unwrap_or_else(|| falhar("Usuário desconhecido."))
}

fn letra_valida(s: &str) -> char {
    let l: Vec<char> = s.chars().collect();
    match l.as_slice() {
        [c] if c.is_ascii_alphabetic() && !matches!(c.to_ascii_uppercase(), 'A' | 'B' | 'C') => c.to_ascii_uppercase(),
        _ => falhar("Letra de unidade inválida (use D a Z)."),
    }
}

/// \\servidor\pasta ou //servidor/pasta → //servidor/pasta
fn pasta_valida(s: &str) -> String {
    let s = s.trim().replace('\\', "/");
    let s = s.trim_start_matches('/');
    let mut partes = s.split('/').filter(|p| !p.is_empty());
    let servidor = partes.next().unwrap_or_else(|| falhar("Informe \\\\servidor\\pasta."));
    let resto: Vec<&str> = partes.collect();
    if resto.is_empty() {
        falhar("Informe também a pasta compartilhada: \\\\servidor\\pasta.");
    }
    let servidor_ok = servidor.len() <= 253
        && servidor.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
        && !servidor.starts_with('-');
    let pasta_ok = resto.iter().all(|p| {
        *p != ".." && *p != "." && p.chars().all(|c| c.is_alphanumeric() || " _.$-+()".contains(c))
    });
    if !servidor_ok || !pasta_ok {
        falhar("Nome de servidor ou de pasta com caracteres não permitidos.");
    }
    format!("//{servidor}/{}", resto.join("/"))
}

fn texto_seguro(s: &str, nome: &str) -> String {
    if s.len() > 64 || !s.chars().all(|c| c.is_alphanumeric() || "._@-".contains(c)) {
        falhar(&format!("{nome} com caracteres não permitidos."));
    }
    s.to_string()
}

fn ponto_de_montagem(usuario: &str, letra: char) -> PathBuf {
    let base = Path::new("/run/media");
    let _ = fs::create_dir_all(base);
    let dono = base.join(usuario);
    let _ = fs::create_dir(&dono);
    let _ = fs::set_permissions(&dono, fs::Permissions::from_mode(0o755));
    let ponto = dono.join(letra.to_string());
    // Nada de links: o ponto é sempre uma pasta comum criada aqui.
    if let Ok(m) = fs::symlink_metadata(&ponto) {
        if !m.is_dir() {
            falhar("O ponto de montagem existe e não é uma pasta.");
        }
    } else if fs::create_dir(&ponto).is_err() {
        falhar("Não deu para criar o ponto de montagem.");
    }
    ponto
}

fn montado(ponto: &Path) -> bool {
    let alvo = ponto.to_string_lossy().replace(' ', "\\040");
    fs::read_to_string("/proc/mounts")
        .unwrap_or_default()
        .lines()
        .any(|l| l.split_whitespace().nth(1) == Some(alvo.as_str()))
}

fn virar_root() {
    // Com o uid real ainda do usuário, o mount.cifs trataria a montagem como
    // "de usuário" e recusaria. Daqui em diante tudo roda como root.
    unsafe {
        if setresgid(0, 0, 0) != 0 || setresuid(0, 0, 0) != 0 {
            falhar("Não deu para obter as permissões necessárias.");
        }
    }
}

fn programa(nomes: &[&str]) -> String {
    nomes
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| p.to_string())
        .unwrap_or_else(|| falhar("Falta o cifs-utils (mount.cifs) no sistema."))
}

/// Traduz o erro do mount.cifs para o motivo, em português.
fn motivo_cifs(erro: &str) -> String {
    let codigo = erro
        .split("error(")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .and_then(|n| n.parse::<i32>().ok());
    let motivo = match codigo {
        Some(13) | Some(1) => "o usuário ou a senha estão incorretos, ou esta conta não tem acesso à pasta",
        Some(2) => "o servidor respondeu, mas essa pasta compartilhada não existe (confira o nome depois da segunda barra)",
        Some(6) | Some(19) => "o servidor não reconheceu o compartilhamento pedido",
        Some(111) => "o servidor recusou a conexão (o compartilhamento de arquivos está ligado nele?)",
        Some(112) | Some(113) => "o servidor não está acessível na rede (desligado, fora da rede ou bloqueado por firewall)",
        Some(110) | Some(115) => "o servidor demorou demais para responder",
        Some(95) => "o servidor só fala uma versão do protocolo SMB que não é suportada",
        Some(16) => "essa letra já está em uso",
        Some(22) => "o endereço da pasta está em um formato inválido (use \\\\servidor\\pasta)",
        Some(126) | Some(127) => "o servidor exige uma forma de login que não está disponível",
        _ if erro.contains("could not resolve") || erro.contains("resolve address") => {
            "não achei o servidor pelo nome (tente o endereço IP, como \\\\192.168.0.10\\pasta)"
        }
        _ => "",
    };
    if motivo.is_empty() {
        format!("Você não conseguiu se conectar: {}", erro.trim())
    } else {
        format!("Você não conseguiu se conectar porque {motivo}.")
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (uid, gid) = unsafe { (getuid(), getgid()) };
    if unsafe { geteuid() } != 0 {
        falhar("ararinha-unidade precisa estar instalado com setuid root.");
    }
    let usuario = nome_do_usuario(uid);

    match args.get(1).map(String::as_str) {
        Some("montar") if args.len() >= 5 => {
            let letra = letra_valida(&args[2]);
            let pasta = pasta_valida(&args[3]);
            let login = texto_seguro(&args[4], "Usuário");
            let dominio = args.get(5).map(|d| texto_seguro(d, "Domínio"));
            let mut senha = String::new();
            let _ = std::io::stdin().lock().read_line(&mut senha);
            let senha = senha.trim_end_matches(['\n', '\r']).to_string();

            let ponto = ponto_de_montagem(&usuario, letra);
            if montado(&ponto) {
                falhar(&format!("A unidade {letra}: já está conectada."));
            }
            virar_root();

            // Credenciais num arquivo só do root (a senha nunca vai na linha
            // de comando), apagado logo depois.
            let dir = Path::new("/run/ararinha-unidades");
            let _ = fs::create_dir_all(dir);
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
            let cred = dir.join(format!("{uid}-{letra}"));
            let _ = fs::remove_file(&cred);
            let mut opcoes = format!(
                "uid={uid},gid={gid},forceuid,forcegid,file_mode=0644,dir_mode=0755,nosuid,nodev,iocharset=utf8"
            );
            if login.is_empty() {
                opcoes.push_str(",guest");
            } else {
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&cred)
                    .unwrap_or_else(|_| falhar("Não deu para guardar as credenciais."));
                let _ = writeln!(f, "username={login}");
                let _ = writeln!(f, "password={senha}");
                if let Some(d) = &dominio {
                    let _ = writeln!(f, "domain={d}");
                }
                opcoes.push_str(&format!(",credentials={}", cred.display()));
            }
            let mount = programa(&["/usr/sbin/mount.cifs", "/sbin/mount.cifs", "/usr/bin/mount.cifs"]);
            let saida = Command::new(mount)
                .env_clear()
                .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                .arg(&pasta)
                .arg(&ponto)
                .arg("-o")
                .arg(&opcoes)
                .stdin(Stdio::null())
                .output();
            let _ = fs::remove_file(&cred);
            match saida {
                Ok(o) if o.status.success() => println!("{}", ponto.display()),
                Ok(o) => {
                    let _ = fs::remove_dir(&ponto);
                    let erro = String::from_utf8_lossy(&o.stderr);
                    let msg = motivo_cifs(&erro);
                    falhar(&msg);
                }
                Err(e) => falhar(&format!("Não deu para conectar: {e}")),
            }
        }
        Some("desmontar") if args.len() >= 3 => {
            let letra = letra_valida(&args[2]);
            let ponto = Path::new("/run/media").join(&usuario).join(letra.to_string());
            if !montado(&ponto) {
                falhar(&format!("A unidade {letra}: não está conectada."));
            }
            virar_root();
            let ok = Command::new(programa(&["/usr/bin/umount", "/bin/umount"]))
                .env_clear()
                .arg(&ponto)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                falhar("Não deu para desconectar (algum arquivo aberto?).");
            }
            let _ = fs::remove_dir(&ponto);
        }
        _ => {
            eprintln!("uso: ararinha-unidade montar LETRA '\\\\servidor\\pasta' usuario [dominio] < senha");
            eprintln!("     ararinha-unidade desmontar LETRA");
            exit(2);
        }
    }
}
