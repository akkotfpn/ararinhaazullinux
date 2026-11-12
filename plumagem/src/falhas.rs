// SPDX-License-Identifier: GPL-3.0-or-later
//! Relatórios de falhas locais.
//!
//! Se a Plumagem quebrar (panic), grava o relatório na pasta da conta
//! (~/.local/state/ararinha/falhas). Na próxima sessão, avisa com uma
//! notificação. Nada sai do computador. "relatorios = nao" desliga.

use std::path::PathBuf;

pub fn pasta() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".local/state/ararinha/falhas")
}

pub fn instalar() {
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if crate::painel::ler_chave("relatorios").as_deref() != Some("nao") && std::env::var_os("PLUMAGEM_NINHO").is_none() {
            let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let p = pasta();
            let _ = std::fs::create_dir_all(&p);
            let texto = format!(
                "App: Plumagem (área de trabalho)\nVersão: {}\nQuando: {agora} (segundos desde 1970)\nErro: {info}\n\nPilha:\n{}\n",
                env!("CARGO_PKG_VERSION"),
                std::backtrace::Backtrace::force_capture()
            );
            let _ = std::fs::write(p.join(format!("plumagem-{agora}.txt")), texto);
            let _ = std::fs::write(p.join(".avisar"), "plumagem");
        }
        anterior(info);
    }));
}

/// Na entrada da sessão: a área de trabalho quebrou da última vez?
pub fn aviso_pendente() -> bool {
    let marca = pasta().join(".avisar");
    let tem = marca.exists();
    if tem {
        let _ = std::fs::remove_file(marca);
    }
    tem
}
