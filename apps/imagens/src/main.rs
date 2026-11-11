// SPDX-License-Identifier: GPL-3.0-or-later
//! Visualizador de Imagens do Ararinha: abre PNG, JPEG, GIF, BMP e WebP,
//! ajusta à janela e navega pelas outras imagens da mesma pasta.

use std::path::{Path, PathBuf};

use penas::{desenho, tema, App, Cor, Opcoes, Tecla, Ui};

const EXTENSOES: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"];
const BARRA_A: f32 = 48.0;

struct Visualizador {
    lista: Vec<PathBuf>,
    atual: usize,
    imagem: Option<image::RgbaImage>,
    erro: Option<String>,
    /// Cache da imagem já reduzida para a janela: (largura, altura, imagem).
    reduzida: Option<(u32, u32, image::RgbaImage)>,
}

fn e_imagem(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSOES.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

impl Visualizador {
    fn novo() -> Visualizador {
        let arquivo = std::env::args().nth(1).map(PathBuf::from);
        let mut lista = Vec::new();
        let mut atual = 0;
        if let Some(a) = &arquivo {
            if let Some(dir) = a.parent() {
                if let Ok(d) = std::fs::read_dir(dir) {
                    lista = d.flatten().map(|e| e.path()).filter(|p| e_imagem(p)).collect();
                    lista.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                }
            }
            atual = lista.iter().position(|p| p == a).unwrap_or(0);
            if lista.is_empty() {
                lista.push(a.clone());
            }
        }
        let mut v = Visualizador { lista, atual, imagem: None, erro: None, reduzida: None };
        v.carregar();
        v
    }

    fn carregar(&mut self) {
        self.reduzida = None;
        self.imagem = None;
        self.erro = None;
        let Some(p) = self.lista.get(self.atual) else {
            self.erro = Some("Abra uma imagem pelo Explorador de Arquivos.".into());
            return;
        };
        match image::open(p) {
            Ok(img) => self.imagem = Some(img.to_rgba8()),
            Err(e) => self.erro = Some(format!("Não deu para abrir esta imagem: {e}")),
        }
    }

    fn mover(&mut self, d: i32) {
        let n = self.lista.len() as i32;
        if n > 1 {
            self.atual = (self.atual as i32 + d).rem_euclid(n) as usize;
            self.carregar();
        }
    }
}

impl App for Visualizador {
    fn desenhar(&mut self, ui: &mut Ui) {
        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Direita => self.mover(1),
                Tecla::Esquerda => self.mover(-1),
                Tecla::Esc => ui.fechar = true,
                _ => {}
            }
        }
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(Cor::hex(0x1B2430));

        // Área da imagem.
        let (al, aa) = (l - 24.0, a - BARRA_A - 24.0);
        if let Some(img) = &self.imagem {
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let f = (al / iw).min(aa / ih).min(1.0);
            let (w, h) = ((iw * f).max(1.0) as u32, (ih * f).max(1.0) as u32);
            let precisa = self.reduzida.as_ref().map(|(rw, rh, _)| *rw != w || *rh != h).unwrap_or(true);
            if precisa {
                let r = if f < 1.0 {
                    image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle)
                } else {
                    img.clone()
                };
                self.reduzida = Some((w, h, r));
            }
            if let Some((w, h, r)) = &self.reduzida {
                let x = (l - *w as f32) / 2.0;
                let y = 12.0 + (aa - *h as f32) / 2.0;
                ui.tela.imagem(x as i32, y as i32, r);
            }
        } else if let Some(e) = &self.erro {
            let lt = desenho::medir(14.0, e);
            ui.tela.texto((l - lt) / 2.0, a / 2.0, 14.0, tema::BRANCO(), e);
        }

        // Barra de baixo: anterior, nome, próxima.
        let by = a - BARRA_A;
        ui.tela.retangulo(0.0, by, l, BARRA_A, 0.0, Cor::hex(0x111821));
        let nome = self
            .lista
            .get(self.atual)
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let info = match &self.imagem {
            Some(i) => format!("{nome}   ·   {} × {}   ·   {} de {}", i.width(), i.height(), self.atual + 1, self.lista.len()),
            None => nome.clone(),
        };
        let info = desenho::encurtar(13.0, &info, l - 140.0);
        let lt = desenho::medir(13.0, &info);
        ui.tela.texto((l - lt) / 2.0, by + 29.0, 13.0, Cor::hex(0xDCE4EC), &info);
        for (i, (x, d)) in [(12.0, -1), (l - 52.0, 1)].iter().enumerate() {
            let sobre = ui.sobre(*x, by + 6.0, 40.0, 36.0);
            if sobre {
                ui.tela.retangulo(*x, by + 6.0, 40.0, 36.0, 8.0, Cor(255, 255, 255, 40));
            }
            let (cx, cy) = (x + 20.0, by + 24.0);
            let s = if i == 0 { 1.0 } else { -1.0 };
            ui.tela.linha(cx + 4.0 * s, cy - 7.0, cx - 3.0 * s, cy, 2.0, tema::BRANCO());
            ui.tela.linha(cx - 3.0 * s, cy, cx + 4.0 * s, cy + 7.0, 2.0, tema::BRANCO());
            if ui.clicou(*x, by + 6.0, 40.0, 36.0) {
                self.mover(*d);
            }
        }
        ui.titulo = Some(if nome.is_empty() { "Visualizador de Imagens".into() } else { format!("{nome} – Imagens") });
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Visualizador de Imagens".into(),
            app_id: "imagens".into(),
            tamanho: (960, 680),
            minimo: (400, 300),
        },
        Box::new(Visualizador::novo()),
    );
}
