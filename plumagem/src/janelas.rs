// SPDX-License-Identifier: GPL-3.0-or-later
//! Gerência de janelas da Plumagem: minimizar, maximizar respeitando a barra
//! de tarefas, restaurar para onde estava, e as ações da barra e do menu.

use std::{cell::RefCell, process::Command, sync::atomic::Ordering};

use smithay::{
    desktop::space::SpaceElement,
    input::pointer::{Focus, GrabStartData},
    wayland::compositor::with_states,
};

use smithay::{
    desktop::Space,
    output::Output,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point, Rectangle, Size, SERIAL_COUNTER},
};
use tracing::{error, info};

use crate::{
    focus::KeyboardFocusTarget,
    painel::{self, Alvo},
    shell::{
        ssd::HEADER_BAR_HEIGHT, FullscreenSurface, PointerResizeSurfaceGrab, ResizeData, ResizeEdge,
        ResizeState, SurfaceData, WindowElement,
    },
    state::{AnvilState, Backend},
};

/// Para onde a janela vai com Win+setas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Encaixe {
    Esquerda,
    Direita,
    Cima,
    Baixo,
}

/// Região onde a janela está encaixada (coluna, linha).
struct RegiaoEncaixe(std::cell::Cell<Option<(i8, i8)>>);

fn regiao_de(w: &WindowElement) -> Option<(i8, i8)> {
    w.0.user_data().get::<RegiaoEncaixe>().and_then(|r| r.0.get())
}

/// Posição e tamanho antes de maximizar, guardados na própria janela.
struct GeometriaAnterior(RefCell<Option<(Point<i32, Logical>, Option<Size<i32, Logical>>)>>);

/// Marca: a janela já foi posicionada no primeiro quadro.
pub struct Posicionada;

/// Janela X11 (Steam, Krita, jogos): não tem os estados do xdg-shell, então
/// maximizar/encaixar é dizer ao X onde ela fica.
#[cfg(feature = "xwayland")]
fn x11_de(w: &WindowElement) -> Option<smithay::xwayland::X11Surface> {
    w.0.x11_surface().cloned()
}
#[cfg(not(feature = "xwayland"))]
fn x11_de(_w: &WindowElement) -> Option<()> {
    None
}

/// Coloca a janela X11 no retângulo `r` (área total, com a barra de título).
#[cfg(feature = "xwayland")]
fn x11_colocar(w: &WindowElement, x: &smithay::xwayland::X11Surface, r: Rectangle<i32, Logical>) {
    let mut g = r;
    if w.decoration_state().is_ssd {
        g.loc.y += HEADER_BAR_HEIGHT;
        g.size.h -= HEADER_BAR_HEIGHT;
    }
    let _ = x.configure(g);
}

fn guardar_anterior(w: &WindowElement, loc: Point<i32, Logical>, tam: Option<Size<i32, Logical>>) {
    w.0.user_data().insert_if_missing(|| GeometriaAnterior(RefCell::new(None)));
    if let Some(g) = w.0.user_data().get::<GeometriaAnterior>() {
        *g.0.borrow_mut() = Some((loc, tam));
    }
}

/// Área da saída que as janelas podem ocupar (tudo menos a barra de tarefas).
pub fn area_util(space: &Space<WindowElement>, output: &Output) -> Option<Rectangle<i32, Logical>> {
    let mut g = space.output_geometry(output)?;
    g.size.h = (g.size.h - painel::altura()).max(1);
    Some(g)
}

impl<B: Backend> AnvilState<B> {
    /// Roda um comando (linha de shell) como cliente desta sessão.
    pub fn executar_programa(&self, cmd: &str) {
        info!(cmd, "abrindo programa");
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(format!("exec {cmd}"));
        if let Some(s) = &self.socket_name {
            c.env("WAYLAND_DISPLAY", s);
        }
        #[cfg(feature = "xwayland")]
        if let Some(d) = self.xdisplay {
            c.env("DISPLAY", format!(":{d}"));
        }
        if let Err(e) = c.spawn() {
            error!(cmd, err = %e, "não deu para abrir o programa");
        }
    }

    /// Põe a janela na frente, com as janelas filhas dela (diálogos como o
    /// "Adicionar fonte" do OBS) sempre por cima.
    pub fn elevar_com_filhos(&mut self, w: &WindowElement) {
        self.space.raise_element(w, true);
        let Some(sup) = w.wl_surface().map(|s| s.into_owned()) else { return };
        let filhos: Vec<WindowElement> = self
            .space
            .elements()
            .filter(|e| e.0.toplevel().and_then(|t| t.parent()).as_ref() == Some(&sup))
            .cloned()
            .collect();
        for f in filhos {
            if &f != w {
                self.elevar_com_filhos(&f);
            }
        }
    }

    /// Win+L: bloqueia a tela (o teclado sai das janelas).
    pub fn bloquear(&mut self) {
        if self.ninho.is_some() || self.bloqueio.is_some() {
            return;
        }
        self.bloqueio = Some(crate::bloqueio::Bloqueio::novo());
        self.painel.menu_aberto = false;
        let teclado = self.seat.get_keyboard().unwrap();
        teclado.set_focus(self, None, SERIAL_COUNTER.next_serial());
    }

    pub fn focar_janela(&mut self, w: &WindowElement) {
        self.elevar_com_filhos(w);
        let teclado = self.seat.get_keyboard().unwrap();
        teclado.set_focus(self, Some(KeyboardFocusTarget::from(w.clone())), SERIAL_COUNTER.next_serial());
    }

    pub fn minimizar(&mut self, w: &WindowElement) {
        if self.painel.esta_minimizada(w) {
            return;
        }
        let Some(loc) = self.space.element_location(w) else { return };
        self.space.unmap_elem(w);
        self.painel.minimizadas.push((w.clone(), loc));
        let proxima = self.space.elements().last().cloned();
        let teclado = self.seat.get_keyboard().unwrap();
        teclado.set_focus(self, proxima.map(KeyboardFocusTarget::from), SERIAL_COUNTER.next_serial());
    }

    pub fn desminimizar(&mut self, w: &WindowElement) {
        if let Some(i) = self.painel.minimizadas.iter().position(|(m, _)| m == w) {
            let (w, loc) = self.painel.minimizadas.remove(i);
            self.space.map_element(w.clone(), loc, true);
            self.focar_janela(&w);
        }
    }

