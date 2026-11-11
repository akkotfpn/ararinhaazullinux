// SPDX-License-Identifier: GPL-3.0-or-later
//! Bloco de Notas do Ararinha: abre, edita e salva texto simples.

use std::path::PathBuf;

use penas::{desenho, tema, App, Keysym, Opcoes, Tecla, Ui};

const BARRA_A: f32 = 44.0;
const MARGEM: f32 = 14.0;
const LINHA: f32 = 20.0;
const TAM: f32 = 14.0;

struct Notas {
    arquivo: Option<PathBuf>,
    linhas: Vec<String>,
    /// Cursor: linha e posição em caracteres.
    cl: usize,
    cc: usize,
    rolagem: f32,
    modificado: bool,
    aviso: Option<String>,
    /// Pedindo o nome para salvar (arquivo novo).
    nome_salvar: Option<String>,
}

impl Notas {
    fn novo() -> Notas {
        let arquivo = std::env::args().nth(1).map(PathBuf::from);
        let texto = arquivo.as_ref().and_then(|a| std::fs::read_to_string(a).ok()).unwrap_or_default();
        let mut linhas: Vec<String> = texto.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
        if linhas.is_empty() {
            linhas.push(String::new());
        }
        Notas { arquivo, linhas, cl: 0, cc: 0, rolagem: 0.0, modificado: false, aviso: None, nome_salvar: None }
    }

    fn byte(&self, linha: usize, col: usize) -> usize {
        self.linhas[linha].char_indices().nth(col).map(|(i, _)| i).unwrap_or(self.linhas[linha].len())
    }

    fn n_chars(&self, linha: usize) -> usize {
        self.linhas[linha].chars().count()
    }

    fn inserir(&mut self, t: &str) {
        for ch in t.chars() {
            if ch == '\n' {
                self.quebrar();
            } else {
                let b = self.byte(self.cl, self.cc);
                self.linhas[self.cl].insert(b, ch);
                self.cc += 1;
            }
        }
        self.modificado = true;
    }

    fn quebrar(&mut self) {
        let b = self.byte(self.cl, self.cc);
        let resto = self.linhas[self.cl].split_off(b);
        self.linhas.insert(self.cl + 1, resto);
        self.cl += 1;
        self.cc = 0;
        self.modificado = true;
    }

    fn apagar(&mut self) {
        if self.cc > 0 {
            let b0 = self.byte(self.cl, self.cc - 1);
            let b1 = self.byte(self.cl, self.cc);
            self.linhas[self.cl].replace_range(b0..b1, "");
            self.cc -= 1;
        } else if self.cl > 0 {
            let l = self.linhas.remove(self.cl);
            self.cl -= 1;
            self.cc = self.n_chars(self.cl);
            self.linhas[self.cl].push_str(&l);
        }
        self.modificado = true;
    }

    fn apagar_frente(&mut self) {
        if self.cc < self.n_chars(self.cl) {
            let b0 = self.byte(self.cl, self.cc);
            let b1 = self.byte(self.cl, self.cc + 1);
            self.linhas[self.cl].replace_range(b0..b1, "");
        } else if self.cl + 1 < self.linhas.len() {
            let l = self.linhas.remove(self.cl + 1);
            self.linhas[self.cl].push_str(&l);
        }
        self.modificado = true;
    }

    fn salvar(&mut self) {
        let Some(a) = self.arquivo.clone() else {
            self.nome_salvar = Some("Sem título.txt".into());
            return;
        };
        match std::fs::write(&a, self.linhas.join("\n")) {
            Ok(()) => {
                self.modificado = false;
                self.aviso = Some("Salvo.".into());
            }
            Err(e) => self.aviso = Some(format!("Não deu para salvar: {e}")),
        }
    }

