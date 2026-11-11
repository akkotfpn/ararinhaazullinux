// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Idioma e teclado": o teclado desta conta (muda na hora, também
//! nos apps abertos) e o idioma do sistema (administrador; vale no próximo
//! login).

use std::{
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{componentes as c, tema, Ui};

use crate::{gravar_chave, ler_chave};

const TECLADOS: [(&str, &str); 6] = [
    ("Português (Brasil, ABNT2)", "br"),
    ("Inglês (EUA, internacional, com acentos)", "us intl"),
    ("Inglês (EUA)", "us"),
    ("Português (Portugal)", "pt"),
    ("Espanhol", "es"),
    ("Inglês (Reino Unido)", "gb"),
];

const IDIOMAS: [(&str, &str); 2] = [("Português (Brasil)", "pt_BR.UTF-8"), ("English (US)", "en_US.UTF-8")];

/// Teclado do Primeiro Voo (vale até a conta escolher outro).
fn teclado_do_sistema() -> String {
    std::fs::read_to_string("/etc/plumagem/teclado").map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_else(|_| "br".into())
}

#[derive(Default)]
pub struct PaginaIdioma {
    idioma: Option<String>,
    rx: Option<Receiver<Result<String, String>>>,
    lido: bool,
    pub aviso: Option<String>,
    teste: String,
    teste_ativo: bool,
}

fn radio(ui: &mut Ui, x: f32, y: f32, w: f32, nome: &str, sel: bool) -> bool {
    if sel {
        ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::DESTAQUE_SUAVE());
    } else if ui.sobre(x, y, w, 30.0) {
        ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::SUPERFICIE_ALTA());
    }
    ui.tela.retangulo(x + 10.0, y + 8.0, 14.0, 14.0, 7.0, if sel { tema::DESTAQUE() } else { tema::BORDA() });
    ui.tela.retangulo(x + 12.0, y + 10.0, 10.0, 10.0, 5.0, tema::JANELA());
    if sel {
        ui.tela.retangulo(x + 14.0, y + 12.0, 6.0, 6.0, 3.0, tema::DESTAQUE());
    }
    ui.tela.texto(x + 34.0, y + 20.0, 13.5, tema::TEXTO(), nome);
    ui.clicou(x, y, w, 30.0) && !sel
}

impl PaginaIdioma {
    fn pedir(&mut self, p: String) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(penas::sistema::pedir_admin(&p));
        });
        self.rx = Some(rx);
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if !self.lido {
            self.lido = true;
            self.pedir("idioma estado".into());
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx = None;
                    match r {
                        Ok(t) => {
                            if self.idioma.is_some() {
                                self.aviso = Some("Idioma do sistema trocado. Vale a partir da próxima entrada na conta.".into());
                            }
                            self.idioma = Some(t);
                        }
                        Err(e) => self.aviso = Some(format!("Não deu: {e}.")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(150)),
            }
        }
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Idioma e teclado");

        let mut y = 96.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Teclado");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Só desta conta. Muda na hora, em todos os apps.");
        y += 34.0;
        let atual = ler_chave("teclado").unwrap_or_else(teclado_do_sistema);
        for (nome, id) in TECLADOS {
            if radio(ui, x, y, w, nome, atual == id) {
                let _ = gravar_chave("teclado", id);
            }
            y += 34.0;
        }
        y += 6.0;
        ui.tela.texto(x, y + 4.0, 12.5, tema::TEXTO_SECUNDARIO(), "Teste aqui (ç, acentos, @):");
        if c::campo(ui, x, y + 12.0, w.min(360.0), &self.teste, "Digite para testar", self.teste_ativo, false) {
            self.teste_ativo = true;
        }
        if self.teste_ativo {
            let teclas = std::mem::take(&mut ui.teclas);
            let _ = c::editar(&mut self.teste, teclas);
        }

        y += 80.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Idioma do sistema");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Menus, datas e números. Precisa de administrador; vale no próximo login.");
        y += 34.0;
        let idioma = self.idioma.clone().unwrap_or_default();
        let mut escolhido = None;
        for (nome, id) in IDIOMAS {
            if radio(ui, x, y, w, nome, idioma == id) && self.rx.is_none() && self.idioma.is_some() {
                escolhido = Some(id);
            }
            y += 34.0;
        }
        if let Some(id) = escolhido {
            self.pedir(format!("idioma definir {id}"));
        }
    }
}
