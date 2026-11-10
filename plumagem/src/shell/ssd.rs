// SPDX-License-Identifier: MIT AND GPL-3.0-or-later
//! Barra de título das janelas (decoração do lado do servidor).
//!
//! Base do Anvil (Smithay, MIT); redesenhada para a Plumagem com o tema
//! Spix: título à esquerda e, à direita, Minimizar, Maximizar e Fechar como
//! no Windows. Clique duplo no título maximiza/restaura.

use std::{
    cell::{RefCell, RefMut},
    time::{Duration, Instant},
};

use smithay::{
    backend::renderer::{
        element::{
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            AsRenderElements, Kind,
        },
        ImportMem, Renderer,
    },
    desktop::WindowSurface,
    input::Seat,
    utils::{Logical, Point, Serial},
};

use crate::{
    desenho::{self, tema, Tela},
    state::Backend,
    AnvilState,
};

use super::WindowElement;

pub struct WindowState {
    pub is_ssd: bool,
    pub header_bar: HeaderBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Botao {
    Minimizar,
    Maximizar,
    Fechar,
}

#[derive(Debug, Clone)]
pub struct HeaderBar {
    pub pointer_loc: Option<Point<f64, Logical>>,
    pub width: u32,
    ultimo_clique: Option<Instant>,
    buffer: Option<(String, MemoryRenderBuffer)>,
}

pub const HEADER_BAR_HEIGHT: i32 = 32;
const BOTAO_L: u32 = 46;
const CLIQUE_DUPLO: Duration = Duration::from_millis(400);

impl HeaderBar {
    pub fn pointer_enter(&mut self, loc: Point<f64, Logical>) {
        self.pointer_loc = Some(loc);
    }

    pub fn pointer_leave(&mut self) {
        self.pointer_loc = None;
    }

    fn botao_em(&self, x: f64) -> Option<Botao> {
        let w = self.width as f64;
        let b = BOTAO_L as f64;
        if x >= w - b {
            Some(Botao::Fechar)
        } else if x >= w - 2.0 * b {
            Some(Botao::Maximizar)
        } else if x >= w - 3.0 * b {
            Some(Botao::Minimizar)
        } else {
            None
        }
    }

    fn acionar<BackendData: Backend>(
        &mut self,
        seat: &Seat<AnvilState<BackendData>>,
        state: &mut AnvilState<BackendData>,
        window: &WindowElement,
        serial: Serial,
        mover: bool,
    ) {
        let Some(loc) = self.pointer_loc else { return };
        // As ações rodam fora deste borrow: maximizar consulta a decoração.
        match self.botao_em(loc.x) {
            Some(Botao::Fechar) => match window.0.underlying_surface() {
                WindowSurface::Wayland(w) => w.send_close(),
                #[cfg(feature = "xwayland")]
                WindowSurface::X11(w) => {
                    let _ = w.close();
                }
            },
            Some(Botao::Maximizar) => {
                let w = window.clone();
                state.handle.insert_idle(move |data| data.alternar_maximizar(&w));
            }
            Some(Botao::Minimizar) => {
                let w = window.clone();
                state.handle.insert_idle(move |data| data.minimizar(&w));
            }
            None => {
                let agora = Instant::now();
                let duplo = self
                    .ultimo_clique
                    .map(|t| agora.duration_since(t) < CLIQUE_DUPLO)
                    .unwrap_or(false);
                self.ultimo_clique = if duplo { None } else { Some(agora) };
                if duplo {
                    let w = window.clone();
                    state.handle.insert_idle(move |data| data.alternar_maximizar(&w));
                } else if mover {
                    match window.0.underlying_surface() {
                        WindowSurface::Wayland(w) => {
                            let seat = seat.clone();
                            let toplevel = w.clone();
                            state
                                .handle
                                .insert_idle(move |data| data.move_request_xdg(&toplevel, &seat, serial));
                        }
                        #[cfg(feature = "xwayland")]
                        WindowSurface::X11(w) => {
                            let window = w.clone();
                            state.handle.insert_idle(move |data| data.move_request_x11(&window));
                        }
                    }
                }
            }
        }
    }

    /// Botão do mouse apertado na barra de título.
    pub fn clicked<BackendData: Backend>(
        &mut self,
        seat: &Seat<AnvilState<BackendData>>,
        state: &mut AnvilState<BackendData>,
        window: &WindowElement,
        serial: Serial,
    ) {
        self.acionar(seat, state, window, serial, true);
    }

    pub fn touch_down<BackendData: Backend>(
        &mut self,
        seat: &Seat<AnvilState<BackendData>>,
        state: &mut AnvilState<BackendData>,
        window: &WindowElement,
        serial: Serial,
    ) {
        // No toque, os botões agem ao soltar; o título começa a mover já.
        if self.pointer_loc.map(|l| self.botao_em(l.x).is_none()).unwrap_or(false) {
            self.acionar(seat, state, window, serial, true);
        }
    }

