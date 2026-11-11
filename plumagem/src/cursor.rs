use std::{io::Read, time::Duration};

use tracing::warn;
use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

/// Seta desenhada pela Plumagem quando não há tema de cursor instalado:
/// branca com contorno preto e uma sombra leve, no tamanho do Windows.
fn seta_plumagem() -> Image {
    const LADO: i32 = 32;
    let mut t = crate::desenho::Tela::nova(LADO, LADO);
    let e = 0.95; // escala da seta (≈ 17 px de altura, como o Windows a 100%)
    let base = [
        (0.0, 0.0),
        (0.0, 16.0),
        (4.0, 12.3),
        (6.8, 18.6),
        (9.4, 17.5),
        (6.7, 11.4),
        (11.6, 11.4),
    ];
    let pts = |dx: f32, dy: f32| -> Vec<(f32, f32)> {
        base.iter().map(|(x, y)| (1.5 + dx + x * e, 1.5 + dy + y * e)).collect()
    };
    let preto = crate::desenho::Cor(0, 0, 0, 255);
    t.poligono(&pts(0.8, 1.2), crate::desenho::Cor(0, 0, 0, 60));
    let seta = pts(0.0, 0.0);
    t.poligono(&seta, crate::desenho::Cor(255, 255, 255, 255));
    for i in 0..seta.len() {
        let (a, b) = (seta[i], seta[(i + 1) % seta.len()]);
        t.linha(a.0, a.1, b.0, b.1, 1.3, preto);
    }
    Image {
        size: 24,
        width: LADO as u32,
        height: LADO as u32,
        xhot: 2,
        yhot: 2,
        delay: 1,
        pixels_rgba: t.pixels().to_vec(),
        pixels_argb: vec![],
    }
}

pub struct Cursor {
    icons: Vec<Image>,
    size: u32,
}

impl Cursor {
    pub fn load() -> Cursor {
        let name = std::env::var("XCURSOR_THEME")
            .ok()
            .unwrap_or_else(|| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        let theme = CursorTheme::load(&name);
        let icons = load_icon(&theme)
            .map_err(|err| warn!("Unable to load xcursor: {}, using fallback cursor", err))
            .unwrap_or_else(|_| vec![seta_plumagem()]);

        Cursor { icons, size }
    }

    pub fn get_image(&self, scale: u32, time: Duration) -> Image {
        let size = self.size * scale;
        frame(time.as_millis() as u32, size, &self.icons)
    }
}

fn nearest_images(size: u32, images: &[Image]) -> impl Iterator<Item = &Image> {
    // Follow the nominal size of the cursor to choose the nearest
    let nearest_image = images
        .iter()
        .min_by_key(|image| (size as i32 - image.size as i32).abs())
        .unwrap();

    images
        .iter()
        .filter(move |image| image.width == nearest_image.width && image.height == nearest_image.height)
}

fn frame(mut millis: u32, size: u32, images: &[Image]) -> Image {
    let total = nearest_images(size, images).fold(0, |acc, image| acc + image.delay);
    if total == 0 {
        return nearest_images(size, images).next().unwrap().clone();
    }
    millis %= total;

    for img in nearest_images(size, images) {
        if millis < img.delay {
            return img.clone();
        }
        millis -= img.delay;
    }

    unreachable!()
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Theme has no default cursor")]
    NoDefaultCursor,
    #[error("Error opening xcursor file: {0}")]
    File(#[from] std::io::Error),
    #[error("Failed to parse XCursor file")]
    Parse,
}

fn load_icon(theme: &CursorTheme) -> Result<Vec<Image>, Error> {
    let icon_path = theme.load_icon("default").ok_or(Error::NoDefaultCursor)?;
    let mut cursor_file = std::fs::File::open(icon_path)?;
    let mut cursor_data = Vec::new();
    cursor_file.read_to_end(&mut cursor_data)?;
    parse_xcursor(&cursor_data).ok_or(Error::Parse)
}

// ---------------------------------------------------------------------------
// Formas de cursor da Plumagem (desenhadas, sem depender de tema instalado).

use smithay::input::pointer::CursorIcon;

use crate::{
    desenho::{Cor, Tela},
    shell::ResizeEdge,
};

/// As formas que a Plumagem desenha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Forma {
    Seta,
    Texto,
    Mao,
    Espera,
    Mover,
    Proibido,
    Mira,
    RedimH,
    RedimV,
    /// Diagonal ↖↘ (noroeste–sudeste).
    RedimNoSe,
    /// Diagonal ↗↙ (nordeste–sudoeste).
    RedimNeSo,
}

impl Forma {
    /// Forma para um cursor pedido pelo programa (protocolo cursor-shape).
    pub fn de_icone(i: CursorIcon) -> Forma {
        use CursorIcon as C;
        match i {
            C::Text | C::VerticalText => Forma::Texto,
            C::Pointer | C::Grab | C::Grabbing | C::Alias | C::Copy => Forma::Mao,
            C::Wait | C::Progress => Forma::Espera,
            C::Move | C::AllScroll => Forma::Mover,
            C::NotAllowed | C::NoDrop => Forma::Proibido,
            C::Crosshair | C::Cell => Forma::Mira,
            C::EResize | C::WResize | C::EwResize | C::ColResize => Forma::RedimH,
            C::NResize | C::SResize | C::NsResize | C::RowResize => Forma::RedimV,
            C::NwResize | C::SeResize | C::NwseResize => Forma::RedimNoSe,
            C::NeResize | C::SwResize | C::NeswResize => Forma::RedimNeSo,
            _ => Forma::Seta,
        }
    }

