// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Firewall": estado, perfil (rede pública ou de casa) e portas
//! liberadas. As mudanças vão pelo serviço do sistema (ararinha-sistema),
//! que confere se quem pediu é administrador.

use std::{
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{componentes as c, desenho, tema, Tecla, Ui};

#[derive(Default)]
pub struct PaginaFirewall {
    ativo: Option<bool>,
    casa: bool,
    portas: Vec<String>,
    nova: String,
    udp: bool,
    foco: bool,
    pub aviso: Option<String>,
    rx: Option<Receiver<Result<String, String>>>,
    lido: bool,
    /// Pediu para desligar: mostra o aviso e espera a confirmação.
    confirmar_desligar: bool,
}

impl PaginaFirewall {
    fn pedir(&mut self, pedido: String) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(penas::sistema::pedir(&pedido));
        });
        self.rx = Some(rx);
    }

    fn aplicar_estado(&mut self, texto: &str) {
        for parte in texto.split_whitespace() {
            match parte.split_once('=') {
                Some(("ativo", v)) => self.ativo = Some(v == "sim"),
                Some(("perfil", v)) => self.casa = v == "casa",
                Some(("portas", v)) => {
                    self.portas = v.split(',').filter(|p| !p.is_empty()).map(str::to_string).collect();
                }
                _ => {}
            }
        }
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Com o diálogo aberto, nada embaixo dele responde a cliques.
        let modal = self.confirmar_desligar;
        let clique_real = ui.clique;
        if modal {
            ui.clique = false;
        }
        if !self.lido {
            self.lido = true;
            self.pedir("firewall estado".into());
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx = None;
                    match r {
                        Ok(t) => self.aplicar_estado(&t),
                        Err(e) => self.aviso = Some(format!("Não deu: {e}.")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(150)),
            }
        }
        if self.foco {
            let teclas = std::mem::take(&mut ui.teclas);
            let mut texto = std::mem::take(&mut self.nova);
            for t in c::editar(&mut texto, teclas) {
                if t == Tecla::Enter {
                    self.foco = false;
                }
            }
            self.nova = texto.chars().filter(|c| c.is_ascii_digit()).take(5).collect();
        }

        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Firewall");
        ui.tela.texto(x, 84.0, 13.0, tema::TEXTO_SECUNDARIO(), "Bloqueia as conexões que chegam sem você ter pedido.");

        // Estado.
        let (cor, txt) = match self.ativo {
            Some(true) => (penas::Cor::hex(0x2E9E5B), "Ligado: este computador está protegido"),
            Some(false) => (tema::ERRO(), "Desligado: o firewall não está ativo"),
            None => (tema::TEXTO_SECUNDARIO(), "Conferindo…"),
        };
        ui.tela.retangulo(x, 104.0, w, 48.0, 8.0, tema::SUPERFICIE_ALTA());
        ui.tela.retangulo(x + 16.0, 120.0, 16.0, 16.0, 8.0, cor);
        ui.tela.texto(x + 44.0, 133.0, 14.0, tema::TEXTO(), txt);
        // Ligar/desligar, com confirmação antes de desligar.
        if let Some(ativo) = self.ativo {
            let livre = self.rx.is_none();
            if ativo {
                let rot = "Desligar";
                if c::botao(ui, x + w - c::largura_botao(rot) - 6.0, 110.0, rot, c::Estilo::Comum, livre) {
                    self.confirmar_desligar = true;
                }
            } else {
                let rot = "Ligar";
                if c::botao(ui, x + w - c::largura_botao(rot) - 6.0, 110.0, rot, c::Estilo::Principal, livre) {
                    self.pedir("firewall ligar".into());
                }
            }
        }


        // Perfis.
        ui.tela.texto(x, 182.0, 16.0, tema::TEXTO(), "Tipo de rede");
        let perfis = [
            (false, "Rede pública", "Café, aeroporto, hotel: o computador fica invisível para os outros."),
            (true, "Rede de casa ou do trabalho", "Os outros aparelhos da rede conseguem achar este computador (ping)."),
        ];
        let mut novo_perfil = None;
        for (i, (casa, nome, desc)) in perfis.iter().enumerate() {
            let y = 196.0 + i as f32 * 70.0;
            let sel = self.casa == *casa;
            let s = ui.sobre(x, y, w, 60.0);
            ui.tela.retangulo(x, y, w, 60.0, 8.0, if sel { tema::DESTAQUE() } else if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
            let e = if sel { 2.0 } else { 1.0 };
            ui.tela.retangulo(x + e, y + e, w - 2.0 * e, 60.0 - 2.0 * e, 8.0 - e, if sel { tema::DESTAQUE_SUAVE() } else { tema::JANELA() });
            ui.tela.retangulo(x + 18.0, y + 21.0, 18.0, 18.0, 9.0, if sel { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() });
            ui.tela.retangulo(x + 20.0, y + 23.0, 14.0, 14.0, 7.0, tema::JANELA());
            if sel {
                ui.tela.retangulo(x + 23.0, y + 26.0, 8.0, 8.0, 4.0, tema::DESTAQUE());
            }
            ui.tela.texto(x + 50.0, y + 25.0, 14.0, tema::TEXTO(), nome);
            ui.tela.texto(x + 50.0, y + 45.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, desc, w - 64.0));
            if ui.clicou(x, y, w, 60.0) && !sel && self.rx.is_none() {
                novo_perfil = Some(*casa);
            }
        }
        if let Some(casa) = novo_perfil {
            self.casa = casa;
            self.pedir(format!("firewall perfil {}", if casa { "casa" } else { "publica" }));
        }

        // Portas liberadas.
        let mut y = 350.0;
        ui.tela.texto(x, y, 16.0, tema::TEXTO(), "Portas liberadas");
        ui.tela.texto(x, y + 20.0, 12.0, tema::TEXTO_SECUNDARIO(), "Só libere uma porta se um programa seu precisar receber conexões (um jogo, um servidor).");
        y += 34.0;
        if self.portas.is_empty() {
            ui.tela.texto(x, y + 18.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhuma: tudo o que chega sem ser pedido é bloqueado.");
            y += 32.0;
        }
        let mut fechar = None;
        for p in self.portas.clone() {
            ui.tela.retangulo(x, y, w, 36.0, 6.0, tema::SUPERFICIE_ALTA());
            ui.tela.texto(x + 14.0, y + 23.0, 13.5, tema::TEXTO(), &format!("Porta {}", p.replace('/', " · ").to_uppercase()));
            let rot = "Bloquear de novo";
            let bw = c::largura_botao(rot);
            if c::botao(ui, x + w - bw, y, rot, c::Estilo::Comum, self.rx.is_none()) {
                fechar = Some(p.clone());
            }
            y += 42.0;
        }
        if let Some(p) = fechar {
            self.pedir(format!("firewall fechar {p}"));
        }
        // Nova porta: número + TCP/UDP + Liberar.
        if c::campo(ui, x, y + 4.0, 120.0, &self.nova, "Nº da porta", self.foco, false) {
            self.foco = true;
        }
        let mut udp = self.udp;
        let rot_proto = if udp { "UDP" } else { "TCP" };
        if c::botao(ui, x + 132.0, y + 4.0, rot_proto, c::Estilo::Comum, true) {
            udp = !udp;
        }
        self.udp = udp;
        let numero: Option<u16> = self.nova.parse().ok().filter(|n| *n > 0);
        let px = x + 132.0 + c::largura_botao(rot_proto) + 8.0;
        if c::botao(ui, px, y + 4.0, "Liberar porta", c::Estilo::Principal, numero.is_some() && self.rx.is_none()) {
            if let Some(n) = numero {
                self.pedir(format!("firewall abrir {n}/{}", if self.udp { "udp" } else { "tcp" }));
                self.nova.clear();
                self.foco = false;
            }
        }
        if modal {
            ui.clique = clique_real;
            // Diálogo por cima da página.
            let (dl, da) = (w.min(460.0), 186.0);
            let (dx, dy) = (x + (w - dl) / 2.0, 150.0);
            ui.tela.retangulo(0.0, 0.0, ui.largura(), ui.altura(), 0.0, penas::Cor(0, 0, 0, 90));
            ui.tela.retangulo(dx - 1.0, dy - 1.0, dl + 2.0, da + 2.0, 13.0, tema::BORDA());
            ui.tela.retangulo(dx, dy, dl, da, 12.0, tema::JANELA());
            ui.tela.texto(dx + 24.0, dy + 38.0, 17.0, tema::TEXTO(), "Desligar o firewall?");
            c::paragrafo(ui, dx + 24.0, dy + 66.0, dl - 48.0, "Qualquer aparelho da rede poderá tentar se conectar a este computador. Só desligue para testar algo e ligue de novo logo depois.");
            let by = dy + da - 56.0;
            if c::botao_direita(ui, dx + dl - 24.0, by, "Desligar", c::Estilo::Perigo, true) {
                self.confirmar_desligar = false;
                self.pedir("firewall desligar".into());
            }
            let lv = c::largura_botao("Desligar") + 10.0;
            if c::botao_direita(ui, dx + dl - 24.0 - lv, by, "Cancelar", c::Estilo::Comum, true) {
                self.confirmar_desligar = false;
            }
        }
    }
}
