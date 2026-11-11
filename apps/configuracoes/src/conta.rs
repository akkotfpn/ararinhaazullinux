// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Conta": trocar a senha da própria conta (pelo ararinha-senha).

use std::{
    io::Write,
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{desenho, tema, Tecla, Ui};

#[derive(Default)]
pub struct PaginaConta {
    campos: [String; 3],
    foco: usize,
    pub aviso: Option<String>,
    rx: Option<Receiver<i32>>,
}

impl PaginaConta {
    fn enviar(&mut self) {
        let [atual, nova, confirma] = &self.campos;
        if nova != confirma {
            self.aviso = Some("A senha nova e a confirmação estão diferentes.".into());
            return;
        }
        if nova.chars().count() < 4 {
            self.aviso = Some("A senha nova precisa ter pelo menos 4 caracteres.".into());
            return;
        }
        let entrada = format!("{atual}\n{nova}\n");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = Command::new("ararinha-senha")
                .arg("--trocar")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .and_then(|mut f| {
                    if let Some(mut e) = f.stdin.take() {
                        let _ = e.write_all(entrada.as_bytes());
                    }
                    f.wait()
                })
                .ok()
                .and_then(|s| s.code())
                .unwrap_or(2);
            let _ = tx.send(r);
        });
        self.rx = Some(rx);
        self.aviso = Some("Trocando a senha…".into());
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, _l: f32) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(c) => {
                    self.rx = None;
                    self.aviso = Some(match c {
                        0 => {
                            self.campos = Default::default();
                            "Senha trocada. Use a nova senha para entrar e desbloquear.".into()
                        }
                        1 => "A senha atual está errada.".into(),
                        _ => "Não deu para trocar a senha.".into(),
                    });
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(200)),
            }
        }
        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Texto(s) => self.campos[self.foco].push_str(&s),
                Tecla::Apagar => {
                    self.campos[self.foco].pop();
                }
                Tecla::Outra(penas::Keysym::Tab) | Tecla::Baixo => self.foco = (self.foco + 1) % 3,
                Tecla::Cima => self.foco = (self.foco + 2) % 3,
                Tecla::Enter => {
                    if self.foco < 2 {
                        self.foco += 1;
                    } else if self.rx.is_none() {
                        self.enviar();
                    }
                }
                _ => {}
            }
        }
        let usuario = std::env::var("USER").unwrap_or_default();
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO(), "Conta");
        ui.tela.texto(x0 + 32.0, 84.0, 13.0, tema::TEXTO_SECUNDARIO(), &format!("Você entrou como \"{usuario}\". Troque aqui a sua senha."));
        let rotulos = ["Senha atual", "Senha nova", "Confirme a senha nova"];
        for (i, r) in rotulos.iter().enumerate() {
            let y = 120.0 + i as f32 * 70.0;
            ui.tela.texto(x0 + 32.0, y, 12.5, tema::TEXTO_SECUNDARIO(), r);
            let (fx, fy, fl) = (x0 + 32.0, y + 8.0, 340.0);
            let foco = self.foco == i;
            ui.tela.retangulo(fx, fy, fl, 38.0, 7.0, if foco { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(fx + 1.0, fy + 1.0, fl - 2.0, 36.0, 6.0, tema::JANELA());
            let pontos = "•".repeat(self.campos[i].chars().count().min(40));
            let fim = ui.tela.texto(fx + 12.0, fy + 25.0, 14.0, tema::TEXTO(), &pontos);
            if foco {
                ui.tela.retangulo(fx + 13.0 + fim, fy + 9.0, 1.5, 20.0, 0.0, tema::DESTAQUE());
            }
            if ui.clicou(fx, fy, fl, 38.0) {
                self.foco = i;
            }
        }
        let (bx, by) = (x0 + 32.0, 330.0);
        let ativo = self.rx.is_none() && !self.campos[1].is_empty();
        let s = ativo && ui.sobre(bx, by, 150.0, 38.0);
        ui.tela.retangulo(bx, by, 150.0, 38.0, 8.0, if !ativo { tema::SUPERFICIE_ALTA() } else if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        let rot = "Alterar senha";
        let lr = desenho::medir(13.5, rot);
        ui.tela.texto(bx + 75.0 - lr / 2.0, by + 24.0, 13.5, if ativo { tema::BRANCO() } else { tema::TEXTO_SECUNDARIO() }, rot);
        if ativo && ui.clicou(bx, by, 150.0, 38.0) {
            self.enviar();
        }
    }
}
