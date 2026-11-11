// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-sistema: o serviço root do Ararinha.
//!
//! No lugar de vários ajudantes setuid, um único serviço pequeno, ativado
//! pelo systemd no socket /run/ararinha/sistema.sock. Cada conexão traz um
//! pedido de uma linha ("firewall perfil casa"); o serviço descobre quem
//! pediu pelo SO_PEERCRED (o kernel informa uid e pid, ninguém consegue
//! mentir), confere se a pessoa pode, registra no journal e responde
//! "OK ..." ou "ERRO ...".
//!
//! Ações de administrador exigem o grupo wheel.
//! Sai sozinho depois de 60 s sem pedidos (o socket o acorda de novo).

mod firewall;

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::net::{UnixListener, UnixStream},
    },
    time::Duration,
};

#[repr(C)]
#[derive(Default)]
struct UCred {
    pid: i32,
    uid: u32,
    gid: u32,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn getsockopt(fd: i32, nivel: i32, opcao: i32, valor: *mut std::ffi::c_void, tam: *mut u32) -> i32;
    fn poll(fds: *mut PollFd, n: u64, tempo_ms: i32) -> i32;
}

const SOL_SOCKET: i32 = 1;
const SO_PEERCRED: i32 = 17;
const POLLIN: i16 = 1;
const TEMPO_OCIOSO_MS: i32 = 60_000;

/// Quem está do outro lado do socket, segundo o kernel.
pub struct Quem {
    pub uid: u32,
    pub pid: i32,
    pub usuario: String,
    pub admin: bool,
}

fn credenciais(s: &UnixStream) -> Option<Quem> {
    let mut c = UCred::default();
    let mut tam = std::mem::size_of::<UCred>() as u32;
    // SAFETY: `c` tem o tamanho informado e vive durante a chamada.
    let r = unsafe { getsockopt(s.as_raw_fd(), SOL_SOCKET, SO_PEERCRED, &mut c as *mut UCred as *mut _, &mut tam) };
    if r != 0 {
        return None;
    }
    let usuario = nome_do_usuario(c.uid).unwrap_or_else(|| format!("uid {}", c.uid));
    let admin = c.uid == 0 || no_grupo(&usuario, "wheel");
    Some(Quem { uid: c.uid, pid: c.pid, usuario, admin })
}

fn nome_do_usuario(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let c: Vec<&str> = l.split(':').collect();
        (c.len() > 2 && c[2].parse() == Ok(uid)).then(|| c[0].to_string())
    })
}

fn no_grupo(usuario: &str, grupo: &str) -> bool {
    let grupos = std::fs::read_to_string("/etc/group").unwrap_or_default();
    grupos.lines().any(|l| {
        let c: Vec<&str> = l.split(':').collect();
        c.len() > 3 && c[0] == grupo && c[3].split(',').any(|m| m == usuario)
    })
}

/// Executa um pedido. Devolve a resposta (sem o "OK"/"ERRO").
fn atender(quem: &Quem, pedido: &str) -> Result<String, String> {
    let partes: Vec<&str> = pedido.split_whitespace().collect();
    let admin = || if quem.admin { Ok(()) } else { Err("só administradores podem fazer isso".to_string()) };
    match partes.as_slice() {
        ["ping"] => Ok("ararinha-sistema".into()),
        ["firewall", "estado"] => Ok(firewall::estado()),
        ["firewall", "perfil", p] => {
            admin()?;
            firewall::definir_perfil(p)
        }
        ["firewall", "abrir", porta] => {
            admin()?;
            firewall::abrir(porta)
        }
        ["firewall", "fechar", porta] => {
            admin()?;
            firewall::fechar(porta)
        }
        // Plano de energia: qualquer pessoa da sessão pode escolher (como no
        // Windows); o ajudante só aceita os três planos.
        ["energia", plano] if ["padrao", "economico", "desempenho"].contains(plano) => {
            let s = std::process::Command::new("/usr/bin/ararinha-energia")
                .arg(plano)
                .env_clear()
                .output()
                .map_err(|e| format!("ararinha-energia: {e}"))?;
            if s.status.success() {
                Ok(String::from_utf8_lossy(&s.stdout).trim().to_string())
            } else {
                Err(String::from_utf8_lossy(&s.stderr).trim().to_string())
            }
        }
        _ => Err("pedido desconhecido".into()),
    }
}

fn servir(mut s: UnixStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
    let Some(quem) = credenciais(&s) else { return };
    let mut linha = String::new();
    // Pedido: uma linha de no máximo 512 bytes.
    let lido = BufReader::new((&s).take(512)).read_line(&mut linha);
    if lido.is_err() {
        return;
    }
    let pedido: String = linha.trim().chars().filter(|c| !c.is_control()).collect();
    let resposta = atender(&quem, &pedido);
    // Registro no journal (a saída de erro do serviço vai para ele).
    eprintln!(
        "{} (uid {}, pid {}) pediu \"{}\": {}",
        quem.usuario,
        quem.uid,
        quem.pid,
        pedido,
        match &resposta {
            Ok(_) => "feito".to_string(),
            Err(e) => format!("recusado ({e})"),
        }
    );
    let texto = match resposta {
        Ok(r) => format!("OK {}\n", r.replace('\n', "\t")),
        Err(e) => format!("ERRO {}\n", e.replace('\n', " ")),
    };
    let _ = s.write_all(texto.as_bytes());
}

fn main() {
    // Ativação por socket: o systemd passa o socket como fd 3.
    let ativado = std::env::var("LISTEN_FDS").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) >= 1;
    let ouvinte = if ativado {
        // SAFETY: o systemd nos entregou o fd 3 aberto, e só ele o usa.
        unsafe { UnixListener::from_raw_fd(3) }
    } else {
        let caminho = "/run/ararinha/sistema.sock";
        let _ = std::fs::create_dir_all("/run/ararinha");
        let _ = std::fs::remove_file(caminho);
        match UnixListener::bind(caminho) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("não deu para abrir {caminho}: {e}");
                std::process::exit(1);
            }
        }
    };
    loop {
        let mut p = PollFd { fd: ouvinte.as_raw_fd(), events: POLLIN, revents: 0 };
        // SAFETY: um único PollFd válido durante a chamada.
        let n = unsafe { poll(&mut p, 1, TEMPO_OCIOSO_MS) };
        if n == 0 {
            // Ocioso: sai; o systemd religa no próximo pedido.
            return;
        }
        if n < 0 {
            continue;
        }
        if let Ok((s, _)) = ouvinte.accept() {
            servir(s);
        }
    }
}
