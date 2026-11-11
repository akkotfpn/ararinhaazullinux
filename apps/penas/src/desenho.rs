// SPDX-License-Identifier: GPL-3.0-or-later
//! Desenho em memória das Penas (cópia do desenho da Plumagem, sem as partes
//! do compositor). Pixels RGBA com alfa pré-multiplicado.

use std::{cell::RefCell, collections::HashMap};

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
    //! Cores do tema. Claro ou escuro conforme `tema = claro | escuro` em
    //! ~/.config/plumagem/plumagem.conf (a Penas relê quando o arquivo muda).
    use super::Cor;
    use std::sync::atomic::{AtomicBool, Ordering};

    static ESCURO: AtomicBool = AtomicBool::new(false);

    pub fn escuro() -> bool {
        ESCURO.load(Ordering::Relaxed)
    }
    pub fn definir_escuro(sim: bool) {
        ESCURO.store(sim, Ordering::Relaxed);
    }

    macro_rules! cores {
        ($($(#[$doc:meta])* $nome:ident = $claro:expr, $esc:expr;)*) => {
            $(
                $(#[$doc])*
                #[allow(non_snake_case)]
                pub fn $nome() -> Cor {
                    if escuro() { Cor::hex($esc) } else { Cor::hex($claro) }
                }
            )*
        };
    }

    cores! {
        DESTAQUE = 0x2B7BC4, 0x3B8DD8;
        DESTAQUE_FORTE = 0x1E5FA3, 0x5AA8EC;
        DESTAQUE_SUAVE = 0xD6E8F7, 0x163A5E;
        FUNDO = 0xF4F7FA, 0x0B1A2E;
        SUPERFICIE = 0xFFFFFF, 0x152A44;
        SUPERFICIE_ALTA = 0xEAF0F5, 0x1A3150;
        TEXTO = 0x0B1A2E, 0xEAF2F9;
        TEXTO_SECUNDARIO = 0x4A5A6A, 0x9FB3C6;
        BORDA = 0xD3DDE6, 0x24405F;
        ERRO = 0xC2413B, 0xE06A63;
        /// Fundo das janelas e dos cartões.
        JANELA = 0xFFFFFF, 0x10223A;
        /// Branco de verdade (texto sobre o destaque), nos dois temas.
        BRANCO = 0xFFFFFF, 0xFFFFFF;
    }
}

pub struct Tela {
    pub largura: i32,
    pub altura: i32,
    px: Vec<u8>,
}

impl Tela {
    /// Limpa a tela com uma cor sólida.
    pub fn limpar(&mut self, cor: Cor) {
        let a = cor.3 as f32 / 255.0;
        let p = [
            (cor.0 as f32 * a) as u8,
            (cor.1 as f32 * a) as u8,
            (cor.2 as f32 * a) as u8,
            cor.3,
        ];
        for px in self.px.chunks_exact_mut(4) {
            px.copy_from_slice(&p);
        }
    }

    /// Copia para um buffer wl_shm ARGB8888 (bytes B, G, R, A).
    pub fn copiar_para_argb(&self, destino: &mut [u8]) {
        for (d, o) in destino.chunks_exact_mut(4).zip(self.px.chunks_exact(4)) {
            d[0] = o[2];
            d[1] = o[1];
            d[2] = o[0];
            d[3] = o[3];
        }
    }

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
                            self.misturar(gx + col as i32, gy + row as i32, cor, reforco(v));
                        }
                    }
                }
                caneta += g.avanco;
            }
        });
        caneta - x
    }

    /// Texto com um halo claro atrás, como os títulos do Aero no Windows 7:
    /// continua legível sobre qualquer fundo.
    pub fn texto_com_brilho(&mut self, x: f32, y: f32, tamanho: f32, cor: Cor, texto: &str) -> f32 {
        // Halo leve: forte demais, deixa letras pequenas borradas.
        let halo = if tema::escuro() { Cor(0, 0, 0, 70) } else { tema::BRANCO().com_alfa(45) };
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            self.texto(x + dx, y + dy, tamanho, halo, texto);
        }
        self.texto(x, y, tamanho, cor, texto)
    }

    /// Preenche um polígono (com suavização por subamostras).
    pub fn poligono(&mut self, pontos: &[(f32, f32)], cor: Cor) {
        if pontos.len() < 3 {
            return;
        }
        let minx = pontos.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let maxx = pontos.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().min(self.largura as f32) as i32;
        let miny = pontos.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let maxy = pontos.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(self.altura as f32) as i32;
        const N: i32 = 4;
        for py in miny..maxy {
            for px in minx..maxx {
                let mut dentro = 0;
                for sy in 0..N {
                    for sx in 0..N {
                        let fx = px as f32 + (sx as f32 + 0.5) / N as f32;
                        let fy = py as f32 + (sy as f32 + 0.5) / N as f32;
                        let mut c = false;
                        let mut j = pontos.len() - 1;
                        for i in 0..pontos.len() {
                            let (xi, yi) = pontos[i];
                            let (xj, yj) = pontos[j];
                            if (yi > fy) != (yj > fy) && fx < (xj - xi) * (fy - yi) / (yj - yi) + xi {
                                c = !c;
                            }
                            j = i;
                        }
                        if c {
                            dentro += 1;
                        }
                    }
                }
                self.misturar(px, py, cor, dentro as f32 / (N * N) as f32);
            }
        }
    }

    /// Pinta um pixel (mistura por cima).
    pub fn pintar(&mut self, x: i32, y: i32, cor: Cor) {
        self.misturar(x, y, cor, 1.0);
    }

    /// Pixels crus (RGBA pré-multiplicado).
    pub fn pixels(&self) -> &[u8] {
        &self.px
    }
}


/// Largura do texto em pixels.
/// Cobertura do glifo com "gama" de texto: as bordas finas ficam mais
/// cheias e o texto pequeno fica mais nítido e escuro (sem isso, a Inter
/// rasterizada sem hinting parece clara e fina).
fn reforco(v: u8) -> f32 {
    static TABELA: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABELA.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, x) in t.iter_mut().enumerate() {
            *x = (i as f32 / 255.0).powf(0.72);
        }
        t
    })[v as usize]
}

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
                            
                            return Some(Fonte { fonte, cache: HashMap::new() });
                        }
                        Err(e) => eprintln!("penas: fonte inválida {caminho}: {e}"),
                    }
                }
            }
            eprintln!("penas: nenhuma fonte encontrada; a interface fica sem texto");
            None
        });
        if let Some(fonte) = fonte.as_mut() {
            f(fonte);
        }
    });
}

/// Imagem RGBA pronta para desenhar com [`Tela::imagem`].
pub type Imagem = image::RgbaImage;

/// Imagem PNG embutida, redimensionada para `largura` x `altura`.
pub fn icone(png: &[u8], largura: u32, altura: u32) -> Option<image::RgbaImage> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(image::imageops::resize(&img, largura, altura, image::imageops::FilterType::Triangle))
}