// SPDX-License-Identifier: GPL-3.0-or-later
//! Canal de controle da Plumagem: um FIFO em $XDG_RUNTIME_DIR/plumagem-ctl
//! (modo 0600, só o dono da sessão e o root escrevem). Cada linha é um
//! pedido; hoje só existe "bloquear".

use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

extern "C" {
    fn mkfifo(caminho: *const std::os::raw::c_char, modo: u32) -> i32;
}

const O_NONBLOCK: i32 = 0o4000;

pub fn abrir() -> Option<File> {
    let dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let caminho = format!("{dir}/plumagem-ctl");
    let _ = std::fs::remove_file(&caminho);
    let c = CString::new(caminho.clone()).ok()?;
    // SAFETY: `c` é uma string C válida durante a chamada.
    if unsafe { mkfifo(c.as_ptr(), 0o600) } != 0 {
        return None;
    }
    let _ = std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o600));
    // Aberto para leitura e escrita: o FIFO nunca fica "sem escritor" (o que
    // faria o laço acordar sem parar com EOF).
    OpenOptions::new().read(true).write(true).custom_flags(O_NONBLOCK).open(&caminho).ok()
}

/// Lê os pedidos que chegaram (linhas curtas, no máximo 16 por vez).
pub fn ler(f: &mut File) -> Vec<String> {
    let mut buf = [0u8; 512];
    let mut texto = String::new();
    while let Ok(n) = f.read(&mut buf) {
        if n == 0 {
            break;
        }
        texto.push_str(&String::from_utf8_lossy(&buf[..n]));
        if texto.len() > 4096 {
            break;
        }
    }
    texto.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).take(16).collect()
}
