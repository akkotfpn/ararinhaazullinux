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

/// Frase para a janela "Permitir esta alteração?".
fn descrever(pedido: &str) -> String {
    let p: Vec<&str> = pedido.split_whitespace().collect();
    match p.as_slice() {
        ["contas", "criar", login, ..] => format!("Configurações quer criar a conta \"{login}\"."),
        ["contas", "remover", login] => format!("Configurações quer apagar a conta \"{login}\" e todos os arquivos dela."),
        ["contas", "admin", login, "sim"] => format!("Configurações quer tornar \"{login}\" administrador."),
        ["contas", "admin", login, _] => format!("Configurações quer tirar o administrador de \"{login}\"."),
        ["hora", "fuso", z] => format!("Configurações quer mudar o fuso horário para {z}."),
        ["hora", "ntp", ..] => "Configurações quer mudar o acerto automático da hora.".into(),
        ["idioma", "definir", id] => format!("Configurações quer mudar o idioma do sistema para {id}."),
        ["firewall", "desligar"] => "Configurações quer DESLIGAR o firewall.".into(),
        ["firewall", ..] => "Configurações quer mudar o firewall.".into(),
        ["energia", ..] => "Configurações quer mudar o plano de energia.".into(),
        ["dns", ..] => "Configurações quer mudar o servidor DNS do computador.".into(),
        _ => "Um app quer mudar uma opção do sistema.".into(),
    }
}

/// Como `pedir`, mas se a conta não for administradora abre a janela
/// "Permitir esta alteração?" (ararinha-admin), que pede a senha de um
/// administrador, e repete o pedido com ela. Bloqueia; chame numa thread.
pub fn pedir_admin(pedido: &str) -> Result<String, String> {
    match pedir(pedido) {
        Err(e) if e.contains("só administradores") => {
            let saida = std::process::Command::new("ararinha-admin")
                .arg(descrever(pedido))
                .stderr(std::process::Stdio::null())
                .output()
                .map_err(|e| format!("ararinha-admin: {e}"))?;
            let t = String::from_utf8_lossy(&saida.stdout).to_string();
            let mut l = t.lines();
            match (l.next(), l.next()) {
                (Some(login), Some(hex)) if !login.is_empty() => pedir(&format!("como {login} {hex} {pedido}")),
                _ => Err("cancelado".into()),
            }
        }
        r => r,
    }
}
