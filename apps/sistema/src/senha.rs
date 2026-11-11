// SPDX-License-Identifier: GPL-3.0-or-later
//! Pedido de administrador (estilo UAC): uma conta padrão digita o login e
//! a senha de um administrador para uma ação. O serviço confere com o
//! /etc/shadow; cada erro atrasa a resposta (2 s, dobrando até 30 s).

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

#[link(name = "crypt")]
extern "C" {
    fn crypt(frase: *const c_char, config: *const c_char) -> *mut c_char;
}

static ERROS: AtomicU32 = AtomicU32::new(0);

fn confere(login: &str, senha: &str) -> bool {
    let Ok(shadow) = std::fs::read_to_string("/etc/shadow") else { return false };
    let Some(hash) = shadow.lines().find_map(|l| {
        let mut c = l.split(':');
        (c.next() == Some(login)).then(|| c.next().unwrap_or("").to_string())
    }) else {
        return false;
    };
    if hash.is_empty() || hash.starts_with('!') || hash.starts_with('*') {
        return false;
    }
    let (Ok(k), Ok(h)) = (CString::new(senha), CString::new(hash.as_str())) else { return false };
    // SAFETY: strings C válidas; o resultado é lido antes de outra chamada.
    let r = unsafe { crypt(k.as_ptr(), h.as_ptr()) };
    if r.is_null() {
        return false;
    }
    let r = unsafe { CStr::from_ptr(r) };
    let (a, b) = (r.to_bytes(), hash.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Confere se `login` é administrador e a senha está certa.
pub fn admin_valido(login: &str, senha: &str) -> Result<(), String> {
    let ok = crate::no_grupo(login, "wheel") && confere(login, senha);
    if ok {
        ERROS.store(0, Ordering::Relaxed);
        return Ok(());
    }
    let n = ERROS.fetch_add(1, Ordering::Relaxed).min(4);
    std::thread::sleep(Duration::from_secs((2u64 << n).min(30)));
    Err("usuário ou senha de administrador incorretos".into())
}
