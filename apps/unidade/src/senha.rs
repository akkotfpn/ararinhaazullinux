// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-senha: confere a senha da conta de quem chamou (lida do stdin)
//! com o /etc/shadow. Usado pela tela de bloqueio da Plumagem.
//!
//! Instalado com setuid root. Só confere a conta do próprio usuário (uid
//! real), não diz nada além de "certo" (saída 0) ou "errado" (saída 1) e,
//! quando erra, espera 2 segundos para atrasar quem tenta adivinhar.

use std::{
    ffi::{CStr, CString},
    io::Read,
    os::raw::c_char,
    process::exit,
    thread::sleep,
    time::Duration,
};

#[link(name = "crypt")]
extern "C" {
    fn crypt(frase: *const c_char, config: *const c_char) -> *mut c_char;
}

extern "C" {
    fn getuid() -> u32;
}

fn nome_do_uid(uid: u32) -> Option<String> {
    std::fs::read_to_string("/etc/passwd").ok()?.lines().find_map(|l| {
        let c: Vec<&str> = l.split(':').collect();
        (c.len() > 2 && c[2].parse::<u32>().ok() == Some(uid)).then(|| c[0].to_string())
    })
}

fn confere(login: &str, senha: &str) -> bool {
    let Ok(shadow) = std::fs::read_to_string("/etc/shadow") else { return false };
    let Some(hash) = shadow.lines().find_map(|l| {
        let mut c = l.split(':');
        (c.next() == Some(login)).then(|| c.next().unwrap_or("").to_string())
    }) else {
        return false;
    };
    if hash.is_empty() {
        return senha.is_empty();
    }
    if hash.starts_with('!') || hash.starts_with('*') {
        return false;
    }
    let (Ok(k), Ok(h)) = (CString::new(senha), CString::new(hash.as_str())) else { return false };
    // SAFETY: strings C válidas; o resultado é lido antes de outra chamada.
    let r = unsafe { crypt(k.as_ptr(), h.as_ptr()) };
    if r.is_null() {
        return false;
    }
    let r = unsafe { CStr::from_ptr(r) };
    // Comparação em tempo constante.
    let (a, b) = (r.to_bytes(), hash.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

extern "C" {
    fn setresuid(r: u32, e: u32, s: u32) -> i32;
    fn setresgid(r: u32, e: u32, s: u32) -> i32;
}

/// `ararinha-senha --trocar`: lê a senha atual e a nova (uma por linha) e
/// troca a senha da própria conta pelo chpasswd. Saída 0 trocou, 1 senha
/// atual errada, 2 senha nova recusada ou falha.
fn trocar(login: &str) -> ! {
    let mut entrada = String::new();
    if std::io::stdin().take(1024).read_to_string(&mut entrada).is_err() {
        exit(2);
    }
    let mut linhas = entrada.lines();
    let (atual, nova) = (linhas.next().unwrap_or(""), linhas.next().unwrap_or(""));
    if !confere(login, atual) {
        sleep(Duration::from_secs(2));
        exit(1);
    }
    if nova.chars().count() < 4 || nova.contains(':') || nova.contains('\n') {
        exit(2);
    }
    if unsafe { setresgid(0, 0, 0) } != 0 || unsafe { setresuid(0, 0, 0) } != 0 {
        exit(2);
    }
    for (k, _) in std::env::vars_os() {
        std::env::remove_var(k);
    }
    let filho = std::process::Command::new("/usr/sbin/chpasswd")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let ok = filho
        .and_then(|mut f| {
            use std::io::Write;
            if let Some(mut e) = f.stdin.take() {
                e.write_all(format!("{login}:{nova}\n").as_bytes())?;
            }
            f.wait()
        })
        .map(|s| s.success())
        .unwrap_or(false);
    exit(if ok { 0 } else { 2 });
}

fn main() {
    let uid = unsafe { getuid() };
    if std::env::args().nth(1).as_deref() == Some("--trocar") {
        let Some(login) = nome_do_uid(uid) else { exit(2) };
        trocar(&login);
    }
    let Some(login) = nome_do_uid(uid) else { exit(1) };
    let mut senha = String::new();
    if std::io::stdin().take(512).read_to_string(&mut senha).is_err() {
        exit(1);
    }
    let senha = senha.trim_end_matches(['\n', '\r']);
    if confere(&login, senha) {
        exit(0);
    }
    sleep(Duration::from_secs(2));
    exit(1);
}
