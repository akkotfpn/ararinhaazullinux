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

/// Recorte pedido (x, y, largura, altura em pixels da saída).
pub static REGIAO: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

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

/// Desenha `elementos` numa textura do tamanho da saída e devolve os
/// pixels RGBA (de cima para baixo), a largura e a altura.
fn pixels<R, E>(renderer: &mut R, output: &Output, elementos: &[E], fundo: Color32F) -> Result<(Vec<u8>, i32, i32), String>
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
    let mut px = renderer.map_texture(&mapa).map_err(|e| format!("leitura: {e:?}"))?.to_vec();
    if invertida {
        let linha = l as usize * 4;
        let n = a as usize;
        for y in 0..n / 2 {
            let (cima, baixo) = px.split_at_mut((n - 1 - y) * linha);
            cima[y * linha..(y + 1) * linha].swap_with_slice(&mut baixo[..linha]);
        }
    }
    Ok((px, l, a))
}

/// Desenha `elementos` numa textura do tamanho da saída e salva em PNG.
pub fn capturar<R, E>(renderer: &mut R, output: &Output, elementos: &[E], fundo: Color32F) -> Result<PathBuf, String>
where
    R: Renderer + Offscreen<GlesTexture> + Bind<GlesTexture> + ExportMem,
    R::TextureId: Clone + 'static,
    E: RenderElement<R>,
{
    let (px, l, a) = pixels(renderer, output, elementos, fundo)?;
    let mut img = image::RgbaImage::from_raw(l as u32, a as u32, px).ok_or("tamanho inesperado dos pixels")?;
    if let Some((x, y, w, h)) = REGIAO.lock().unwrap().take() {
        let (x, y) = (x.clamp(0, l - 1) as u32, y.clamp(0, a - 1) as u32);
        let (w, h) = ((w.max(1) as u32).min(l as u32 - x), (h.max(1) as u32).min(a as u32 - y));
        img = image::imageops::crop_imm(&img, x, y, w, h).to_image();
    }
    // Sem transparência no arquivo.
    for p in img.pixels_mut() {
        p.0[3] = 255;
    }
    let caminho = nome_arquivo();
    img.save_with_format(&caminho, image::ImageFormat::Png).map_err(|e| format!("salvar: {e}"))?;
    Ok(caminho)
}

// ---------------------------------------------------------------- pedidos
//
// A Ferramenta de Captura (app `captura`) pede uma captura escrevendo o
// modo em $XDG_RUNTIME_DIR/ararinha-captura.pedido ("tela", "recorte" ou
// "janela"); a Plumagem esconde a janela do app, captura e responde com o
// caminho do PNG (ou "cancelado") em ararinha-captura.resultado.

/// A captura em andamento foi pedida pelo app (responde no arquivo).
pub static PARA_APP: AtomicBool = AtomicBool::new(false);
/// Recorte escolhendo uma janela (clique) em vez de um retângulo.
pub static MODO_JANELA: AtomicBool = AtomicBool::new(false);

fn arquivo_runtime(nome: &str) -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR").ok().map(|d| PathBuf::from(d).join(nome))
}

/// Lê (e apaga) um pedido da Ferramenta de Captura.
pub fn ler_pedido() -> Option<String> {
    let p = arquivo_runtime("ararinha-captura.pedido")?;
    let t = std::fs::read_to_string(&p).ok()?;
    let _ = std::fs::remove_file(&p);
    Some(t.trim().to_string())
}

/// Responde ao app: o caminho da captura ou "cancelado".
pub fn responder(texto: &str) {
    if let Some(p) = arquivo_runtime("ararinha-captura.resultado") {
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

// ---------------------------------------------------------- transmissão
//
// Compartilhar a tela (OBS, Discord, navegador): o ajudante do portal
// (`ararinha-tela-pw`) cria $XDG_RUNTIME_DIR/ararinha-tela.ativa com o PID
// dele. Enquanto ele existir, cada quadro desenhado também vai, até 30 por
// segundo, para ararinha-tela.quadro: cabeçalho de 64 bytes (u32 LE:
// "ARAT", número do quadro, largura, altura, passo) e os pixels RGBA. O
// ajudante lê esse arquivo e manda para o PipeWire.

const MAGICO: u32 = 0x5441_5241; // "ARAT"

struct Transmissao {
    visto: Option<std::time::Instant>,
    ativa: bool,
    ultimo_quadro: Option<std::time::Instant>,
    numero: u32,
    arquivo: Option<std::fs::File>,
}

static TRANSMISSAO: Mutex<Transmissao> =
    Mutex::new(Transmissao { visto: None, ativa: false, ultimo_quadro: None, numero: 0, arquivo: None });

/// A tela começou a ser compartilhada agora (para avisar a pessoa).
pub static COMECOU: AtomicBool = AtomicBool::new(false);

/// Há alguém recebendo a tela? (confere o arquivo no máximo 2x por segundo)
pub fn transmitindo() -> bool {
    let Ok(mut t) = TRANSMISSAO.lock() else { return false };
    if t.visto.map(|v| v.elapsed().as_millis() < 500).unwrap_or(false) {
        return t.ativa;
    }
    t.visto = Some(std::time::Instant::now());
    let ativa = arquivo_runtime("ararinha-tela.ativa")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .map(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
        .unwrap_or(false);
    if ativa && !t.ativa {
        COMECOU.store(true, Ordering::Relaxed);
    }
    if !ativa {
        t.arquivo = None;
    }
    t.ativa = ativa;
    ativa
}

/// Manda o quadro atual para o ajudante (no máximo 30 por segundo).
pub fn transmitir<R, E>(renderer: &mut R, output: &Output, elementos: &[E], fundo: Color32F)
where
    R: Renderer + Offscreen<GlesTexture> + Bind<GlesTexture> + ExportMem,
    R::TextureId: Clone + 'static,
    E: RenderElement<R>,
{
    use std::os::unix::fs::FileExt;
    {
        let Ok(t) = TRANSMISSAO.lock() else { return };
        if t.ultimo_quadro.map(|q| q.elapsed().as_millis() < 30).unwrap_or(false) {
            return;
        }
    }
    let Ok((px, l, a)) = pixels(renderer, output, elementos, fundo) else { return };
    let Ok(mut t) = TRANSMISSAO.lock() else { return };
    t.ultimo_quadro = Some(std::time::Instant::now());
    if t.arquivo.is_none() {
        t.arquivo = arquivo_runtime("ararinha-tela.quadro").and_then(|p| {
            std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(p).ok()
        });
    }
    t.numero = t.numero.wrapping_add(1);
    let numero = t.numero;
    let Some(f) = t.arquivo.as_ref() else { return };
    let mut cab = [0u8; 64];
    for (i, v) in [MAGICO, 0, l as u32, a as u32, l as u32 * 4].iter().enumerate() {
        cab[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    // Pixels primeiro e o número do quadro por último: o ajudante só lê
    // quando o número muda.
    let _ = f.set_len(64 + px.len() as u64);
    let _ = f.write_all_at(&px, 64);
    let _ = f.write_all_at(&cab, 0);
    let _ = f.write_all_at(&numero.to_le_bytes(), 4);
}
