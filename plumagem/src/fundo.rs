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

#[derive(Debug, Clone)]
pub struct Fundo {
    buffer: MemoryRenderBuffer,
    tamanho: Size<f64, Logical>,
}

impl Fundo {
    pub fn spix() -> Option<Fundo> {
        let imagem = image::load_from_memory_with_format(FUNDO_SPIX_PNG, image::ImageFormat::Png)
            .map_err(|e| tracing::warn!("papel de parede inválido: {e}"))
            .ok()?
            .to_rgba8();
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
        Some(Fundo { buffer, tamanho: (l as f64, a as f64).into() })
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
