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
    /// Início da seleção (a outra ponta é o cursor).
    ancora: Option<(usize, usize)>,
    /// Desfazer/refazer: texto e cursor de antes de cada mudança.
    desfazer: Vec<(Vec<String>, (usize, usize))>,
    refazer: Vec<(Vec<String>, (usize, usize))>,
    /// Última mudança foi digitação (junta letras seguidas num desfazer só).
    digitando: Option<std::time::Instant>,
    arrastando: bool,
}

impl Notas {
    fn novo() -> Notas {
        let arquivo = std::env::args().nth(1).map(PathBuf::from);
        let texto = arquivo.as_ref().and_then(|a| std::fs::read_to_string(a).ok()).unwrap_or_default();
        let mut linhas: Vec<String> = texto.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
        if linhas.is_empty() {
            linhas.push(String::new());
        }
        Notas {
            arquivo,
            linhas,
            cl: 0,
            cc: 0,
            rolagem: 0.0,
            modificado: false,
            aviso: None,
            nome_salvar: None,
            ancora: None,
            desfazer: Vec::new(),
            refazer: Vec::new(),
            digitando: None,
            arrastando: false,
        }
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

    /// Seleção em ordem: (início, fim), se houver.
    fn faixa(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.ancora?;
        let c = (self.cl, self.cc);
        if a == c {
            return None;
        }
        Some(if a < c { (a, c) } else { (c, a) })
    }

    fn texto_selecionado(&self) -> String {
        let Some(((l0, c0), (l1, c1))) = self.faixa() else { return String::new() };
        if l0 == l1 {
            return self.linhas[l0].chars().skip(c0).take(c1 - c0).collect();
        }
        let mut t: String = self.linhas[l0].chars().skip(c0).collect();
        for l in l0 + 1..l1 {
            t.push('\n');
            t.push_str(&self.linhas[l]);
        }
        t.push('\n');
        t.extend(self.linhas[l1].chars().take(c1));
        t
    }

    /// Apaga a seleção (se houver) e põe o cursor no início dela.
    fn apagar_selecao(&mut self) -> bool {
        let Some(((l0, c0), (l1, c1))) = self.faixa() else {
            self.ancora = None;
            return false;
        };
        let fim: String = self.linhas[l1].chars().skip(c1).collect();
        let b0 = self.byte(l0, c0);
        self.linhas[l0].truncate(b0);
        self.linhas[l0].push_str(&fim);
        self.linhas.drain(l0 + 1..=l1);
        self.cl = l0;
        self.cc = c0;
        self.ancora = None;
        self.modificado = true;
        true
    }

    /// Guarda o estado para o Ctrl+Z. `digitar` junta letras seguidas.
    fn lembrar(&mut self, digitar: bool) {
        let agora = std::time::Instant::now();
        let juntar = digitar && self.digitando.is_some_and(|t| agora.duration_since(t).as_millis() < 1500);
        self.digitando = if digitar { Some(agora) } else { None };
        if juntar {
            return;
        }
        self.desfazer.push((self.linhas.clone(), (self.cl, self.cc)));
        if self.desfazer.len() > 200 {
            self.desfazer.remove(0);
        }
        self.refazer.clear();
    }

    fn voltar_estado(&mut self, refazer: bool) {
        let (de, para) = if refazer { (&mut self.refazer, &mut self.desfazer) } else { (&mut self.desfazer, &mut self.refazer) };
        let Some((linhas, (l, c))) = de.pop() else { return };
        para.push((std::mem::replace(&mut self.linhas, linhas), (self.cl, self.cc)));
        self.cl = l.min(self.linhas.len() - 1);
        self.cc = c.min(self.n_chars(self.cl));
        self.ancora = None;
        self.digitando = None;
        self.modificado = true;
    }

    /// Move o cursor; com Shift, estende a seleção.
    fn mover(&mut self, l: usize, c: usize, shift: bool) {
        if shift {
            if self.ancora.is_none() {
                self.ancora = Some((self.cl, self.cc));
            }
        } else {
            self.ancora = None;
        }
        self.cl = l.min(self.linhas.len() - 1);
        self.cc = c.min(self.n_chars(self.cl));
        self.digitando = None;
    }

    /// Começo da palavra anterior / fim da próxima (Ctrl+setas).
    fn palavra(&self, frente: bool) -> (usize, usize) {
        let (mut l, mut c) = (self.cl, self.cc);
        let chars: Vec<char> = self.linhas[l].chars().collect();
        if frente {
            if c >= chars.len() {
                return if l + 1 < self.linhas.len() { (l + 1, 0) } else { (l, c) };
            }
            while c < chars.len() && !chars[c].is_alphanumeric() {
                c += 1;
            }
            while c < chars.len() && chars[c].is_alphanumeric() {
                c += 1;
            }
        } else {
            if c == 0 {
                if l > 0 {
                    l -= 1;
                    return (l, self.n_chars(l));
                }
                return (0, 0);
            }
            while c > 0 && !chars[c - 1].is_alphanumeric() {
                c -= 1;
            }
            while c > 0 && chars[c - 1].is_alphanumeric() {
                c -= 1;
            }
        }
        (l, c)
    }

    /// Posição (linha, coluna) do ponto clicado.
    fn posicao_em(&self, mx: f32, my: f32) -> (usize, usize) {
        let lin = (((my - BARRA_A - 8.0 + self.rolagem) / LINHA).floor().max(0.0) as usize).min(self.linhas.len() - 1);
        let alvo = mx - MARGEM;
        let mut col = 0;
        let mut acum = String::new();
        for ch in self.linhas[lin].chars() {
            acum.push(ch);
            let w = desenho::medir(TAM, &acum);
            let wc = desenho::medir(TAM, &ch.to_string());
            if w - wc / 2.0 > alvo {
                break;
            }
            col += 1;
        }
        (lin, col)
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
            let (ctrl, shift) = (ui.ctrl, ui.shift);
            let visiveis = ((ui.altura() - BARRA_A - 28.0) / LINHA).floor().max(1.0) as usize;
            for t in teclas {
                let letra = |k: Keysym, a: Keysym, b: Keysym| k == a || k == b;
                match t {
                    Tecla::Texto(s) | Tecla::Colado(s) => {
                        let digitar = s.chars().count() == 1;
                        self.lembrar(digitar);
                        self.apagar_selecao();
                        self.inserir(&s.replace("\r\n", "\n").replace('\r', "\n"));
                    }
                    Tecla::Enter => {
                        self.lembrar(false);
                        self.apagar_selecao();
                        self.quebrar();
                    }
                    Tecla::Apagar => {
                        self.lembrar(true);
                        if !self.apagar_selecao() {
                            self.apagar();
                        }
                    }
                    Tecla::Outra(Keysym::BackSpace) => {
                        // Ctrl+Backspace: apaga a palavra antes do cursor.
                        self.lembrar(false);
                        if !self.apagar_selecao() {
                            let (l, c) = self.palavra(false);
                            self.ancora = Some((l, c));
                            self.apagar_selecao();
                        }
                    }
                    Tecla::Outra(Keysym::Delete) => {
                        self.lembrar(false);
                        if !self.apagar_selecao() {
                            if ctrl {
                                let (l, c) = self.palavra(true);
                                self.ancora = Some((l, c));
                                self.apagar_selecao();
                            } else {
                                self.apagar_frente();
                            }
                        }
                    }
                    Tecla::Outra(Keysym::Tab) => {
                        self.lembrar(false);
                        self.apagar_selecao();
                        self.inserir("    ");
                    }
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::s, Keysym::S) => self.salvar(),
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::a, Keysym::A) => {
                        self.ancora = Some((0, 0));
                        self.cl = self.linhas.len() - 1;
                        self.cc = self.n_chars(self.cl);
                    }
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::c, Keysym::C) => {
                        let t = self.texto_selecionado();
                        if !t.is_empty() {
                            penas::copiar(&t);
                        }
                    }
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::x, Keysym::X) => {
                        let t = self.texto_selecionado();
                        if !t.is_empty() {
                            penas::copiar(&t);
                            self.lembrar(false);
                            self.apagar_selecao();
                        }
                    }
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::v, Keysym::V) => penas::pedir_colar(),
                    Tecla::Outra(k) if ctrl && (letra(k, Keysym::y, Keysym::Y) || shift && letra(k, Keysym::z, Keysym::Z)) => self.voltar_estado(true),
                    Tecla::Outra(k) if ctrl && letra(k, Keysym::z, Keysym::Z) => self.voltar_estado(false),
                    Tecla::Esquerda | Tecla::Voltar => {
                        if !shift && self.faixa().is_some() {
                            let ((l, c), _) = self.faixa().unwrap();
                            self.mover(l, c, false);
                        } else if ctrl {
                            let (l, c) = self.palavra(false);
                            self.mover(l, c, shift);
                        } else if self.cc > 0 {
                            self.mover(self.cl, self.cc - 1, shift);
                        } else if self.cl > 0 {
                            let l = self.cl - 1;
                            self.mover(l, self.n_chars(l), shift);
                        } else {
                            self.mover(0, 0, shift);
                        }
                    }
                    Tecla::Direita | Tecla::Avancar => {
                        if !shift && self.faixa().is_some() {
                            let (_, (l, c)) = self.faixa().unwrap();
                            self.mover(l, c, false);
                        } else if ctrl {
                            let (l, c) = self.palavra(true);
                            self.mover(l, c, shift);
                        } else if self.cc < self.n_chars(self.cl) {
                            self.mover(self.cl, self.cc + 1, shift);
                        } else if self.cl + 1 < self.linhas.len() {
                            self.mover(self.cl + 1, 0, shift);
                        } else {
                            self.mover(self.cl, self.cc, shift);
                        }
                    }
                    Tecla::Cima | Tecla::Subir => {
                        let l = self.cl.saturating_sub(1);
                        let c = if self.cl == 0 { 0 } else { self.cc };
                        self.mover(l, c, shift);
                    }
                    Tecla::Baixo => {
                        if self.cl + 1 < self.linhas.len() {
                            self.mover(self.cl + 1, self.cc, shift);
                        } else {
                            let c = self.n_chars(self.cl);
                            self.mover(self.cl, c, shift);
                        }
                    }
                    Tecla::Outra(Keysym::Home) => {
                        if ctrl {
                            self.mover(0, 0, shift);
                        } else {
                            self.mover(self.cl, 0, shift);
                        }
                    }
                    Tecla::Outra(Keysym::End) => {
                        if ctrl {
                            let l = self.linhas.len() - 1;
                            self.mover(l, self.n_chars(l), shift);
                        } else {
                            self.mover(self.cl, self.n_chars(self.cl), shift);
                        }
                    }
                    Tecla::Outra(Keysym::Page_Up) => self.mover(self.cl.saturating_sub(visiveis), self.cc, shift),
                    Tecla::Outra(Keysym::Page_Down) => self.mover(self.cl + visiveis, self.cc, shift),
                    _ => {}
                }
                self.aviso = None;
            }
        }
        ui.titulo = Some(self.titulo());

        ui.tela.limpar(tema::JANELA());
        let topo = BARRA_A;
        let visivel = a - topo - 28.0;

        // Clique posiciona o cursor; arrastar seleciona; clique duplo
        // seleciona a palavra; Shift+clique estende.
        if let Some((mx, my)) = ui.mouse {
            if my > topo && my < a - 26.0 && self.nome_salvar.is_none() {
                if ui.clique {
                    let (l, c) = self.posicao_em(mx, my);
                    if ui.duplo {
                        self.cl = l;
                        self.cc = c;
                        let (_, ini) = { let (l0, c0) = self.palavra(false); (l0, c0) };
                        let (_, fim) = self.palavra(true);
                        let fim = if fim < c { c } else { fim };
                        self.ancora = Some((l, ini.min(c)));
                        self.cc = fim;
                    } else {
                        self.mover(l, c, ui.shift);
                        if !ui.shift {
                            self.ancora = Some((l, c));
                        }
                        self.arrastando = true;
                    }
                } else if self.arrastando && ui.pressionado {
                    let (l, c) = self.posicao_em(mx, my);
                    self.cl = l;
                    self.cc = c;
                    ui.acordar_em = Some(std::time::Duration::from_millis(30));
                }
            }
        }
        if !ui.pressionado {
            self.arrastando = false;
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
            // Seleção desta linha.
            if let Some(((l0, c0), (l1, c1))) = self.faixa() {
                if i >= l0 && i <= l1 {
                    let ini = if i == l0 { c0 } else { 0 };
                    let fim = if i == l1 { c1 } else { self.n_chars(i) };
                    let x0 = MARGEM + desenho::medir(TAM, &self.linhas[i].chars().take(ini).collect::<String>());
                    let mut x1 = MARGEM + desenho::medir(TAM, &self.linhas[i].chars().take(fim).collect::<String>());
                    if i < l1 {
                        x1 += 6.0; // a quebra de linha também está selecionada
                    }
                    ui.tela.retangulo(x0, y + 1.0, (x1 - x0).max(0.0), LINHA, 0.0, tema::DESTAQUE_SUAVE());
                }
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
        ui.tela.texto(120.0, 27.0, 12.0, tema::TEXTO_SECUNDARIO(), "Ctrl+S salva · Ctrl+Z desfaz · Ctrl+A seleciona tudo");

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