    /// Forma para redimensionar pelas bordas dadas.
    pub fn de_bordas(e: ResizeEdge) -> Forma {
        let h = e.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT);
        let v = e.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM);
        match (h, v) {
            (true, true) => {
                let no_se = e.contains(ResizeEdge::TOP_LEFT) || e.contains(ResizeEdge::BOTTOM_RIGHT);
                if no_se { Forma::RedimNoSe } else { Forma::RedimNeSo }
            }
            (true, false) => Forma::RedimH,
            _ => Forma::RedimV,
        }
    }
}

const LADO: i32 = 32;
const PRETO: Cor = Cor(0, 0, 0, 255);
const BRANCO: Cor = Cor(255, 255, 255, 255);

/// Polígono branco com contorno preto e sombra leve.
fn contornado(t: &mut Tela, pts: &[(f32, f32)]) {
    let sombra: Vec<(f32, f32)> = pts.iter().map(|(x, y)| (x + 0.8, y + 1.2)).collect();
    t.poligono(&sombra, Cor(0, 0, 0, 60));
    t.poligono(pts, BRANCO);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        t.linha(a.0, a.1, b.0, b.1, 1.3, PRETO);
    }
}

/// Seta dupla (←→) girada pelo ângulo dado, centrada no meio da imagem.
fn seta_dupla(t: &mut Tela, angulo: f32) {
    let base = [
        (-10.5, 0.0),
        (-5.5, -5.0),
        (-5.5, -1.6),
        (5.5, -1.6),
        (5.5, -5.0),
        (10.5, 0.0),
        (5.5, 5.0),
        (5.5, 1.6),
        (-5.5, 1.6),
        (-5.5, 5.0),
    ];
    let (s, c) = angulo.to_radians().sin_cos();
    let m = LADO as f32 / 2.0;
    let k = 0.85;
    let pts: Vec<(f32, f32)> = base.iter().map(|(x, y)| (m + (x * c - y * s) * k, m + (x * s + y * c) * k)).collect();
    contornado(t, &pts);
}

