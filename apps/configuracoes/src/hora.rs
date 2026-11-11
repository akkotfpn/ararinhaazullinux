// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Data e hora": fuso horário, acertar a hora pela internet (NTP) e
//! o formato do relógio. As mudanças do sistema vão pelo ararinha-sistema.

use std::{
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{componentes as c, desenho, tema, Ui};

use crate::{gravar_chave, ler_chave};

/// Fusos do Brasil primeiro, depois alguns do mundo.
const FUSOS: [(&str, &str); 14] = [
    ("Brasília, São Paulo, Rio (UTC−3)", "America/Sao_Paulo"),
    ("Manaus, Cuiabá (UTC−4)", "America/Manaus"),
    ("Rio Branco, Acre (UTC−5)", "America/Rio_Branco"),
    ("Fernando de Noronha (UTC−2)", "America/Noronha"),
    ("Belém, Fortaleza, Recife (UTC−3)", "America/Fortaleza"),
    ("Lisboa", "Europe/Lisbon"),
    ("Londres", "Europe/London"),
    ("Madri, Paris, Berlim", "Europe/Madrid"),
    ("Nova York", "America/New_York"),
    ("Chicago", "America/Chicago"),
    ("Los Angeles", "America/Los_Angeles"),
    ("Buenos Aires", "America/Argentina/Buenos_Aires"),
    ("Tóquio", "Asia/Tokyo"),
    ("UTC", "UTC"),
];

#[derive(Default)]
pub struct PaginaHora {
    fuso: String,
    ntp: Option<bool>,
    pub aviso: Option<String>,
    rx: Option<Receiver<Result<String, String>>>,
    lido: bool,
    rolagem: f32,
}

impl PaginaHora {
    fn pedir(&mut self, p: String) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(penas::sistema::pedir_admin(&p));
        });
        self.rx = Some(rx);
    }

    fn aplicar(&mut self, t: &str) {
        for parte in t.split_whitespace() {
            match parte.split_once('=') {
                Some(("fuso", v)) => self.fuso = v.to_string(),
                Some(("ntp", v)) => self.ntp = Some(v == "sim"),
                _ => {}
            }
        }
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if !self.lido {
            self.lido = true;
            self.pedir("hora estado".into());
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx = None;
                    match r {
                        Ok(t) => self.aplicar(&t),
                        Err(e) => self.aviso = Some(format!("Não deu: {e}.")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(150)),
            }
        }
        // O relógio da página anda sozinho.
        if ui.acordar_em.is_none() {
            ui.acordar_em = Some(Duration::from_secs(1));
        }
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        let total = 380.0 + FUSOS.len() as f32 * 34.0;
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (total - ui.altura()).max(0.0));
        let dy = -self.rolagem;

        ui.tela.texto(x, 52.0 + dy, 22.0, tema::TEXTO(), "Data e hora");
        let agora = std::process::Command::new("date")
            .arg("+%A, %d de %B de %Y · %H:%M:%S")
            .env("LC_TIME", "pt_BR.UTF-8")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        ui.tela.retangulo(x, 76.0 + dy, w, 56.0, 8.0, tema::SUPERFICIE_ALTA());
        ui.tela.texto(x + 18.0, 110.0 + dy, 16.0, tema::TEXTO(), &desenho::encurtar(16.0, &agora, w - 36.0));

        // Hora automática.
        let mut y = 160.0 + dy;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Acertar a hora pela internet");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Usa servidores de hora (NTP); a hora fica sempre certa.");
        if let Some(mut ligado) = self.ntp {
            if c::chave(ui, x + w - 44.0, y - 8.0, &mut ligado) && self.rx.is_none() {
                self.ntp = Some(ligado);
                self.pedir(format!("hora ntp {}", if ligado { "sim" } else { "nao" }));
            }
        }

        // Formato do relógio da barra.
        y += 56.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Relógio de 24 horas");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Desligado, a barra mostra 1:30 PM em vez de 13:30.");
        let mut h24 = ler_chave("relogio_12h").as_deref() != Some("sim");
        if c::chave(ui, x + w - 44.0, y - 8.0, &mut h24) {
            let _ = gravar_chave("relogio_12h", if h24 { "nao" } else { "sim" });
        }

        // Fuso.
        y += 64.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Fuso horário");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), &format!("Agora: {}", if self.fuso.is_empty() { "…" } else { &self.fuso }));
        y += 34.0;
        let mut escolhido = None;
        for (nome, id) in FUSOS {
            let sel = self.fuso == id;
            let s = ui.sobre(x, y, w, 30.0);
            if sel {
                ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::DESTAQUE_SUAVE());
            } else if s {
                ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            ui.tela.retangulo(x + 10.0, y + 8.0, 14.0, 14.0, 7.0, if sel { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(x + 12.0, y + 10.0, 10.0, 10.0, 5.0, tema::JANELA());
            if sel {
                ui.tela.retangulo(x + 14.0, y + 12.0, 6.0, 6.0, 3.0, tema::DESTAQUE());
            }
            ui.tela.texto(x + 34.0, y + 20.0, 13.5, tema::TEXTO(), nome);
            if ui.clicou(x, y, w, 30.0) && !sel && self.rx.is_none() {
                escolhido = Some(id);
            }
            y += 34.0;
        }
        if let Some(id) = escolhido {
            self.fuso = id.to_string();
            self.pedir(format!("hora fuso {id}"));
        }
    }
}
