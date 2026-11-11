// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-som: toca um som curto e sai. Usado pela Plumagem para os sons
//! do sistema; não toca nada se "sons = nao" estiver na configuração.
//!
//!   ararinha-som /usr/share/sounds/ararinha/notificacao.wav

#[allow(dead_code)]
#[path = "tocador.rs"]
mod tocador;

use std::{
    path::PathBuf,
    sync::{mpsc::channel, Arc, Mutex},
};

fn sons_ligados() -> bool {
    let casa = std::env::var("HOME").unwrap_or_default();
    for c in [format!("{casa}/.config/plumagem/plumagem.conf"), "/etc/plumagem/plumagem.conf".to_string()] {
        let Ok(t) = std::fs::read_to_string(&c) else { continue };
        for l in t.lines() {
            if l.trim_start().starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "sons" {
                    return !matches!(v.trim().trim_matches('"'), "nao" | "não" | "desligado");
                }
            }
        }
    }
    true
}

fn main() {
    let Some(arquivo) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("uso: ararinha-som <arquivo.wav>");
        std::process::exit(2);
    };
    if !sons_ligados() {
        return;
    }
    let (_tx, rx) = channel();
    let estado = Arc::new(Mutex::new(tocador::Estado::default()));
    if let Err(e) = tocador::tocar(&arquivo, &rx, &estado) {
        eprintln!("ararinha-som: {e}");
        std::process::exit(1);
    }
}
