// SPDX-License-Identifier: GPL-3.0-or-later
//! Histórico da área de transferência (Win+V).
//!
//! Quando um app copia texto, a Plumagem pede uma cópia do conteúdo (por um
//! pipe, como qualquer app que cola) e guarda na lista. Escolher um item da
//! lista faz da Plumagem a dona da área de transferência com aquele texto;
//! os apps que colarem recebem o texto por `enviar`.
//!
//! Só texto, até 25 itens e 256 KB cada; nada vai para o disco (sai ao
//! encerrar a sessão, como no Windows com a sincronização desligada).

use std::{
    io::{Read, Write},
    os::fd::{FromRawFd, OwnedFd},
    sync::Mutex,
};

pub const MAX_ITENS: usize = 25;
const MAX_BYTES: usize = 256 * 1024;

/// Mais recente primeiro.
pub static HISTORICO: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Texto que a própria Plumagem está oferecendo (item escolhido no Win+V).
pub static PROPRIA: Mutex<Option<String>> = Mutex::new(None);
/// Muda a cada item novo (para redesenhar o painel).
pub static VERSAO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub const TIPOS_TEXTO: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];

extern "C" {
    fn pipe2(fds: *mut i32, flags: i32) -> i32;
}
const O_CLOEXEC: i32 = 0o2000000;

/// Um pipe: (leitura, escrita).
pub fn cano() -> Option<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` tem espaço para os dois descritores.
    if unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC) } != 0 {
        return None;
    }
    // SAFETY: os descritores acabaram de ser criados e são nossos.
    Some(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// Melhor tipo de texto entre os oferecidos.
pub fn tipo_texto(tipos: &[String]) -> Option<String> {
    TIPOS_TEXTO.iter().find(|t| tipos.iter().any(|m| m == *t)).map(|t| t.to_string())
}

/// Lê o texto do pipe numa thread e guarda no histórico.
pub fn ler_em_segundo_plano(leitura: OwnedFd) {
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(leitura);
        let mut b = Vec::new();
        let _ = (&mut f).take(MAX_BYTES as u64 + 1).read_to_end(&mut b);
        if b.is_empty() || b.len() > MAX_BYTES {
            return;
        }
        let Ok(t) = String::from_utf8(b) else { return };
        if t.trim().is_empty() {
            return;
        }
        guardar(t);
    });
}

pub fn guardar(t: String) {
    // Configurações › Privacidade: histórico desligado.
    if crate::painel::ler_chave("historico_area").as_deref() == Some("nao") {
        return;
    }
    if let Ok(mut h) = HISTORICO.lock() {
        h.retain(|x| x != &t);
        h.insert(0, t);
        h.truncate(MAX_ITENS);
    }
    VERSAO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn itens() -> Vec<String> {
    HISTORICO.lock().map(|h| h.clone()).unwrap_or_default()
}

pub fn limpar() {
    if let Ok(mut h) = HISTORICO.lock() {
        h.clear();
    }
    VERSAO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Se a Plumagem é a dona da área de transferência, escreve o texto no fd
/// de quem pediu (numa thread: o app pode demorar para ler). Devolve false
/// se não for (o pedido segue para o Xwayland).
pub fn enviar(fd: OwnedFd) -> bool {
    let Some(t) = PROPRIA.lock().ok().and_then(|p| p.clone()) else { return false };
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(fd);
        let _ = f.write_all(t.as_bytes());
    });
    true
}
