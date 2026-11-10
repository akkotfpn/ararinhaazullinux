// SPDX-License-Identifier: GPL-3.0-or-later
//! Gerência de janelas da Plumagem: minimizar, maximizar respeitando a barra
//! de tarefas, restaurar para onde estava, e as ações da barra e do menu.

use std::{cell::RefCell, process::Command, sync::atomic::Ordering};

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
    shell::{ssd::HEADER_BAR_HEIGHT, FullscreenSurface, WindowElement},
    state::{AnvilState, Backend},
};

/// Posição e tamanho antes de maximizar, guardados na própria janela.
struct GeometriaAnterior(RefCell<Option<(Point<i32, Logical>, Option<Size<i32, Logical>>)>>);

/// Área da saída que as janelas podem ocupar (tudo menos a barra de tarefas).
pub fn area_util(space: &Space<WindowElement>, output: &Output) -> Option<Rectangle<i32, Logical>> {
    let mut g = space.output_geometry(output)?;
    g.size.h = (g.size.h - painel::ALTURA).max(1);
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

    pub fn focar_janela(&mut self, w: &WindowElement) {
        self.space.raise_element(w, true);
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
        w.0.toplevel()
            .map(|t| t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Maximized)))
            .unwrap_or(false)
    }

    /// Maximiza ocupando a tela menos a barra de tarefas.
    pub fn maximizar(&mut self, w: &WindowElement) {
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

    /// Volta ao tamanho e lugar de antes de maximizar.
    pub fn restaurar(&mut self, w: &WindowElement) {
        let Some(t) = w.0.toplevel().cloned() else { return };
        let anterior = w
            .0
            .user_data()
            .get::<GeometriaAnterior>()
            .and_then(|g| g.0.borrow_mut().take());
        t.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Maximized);
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
    pub fn clique_na_interface(&mut self, pressionado: bool) -> bool {
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
            // Clique fora: fecha o menu e segue para a janela.
            self.painel.menu_aberto = false;
            return false;
        };
        self.painel.segurando = true;

        match alvo {
            Alvo::Iniciar => self.alternar_menu(),
            Alvo::Tarefa(i) => {
                self.painel.menu_aberto = false;
                if let Some(w) = self.painel.ordem.get(i).cloned() {
                    if self.painel.esta_minimizada(&w) {
                        self.desminimizar(&w);
                    } else if self.space.elements().last() == Some(&w) {
                        self.minimizar(&w);
                    } else {
                        self.focar_janela(&w);
                    }
                }
            }
            Alvo::App(i) => {
                self.painel.menu_aberto = false;
                if let Some(app) = self.painel.apps.as_ref().and_then(|a| a.get(i)).cloned() {
                    self.executar_programa(&app.exec);
                }
            }
            Alvo::Sair => {
                info!("saindo pela opção do menu");
                self.running.store(false, Ordering::SeqCst);
            }
            Alvo::Reiniciar => self.executar_programa("systemctl reboot"),
            Alvo::Desligar => self.executar_programa("systemctl poweroff"),
            Alvo::Barra | Alvo::Menu => {}
        }
        true
    }

    pub fn alternar_menu(&mut self) {
        self.painel.menu_aberto = !self.painel.menu_aberto;
        if self.painel.menu_aberto {
            self.painel.carregar_apps();
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
}
