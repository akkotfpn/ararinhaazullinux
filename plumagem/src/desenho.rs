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
    utils::Transform,
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

    /// Cores de destaque que a pessoa pode escolher: nome e, para claro e
    /// escuro, (destaque, forte, suave).
    pub const DESTAQUES: [(&str, [u32; 3], [u32; 3]); 6] = [
        ("azul", [0x2B7BC4, 0x1E5FA3, 0xD6E8F7], [0x3B8DD8, 0x5AA8EC, 0x163A5E]),
        ("verde", [0x2E8B57, 0x1F6B42, 0xD5EFE0], [0x3FAE6F, 0x62C98C, 0x173D2A]),
        ("roxo", [0x7B4FC9, 0x5E37A6, 0xE6DCF7], [0x9670DC, 0xB091E8, 0x2E2150]),
        ("laranja", [0xD9731A, 0xB35A0C, 0xFAE3CF], [0xE88B3A, 0xF0A562, 0x45290F]),
        ("rosa", [0xC94A86, 0xA3336A, 0xF6D9E7], [0xDC6CA0, 0xE88EB7, 0x47203A]),
        ("grafite", [0x4A5A6A, 0x34414E, 0xE1E6EB], [0x8EA0B2, 0xAAB9C8, 0x24303D]),
    ];

    static DESTAQUE_ATUAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// Escolhe a cor de destaque pelo nome ("azul", "verde"...).
    pub fn definir_destaque(nome: &str) {
        let i = DESTAQUES.iter().position(|d| d.0 == nome).unwrap_or(0);
        DESTAQUE_ATUAL.store(i, Ordering::Relaxed);
    }

    pub fn destaque_atual() -> &'static str {
        DESTAQUES[DESTAQUE_ATUAL.load(Ordering::Relaxed).min(DESTAQUES.len() - 1)].0
    }

    fn destaque(k: usize) -> Cor {
        let d = &DESTAQUES[DESTAQUE_ATUAL.load(Ordering::Relaxed).min(DESTAQUES.len() - 1)];
        Cor::hex(if escuro() { d.2[k] } else { d.1[k] })
    }

    /// Cor de destaque (botões principais, seleção).
    #[allow(non_snake_case)]
    pub fn DESTAQUE() -> Cor {
        destaque(0)
    }
    #[allow(non_snake_case)]
    pub fn DESTAQUE_FORTE() -> Cor {
        destaque(1)
    }
    #[allow(non_snake_case)]
    pub fn DESTAQUE_SUAVE() -> Cor {
        destaque(2)
    }

    cores! {
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
    /// continua legível sobre o vidro, qualquer que seja o fundo.
    pub fn texto_com_brilho(&mut self, x: f32, y: f32, tamanho: f32, cor: Cor, texto: &str) -> f32 {
        // Halo leve: forte demais, deixa letras pequenas borradas.
        let halo = if tema::escuro() { Cor(0, 0, 0, 70) } else { tema::BRANCO().com_alfa(45) };
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            self.texto(x + dx, y + dy, tamanho, halo, texto);
        }
        self.texto(x, y, tamanho, cor, texto)
    }

    /// Vidro (efeito Aero): o papel de parede desfocado atrás do retângulo,
    /// uma tinta por cima, um brilho na metade de cima e borda fina.
    /// `origem` é a posição deste desenho na tela, para alinhar o desfoque.
    #[allow(clippy::too_many_arguments)]
    pub fn vidro(&mut self, x: f32, y: f32, l: f32, a: f32, raio: f32, origem: (i32, i32), tinta: Cor) {
        // Efeitos visuais desligados: cor sólida, sem desfoque nem brilho.
        if !EFEITOS.load(std::sync::atomic::Ordering::Relaxed) {
            self.retangulo(x, y, l, a, raio, tema::BORDA());
            self.retangulo(x + 1.0, y + 1.0, l - 2.0, a - 2.0, (raio - 1.0).max(0.0), tema::SUPERFICIE());
            self.retangulo(x + 1.0, y + 1.0, l - 2.0, a - 2.0, (raio - 1.0).max(0.0), tinta.com_alfa(255));
            return;
        }
        // Borda escura translúcida e, por dentro, o desfoque.
        self.retangulo(x, y, l, a, raio, Cor(11, 26, 46, 90));
        let (x, y, l, a, raio) = (x + 1.0, y + 1.0, l - 2.0, a - 2.0, (raio - 1.0).max(0.0));
        let tem_fundo = VIDRO.with(|v| {
            let v = v.borrow();
            let Some(v) = v.as_ref() else { return false };
            let mascara = {
                let mut m = Tela::nova(self.largura, self.altura);
                m.retangulo(x, y, l, a, raio, tema::BRANCO());
                m
            };
            for py in (y.floor() as i32).max(0)..((y + a).ceil() as i32).min(self.altura) {
                for px in (x.floor() as i32).max(0)..((x + l).ceil() as i32).min(self.largura) {
                    let cob = mascara.px[((py * self.largura + px) * 4 + 3) as usize] as f32 / 255.0;
                    if cob <= 0.0 {
                        continue;
                    }
                    let sx = (origem.0 + px).clamp(0, v.l - 1);
                    let sy = (origem.1 + py).clamp(0, v.a - 1);
                    let i = ((sy * v.l + sx) * 3) as usize;
                    self.misturar(px, py, Cor(v.px[i], v.px[i + 1], v.px[i + 2], 255), cob);
                }
            }
            true
        });
        if !tem_fundo {
            self.retangulo(x, y, l, a, raio, tema::SUPERFICIE().com_alfa(200));
        }
        self.retangulo(x, y, l, a, raio, tinta);
        // Brilho: branco que some do topo até a metade.
        let metade = (a / 2.0).max(1.0);
        let mut linha = 0.0;
        while linha < metade {
            let alfa = (26.0 * (1.0 - linha / metade)) as u8;
            self.retangulo_recortado(x, y, l, a, raio, y + linha, tema::BRANCO().com_alfa(alfa));
            linha += 1.0;
        }
        // Reflexo de 1 px logo abaixo da borda de cima.
        self.retangulo_recortado(x, y, l, a, raio, y, tema::BRANCO().com_alfa(70));
    }

    /// Uma linha horizontal (1 px, na altura `yl`) dentro do retângulo
    /// arredondado dado, respeitando os cantos.
    #[allow(clippy::too_many_arguments)]
    fn retangulo_recortado(&mut self, x: f32, y: f32, l: f32, a: f32, raio: f32, yl: f32, cor: Cor) {
        let py = yl.floor() as i32;
        if py < 0 || py >= self.altura {
            return;
        }
        let (cx, cy) = (x + l / 2.0, y + a / 2.0);
        let r = raio.min(l / 2.0).min(a / 2.0).max(0.0);
        let (hx, hy) = (l / 2.0 - r, a / 2.0 - r);
        for px in (x.floor() as i32).max(0)..((x + l).ceil() as i32).min(self.largura) {
            let dx = (px as f32 + 0.5 - cx).abs() - hx;
            let dy = (py as f32 + 0.5 - cy).abs() - hy;
            let (qx, qy) = (dx.max(0.0), dy.max(0.0));
            let d = (qx * qx + qy * qy).sqrt() + dx.max(dy).min(0.0) - r;
            self.misturar(px, py, cor, (0.5 - d).clamp(0.0, 1.0));
        }
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

    pub fn em_buffer(&self) -> MemoryRenderBuffer {
        MemoryRenderBuffer::from_slice(
            &self.px,
            // Bytes R, G, B, A em memória = Abgr8888 no DRM (little-endian).
            Fourcc::Abgr8888,
            (self.largura, self.altura),
            1,
            Transform::Normal,
            // Sem região opaca: a barra é translúcida e o menu tem cantos
            // arredondados; com a tela inteira marcada como opaca, o que
            // está atrás não era desenhado e aparecia preto/cinza.
            None,
        )
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

/// Foto da conta (`~/.face`, PNG gravado pelas Configurações), recortada
/// num círculo com borda suave e um aro branco fino.
pub fn foto_da_conta(casa: &str, lado: u32) -> Option<image::RgbaImage> {
    let bytes = std::fs::read(format!("{casa}/.face")).ok()?;
    if bytes.len() > 8 * 1024 * 1024 {
        return None;
    }
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let q = img.width().min(img.height());
    let recorte = image::imageops::crop_imm(&img, (img.width() - q) / 2, (img.height() - q) / 2, q, q).to_image();
    let mut r = image::imageops::resize(&recorte, lado, lado, image::imageops::FilterType::Triangle);
    let c = lado as f32 / 2.0;
    for (x, y, p) in r.enumerate_pixels_mut() {
        let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
        let dentro = (c - d).clamp(0.0, 1.0);
        let aro = (1.0 - ((d - (c - 1.5)).abs() - 0.75).clamp(0.0, 1.0)) * dentro;
        for k in 0..3 {
            p[k] = (p[k] as f32 * (1.0 - aro * 0.85) + 255.0 * aro * 0.85) as u8;
        }
        p[3] = (p[3] as f32 * dentro) as u8;
    }
    Some(r)
}

/// Papel de parede desfocado na resolução da tela, base do efeito de vidro.
struct Vidro {
    l: i32,
    a: i32,
    versao: u32,
    px: Vec<u8>,
}

thread_local! {
    static VIDRO: RefCell<Option<Vidro>> = const { RefCell::new(None) };
}

/// Efeitos visuais (vidro, desfoque, transparência). Configurações ›
/// Personalização › Efeitos visuais ("efeitos = nao" desliga).
pub static EFEITOS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Prepara (uma vez por tamanho de tela) o papel de parede desfocado.
/// Usa o mesmo enquadramento do papel de parede ("preencher", centralizado).
pub fn preparar_vidro(l: i32, a: i32) {
    if l <= 0 || a <= 0 {
        return;
    }
    let versao = crate::fundo::versao();
    let pronto = VIDRO.with(|v| {
        v.borrow().as_ref().map(|v| v.l == l && v.a == a && v.versao == versao).unwrap_or(false)
    });
    if pronto {
        return;
    }
    let inicio = std::time::Instant::now();
    let img = crate::fundo::imagem_escolhida().to_rgb8();
    // O desfoque é feito em 1/4 da resolução (mais rápido e mais suave).
    const Q: f64 = 4.0;
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let fator = f64::max(l as f64 / iw, a as f64 / ih) / Q;
    let (sw, sh) = ((iw * fator).round().max(1.0) as u32, (ih * fator).round().max(1.0) as u32);
    let pequena = image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle);
    let (ql, qa) = (((l as f64) / Q).ceil() as u32, ((a as f64) / Q).ceil() as u32);
    let ox = (sw.saturating_sub(ql)) / 2;
    let oy = (sh.saturating_sub(qa)) / 2;
    let mut recorte = image::imageops::crop_imm(&pequena, ox, oy, ql.min(sw), qa.min(sh)).to_image();
    for _ in 0..3 {
        desfocar(&mut recorte, 5);
    }
    let cheia = image::imageops::resize(&recorte, l as u32, a as u32, image::imageops::FilterType::Triangle);
    VIDRO.with(|v| *v.borrow_mut() = Some(Vidro { l, a, versao, px: cheia.into_raw() }));
    tracing::info!(l, a, tempo = ?inicio.elapsed(), "vidro preparado");
}

/// Desfoque por média móvel (uma passada horizontal e uma vertical).
fn desfocar(img: &mut image::RgbImage, raio: i32) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if w == 0 || h == 0 {
        return;
    }
    let janela = (2 * raio + 1) as u32;
    let mut tmp = img.clone();
    for y in 0..h {
        for c in 0..3 {
            let mut soma: u32 = 0;
            for k in -raio..=raio {
                soma += img.get_pixel(k.clamp(0, w - 1) as u32, y as u32)[c] as u32;
            }
            for x in 0..w {
                tmp.get_pixel_mut(x as u32, y as u32)[c] = (soma / janela) as u8;
                let sai = (x - raio).clamp(0, w - 1) as u32;
                let entra = (x + raio + 1).clamp(0, w - 1) as u32;
                soma = soma + img.get_pixel(entra, y as u32)[c] as u32 - img.get_pixel(sai, y as u32)[c] as u32;
            }
        }
    }
    for x in 0..w {
        for c in 0..3 {
            let mut soma: u32 = 0;
            for k in -raio..=raio {
                soma += tmp.get_pixel(x as u32, k.clamp(0, h - 1) as u32)[c] as u32;
            }
            for y in 0..h {
                img.get_pixel_mut(x as u32, y as u32)[c] = (soma / janela) as u8;
                let sai = (y - raio).clamp(0, h - 1) as u32;
                let entra = (y + raio + 1).clamp(0, h - 1) as u32;
                soma = soma + tmp.get_pixel(x as u32, entra)[c] as u32 - tmp.get_pixel(x as u32, sai)[c] as u32;
            }
        }
    }
}