    fn titulo(&self) -> String {
        let nome = self
            .arquivo
            .as_ref()
            .and_then(|a| a.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Sem título".into());
        format!("{}{nome} – Bloco de Notas", if self.modificado { "• " } else { "" })
    }
}

impl App for Notas {
    fn desenhar(&mut self, ui: &mut Ui) {
        let (l, a) = (ui.largura(), ui.altura());
        let teclas = std::mem::take(&mut ui.teclas);

        // Pedindo nome para salvar (vai para Documentos).
        if let Some(mut nome) = self.nome_salvar.take() {
            let mut fechar = false;
            for t in teclas {
                match t {
                    Tecla::Texto(s) => nome.push_str(&s),
                    Tecla::Apagar => {
                        nome.pop();
                    }
                    Tecla::Esc => fechar = true,
                    Tecla::Enter => {
                        let casa = std::env::var("HOME").unwrap_or_else(|_| "/".into());
                        let p = PathBuf::from(&casa).join("Documentos").join(nome.trim());
                        self.arquivo = Some(p);
                        self.salvar();
                        fechar = true;
                    }
                    _ => {}
                }
            }
            if !fechar {
                self.nome_salvar = Some(nome);
            }
        } else {
            for t in teclas {
                match t {
                    Tecla::Texto(s) => self.inserir(&s),
                    Tecla::Enter => self.quebrar(),
                    Tecla::Apagar => self.apagar(),
                    Tecla::Outra(Keysym::Delete) => self.apagar_frente(),
                    Tecla::Outra(Keysym::Tab) => self.inserir("    "),
                    Tecla::Outra(k) if ui.ctrl && (k == Keysym::s || k == Keysym::S) => self.salvar(),
                    Tecla::Esquerda | Tecla::Voltar => {
                        if self.cc > 0 {
                            self.cc -= 1;
                        } else if self.cl > 0 {
                            self.cl -= 1;
                            self.cc = self.n_chars(self.cl);
                        }
                    }
                    Tecla::Direita | Tecla::Avancar => {
                        if self.cc < self.n_chars(self.cl) {
                            self.cc += 1;
                        } else if self.cl + 1 < self.linhas.len() {
                            self.cl += 1;
                            self.cc = 0;
                        }
                    }
                    Tecla::Cima | Tecla::Subir => {
                        if self.cl > 0 {
                            self.cl -= 1;
                            self.cc = self.cc.min(self.n_chars(self.cl));
                        }
                    }
                    Tecla::Baixo => {
                        if self.cl + 1 < self.linhas.len() {
                            self.cl += 1;
                            self.cc = self.cc.min(self.n_chars(self.cl));
                        }
                    }
                    Tecla::Outra(Keysym::Home) => self.cc = 0,
                    Tecla::Outra(Keysym::End) => self.cc = self.n_chars(self.cl),
                    _ => {}
                }
                self.aviso = None;
            }
        }
        ui.titulo = Some(self.titulo());

        ui.tela.limpar(tema::JANELA());
        let topo = BARRA_A;
        let visivel = a - topo - 28.0;

        // Clique posiciona o cursor.
        if ui.clique {
            if let Some((mx, my)) = ui.mouse {
                if my > topo {
                    let lin = (((my - topo - 8.0 + self.rolagem) / LINHA).floor().max(0.0) as usize).min(self.linhas.len() - 1);
                    self.cl = lin;
                    let mut col = 0;
                    let alvo = mx - MARGEM;
                    let mut acum = String::new();
                    for ch in self.linhas[lin].chars() {
                        acum.push(ch);
                        if desenho::medir(TAM, &acum) > alvo + 3.0 {
                            break;
                        }
                        col += 1;
                    }
                    self.cc = col;
                }
            }
        }

        self.rolagem = (self.rolagem + ui.rolagem * 2.0).max(0.0);
        let cy = self.cl as f32 * LINHA;
        if cy < self.rolagem {
            self.rolagem = cy;
        } else if cy + LINHA > self.rolagem + visivel {
            self.rolagem = cy + LINHA - visivel;
        }

        let primeiro = (self.rolagem / LINHA).floor() as usize;
        for i in primeiro..self.linhas.len() {
            let y = topo + 8.0 + i as f32 * LINHA - self.rolagem;
            if y > a - 28.0 {
                break;
            }
            ui.tela.texto(MARGEM, y + 15.0, TAM, tema::TEXTO(), &self.linhas[i]);
            if i == self.cl && self.nome_salvar.is_none() {
                let antes: String = self.linhas[i].chars().take(self.cc).collect();
                let x = MARGEM + desenho::medir(TAM, &antes);
                ui.tela.retangulo(x, y + 2.0, 1.5, LINHA - 2.0, 0.0, tema::DESTAQUE());
            }
        }

        // Barra de cima: Salvar.
        ui.tela.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA());
        let sobre = ui.sobre(10.0, 7.0, 96.0, 30.0);
        ui.tela.retangulo(10.0, 7.0, 96.0, 30.0, 6.0, if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        ui.tela.texto(24.0, 27.0, 13.0, tema::BRANCO(), "Salvar");
        if ui.clicou(10.0, 7.0, 96.0, 30.0) {
            self.salvar();
        }
        ui.tela.texto(120.0, 27.0, 12.0, tema::TEXTO_SECUNDARIO(), "Ctrl+S salva");

        // Barra de baixo: posição e avisos.
        ui.tela.retangulo(0.0, a - 26.0, l, 26.0, 0.0, tema::FUNDO());
        let pos = format!("Linha {}, coluna {}", self.cl + 1, self.cc + 1);
        ui.tela.texto(12.0, a - 8.0, 12.0, tema::TEXTO_SECUNDARIO(), &pos);
        if let Some(av) = &self.aviso {
            let lt = desenho::medir(12.0, av);
            ui.tela.texto(l - 12.0 - lt, a - 8.0, 12.0, tema::TEXTO(), av);
        }

        if let Some(nome) = &self.nome_salvar {
            let (cl, ca) = (420.0, 130.0);
            let (x, y) = ((l - cl) / 2.0, (a - ca) / 2.0);
            ui.tela.retangulo(x + 2.0, y + 4.0, cl, ca, 10.0, penas::Cor(0, 0, 0, 50));
            ui.tela.retangulo(x, y, cl, ca, 10.0, tema::BORDA());
            ui.tela.retangulo(x + 1.0, y + 1.0, cl - 2.0, ca - 2.0, 9.0, tema::JANELA());
            ui.tela.texto(x + 18.0, y + 30.0, 14.0, tema::TEXTO(), "Salvar em Documentos como:");
            ui.tela.retangulo(x + 16.0, y + 46.0, cl - 32.0, 34.0, 6.0, tema::DESTAQUE());
            ui.tela.retangulo(x + 18.0, y + 48.0, cl - 36.0, 30.0, 5.0, tema::JANELA());
            let fim = ui.tela.texto(x + 26.0, y + 68.0, 14.0, tema::TEXTO(), nome);
            ui.tela.retangulo(x + 27.0 + fim, y + 53.0, 1.5, 20.0, 0.0, tema::DESTAQUE());
            ui.tela.texto(x + 18.0, y + 108.0, 12.0, tema::TEXTO_SECUNDARIO(), "Enter salva · Esc cancela");
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Bloco de Notas".into(),
            app_id: "notas".into(),
            tamanho: (820, 600),
            minimo: (360, 240),
        },
        Box::new(Notas::novo()),
    );
}
