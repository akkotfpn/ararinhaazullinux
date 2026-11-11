// SPDX-License-Identifier: GPL-3.0-or-later
//! Papel de parede da Plumagem. Na v0 é sempre o "Spix" (amanhecer no sertão
//! com o Mingau na caraibeira), desenhado para cobrir a tela inteira.

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                Kind,
            },
            ImportMem, Renderer,
        },
    },
    output::Output,
    utils::{Logical, Rectangle, Size},
};

/// PNG gerado de `arte/papeis-de-parede/spix.svg` (1920x1080).
pub static FUNDO_SPIX_PNG: &[u8] = include_bytes!("../resources/fundo-spix.png");

/// Pasta dos papéis de parede que vêm com o sistema.
pub const PASTA_PAPEIS: &str = "/usr/share/backgrounds/ararinha";

/// Muda quando a configuração muda; quem guarda uma cópia do papel de
/// parede (o fundo e o vidro) compara para saber se precisa recarregar.
pub static VERSAO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn versao() -> u32 {
    VERSAO.load(std::sync::atomic::Ordering::Relaxed)
}

/// O papel de parede escolhido em `papel = ...` na configuração: um nome
/// da pasta do sistema (sem ".png") ou o caminho de uma imagem PNG.
/// Sem escolha, ou se a imagem não abrir, fica o Spix.
pub fn imagem_escolhida() -> image::DynamicImage {
    if let Some(v) = crate::painel::ler_chave("papel") {
        let caminho = if v.contains('/') { v.clone() } else { format!("{PASTA_PAPEIS}/{v}.png") };
        if v != "spix" {
            match std::fs::read(&caminho).map_err(|e| e.to_string()).and_then(|b| {
                image::load_from_memory_with_format(&b, image::ImageFormat::Png).map_err(|e| e.to_string())
            }) {
                Ok(img) => return img,
                Err(e) => tracing::warn!("papel de parede {caminho}: {e}; fica o Spix"),
            }
        }
    }
    image::load_from_memory_with_format(FUNDO_SPIX_PNG, image::ImageFormat::Png)
        .unwrap_or_else(|_| image::DynamicImage::new_rgba8(1, 1))
}

#[derive(Debug, Clone)]
pub struct Fundo {
    buffer: MemoryRenderBuffer,
    tamanho: Size<f64, Logical>,
    pub versao: u32,
}

impl Fundo {
    /// O papel de parede da configuração atual.
    pub fn atual() -> Option<Fundo> {
        let versao = versao();
        let imagem = imagem_escolhida().to_rgba8();
        let (l, a) = (imagem.width() as i32, imagem.height() as i32);
        let buffer = MemoryRenderBuffer::from_slice(
            imagem.as_raw(),
            // Bytes R, G, B, A em memória = Abgr8888 no DRM (little-endian).
            Fourcc::Abgr8888,
            (l, a),
            1,
            smithay::utils::Transform::Normal,
            Some(vec![Rectangle::from_size((l, a).into())]),
        );
        Some(Fundo { buffer, tamanho: (l as f64, a as f64).into(), versao })
    }

    /// Elemento que cobre a saída inteira, mantendo a proporção da imagem
    /// (o excesso fica de fora, como "preencher" no Windows).
    pub fn elemento<R>(&self, renderer: &mut R, output: &Output) -> Option<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        let modo = output.current_mode()?;
        let escala = output.current_scale().fractional_scale();
        let tela = output.current_transform().transform_size(modo.size).to_f64().to_logical(escala);
        let fator = f64::max(tela.w / self.tamanho.w, tela.h / self.tamanho.h);
        let alvo: Size<f64, Logical> = (self.tamanho.w * fator, self.tamanho.h * fator).into();
        let canto = smithay::utils::Point::<f64, Logical>::from(((tela.w - alvo.w) / 2.0, (tela.h - alvo.h) / 2.0));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            canto.to_physical(escala),
            &self.buffer,
            None,
            None,
            Some(alvo.to_i32_round()),
            Kind::Unspecified,
        )
        .map_err(|e| tracing::warn!("não deu para desenhar o papel de parede: {e:?}"))
        .ok()
    }
}
