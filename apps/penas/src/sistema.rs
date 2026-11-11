// SPDX-License-Identifier: GPL-3.0-or-later
//! Pedidos ao serviço do sistema (ararinha-sistema), pelo socket
//! /run/ararinha/sistema.sock. O serviço sabe quem pediu pelo kernel
//! (SO_PEERCRED) e decide se pode.

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

/// Envia um pedido de uma linha e devolve a resposta ("OK ..." vira Ok).
/// Bloqueia: chame numa thread se a ação puder demorar.
pub fn pedir(pedido: &str) -> Result<String, String> {
    let mut s = UnixStream::connect("/run/ararinha/sistema.sock").map_err(|e| format!("serviço do sistema indisponível: {e}"))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
    s.write_all(format!("{pedido}\n").as_bytes()).map_err(|e| e.to_string())?;
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut resposta = String::new();
    s.take(64 * 1024).read_to_string(&mut resposta).map_err(|e| e.to_string())?;
    let r = resposta.trim_end();
    if let Some(ok) = r.strip_prefix("OK") {
        Ok(ok.trim_start().to_string())
    } else if let Some(e) = r.strip_prefix("ERRO") {
        Err(e.trim_start().to_string())
    } else {
        Err("resposta inválida do serviço do sistema".into())
    }
}