    pub fn esta_maximizada(&self, w: &WindowElement) -> bool {
        #[cfg(feature = "xwayland")]
        if let Some(x) = x11_de(w) {
            return x.is_maximized();
        }
        w.0.toplevel()
            .map(|t| t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Maximized)))
            .unwrap_or(false)
    }

    /// Maximiza ocupando a tela menos a barra de tarefas.
    pub fn maximizar(&mut self, w: &WindowElement) {
        #[cfg(feature = "xwayland")]
        if let Some(x) = x11_de(w) {
            let output = self.space.outputs_for_element(w).first().cloned().or_else(|| self.space.outputs().next().cloned());
            let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };
            if !self.esta_maximizada(w) && regiao_de(w).is_none() {
                let g = x.geometry();
                if let Some(loc) = self.space.element_location(w) {
                    guardar_anterior(w, loc, Some(g.size));
                }
            }
            if let Some(r) = w.0.user_data().get::<RegiaoEncaixe>() {
                r.0.set(None);
            }
            let _ = x.set_maximized(true);
            x11_colocar(w, &x, area);
            self.space.map_element(w.clone(), area.loc, true);
            return;
        }
        let Some(t) = w.0.toplevel().cloned() else { return };
        let output = self
            .space
            .outputs_for_element(w)
            .first()
            .cloned()
            .or_else(|| self.space.outputs().next().cloned());
        let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };

        if !self.esta_maximizada(w) {
            if let Some(loc) = self.space.element_location(w) {
                let tamanho = t.current_state().size;
                w.0.user_data()
                    .insert_if_missing(|| GeometriaAnterior(RefCell::new(None)));
                if let Some(g) = w.0.user_data().get::<GeometriaAnterior>() {
                    *g.0.borrow_mut() = Some((loc, tamanho));
                }
            }
        }

        let mut tamanho = area.size;
        if w.decoration_state().is_ssd {
            tamanho.h -= HEADER_BAR_HEIGHT;
        }
        t.with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Maximized);
            s.size = Some(tamanho);
        });
        self.space.map_element(w.clone(), area.loc, true);
        if t.is_initial_configure_sent() {
            t.send_configure();
        }
    }

    /// Faz a janela caber na área útil da tela (sem a barra): diminui se
    /// for maior e traz para dentro se a barra de título ficou de fora.
    /// `centralizar`: primeira vez (abre no meio, um pouco em cascata).
    pub fn ajustar_a_tela(&mut self, w: &WindowElement, centralizar: bool) {
        if self.esta_maximizada(w) || self.esta_encaixada(w) {
            return;
        }
        let output = self.space.outputs_for_element(w).first().cloned().or_else(|| self.space.outputs().next().cloned());
        let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };
        let geo = w.geometry();
        if geo.size.w <= 0 || geo.size.h <= 0 {
            return;
        }
        let Some(mut loc) = self.space.element_location(w) else { return };
        let (mut gw, mut gh) = (geo.size.w, geo.size.h);
        if gw > area.size.w || gh > area.size.h {
            gw = gw.min(area.size.w);
            gh = gh.min(area.size.h);
            let header = if w.decoration_state().is_ssd { HEADER_BAR_HEIGHT } else { 0 };
            if let Some(t) = w.0.toplevel() {
                t.with_pending_state(|s| s.size = Some((gw, gh - header).into()));
                if t.is_initial_configure_sent() {
                    t.send_pending_configure();
                }
            }
            #[cfg(feature = "xwayland")]
            if let Some(x) = x11_de(w) {
                x11_colocar(w, &x, Rectangle::new(area.loc, (gw, gh).into()));
            }
        }
        if centralizar {
            let n = (self.space.elements().count().saturating_sub(1) % 5) as i32 * 24;
            loc.x = area.loc.x + (area.size.w - gw) / 2 + n;
            loc.y = area.loc.y + (area.size.h - gh) / 2 + n;
        }
        loc.x = loc.x.clamp(area.loc.x, (area.loc.x + area.size.w - gw).max(area.loc.x));
        loc.y = loc.y.clamp(area.loc.y, (area.loc.y + area.size.h - gh).max(area.loc.y));
        self.space.map_element(w.clone(), loc, false);
    }

    /// Win+Z: layouts de encaixe para a janela ativa (funciona com qualquer
    /// app, até os que desenham a própria barra de título).
    pub fn layouts_janela_ativa(&mut self) {
        let Some(w) = self.space.elements().last().cloned() else { return };
        let Some(g) = self.space.element_geometry(&w) else { return };
        let Some(o) = self.space.outputs_for_element(&w).first().cloned().or_else(|| self.space.outputs().next().cloned()) else { return };
        let Some(tela) = self.space.output_geometry(&o) else { return };
        self.painel.abrir_encaixe(w, g, tela);
    }

    /// Encaixa a janela ativa (Win+setas): metade esquerda/direita, maximizar,
    /// e para baixo restaura ou minimiza, como no Windows.
    /// Win+setas, como no Windows: esquerda/direita encaixam na metade;
    /// com a janela numa metade, cima/baixo levam ao quarto (e de volta).
    pub fn encaixar(&mut self, lado: Encaixe) {
        let Some(w) = self.space.elements().last().cloned() else { return };
        let atual = if self.esta_encaixada(&w) { regiao_de(&w) } else { None };
        let nova = match (lado, atual) {
            (Encaixe::Esquerda, _) => (-1, 0),
            (Encaixe::Direita, _) => (1, 0),
            (Encaixe::Cima, Some((c, 1))) => (c, 0),
            (Encaixe::Cima, Some((c, 0))) => (c, -1),
            (Encaixe::Baixo, Some((c, -1))) => (c, 0),
            (Encaixe::Baixo, Some((c, 0))) => (c, 1),
            (Encaixe::Cima, _) => {
                if !self.esta_maximizada(&w) {
                    self.maximizar(&w);
                }
                return;
            }
            (Encaixe::Baixo, _) => {
                if self.esta_maximizada(&w) || self.esta_encaixada(&w) {
                    self.restaurar(&w);
                } else {
                    self.minimizar(&w);
                }
                return;
            }
        };
        self.encaixar_em(&w, nova);
    }

    /// Encaixa numa região: coluna -1/1 (esquerda/direita) e linha
    /// 0 (altura toda), -1 (quarto de cima) ou 1 (quarto de baixo).
    pub fn encaixar_em(&mut self, w: &WindowElement, (col, lin): (i8, i8)) {
        #[cfg(feature = "xwayland")]
        if let Some(x) = x11_de(w) {
            let output = self.space.outputs_for_element(w).first().cloned().or_else(|| self.space.outputs().next().cloned());
            let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };
            if !x.is_maximized() && regiao_de(w).is_none() {
                if let Some(loc) = self.space.element_location(w) {
                    guardar_anterior(w, loc, Some(x.geometry().size));
                }
            }
            w.0.user_data().insert_if_missing(|| RegiaoEncaixe(std::cell::Cell::new(None)));
            if let Some(r) = w.0.user_data().get::<RegiaoEncaixe>() {
                r.0.set(Some((col, lin)));
            }
            let meia = area.size.w / 2;
            let xx = if col < 0 { area.loc.x } else { area.loc.x + meia };
            let (y, alt) = match lin {
                0 => (area.loc.y, area.size.h),
                -1 => (area.loc.y, area.size.h / 2),
                _ => (area.loc.y + area.size.h / 2, area.size.h - area.size.h / 2),
            };
            let _ = x.set_maximized(false);
            let r = Rectangle::new((xx, y).into(), (meia, alt).into());
            x11_colocar(w, &x, r);
            self.space.map_element(w.clone(), r.loc, true);
            return;
        }
        let Some(t) = w.0.toplevel().cloned() else { return };
        let output = self.space.outputs_for_element(w).first().cloned().or_else(|| self.space.outputs().next().cloned());
        let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };
        // Guarda o tamanho de antes (se ainda não está maximizada/encaixada).
        if !self.esta_maximizada(w) && !self.esta_encaixada(w) {
            if let Some(loc) = self.space.element_location(w) {
                let tamanho = t.current_state().size;
                w.0.user_data().insert_if_missing(|| GeometriaAnterior(RefCell::new(None)));
                if let Some(g) = w.0.user_data().get::<GeometriaAnterior>() {
                    *g.0.borrow_mut() = Some((loc, tamanho));
                }
            }
        }
        w.0.user_data().insert_if_missing(|| RegiaoEncaixe(std::cell::Cell::new(None)));
        if let Some(r) = w.0.user_data().get::<RegiaoEncaixe>() {
            r.0.set(Some((col, lin)));
        }
        let meia = area.size.w / 2;
        let x = if col < 0 { area.loc.x } else { area.loc.x + meia };
        let (y, alt) = match lin {
            0 => (area.loc.y, area.size.h),
            -1 => (area.loc.y, area.size.h / 2),
            _ => (area.loc.y + area.size.h / 2, area.size.h - area.size.h / 2),
        };
        let mut tamanho: Size<i32, Logical> = (meia, alt).into();
        if w.decoration_state().is_ssd {
            tamanho.h -= HEADER_BAR_HEIGHT;
        }
        t.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Maximized);
            s.states.set(xdg_toplevel::State::TiledLeft);
            s.states.set(xdg_toplevel::State::TiledRight);
            s.states.set(xdg_toplevel::State::TiledTop);
            s.states.set(xdg_toplevel::State::TiledBottom);
            s.size = Some(tamanho);
        });
        self.space.map_element(w.clone(), (x, y), true);
        if t.is_initial_configure_sent() {
            t.send_configure();
        }
    }

    fn esta_encaixada(&self, w: &WindowElement) -> bool {
        if x11_de(w).is_some() {
            return regiao_de(w).is_some();
        }
        w.0.toplevel()
            .map(|t| t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::TiledLeft)))
            .unwrap_or(false)
    }

    /// Volta ao tamanho e lugar de antes de maximizar.
    pub fn restaurar(&mut self, w: &WindowElement) {
        #[cfg(feature = "xwayland")]
        if let Some(x) = x11_de(w) {
            if let Some(r) = w.0.user_data().get::<RegiaoEncaixe>() {
                r.0.set(None);
            }
            let anterior = w.0.user_data().get::<GeometriaAnterior>().and_then(|g| g.0.borrow_mut().take());
            let _ = x.set_maximized(false);
            if let Some((loc, Some(tam))) = anterior {
                let mut alt = tam.h;
                if w.decoration_state().is_ssd {
                    alt += HEADER_BAR_HEIGHT;
                }
                x11_colocar(w, &x, Rectangle::new(loc, (tam.w, alt).into()));
                self.space.map_element(w.clone(), loc, true);
            }
            return;
        }
        let Some(t) = w.0.toplevel().cloned() else { return };
        let anterior = w
            .0
            .user_data()
            .get::<GeometriaAnterior>()
            .and_then(|g| g.0.borrow_mut().take());
        t.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Maximized);
            for e in [
                xdg_toplevel::State::TiledLeft,
                xdg_toplevel::State::TiledRight,
                xdg_toplevel::State::TiledTop,
                xdg_toplevel::State::TiledBottom,
            ] {
                s.states.unset(e);
            }
            s.size = anterior.and_then(|(_, tam)| tam);
        });
        if let Some((loc, _)) = anterior {
            self.space.map_element(w.clone(), loc, true);
        }
        t.send_pending_configure();
    }

    pub fn alternar_maximizar(&mut self, w: &WindowElement) {
        if self.esta_maximizada(w) {
            self.restaurar(w);
        } else {
            self.maximizar(w);
        }
    }

    /// Trata cliques na barra e no menu. Devolve `true` se o clique foi da
    /// interface (e não deve chegar às janelas).
    pub fn clique_na_interface(&mut self, pressionado: bool, direito: bool) -> bool {
        let pos = self.pointer.current_location();
        let Some(output) = self.space.output_under(pos).next().cloned() else { return false };
        if output
            .user_data()
            .get::<FullscreenSurface>()
            .and_then(|f| f.get())
            .is_some()
        {
            return false;
        }
        let Some(geo) = self.space.output_geometry(&output) else { return false };
        let alvo = self.painel.alvo(pos - geo.loc.to_f64(), geo.size);

        if !pressionado {
            return std::mem::take(&mut self.painel.segurando) || alvo.is_some();
        }
        let Some(alvo) = alvo else {
            // Clique fora: fecha o menu e o painel de som e segue para a janela.
            self.painel.menu_aberto = false;
            self.painel.som_aberto = false;
            self.painel.rede_aberta = false;
            self.painel.central_aberta = false;
            self.painel.area_aberta = false;
            let tinha_contexto = self.painel.menu_contexto.take().is_some();
            // Clique fora do menu de contexto só fecha o menu.
            return tinha_contexto && !direito;
        };
        // Item do menu do botão direito.
        if let Alvo::Contexto(i) = alvo {
            if let Some(m) = self.painel.menu_contexto.take() {
                if let Some((_, acao)) = m.itens.get(i) {
                    match acao.as_str() {
                        "#atualizar" | "#nada" => {}
                        a if a.starts_with("#bandeja:") => {
                            let resto = &a["#bandeja:".len()..];
                            if let Some((n, id)) = resto.split_once(':') {
                                painel::pedir_bandeja(&format!("clicar {n} {id}"));
                            }
                        }
                        "#papel" => self.painel.proximo_papel(),
                        "#esvaziar" => {
                            // Como no Windows: apaga de vez o que está na Lixeira.
                            std::thread::spawn(|| {
                                if let Ok(h) = std::env::var("HOME") {
                                    let t = std::path::PathBuf::from(h).join(".local/share/Trash");
                                    let _ = std::fs::remove_dir_all(t.join("files"));
                                    let _ = std::fs::remove_dir_all(t.join("info"));
                                    let _ = std::fs::create_dir_all(t.join("files"));
                                    let _ = std::fs::create_dir_all(t.join("info"));
                                }
                            });
                        }
                        "#central" => self.painel.central_aberta = true,
                        a if a.starts_with("#fechar:") => {
                            // Fecha as janelas do grupo (índices em `ordem`).
                            let ws: Vec<WindowElement> = a["#fechar:".len()..]
                                .split(',')
                                .filter_map(|j| j.parse::<usize>().ok())
                                .filter_map(|j| self.painel.ordem.get(j).cloned())
                                .collect();
                            for w in ws {
                                fechar(&w);
                            }
                        }
                        a if a.starts_with("#fixar:") => self.painel.fixar(&a["#fixar:".len()..], true),
                        a if a.starts_with("#desafixar:") => self.painel.fixar(&a["#desafixar:".len()..], false),
                        a if a.starts_with("#mover:") => {
                            if let Some((id, p)) = a["#mover:".len()..].rsplit_once(':') {
                                self.painel.mover_fixado(id, p.parse().unwrap_or(0));
                            }
                        }
                        "#area" => self.mostrar_area_de_trabalho(),
                        a if a.starts_with("#copiar:") || a.starts_with("#recortar:") => {
                            let corte = a.starts_with("#recortar:");
                            let (_, c) = a.split_once(':').unwrap_or_default();
                            self.copiar_arquivos(vec![std::path::PathBuf::from(c)], corte);
                        }
                        a if a.starts_with("#lixeira:") => {
                            let caminho = std::path::PathBuf::from(&a["#lixeira:".len()..]);
                            if caminho.starts_with(painel::pasta_area().unwrap_or_default()) {
                                if let Err(e) = painel::mandar_para_lixeira(&caminho) {
                                    let _ = std::process::Command::new("ararinha-notificar")
                                        .args(["Não deu para excluir", &e.to_string()])
                                        .spawn();
                                }
                            }
                        }
                        "#organizar" => self.painel.organizar_icones(),
                        a if a.starts_with("#mover-mesa:") => {
                            let mut it = a["#mover-mesa:".len()..].split(':').filter_map(|x| x.parse::<usize>().ok());
                            if let (Some(k), Some(m)) = (it.next(), it.next()) {
                                if let Some(w) = self.painel.visao_janelas.get(k).cloned() {
                                    self.mover_para_mesa(&w, m);
                                }
                            }
                            self.atualizar_visao();
                        }
                        a if a.starts_with("#fechar-visao:") => {
                            if let Some(w) = a["#fechar-visao:".len()..].parse::<usize>().ok().and_then(|k| self.painel.visao_janelas.get(k).cloned()) {
                                fechar(&w);
                            }
                        }
                        a if a.starts_with("#renomear:") => {
                            if let Ok(i) = a["#renomear:".len()..].parse::<usize>() {
                                self.painel.renomear_icone(i);
                            }
                        }
                        "#colar-area" => {
                            if let Some(p) = painel::pasta_area() {
                                let _ = std::fs::create_dir_all(&p);
                                crate::area::colar_em(p);
                            }
                        }
                        a @ ("#nova-pasta" | "#novo-texto") => {
                            if let Some(p) = painel::pasta_area() {
                                let _ = std::fs::create_dir_all(&p);
                                if a == "#nova-pasta" {
                                    let _ = std::fs::create_dir(crate::area::nome_livre(&p, "Nova pasta", ""));
                                } else {
                                    let _ = std::fs::write(crate::area::nome_livre(&p, "Novo documento de texto.txt", ""), "");
                                }
                            }
                        }
                        a if a.starts_with("#apagar:") => {
                            // Só apaga atalhos e pastas vazias da Área de Trabalho
                            // (pasta com coisas vai para a Lixeira pelo Explorador).
                            let caminho = std::path::PathBuf::from(&a["#apagar:".len()..]);
                            if caminho.starts_with(painel::pasta_area().unwrap_or_default()) {
                                let _ = if caminho.is_dir() {
                                    std::fs::remove_dir(&caminho)
                                } else {
                                    std::fs::remove_file(&caminho)
                                };
                            }
                        }
                        cmd => self.executar_programa(cmd),
                    }
                }
            }
            self.painel.segurando = true;
            return true;
        }
        self.painel.menu_contexto = None;
        // Botão direito num ícone da bandeja: o menu do próprio app.
        if direito {
            if let Alvo::ItemBandeja(i) = alvo {
                if let Some(item) = self.painel.bandeja.get(i) {
                    let p = (pos - geo.loc.to_f64()).to_i32_round();
                    self.painel.bandeja_clique = p;
                    painel::pedir_bandeja(&format!("menu {}", item.n));
                }
                self.painel.segurando = true;
                return true;
            }
        }
        // Botão direito na barra de tarefas: menu da barra.
        if direito {
            let p = (pos - geo.loc.to_f64()).to_i32_round();
            if let Alvo::VisaoJanela(k) = alvo {
                let mut itens: Vec<(String, String)> = (0..self.painel.mesas)
                    .filter(|m| *m != self.painel.mesa_atual)
                    .map(|m| (format!("Mover para Área de trabalho {}", m + 1), format!("#mover-mesa:{k}:{m}")))
                    .collect();
                if self.painel.mesas < 9 {
                    itens.push(("Mover para nova área de trabalho".into(), format!("#mover-mesa:{k}:{}", self.painel.mesas)));
                }
                itens.push(("Fechar".into(), format!("#fechar-visao:{k}")));
                let v: Vec<(&str, &str)> = itens.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                self.painel.abrir_contexto_pub(p, geo.size, v);
                self.painel.segurando = true;
                return true;
            }
            if alvo == Alvo::Relogio {
                self.painel.contexto_relogio(p, geo.size);
            } else if matches!(alvo, Alvo::Som) {
                self.painel.contexto_som(p, geo.size);
            } else if matches!(alvo, Alvo::Rede) {
                self.painel.contexto_rede(p, geo.size);
            } else if let Alvo::Tarefa(i) = alvo {
                let p = (pos - geo.loc.to_f64()).to_i32_round();
                self.painel.contexto_grupo(i, p, geo.size);
            } else if matches!(alvo, Alvo::Barra | Alvo::Iniciar) {
                let p = (pos - geo.loc.to_f64()).to_i32_round();
                self.painel.contexto_barra(p, geo.size);
            }
            self.painel.segurando = true;
            return true;
        }
        if !matches!(alvo, Alvo::Som | Alvo::SomBarra(_) | Alvo::SomMudo | Alvo::SomPainel | Alvo::SomMixer) {
            self.painel.som_aberto = false;
        }
        if !matches!(alvo, Alvo::Rede | Alvo::RedePainel | Alvo::RedeConfig) {
            self.painel.rede_aberta = false;
        }
        if !matches!(alvo, Alvo::Relogio | Alvo::CentralPainel | Alvo::CentralItem(_) | Alvo::CentralLimpar | Alvo::CentralAcao(_)) {
            self.painel.central_aberta = false;
        }
        if !matches!(alvo, Alvo::AreaPainel | Alvo::AreaLimpar) {
            self.painel.area_aberta = false;
        }
        if !matches!(alvo, Alvo::Layout(..) | Alvo::LayoutPainel) && self.painel.encaixe_fixo {
            self.painel.encaixe_fixo = false;
            self.painel.encaixe = None;
        }
        self.painel.segurando = true;
        if alvo != Alvo::Energia {
            self.painel.energia_aberta = false;
        }

        match alvo {
            Alvo::Iniciar => self.alternar_menu(),
            Alvo::Tarefa(i) => {
                self.painel.menu_aberto = false;
                self.clicar_grupo(i);
            }
            Alvo::Previa(j) => {
                if let Some(w) = self.painel.ordem.get(j).cloned() {
                    if self.painel.esta_minimizada(&w) {
                        self.desminimizar(&w);
                    } else {
                        self.focar_janela(&w);
                    }
                }
                self.painel.fechar_previa();
            }
            Alvo::PreviaFechar(j) => {
                if let Some(w) = self.painel.ordem.get(j).cloned() {
                    fechar(&w);
                }
            }
            Alvo::PreviaPainel | Alvo::LayoutPainel => {}
            Alvo::Layout(k, z) => {
                self.painel.encaixe_fixo = false;
                if let Some((w, _, _)) = self.painel.encaixe.take() {
                    if let Some((_, zona)) = painel::zonas_layout(k).get(z as usize) {
                        self.focar_janela(&w);
                        self.encaixar_em(&w, *zona);
                    }
                }
            }
            Alvo::App(i) => {
                self.painel.menu_aberto = false;
                if let Some(exec) = self.painel.apps_visiveis().get(i).map(|a| a.exec.clone()) {
                    self.executar_programa(&exec);
                }
            }
            Alvo::Energia => self.painel.energia_aberta = !self.painel.energia_aberta,
            Alvo::Relogio => {
                self.painel.central_aberta = !self.painel.central_aberta;
                self.painel.menu_aberto = false;
            }
            Alvo::CentralPainel | Alvo::AreaPainel => {}
            Alvo::AreaLimpar => crate::area::limpar(),
            Alvo::AreaItem(i) => {
                if let Some(t) = crate::area::itens().get(i).cloned() {
                    self.colar_texto(t);
                }
            }
            Alvo::CentralLimpar => self.painel.historico.clear(),
            Alvo::CentralItem(i) => {
                if let Some((_, _, _, acao)) = self.painel.historico.get(i).cloned() {
                    self.painel.historico.remove(i);
                    if let Some(cmd) = acao {
                        self.painel.central_aberta = false;
                        self.executar_programa(&cmd);
                    }
                }
            }
            Alvo::CentralAcao(i) => match i {
                0 => {
                    self.painel.nao_perturbe = !self.painel.nao_perturbe;
                    painel::gravar_chave("nao_perturbe", if self.painel.nao_perturbe { "sim" } else { "nao" });
                }
                1 => {
                    let escuro = !crate::desenho::tema::escuro();
                    painel::gravar_chave("tema", if escuro { "escuro" } else { "claro" });
                    crate::desenho::tema::definir_escuro(escuro);
                }
                2 => {
                    self.painel.central_aberta = false;
                    self.painel.recorte = Some(None);
                    crate::render::RECORTE.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ => {
                    self.painel.central_aberta = false;
                    self.executar_programa("configuracoes hora");
                }
            },
            Alvo::Dormir => {
                self.painel.menu_aberto = false;
                // O gancho do system-sleep bloqueia a tela antes de dormir.
                pedir_sistema("dormir", None);
            }
            Alvo::Hibernar => {
                self.painel.menu_aberto = false;
                pedir_sistema("hibernar", None);
            }
            Alvo::Bloquear => {
                self.painel.menu_aberto = false;
                self.bloquear();
            }
            Alvo::Usuario => {
                self.painel.menu_aberto = false;
                self.executar_programa("configuracoes conta");
            }
            Alvo::Sair => {
                info!("saindo pela opção do menu");
                self.running.store(false, Ordering::SeqCst);
            }
            Alvo::Reiniciar => {
                self.painel.menu_aberto = false;
                pedir_sistema("reiniciar", Some("/usr/share/sounds/ararinha/desligar.wav"));
            }
            Alvo::Desligar => {
                self.painel.menu_aberto = false;
                pedir_sistema("desligar", Some("/usr/share/sounds/ararinha/desligar.wav"));
            }
            Alvo::Rede => {
                self.painel.menu_aberto = false;
                self.painel.rede_aberta = !self.painel.rede_aberta;
            }
            Alvo::RedePainel => {}
            Alvo::Aviso(i) => {
                if i < self.painel.avisos.len() {
                    let aviso = self.painel.avisos.remove(i);
                    if let Some(cmd) = aviso.acao {
                        self.executar_programa(&cmd);
                    }
                }
            }
            Alvo::RedeConfig => {
                self.painel.rede_aberta = false;
                self.executar_programa("configuracoes rede");
            }
            Alvo::Som => {
                self.painel.menu_aberto = false;
                self.painel.som_aberto = !self.painel.som_aberto;
                self.painel.ler_som(true);
            }
            Alvo::SomBarra(f) => self.painel.definir_volume(f as f32 / 1000.0),
            Alvo::SomMudo => self.painel.alternar_mudo(),
            Alvo::SomMixer => {
                self.painel.som_aberto = false;
                self.executar_programa("configuracoes som");
            }
            Alvo::Barra | Alvo::Menu | Alvo::SomPainel | Alvo::Contexto(_) => {}
            Alvo::CantoArea => self.mostrar_area_de_trabalho(),
            Alvo::VisaoBotao => self.alternar_visao(),
            Alvo::VisaoFundo => self.painel.visao = false,
            Alvo::VisaoJanela(k) => {
                self.painel.visao = false;
                if let Some(w) = self.painel.visao_janelas.get(k).cloned() {
                    if self.painel.esta_minimizada(&w) {
                        self.desminimizar(&w);
                    } else {
                        self.focar_janela(&w);
                    }
                }
            }
            Alvo::VisaoMesa(k) => {
                self.trocar_mesa(k);
                self.painel.visao = false;
            }
            Alvo::VisaoFecharMesa(k) => {
                self.fechar_mesa(k);
                self.atualizar_visao();
            }
            Alvo::VisaoNova => {
                self.painel.mesas = (self.painel.mesas + 1).min(9);
                self.atualizar_visao();
            }
            Alvo::ItemBandeja(i) => {
                self.painel.menu_aberto = false;
                if let Some(item) = self.painel.bandeja.get(i) {
                    let p = pos.to_i32_round::<i32>();
                    painel::pedir_bandeja(&format!("ativar {} {} {}", item.n, p.x, p.y));
                }
            }
        }
        true
    }

    pub fn alternar_menu(&mut self) {
        if self.painel.menu_aberto {
            self.painel.menu_aberto = false;
        } else {
            self.painel.abrir_menu();
        }
    }

    /// O ponto está sobre a interface (barra ou menu aberto)?
    pub fn sobre_a_interface(&self, pos: Point<f64, Logical>) -> bool {
        let Some(output) = self.space.output_under(pos).next() else { return false };
        if output
            .user_data()
            .get::<FullscreenSurface>()
            .and_then(|f| f.get())
            .is_some()
        {
            return false;
        }
        let Some(geo) = self.space.output_geometry(output) else { return false };
        self.painel.alvo(pos - geo.loc.to_f64(), geo.size).is_some()
    }

    /// Borda de janela sob o ponteiro (para redimensionar), se houver.
    /// Vale para janelas com a barra de título da Plumagem; as que se
    /// decoram sozinhas cuidam das próprias bordas.
    pub fn borda_sob(&self, pos: Point<f64, Logical>) -> Option<(WindowElement, ResizeEdge)> {
        const FORA: f64 = 6.0; // além da janela
        const DENTRO: f64 = 3.0; // para dentro da janela
        const CANTO: f64 = 14.0;
        for w in self.space.elements().rev() {
            let Some(r) = self.space.element_geometry(w) else { continue };
            let (x0, y0) = (r.loc.x as f64, r.loc.y as f64);
            let (x1, y1) = (x0 + r.size.w as f64, y0 + r.size.h as f64);
            if pos.x < x0 - FORA || pos.x >= x1 + FORA || pos.y < y0 - FORA || pos.y >= y1 + FORA {
                continue;
            }
            let miolo = pos.x >= x0 + DENTRO && pos.x < x1 - DENTRO && pos.y >= y0 + DENTRO && pos.y < y1 - DENTRO;
            if miolo || !w.decoration_state().is_ssd || self.esta_maximizada(w) {
                // A janela de cima ganha: não procura bordas das de baixo.
                return None;
            }
            let perto_x = pos.x < x0 + CANTO || pos.x >= x1 - CANTO;
            let perto_y = pos.y < y0 + CANTO || pos.y >= y1 - CANTO;
            let na_borda_x = pos.x < x0 + DENTRO || pos.x >= x1 - DENTRO;
            let na_borda_y = pos.y < y0 + DENTRO || pos.y >= y1 - DENTRO;
            let mut e = ResizeEdge::NONE;
            if na_borda_x || (na_borda_y && perto_x) {
                e |= if pos.x < (x0 + x1) / 2.0 { ResizeEdge::LEFT } else { ResizeEdge::RIGHT };
            }
            if na_borda_y || (na_borda_x && perto_y) {
                e |= if pos.y < (y0 + y1) / 2.0 { ResizeEdge::TOP } else { ResizeEdge::BOTTOM };
            }
            return (e != ResizeEdge::NONE).then(|| (w.clone(), e));
        }
        None
    }

    /// Começa a redimensionar a janela pela borda, com o mouse.
    pub fn iniciar_redimensionar(
        &mut self,
        w: &WindowElement,
        bordas: ResizeEdge,
        botao: u32,
        serial: smithay::utils::Serial,
    ) {
        let Some(t) = w.0.toplevel().cloned() else { return };
        let Some(loc) = self.space.element_location(w) else { return };
        let tamanho = w.geometry().size;
        let dados = ResizeData {
            edges: bordas,
            initial_window_location: loc,
            initial_window_size: tamanho,
        };
        with_states(t.wl_surface(), |states| {
            if let Some(d) = states.data_map.get::<RefCell<SurfaceData>>() {
                d.borrow_mut().resize_state = ResizeState::Resizing(dados);
            }
        });
        self.focar_janela(w);
        self.painel.cursor_redim = Some(bordas);
        let grab = PointerResizeSurfaceGrab {
            start_data: GrabStartData {
                focus: None,
                button: botao,
                location: self.pointer.current_location(),
            },
            window: w.clone(),
            edges: bordas,
            initial_window_location: loc,
            initial_window_size: tamanho,
            last_window_size: tamanho,
        };
        let ponteiro = self.pointer.clone();
        ponteiro.set_grab(self, grab, serial, Focus::Clear);
    }

    /// Executa o que foi pedido no Ninho; se a entrada foi aceita, grava o
    /// usuário em $XDG_RUNTIME_DIR/escolha e encerra (o script abre a sessão).
    pub fn processar_ninho(&mut self) {
        let Some(n) = self.ninho.as_mut() else { return };
        if let Some(login) = n.processar() {
            let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/ninho".into());
            if let Err(e) = std::fs::write(format!("{dir}/escolha"), &login) {
                error!(err = %e, "não deu para gravar a escolha do Ninho");
                return;
            }
            self.running.store(false, Ordering::SeqCst);
        }
    }

    /// Alt+Tab: traz a janela que estava logo atrás (ou restaura uma
    /// minimizada, se só houver ela).
    /// Alt+Tab: abre o alternador (ou anda para a próxima janela nele).
    /// A troca acontece quando o Alt é solto (`alt_tab_soltar`).
    /// Win+V: a Plumagem passa a oferecer o texto e cola no app em foco
    /// (aperta Ctrl+V por ele, como o Windows faz).
    /// Atalhos de teclado na área de trabalho (ícone selecionado).
    pub fn atalho_area(&mut self, acao: &str) {
        let sel = self.painel.icone_sel;
        let arquivo = sel.and_then(|i| self.painel.arquivo_do_icone(i));
        match acao {
            "renomear" => {
                if let Some(i) = sel {
                    self.painel.renomear_icone(i);
                }
            }
            "excluir" => {
                if let Some(a) = arquivo {
                    if a.extension().and_then(|x| x.to_str()) == Some("desktop") {
                        let _ = std::fs::remove_file(&a);
                    } else if let Err(e) = painel::mandar_para_lixeira(&a) {
                        let _ = std::process::Command::new("ararinha-notificar").args(["Não deu para excluir", &e.to_string()]).spawn();
                    }
                    self.painel.icone_sel = None;
                }
            }
            "abrir" => {
                if let Some(cmd) = sel.and_then(|i| self.painel.comando_do_icone(i)) {
                    self.executar_programa(&cmd);
                }
            }
            "copiar" | "recortar" => {
                if let Some(a) = arquivo {
                    self.copiar_arquivos(vec![a], acao == "recortar");
                }
            }
            "colar" => {
                if let Some(p) = painel::pasta_area() {
                    let _ = std::fs::create_dir_all(&p);
                    crate::area::colar_em(p);
                }
            }
            _ => {}
        }
    }

    /// Copiar/Recortar na área de trabalho: a Plumagem passa a oferecer os
    /// arquivos (colam no Explorador e em outros apps).
    pub fn copiar_arquivos(&mut self, v: Vec<std::path::PathBuf>, corte: bool) {
        use smithay::wayland::selection::data_device::set_data_device_selection;
        if let Ok(mut p) = crate::area::PROPRIA.lock() {
            *p = None;
        }
        if let Ok(mut a) = crate::area::ARQUIVOS.lock() {
            *a = Some((v, corte));
        }
        crate::area::PROPRIA_ARQUIVOS.store(true, std::sync::atomic::Ordering::Relaxed);
        let tipos: Vec<String> = crate::area::TIPOS_ARQUIVOS.iter().map(|s| s.to_string()).collect();
        set_data_device_selection(&self.display_handle, &self.seat, tipos.clone(), ());
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut() {
            let _ = xwm.new_selection(smithay::wayland::selection::SelectionTarget::Clipboard, Some(tipos));
        }
    }

    pub fn colar_texto(&mut self, t: String) {
        use smithay::wayland::selection::data_device::set_data_device_selection;
        crate::area::guardar(t.clone());
        if let Ok(mut p) = crate::area::PROPRIA.lock() {
            *p = Some(t);
        }
        crate::area::PROPRIA_ARQUIVOS.store(false, std::sync::atomic::Ordering::Relaxed);
        let tipos: Vec<String> = crate::area::TIPOS_TEXTO.iter().map(|s| s.to_string()).collect();
        set_data_device_selection(&self.display_handle, &self.seat, tipos.clone(), ());
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut() {
            let _ = xwm.new_selection(smithay::wayland::selection::SelectionTarget::Clipboard, Some(tipos));
        }
        self.painel.area_aberta = false;
        // Ctrl+V sintético (códigos evdev + 8: Ctrl esquerdo 37, V 55).
        if let Some(teclado) = self.seat.get_keyboard() {
            use smithay::backend::input::KeyState;
            use smithay::input::keyboard::{xkb::Keycode, FilterResult};
            let time = self.clock.now().as_millis();
            for (codigo, estado) in [(37, KeyState::Pressed), (55, KeyState::Pressed), (55, KeyState::Released), (37, KeyState::Released)] {
                let serial = SERIAL_COUNTER.next_serial();
                teclado.input::<(), _>(self, Keycode::new(codigo), estado, serial, time, |_, _, _| FilterResult::Forward);
            }
        }
    }

    pub fn alt_tab(&mut self, voltar: bool) {
        if self.alternador_janelas.is_empty() || self.painel.alternador.is_none() {
            // Da mais recente para a mais antiga; minimizadas no fim.
            let mut v: Vec<WindowElement> = self.space.elements().rev().cloned().collect();
            v.extend(self.painel.minimizadas.iter().rev().map(|(w, _)| w.clone()));
            if v.is_empty() {
                return;
            }
            let lista: Vec<(String, Option<usize>)> = v
                .iter()
                .map(|w| (crate::painel::titulo(w), self.painel.app_de(w)))
                .collect();
            let sel = if v.len() > 1 { 1 } else { 0 };
            self.alternador_janelas = v;
            self.painel.alternador = Some((lista, sel));
            return;
        }
        if let Some((lista, sel)) = self.painel.alternador.as_mut() {
            let n = lista.len();
            *sel = if voltar { (*sel + n - 1) % n } else { (*sel + 1) % n };
        }
    }

    /// Alt solto: vai para a janela escolhida.
    pub fn alt_tab_soltar(&mut self, cancelar: bool) {
        let Some((_, sel)) = self.painel.alternador.take() else { return };
        let janelas = std::mem::take(&mut self.alternador_janelas);
        if cancelar {
            return;
        }
        if let Some(w) = janelas.get(sel).cloned() {
            if self.painel.minimizadas.iter().any(|(m, _)| m == &w) {
                self.desminimizar(&w);
            } else {
                self.focar_janela(&w);
            }
        }
    }

    /// Alt+F4: pede para a janela ativa fechar.
    pub fn fechar_janela_ativa(&mut self) {
        if let Some(w) = self.space.elements().last().cloned() {
            fechar(&w);
        }
    }

    /// Clique num botão da barra: abre o app, alterna a janela ou passa
    /// para a próxima janela do mesmo app.
    pub fn clicar_grupo(&mut self, i: usize) {
        let gs = self.painel.grupos();
        let Some(g) = gs.get(i) else { return };
        let ws: Vec<WindowElement> = g.janelas.iter().filter_map(|j| self.painel.ordem.get(*j).cloned()).collect();
        if ws.is_empty() {
            let cmd = g
                .app
                .and_then(|a| self.painel.apps_todos().get(a).map(|a| a.exec.clone()))
                .or_else(|| g.fixado.and_then(|f| self.painel.fixados.get(f).cloned()));
            if let Some(cmd) = cmd {
                self.executar_programa(&cmd);
            }
            return;
        }
        let ativa = self.space.elements().last().cloned();
        if ws.len() == 1 {
            let w = &ws[0];
            if self.painel.esta_minimizada(w) {
                self.desminimizar(w);
            } else if ativa.as_ref() == Some(w) {
                self.minimizar(w);
            } else {
                self.focar_janela(w);
            }
            return;
        }
        // Várias: a próxima do grupo depois da que está em uso.
        let k = ativa.as_ref().and_then(|a| ws.iter().position(|w| w == a)).map(|k| (k + 1) % ws.len()).unwrap_or(0);
        let w = ws[k].clone();
        if self.painel.esta_minimizada(&w) {
            self.desminimizar(&w);
        } else {
            self.focar_janela(&w);
        }
    }

    /// Abre ou fecha a Visão de Tarefas (Win+Tab).
    pub fn alternar_visao(&mut self) {
        self.painel.visao = !self.painel.visao;
        self.painel.menu_aberto = false;
        self.painel.central_aberta = false;
        if self.painel.visao {
            self.atualizar_visao();
        }
    }

    /// Janelas da área de trabalho atual (da frente para trás; minimizadas no fim).
    pub fn atualizar_visao(&mut self) {
        let mut v: Vec<WindowElement> = self.space.elements().rev().cloned().collect();
        v.extend(self.painel.minimizadas.iter().rev().map(|(w, _)| w.clone()));
        self.painel.visao_janelas = v;
    }

    /// Troca de área de trabalho: guarda as janelas desta e mostra as da outra.
    pub fn trocar_mesa(&mut self, n: usize) {
        let atual = self.painel.mesa_atual;
        if n == atual {
            return;
        }
        if n >= self.painel.mesas {
            self.painel.mesas = (n + 1).min(9);
        }
        let n = n.min(self.painel.mesas - 1);
        let visiveis: Vec<WindowElement> = self.space.elements().cloned().collect();
        for w in visiveis {
            if let Some(loc) = self.space.element_location(&w) {
                self.space.unmap_elem(&w);
                self.painel.outras_mesas.push((w, loc, atual, false));
            }
        }
        for (w, loc) in std::mem::take(&mut self.painel.minimizadas) {
            self.painel.outras_mesas.push((w, loc, atual, true));
        }
        let (dela, resto): (Vec<_>, Vec<_>) = std::mem::take(&mut self.painel.outras_mesas).into_iter().partition(|o| o.2 == n);
        self.painel.outras_mesas = resto;
        let mut ultima = None;
        for (w, loc, _, min) in dela {
            if min {
                self.painel.minimizadas.push((w, loc));
            } else {
                self.space.map_element(w.clone(), loc, false);
                ultima = Some(w);
            }
        }
        self.painel.mesa_atual = n;
        let teclado = self.seat.get_keyboard().unwrap();
        teclado.set_focus(self, ultima.map(KeyboardFocusTarget::from), SERIAL_COUNTER.next_serial());
        self.painel.barra = None;
        self.painel.avisar_mesa();
    }

    /// Manda uma janela para outra área de trabalho.
    pub fn mover_para_mesa(&mut self, w: &WindowElement, n: usize) {
        if n == self.painel.mesa_atual {
            return;
        }
        if n >= self.painel.mesas {
            self.painel.mesas = (n + 1).min(9);
        }
        let n = n.min(self.painel.mesas - 1);
        if let Some(i) = self.painel.minimizadas.iter().position(|(m, _)| m == w) {
            let (w, loc) = self.painel.minimizadas.remove(i);
            self.painel.outras_mesas.push((w, loc, n, true));
        } else if let Some(loc) = self.space.element_location(w) {
            self.space.unmap_elem(w);
            self.painel.outras_mesas.push((w.clone(), loc, n, false));
        }
        self.painel.barra = None;
    }

    /// Fecha uma área de trabalho: as janelas dela vão para a vizinha.
    pub fn fechar_mesa(&mut self, k: usize) {
        if self.painel.mesas <= 1 || k >= self.painel.mesas {
            return;
        }
        let destino = if k == 0 { 1 } else { k - 1 };
        if k == self.painel.mesa_atual {
            self.trocar_mesa(destino);
        }
        // Renumera: a fechada some, as de depois andam uma para trás.
        for o in self.painel.outras_mesas.iter_mut() {
            if o.2 == k {
                o.2 = destino;
            }
            if o.2 > k {
                o.2 -= 1;
            }
        }
        if self.painel.mesa_atual > k {
            self.painel.mesa_atual -= 1;
        }
        // A janela que veio da fechada para a atual aparece agora.
        let atual = self.painel.mesa_atual;
        let (dela, resto): (Vec<_>, Vec<_>) = std::mem::take(&mut self.painel.outras_mesas).into_iter().partition(|o| o.2 == atual);
        self.painel.outras_mesas = resto;
        for (w, loc, _, min) in dela {
            if min {
                self.painel.minimizadas.push((w, loc));
            } else {
                self.space.map_element(w, loc, false);
            }
        }
        self.painel.mesas -= 1;
        self.painel.barra = None;
    }

    /// Win+Ctrl+setas: área de trabalho vizinha; Win+Ctrl+D cria outra.
    pub fn mesa_vizinha(&mut self, passo: i32) {
        let n = self.painel.mesa_atual as i32 + passo;
        if n >= 0 && (n as usize) < self.painel.mesas {
            self.trocar_mesa(n as usize);
        }
    }

    /// Win+D: minimiza todas; se já está tudo minimizado, restaura.
    pub fn mostrar_area_de_trabalho(&mut self) {
        let abertas: Vec<WindowElement> = self.space.elements().cloned().collect();
        if abertas.is_empty() {
            let todas: Vec<WindowElement> = self.painel.minimizadas.iter().map(|(w, _)| w.clone()).collect();
            for w in todas {
                self.desminimizar(&w);
            }
        } else {
            for w in abertas {
                self.minimizar(&w);
            }
        }
    }
}

