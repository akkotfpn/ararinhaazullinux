// SPDX-License-Identifier: GPL-3.0-or-later
//! Plumagem: o compositor Wayland do Ararinha Azul Linux (ADR 0004).
//!
//! A v0 nasce do Anvil, o compositor de referência do Smithay (licença MIT,
//! veja LICENSE-smithay-MIT.txt), e ganha aos poucos a identidade do Ararinha.

fn main() {
    if let Ok(filtro) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().compact().with_env_filter(filtro).init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }
    profiling::register_thread!("Main Thread");
    plumagem::falhas::instalar();

    match std::env::args().nth(1).as_deref() {
        None | Some("--tty-udev") | Some("--tela") => {
            tracing::info!("Plumagem iniciando direto na tela (DRM/KMS)");
            plumagem::udev::run_udev();
        }
        Some("--ninho") => {
            tracing::info!("Plumagem iniciando como Ninho (tela de entrada)");
            std::env::set_var("PLUMAGEM_NINHO", "1");
            plumagem::udev::run_udev();
        }
        Some("--versao") | Some("--version") => {
            println!("plumagem {}", env!("CARGO_PKG_VERSION"));
        }
        Some(outro) => {
            eprintln!("opção desconhecida: {outro}");
            eprintln!("uso: plumagem [--tela | --ninho | --versao]");
            std::process::exit(2);
        }
    }
}