fn desenhar(forma: Forma) -> (Tela, (i32, i32)) {
    let mut t = Tela::nova(LADO, LADO);
    let m = LADO as f32 / 2.0;
    let centro = (LADO / 2, LADO / 2);
    let quente = match forma {
        Forma::Seta => {
            let img = seta_plumagem();
            return (tela_de_imagem(&img), (2, 2));
        }
        Forma::Texto => {
            // I-beam: haste com serifas.
            for (cor, esp) in [(BRANCO, 3.6), (PRETO, 1.4)] {
                t.linha(m, m - 8.0, m, m + 8.0, esp, cor);
                t.linha(m - 3.5, m - 9.0, m + 3.5, m - 9.0, esp, cor);
                t.linha(m - 3.5, m + 9.0, m + 3.5, m + 9.0, esp, cor);
            }
            centro
        }
        Forma::Mao => {
            // Mão apontando: dedo indicador para cima e a palma.
            let dedo = [(9.0, 3.0), (12.0, 3.0), (12.5, 4.0), (12.5, 11.0), (21.0, 12.5), (22.5, 14.0),
                (22.5, 21.0), (20.0, 26.0), (11.5, 26.0), (6.0, 19.0), (5.5, 17.0), (6.5, 16.0), (8.0, 16.2),
                (8.5, 4.0)];
            contornado(&mut t, &dedo);
            for x in [14.5, 18.0] {
                t.linha(x, 13.0, x, 17.0, 1.0, PRETO);
            }
            (10, 3)
        }
        Forma::Espera => {
            // Anel azul-ararinha com um arco mais forte.
            for k in 0..64 {
                let a = k as f32 / 64.0 * std::f32::consts::TAU;
                let (s, c) = a.sin_cos();
                let cor = if k < 20 { Cor(0x2B, 0x7B, 0xC4, 255) } else { Cor(0xD6, 0xE8, 0xF7, 255) };
                t.linha(m + 7.0 * c, m + 7.0 * s, m + 7.01 * c, m + 7.01 * s, 4.0, cor);
            }
            centro
        }
        Forma::Mover => {
            seta_dupla(&mut t, 0.0);
            seta_dupla(&mut t, 90.0);
            centro
        }
        Forma::Proibido => {
            for k in 0..72 {
                let a = k as f32 / 72.0 * std::f32::consts::TAU;
                let (s, c) = a.sin_cos();
                t.linha(m + 8.0 * c, m + 8.0 * s, m + 8.01 * c, m + 8.01 * s, 3.0, Cor(0xC2, 0x41, 0x3B, 255));
            }
            t.linha(m - 5.5, m - 5.5, m + 5.5, m + 5.5, 3.0, Cor(0xC2, 0x41, 0x3B, 255));
            centro
        }
        Forma::Mira => {
            for (cor, esp) in [(BRANCO, 3.4), (PRETO, 1.2)] {
                t.linha(m - 9.0, m, m + 9.0, m, esp, cor);
                t.linha(m, m - 9.0, m, m + 9.0, esp, cor);
            }
            centro
        }
        Forma::RedimH => {
            seta_dupla(&mut t, 0.0);
            centro
        }
        Forma::RedimV => {
            seta_dupla(&mut t, 90.0);
            centro
        }
        Forma::RedimNoSe => {
            seta_dupla(&mut t, 45.0);
            centro
        }
        Forma::RedimNeSo => {
            seta_dupla(&mut t, -45.0);
            centro
        }
    };
    (t, quente)
}

fn tela_de_imagem(img: &Image) -> Tela {
    let mut t = Tela::nova(img.width as i32, img.height as i32);
    for (i, p) in img.pixels_rgba.chunks_exact(4).enumerate() {
        if p[3] > 0 {
            // Os pixels já vêm pré-multiplicados; desfaz para misturar.
            let a = p[3] as f32 / 255.0;
            let c = |v: u8| ((v as f32 / a).round().min(255.0)) as u8;
            let (x, y) = (i as i32 % img.width as i32, i as i32 / img.width as i32);
            t.pintar(x, y, Cor(c(p[0]), c(p[1]), c(p[2]), p[3]));
        }
    }
    t
}

thread_local! {
    static FORMAS: std::cell::RefCell<std::collections::HashMap<Forma, Image>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Imagem (com o ponto quente em xhot/yhot) da forma pedida.
pub fn imagem_da_forma(forma: Forma) -> Image {
    FORMAS.with(|f| {
        f.borrow_mut()
            .entry(forma)
            .or_insert_with(|| {
                let (t, (xhot, yhot)) = desenhar(forma);
                Image {
                    size: 24,
                    width: t.largura as u32,
                    height: t.altura as u32,
                    xhot: xhot as u32,
                    yhot: yhot as u32,
                    delay: 1,
                    pixels_rgba: t.pixels().to_vec(),
                    pixels_argb: vec![],
                }
            })
            .clone()
    })
}