/// Pede ao serviço do sistema (ararinha-sistema, root) para desligar,
/// reiniciar ou dormir: a sessão do usuário não tem permissão para chamar
/// o systemctl. Toca o som antes, numa thread, sem travar a tela.
/// Pede para a janela fechar (xdg ou X11).
pub fn fechar(w: &WindowElement) {
    if let Some(t) = w.0.toplevel() {
        t.send_close();
    }
    #[cfg(feature = "xwayland")]
    if let Some(x) = w.0.x11_surface() {
        let _ = x.close();
    }
}

pub fn pedir_sistema(pedido: &'static str, som: Option<&'static str>) {
    std::thread::spawn(move || {
        // O som toca junto, mas não pode segurar o pedido (se o PipeWire
        // estiver travado, o ararinha-som nunca termina): espera até 1,5 s.
        if let Some(s) = som {
            if let Ok(mut f) = std::process::Command::new("ararinha-som").arg(s).spawn() {
                let ate = std::time::Instant::now() + std::time::Duration::from_millis(1500);
                while std::time::Instant::now() < ate {
                    if matches!(f.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        use std::io::{Read, Write};
        let erro = match std::os::unix::net::UnixStream::connect("/run/ararinha/sistema.sock") {
            Ok(mut c) => {
                let _ = c.set_read_timeout(Some(std::time::Duration::from_secs(30)));
                let _ = c.write_all(format!("{pedido}\n").as_bytes());
                let _ = c.shutdown(std::net::Shutdown::Write);
                let mut r = String::new();
                let _ = c.read_to_string(&mut r);
                if r.starts_with("OK") {
                    None
                } else if r.trim().is_empty() {
                    Some("o serviço do sistema não respondeu".to_string())
                } else {
                    Some(r.trim().trim_start_matches("ERRO").trim().to_string())
                }
            }
            Err(e) => Some(format!("o serviço do sistema não está rodando ({e})")),
        };
        // Mostra o motivo na tela: sem isso, o botão parece não fazer nada.
        if let Some(e) = erro {
            tracing::warn!("ararinha-sistema recusou {pedido}: {e}");
            let titulo = match pedido {
                "desligar" => "Não deu para desligar",
                "reiniciar" => "Não deu para reiniciar",
                "hibernar" => "Não deu para hibernar",
                _ => "Não deu para dormir",
            };
            let _ = std::process::Command::new("ararinha-notificar").args([titulo, &e]).status();
        }
    });
}

impl<BackendData: crate::state::Backend + 'static> crate::state::AnvilState<BackendData> {
    /// Sai do modo de recorte da captura.
    pub fn cancelar_recorte(&mut self) {
        self.painel.recorte = None;
        crate::render::RECORTE.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::captura::MODO_JANELA.store(false, std::sync::atomic::Ordering::Relaxed);
        // Fechar o recorte sem captura pendente = cancelado (com captura
        // pendente, a resposta sai quando o arquivo for salvo).
        if !crate::captura::PEDIDA.load(std::sync::atomic::Ordering::Relaxed)
            && crate::captura::PARA_APP.swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            crate::captura::responder("cancelado");
            self.restaurar_captura();
        }
    }

    /// Oferece uma imagem PNG na área de transferência (captura de tela).
    pub fn copiar_imagem(&mut self, png: Vec<u8>) {
        use smithay::wayland::selection::data_device::set_data_device_selection;
        if let Ok(mut p) = crate::area::PROPRIA.lock() {
            *p = None;
        }
        crate::area::PROPRIA_ARQUIVOS.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut i) = crate::area::PROPRIA_IMAGEM.lock() {
            *i = Some(png);
        }
        let tipos = vec!["image/png".to_string()];
        set_data_device_selection(&self.display_handle, &self.seat, tipos.clone(), ());
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut() {
            let _ = xwm.new_selection(smithay::wayland::selection::SelectionTarget::Clipboard, Some(tipos));
        }
    }

    /// Pedido da Ferramenta de Captura: esconde a janela dela e captura a
    /// tela inteira, um recorte ou uma janela.
    pub fn verificar_pedido_captura(&mut self) {
        let Some(modo) = crate::captura::ler_pedido() else { return };
        use std::sync::atomic::Ordering::Relaxed;
        let dela: Vec<WindowElement> = self
            .space
            .elements()
            .filter(|w| painel::id_da_janela(w).as_deref() == Some("captura"))
            .cloned()
            .collect();
        for w in &dela {
            self.minimizar(w);
        }
        self.painel.captura_escondidas = dela;
        self.painel.menu_aberto = false;
        crate::captura::PARA_APP.store(true, Relaxed);
        match modo.as_str() {
            "tela" => crate::captura::pedir(),
            m => {
                crate::captura::MODO_JANELA.store(m == "janela", Relaxed);
                self.painel.recorte = Some(None);
                crate::render::RECORTE.store(true, Relaxed);
            }
        }
    }

    /// Volta a mostrar a Ferramenta de Captura depois da captura.
    pub fn restaurar_captura(&mut self) {
        for w in std::mem::take(&mut self.painel.captura_escondidas) {
            self.desminimizar(&w);
        }
    }
}
