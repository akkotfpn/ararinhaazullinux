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

mod contas;
mod firewall;
mod hardware;
mod usb;
mod senha;

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
    pub gid: u32,
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
    Some(Quem { uid: c.uid, gid: c.gid, pid: c.pid, usuario, admin })
}

fn nome_do_usuario(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let c: Vec<&str> = l.split(':').collect();
        (c.len() > 2 && c[2].parse() == Ok(uid)).then(|| c[0].to_string())
    })
}

pub(crate) fn no_grupo(usuario: &str, grupo: &str) -> bool {
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
        ["hardware", "memoria"] => hardware::memoria(),
        ["contas", "listar"] => Ok(contas::listar()),
        ["contas", "criar", login, tipo, nome, senha] => {
            admin()?;
            contas::criar(login, tipo, nome, senha)
        }
        ["contas", "remover", login] => {
            admin()?;
            contas::remover(login, &quem.usuario)
        }
        ["contas", "admin", login, sim] => {
            admin()?;
            contas::tipo(login, *sim == "sim", &quem.usuario)
        }
        // Brilho da tela (teclas do notebook): quem está logado pode.
        ["brilho", pct] => {
            let logado = quem.uid == 0 || no_grupo(&quem.usuario, "seat");
            if !logado {
                return Err("só quem está usando o computador".into());
            }
            let pct: u32 = pct.parse().map_err(|_| "valor inválido".to_string())?;
            let base = std::fs::read_dir("/sys/class/backlight")
                .ok()
                .and_then(|mut r| r.next())
                .and_then(|e| e.ok())
                .map(|e| e.path())
                .ok_or("sem controle de brilho")?;
            let max: u32 = std::fs::read_to_string(base.join("max_brightness"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or("sem controle de brilho")?;
            let v = (max * pct.clamp(5, 100) / 100).max(1);
            std::fs::write(base.join("brightness"), v.to_string()).map_err(|e| e.to_string())?;
            Ok(format!("{pct}"))
        }
        ["hora", "estado"] => Ok(contas::hora_estado()),
        ["idioma", "estado"] => Ok(contas::idioma()),
        ["dns", "estado"] => Ok(contas::dns_estado()),
        ["dns", "automatico"] => {
            admin()?;
            contas::dns_automatico()
        }
        ["dns", "definir", servidores @ ..] => {
            admin()?;
            contas::dns_definir(servidores)
        }
        ["idioma", "definir", id] => {
            admin()?;
            contas::definir_idioma(id)
        }
        ["hora", "fuso", z] => {
            admin()?;
            contas::fuso(z)
        }
        ["hora", "ntp", sim] => {
            admin()?;
            contas::ntp(*sim == "sim")
        }
        ["firewall", "perfil", p] => {
            admin()?;
            firewall::definir_perfil(p)
        }
        ["firewall", "ligar"] => {
            admin()?;
            firewall::ligar(true)
        }
        ["firewall", "desligar"] => {
            admin()?;
            firewall::ligar(false)
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
        // Desligar, reiniciar e dormir: quem está logado no computador pode
        // (como no Windows). "Logado" = conta de quem usa a tela (grupo
        // "seat"). O /run/user não serve: o serviço roda com ProtectHome,
        // que esconde essa pasta dele.
        // Pendrives e HDs externos: quem está usando o computador pode.
        ["usb", "listar"] => Ok(usb::listar_texto()),
        ["usb", verbo @ ("montar" | "ejetar"), dev] => {
            let logado = quem.uid == 0 || no_grupo(&quem.usuario, "seat");
            if !logado {
                return Err("só quem está usando o computador pode fazer isso".into());
            }
            if *verbo == "montar" {
                usb::montar(dev, &quem.usuario, quem.uid, quem.gid)
            } else {
                usb::ejetar(dev)
            }
        }
        [acao @ ("desligar" | "reiniciar" | "dormir" | "hibernar")] => {
            let logado = quem.uid == 0 || no_grupo(&quem.usuario, "seat");
            if !logado {
                return Err("só quem está usando o computador pode fazer isso".into());
            }
            let verbo = match *acao {
                "desligar" => "poweroff",
                "reiniciar" => "reboot",
                "hibernar" => "hibernate",
                _ => "suspend",
            };
            // Primeiro pelo logind (avisa os apps e respeita quem pediu para
            // esperar); se ele recusar, direto pelo systemd (PID 1), que
            // sempre aceita root.
            let s = std::process::Command::new("/usr/bin/systemctl")
                .args(["--no-ask-password", verbo])
                .output()
                .map_err(|e| format!("systemctl: {e}"))?;
            if s.status.success() {
                return Ok(String::new());
            }
            let erro1 = String::from_utf8_lossy(&s.stderr).trim().to_string();
            eprintln!("systemctl {verbo} falhou ({erro1}); tentando {verbo}.target");
            let alvo = format!("{verbo}.target");
            let s2 = std::process::Command::new("/usr/bin/systemctl")
                .args(["--no-ask-password", "--no-block", "--job-mode=replace-irreversibly", "start", &alvo])
                .output()
                .map_err(|e| format!("systemctl: {e}"))?;
            if s2.status.success() {
                Ok(String::new())
            } else {
                Err(format!("{erro1} / {}", String::from_utf8_lossy(&s2.stderr).trim()))
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
    let lido = BufReader::new((&s).take(1024)).read_line(&mut linha);
    if lido.is_err() {
        return;
    }
    let pedido: String = linha.trim().chars().filter(|c| !c.is_control()).collect();
    // Pedido de administrador (UAC): "como <login> <senha_hex> <pedido>".
    let (quem, pedido, como) = match pedido.strip_prefix("como ").map(|r| r.splitn(3, ' ').collect::<Vec<_>>()) {
        Some(p) if p.len() == 3 => {
            let senha = contas::de_hex(p[1]).unwrap_or_default();
            match senha::admin_valido(p[0], &senha) {
                Ok(()) => (Quem { admin: true, ..quem }, p[2].to_string(), Some(Ok(p[0].to_string()))),
                Err(e) => (quem, p[2].to_string(), Some(Err((p[0].to_string(), e)))),
            }
        }
        _ => (quem, pedido, None),
    };
    // No journal, a senha da conta nova não aparece.
    let mut registro = if pedido.starts_with("contas criar ") {
        pedido.split_whitespace().take(4).collect::<Vec<_>>().join(" ") + " …"
    } else {
        pedido.clone()
    };
    let resposta = match &como {
        Some(Err((login, e))) => {
            registro = format!("[como {login}] {registro}");
            Err(e.clone())
        }
        Some(Ok(login)) => {
            registro = format!("[como {login}] {registro}");
            atender(&quem, &pedido)
        }
        None => atender(&quem, &pedido),
    };
    // Registro no journal (a saída de erro do serviço vai para ele).
    eprintln!(
        "{} (uid {}, pid {}) pediu \"{}\": {}",
        quem.usuario,
        quem.uid,
        quem.pid,
        registro,
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
