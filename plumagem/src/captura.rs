// SPDX-License-Identifier: GPL-3.0-or-later
//! Captura de tela (tecla Print Screen): desenha a saída mais uma vez numa
//! textura fora da tela, lê os pixels e salva um PNG em
//! ~/Imagens/Capturas de Tela. Uma notificação avisa; o clique nela abre a
//! captura no Visualizador de Imagens.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker, element::RenderElement, gles::GlesTexture, Bind, Color32F, ExportMem,
            Offscreen, Renderer, TextureMapping,
        },
    },
    output::Output,
    utils::{Buffer as BufferCoord, Rectangle, Size, Transform},
};

/// O usuário apertou Print Screen: o próximo quadro é capturado.
pub static PEDIDA: AtomicBool = AtomicBool::new(false);
/// Última captura salva (a Plumagem abre no Visualizador).
pub static SALVA: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn pedir() {
    PEDIDA.store(true, Ordering::Relaxed);
}

pub fn pedida() -> bool {
    PEDIDA.swap(false, Ordering::Relaxed)
}

fn nome_arquivo() -> PathBuf {
    let casa = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let pasta = PathBuf::from(casa).join("Imagens").join("Capturas de Tela");
    let _ = std::fs::create_dir_all(&pasta);
    let quando = std::process::Command::new("date")
        .arg("+%Y-%m-%d %H.%M.%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "sem data".into());
    let mut p = pasta.join(format!("Captura de tela {quando}.png"));
    let mut n = 2;
    while p.exists() {
        p = pasta.join(format!("Captura de tela {quando} ({n}).png"));
        n += 1;
    }
    p
}

/// Desenha `elementos` numa textura do tamanho da saída e salva em PNG.
pub fn capturar<R, E>(renderer: &mut R, output: &Output, elementos: &[E], fundo: Color32F) -> Result<PathBuf, String>
where
    R: Renderer + Offscreen<GlesTexture> + Bind<GlesTexture> + ExportMem,
    R::TextureId: Clone + 'static,
    E: RenderElement<R>,
{
    let modo = output.current_mode().ok_or("a saída não tem modo")?;
    let (l, a) = (modo.size.w, modo.size.h);
    let tamanho: Size<i32, BufferCoord> = (l, a).into();
    let mut textura: GlesTexture =
        renderer.create_buffer(Fourcc::Abgr8888, tamanho).map_err(|e| format!("textura: {e:?}"))?;
    let mut fb = renderer.bind(&mut textura).map_err(|e| format!("bind: {e:?}"))?;
    let mut dano = OutputDamageTracker::new(modo.size, output.current_scale().fractional_scale(), Transform::Normal);
    dano.render_output(renderer, &mut fb, 0, elementos, fundo).map_err(|e| format!("desenho: {e:?}"))?;
    let mapa = renderer
        .copy_framebuffer(&fb, Rectangle::from_size(tamanho), Fourcc::Abgr8888)
        .map_err(|e| format!("cópia: {e:?}"))?;
    let invertida = mapa.flipped();
    let px = renderer.map_texture(&mapa).map_err(|e| format!("leitura: {e:?}"))?.to_vec();
    let mut img = image::RgbaImage::from_raw(l as u32, a as u32, px).ok_or("tamanho inesperado dos pixels")?;
    if invertida {
        image::imageops::flip_vertical_in_place(&mut img);
    }
    // Sem transparência no arquivo.
    for p in img.pixels_mut() {
        p.0[3] = 255;
    }
    let caminho = nome_arquivo();
    img.save_with_format(&caminho, image::ImageFormat::Png).map_err(|e| format!("salvar: {e}"))?;
    Ok(caminho)
}
