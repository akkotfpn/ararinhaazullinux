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
        let chave = format!(
            "{width}|{titulo}|{ativa}|{maximizada}|{sobre:?}|{origem:?}|{}|{}|{}",
            crate::desenho::EFEITOS.load(std::sync::atomic::Ordering::Relaxed),
            crate::desenho::tema::escuro(),
            crate::desenho::tema::destaque_atual()
        );
        if self.buffer.as_ref().map(|(c, _)| c == &chave).unwrap_or(false) {
            return;
        }

        let (l, a) = (width as f32, HEADER_BAR_HEIGHT as f32);
        let mut t = Tela::nova(width as i32, HEADER_BAR_HEIGHT);
        // Vidro (Aero): azul-ararinha na janela ativa, mais apagado nas outras.
        let tinta = if ativa { tema::DESTAQUE().com_alfa(70) } else { tema::FUNDO().com_alfa(150) };
        t.vidro(0.0, 0.0, l, a + 1.0, 0.0, origem, tinta);

        // O vidro da barra é escuro (papel de parede + azul): título branco
        // com uma sombra leve, legível em qualquer papel de parede.
        let cor_titulo = if ativa { tema::BRANCO() } else { tema::BRANCO().com_alfa(185) };
        let espaco = l - 3.0 * BOTAO_L as f32 - 24.0;
        if espaco > 20.0 {
            let txt = desenho::encurtar(13.5, titulo, espaco);
            t.texto(12.0, 22.0, 13.5, desenho::Cor(0, 0, 0, 110), &txt);
            t.texto(12.0, 21.0, 13.5, cor_titulo, &txt);
        }

        let b = BOTAO_L as f32;
        for (i, botao) in [Botao::Minimizar, Botao::Maximizar, Botao::Fechar].iter().enumerate() {
            let x = l - (3 - i) as f32 * b;
            let (cx, cy) = (x + b / 2.0, a / 2.0);
            let mut cor = cor_titulo;
            if sobre == Some(*botao) {
                if *botao == Botao::Fechar {
                    t.retangulo(x, 0.0, b, a - 1.0, 0.0, tema::ERRO());
                    cor = tema::BRANCO();
                } else {
                    t.retangulo(x, 0.0, b, a - 1.0, 0.0, tema::BRANCO().com_alfa(110));
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

/// Alcance da sombra das janelas, em pixels.
const SOMBRA: i32 = 18;
/// Deslocamento para baixo (a luz vem de cima).
const SOMBRA_DY: i32 = 3;

fn alfa_sombra(d: f32) -> u8 {
    let t = (1.0 - d / SOMBRA as f32).clamp(0.0, 1.0);
    (t * t * 70.0) as u8
}

/// Peças da sombra (cantos e faixas), desenhadas uma vez e esticadas.
struct PecasSombra {
    cantos: [MemoryRenderBuffer; 4], // sup-esq, sup-dir, inf-esq, inf-dir
    faixas: [MemoryRenderBuffer; 4], // cima, baixo, esquerda, direita
}

thread_local! {
    static SOMBRA_PECAS: PecasSombra = {
        let s = SOMBRA;
        let canto = |rx: f32, ry: f32| {
            let mut t = Tela::nova(s, s);
            for y in 0..s {
                for x in 0..s {
                    let dx = ((x as f32 + 0.5) - rx).abs();
                    let dy = ((y as f32 + 0.5) - ry).abs();
                    t.pintar(x, y, desenho::Cor(0, 0, 0, alfa_sombra((dx * dx + dy * dy).sqrt())));
                }
            }
            t.em_buffer()
        };
        let faixa = |l: i32, a: i32, f: &dyn Fn(i32, i32) -> f32| {
            let mut t = Tela::nova(l, a);
            for y in 0..a {
                for x in 0..l {
                    t.pintar(x, y, desenho::Cor(0, 0, 0, alfa_sombra(f(x, y))));
                }
            }
            t.em_buffer()
        };
        let sf = s as f32;
        PecasSombra {
            cantos: [canto(sf, sf), canto(0.0, sf), canto(sf, 0.0), canto(0.0, 0.0)],
            faixas: [
                faixa(1, s, &|_, y| sf - y as f32 - 0.5),
                faixa(1, s, &|_, y| y as f32 + 0.5),
                faixa(s, 1, &|x, _| sf - x as f32 - 0.5),
                faixa(s, 1, &|x, _| x as f32 + 0.5),
            ],
        }
    };
}

/// Sombra suave em volta de uma janela de `largura` x `altura` (lógicos)
/// cujo canto superior esquerdo está em `local` (físico).
pub fn elementos_sombra<R>(
    renderer: &mut R,
    local: Point<i32, smithay::utils::Physical>,
    largura: i32,
    altura: i32,
    escala: smithay::utils::Scale<f64>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let (s, dy, w, h) = (SOMBRA, SOMBRA_DY, largura, altura);
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    // (peça, posição lógica relativa, tamanho lógico)
    SOMBRA_PECAS.with(|p| {
        let pecas: [(&MemoryRenderBuffer, (i32, i32), (i32, i32)); 8] = [
            (&p.cantos[0], (-s, -s + dy), (s, s)),
            (&p.cantos[1], (w, -s + dy), (s, s)),
            (&p.cantos[2], (-s, h + dy), (s, s)),
            (&p.cantos[3], (w, h + dy), (s, s)),
            (&p.faixas[0], (0, -s + dy), (w, s)),
            (&p.faixas[1], (0, h + dy), (w, s)),
            (&p.faixas[2], (-s, dy), (s, h)),
            (&p.faixas[3], (w, dy), (s, h)),
        ];
        pecas
            .into_iter()
            .filter_map(|(buf, (x, y), (pl, pa))| {
                let pos = local.to_f64()
                    + Point::<f64, Logical>::from((x as f64, y as f64)).to_physical(escala);
                MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos,
                    buf,
                    None,
                    None,
                    Some((pl, pa).into()),
                    Kind::Unspecified,
                )
                .ok()
            })
            .collect()
    })
}
