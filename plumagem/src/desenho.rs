// SPDX-License-Identifier: GPL-3.0-or-later
//! Desenho em memória para a interface da Plumagem (barra de tarefas, menu
//! iniciar, barras de título). Tudo é desenhado pelo processador numa tela
//! RGBA e enviado à GPU como um `MemoryRenderBuffer`; só é redesenhado quando
//! algo visível muda.
//!
//! Os pixels ficam com alfa pré-multiplicado, que é como o renderizador do
//! Smithay mistura as camadas.

use std::{cell::RefCell, collections::HashMap};

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Rectangle, Transform},
};

/// Cor em RGBA (0–255), sem pré-multiplicar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cor(pub u8, pub u8, pub u8, pub u8);

impl Cor {
    pub const fn hex(rgb: u32) -> Cor {
        Cor((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }
    pub const fn com_alfa(self, a: u8) -> Cor {
        Cor(self.0, self.1, self.2, a)
    }
}

/// Cores do tema Spix (`arte/temas/spix.toml`).
pub mod tema {
    use super::Cor;
    pub const DESTAQUE: Cor = Cor::hex(0x2B7BC4);
    pub const DESTAQUE_FORTE: Cor = Cor::hex(0x1E5FA3);
    pub const DESTAQUE_SUAVE: Cor = Cor::hex(0xD6E8F7);
    pub const FUNDO: Cor = Cor::hex(0xF4F7FA);
    pub const SUPERFICIE: Cor = Cor::hex(0xFFFFFF);
    pub const SUPERFICIE_ALTA: Cor = Cor::hex(0xEAF0F5);
    pub const TEXTO: Cor = Cor::hex(0x0B1A2E);
    pub const TEXTO_SECUNDARIO: Cor = Cor::hex(0x4A5A6A);
    pub const BORDA: Cor = Cor::hex(0xD3DDE6);
    pub const ERRO: Cor = Cor::hex(0xC2413B);
    pub const BRANCO: Cor = Cor::hex(0xFFFFFF);
}

pub struct Tela {
    pub largura: i32,
    pub altura: i32,
    px: Vec<u8>,
}

impl Tela {
    pub fn nova(largura: i32, altura: i32) -> Tela {
        let (l, a) = (largura.max(1), altura.max(1));
        Tela { largura: l, altura: a, px: vec![0; (l * a * 4) as usize] }
    }

    /// Mistura um pixel com cobertura `cob` (0.0–1.0).
    #[inline]
    fn misturar(&mut self, x: i32, y: i32, cor: Cor, cob: f32) {
        if x < 0 || y < 0 || x >= self.largura || y >= self.altura || cob <= 0.0 {
            return;
        }
        let a = (cor.3 as f32 / 255.0) * cob.min(1.0);
        if a <= 0.0 {
            return;
        }
        let i = ((y * self.largura + x) * 4) as usize;
        let inv = 1.0 - a;
        let p = &mut self.px[i..i + 4];
        p[0] = (cor.0 as f32 * a + p[0] as f32 * inv).round() as u8;
        p[1] = (cor.1 as f32 * a + p[1] as f32 * inv).round() as u8;
        p[2] = (cor.2 as f32 * a + p[2] as f32 * inv).round() as u8;
        p[3] = (255.0 * a + p[3] as f32 * inv).round() as u8;
    }

    /// Retângulo com cantos arredondados e borda suavizada.
    pub fn retangulo(&mut self, x: f32, y: f32, l: f32, a: f32, raio: f32, cor: Cor) {
        if l <= 0.0 || a <= 0.0 {
            return;
        }
        let r = raio.min(l / 2.0).min(a / 2.0).max(0.0);
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x + l).ceil() as i32, (y + a).ceil() as i32);
        let (cx, cy) = (x + l / 2.0, y + a / 2.0);
        let (hx, hy) = (l / 2.0 - r, a / 2.0 - r);
        for py in y0.max(0)..y1.min(self.altura) {
            for px in x0.max(0)..x1.min(self.largura) {
                // Distância com sinal até um retângulo arredondado.
                let qx = ((px as f32 + 0.5 - cx).abs() - hx).max(0.0);
                let qy = ((py as f32 + 0.5 - cy).abs() - hy).max(0.0);
                let dentro_x = (px as f32 + 0.5 - cx).abs() - hx;
                let dentro_y = (py as f32 + 0.5 - cy).abs() - hy;
                let d = (qx * qx + qy * qy).sqrt() + dentro_x.max(dentro_y).min(0.0) - r;
                let cob = (0.5 - d).clamp(0.0, 1.0);
                // Bordas retas sem cantos: recorta pela área do pixel.
                let cob_x = ((px as f32 + 1.0).min(x + l) - (px as f32).max(x)).clamp(0.0, 1.0);
                let cob_y = ((py as f32 + 1.0).min(y + a) - (py as f32).max(y)).clamp(0.0, 1.0);
                self.misturar(px, py, cor, cob.min(cob_x).min(cob_y));
            }
        }
    }