    pub fn touch_up<BackendData: Backend>(
        &mut self,
        seat: &Seat<AnvilState<BackendData>>,
        state: &mut AnvilState<BackendData>,
        window: &WindowElement,
        serial: Serial,
    ) {
        if self.pointer_loc.map(|l| self.botao_em(l.x).is_some()).unwrap_or(false) {
            self.acionar(seat, state, window, serial, false);
        }
    }

    /// `origem`: posição da barra na tela, para o vidro mostrar o pedaço
    /// certo do papel de parede desfocado.
    pub fn redraw(&mut self, width: u32, titulo: &str, ativa: bool, maximizada: bool, origem: (i32, i32)) {
        self.width = width;
        if width == 0 {
            return;
        }
        let sobre = self.pointer_loc.and_then(|l| self.botao_em(l.x));
        let chave = format!("{width}|{titulo}|{ativa}|{maximizada}|{sobre:?}|{origem:?}");
        if self.buffer.as_ref().map(|(c, _)| c == &chave).unwrap_or(false) {
            return;
        }

        let (l, a) = (width as f32, HEADER_BAR_HEIGHT as f32);
        let mut t = Tela::nova(width as i32, HEADER_BAR_HEIGHT);
        // Vidro (Aero): azul-ararinha na janela ativa, mais apagado nas outras.
        let tinta = if ativa { tema::DESTAQUE.com_alfa(70) } else { tema::FUNDO.com_alfa(150) };
        t.vidro(0.0, 0.0, l, a + 1.0, 0.0, origem, tinta);

        let cor_titulo = if ativa { tema::TEXTO } else { tema::TEXTO_SECUNDARIO };
        let espaco = l - 3.0 * BOTAO_L as f32 - 24.0;
        if espaco > 20.0 {
            let txt = desenho::encurtar(13.0, titulo, espaco);
            t.texto_com_brilho(12.0, 21.0, 13.0, cor_titulo, &txt);
        }

        let b = BOTAO_L as f32;
        for (i, botao) in [Botao::Minimizar, Botao::Maximizar, Botao::Fechar].iter().enumerate() {
            let x = l - (3 - i) as f32 * b;
            let (cx, cy) = (x + b / 2.0, a / 2.0);
            let mut cor = cor_titulo;
            if sobre == Some(*botao) {
                if *botao == Botao::Fechar {
                    t.retangulo(x, 0.0, b, a - 1.0, 0.0, tema::ERRO);
                    cor = tema::BRANCO;
                } else {
                    t.retangulo(x, 0.0, b, a - 1.0, 0.0, tema::BRANCO.com_alfa(110));
                }
            }
            match botao {
                Botao::Minimizar => t.linha(cx - 5.0, cy + 0.5, cx + 5.0, cy + 0.5, 1.2, cor),
                Botao::Maximizar if maximizada => {
                    t.contorno(cx - 5.0, cy - 2.5, 7.5, 7.5, 1.1, cor);
                    t.linha(cx - 2.5, cy - 5.0, cx + 5.0, cy - 5.0, 1.1, cor);
                    t.linha(cx + 5.0, cy - 5.0, cx + 5.0, cy + 2.5, 1.1, cor);
                }
                Botao::Maximizar => t.contorno(cx - 5.0, cy - 5.0, 10.0, 10.0, 1.1, cor),
                Botao::Fechar => {
                    t.linha(cx - 5.0, cy - 5.0, cx + 5.0, cy + 5.0, 1.2, cor);
                    t.linha(cx - 5.0, cy + 5.0, cx + 5.0, cy - 5.0, 1.2, cor);
                }
            }
        }
        self.buffer = Some((chave, t.em_buffer()));
    }
}

impl<R> AsRenderElements<R> for HeaderBar
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    type RenderElement = MemoryRenderBufferRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, smithay::utils::Physical>,
        _scale: smithay::utils::Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        let Some((_, buffer)) = &self.buffer else { return Vec::new() };
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location.to_f64(),
            buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )
        .map(|e| vec![C::from(e)])
        .unwrap_or_default()
    }
}

impl WindowElement {
    pub fn decoration_state(&self) -> RefMut<'_, WindowState> {
        self.user_data().insert_if_missing(|| {
            RefCell::new(WindowState {
                is_ssd: false,
                header_bar: HeaderBar {
                    pointer_loc: None,
                    width: 0,
                    ultimo_clique: None,
                    buffer: None,
                },
            })
        });

        self.user_data()
            .get::<RefCell<WindowState>>()
            .unwrap()
            .borrow_mut()
    }

    pub fn set_ssd(&self, ssd: bool) {
        self.decoration_state().is_ssd = ssd;
    }
}