    /// Segmento de reta com espessura (para desenhar ícones simples).
    pub fn linha(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, esp: f32, cor: Cor) {
        let (minx, maxx) = (x0.min(x1) - esp, x0.max(x1) + esp);
        let (miny, maxy) = (y0.min(y1) - esp, y0.max(y1) + esp);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let comp2 = (dx * dx + dy * dy).max(1e-6);
        for py in (miny.floor() as i32).max(0)..(maxy.ceil() as i32).min(self.altura) {
            for px in (minx.floor() as i32).max(0)..(maxx.ceil() as i32).min(self.largura) {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let t = (((fx - x0) * dx + (fy - y0) * dy) / comp2).clamp(0.0, 1.0);
                let (qx, qy) = (x0 + t * dx - fx, y0 + t * dy - fy);
                let d = (qx * qx + qy * qy).sqrt() - esp / 2.0;
                self.misturar(px, py, cor, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Contorno de retângulo (1 linha de cada lado).
    pub fn contorno(&mut self, x: f32, y: f32, l: f32, a: f32, esp: f32, cor: Cor) {
        self.linha(x, y, x + l, y, esp, cor);
        self.linha(x, y + a, x + l, y + a, esp, cor);
        self.linha(x, y, x, y + a, esp, cor);
        self.linha(x + l, y, x + l, y + a, esp, cor);
    }

    /// Copia uma imagem RGBA (não pré-multiplicada) para a posição dada.
    pub fn imagem(&mut self, x: i32, y: i32, img: &image::RgbaImage) {
        for (ix, iy, p) in img.enumerate_pixels() {
            let c = Cor(p[0], p[1], p[2], 255);
            self.misturar(x + ix as i32, y + iy as i32, c, p[3] as f32 / 255.0);
        }
    }

    /// Escreve texto com a linha de base em `y`. Devolve a largura usada.
    pub fn texto(&mut self, x: f32, y: f32, tamanho: f32, cor: Cor, texto: &str) -> f32 {
        let mut caneta = x;
        com_fonte(|fonte| {
            for ch in texto.chars() {
                let g = fonte.glifo(ch, tamanho);
                let gx = (caneta + g.xmin as f32).round() as i32;
                let gy = (y - g.ymin as f32 - g.altura as f32).round() as i32;
                for row in 0..g.altura {
                    for col in 0..g.largura {
                        let v = g.cobertura[row * g.largura + col];
                        if v > 0 {
                            self.misturar(gx + col as i32, gy + row as i32, cor, v as f32 / 255.0);
                        }
                    }
                }
                caneta += g.avanco;
            }
        });
        caneta - x
    }

    pub fn em_buffer(&self) -> MemoryRenderBuffer {
        MemoryRenderBuffer::from_slice(
            &self.px,
            // Bytes R, G, B, A em memória = Abgr8888 no DRM (little-endian).
            Fourcc::Abgr8888,
            (self.largura, self.altura),
            1,
            Transform::Normal,
            Some(vec![Rectangle::from_size((self.largura, self.altura).into())]),
        )
    }
}

/// Largura do texto em pixels.
pub fn medir(tamanho: f32, texto: &str) -> f32 {
    let mut l = 0.0;
    com_fonte(|fonte| {
        for ch in texto.chars() {
            l += fonte.glifo(ch, tamanho).avanco;
        }
    });
    l
}

/// Corta o texto com "…" para caber em `max` pixels.
pub fn encurtar(tamanho: f32, texto: &str, max: f32) -> String {
    if medir(tamanho, texto) <= max {
        return texto.to_string();
    }
    let mut s: String = texto.to_string();
    while !s.is_empty() {
        s.pop();
        let t = format!("{}…", s.trim_end());
        if medir(tamanho, &t) <= max {
            return t;
        }
    }
    String::new()
}

struct Glifo {
    xmin: i32,
    ymin: i32,
    largura: usize,
    altura: usize,
    avanco: f32,
    cobertura: Vec<u8>,
}

struct Fonte {
    fonte: fontdue::Font,
    cache: HashMap<(char, u32), Glifo>,
}

impl Fonte {
    fn glifo(&mut self, ch: char, tamanho: f32) -> &Glifo {
        let chave = (ch, (tamanho * 10.0) as u32);
        let fonte = &self.fonte;
        self.cache.entry(chave).or_insert_with(|| {
            let (m, cobertura) = fonte.rasterize(ch, tamanho);
            Glifo {
                xmin: m.xmin,
                ymin: m.ymin,
                largura: m.width,
                altura: m.height,
                avanco: m.advance_width,
                cobertura,
            }
        })
    }
}

/// Fontes procuradas, na ordem: Inter (interface), JetBrains Mono, DejaVu.
const CAMINHOS_FONTE: &[&str] = &[
    "/usr/share/fonts/inter/InterVariable.ttf",
    "/usr/share/fonts/inter/Inter-Regular.ttf",
    "/usr/share/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

thread_local! {
    static FONTE: RefCell<Option<Option<Fonte>>> = const { RefCell::new(None) };
}

fn com_fonte(f: impl FnOnce(&mut Fonte)) {
    FONTE.with(|celula| {
        let mut celula = celula.borrow_mut();
        let fonte = celula.get_or_insert_with(|| {
            for caminho in CAMINHOS_FONTE {
                if let Ok(bytes) = std::fs::read(caminho) {
                    match fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                        Ok(fonte) => {
                            tracing::info!("fonte da interface: {caminho}");
                            return Some(Fonte { fonte, cache: HashMap::new() });
                        }
                        Err(e) => tracing::warn!("fonte inválida {caminho}: {e}"),
                    }
                }
            }
            tracing::warn!("nenhuma fonte encontrada: a interface fica sem texto");
            None
        });
        if let Some(fonte) = fonte.as_mut() {
            f(fonte);
        }
    });
}

/// Imagem PNG embutida, redimensionada para `largura` x `altura`.
pub fn icone(png: &[u8], largura: u32, altura: u32) -> Option<image::RgbaImage> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(image::imageops::resize(&img, largura, altura, image::imageops::FilterType::Triangle))
}
