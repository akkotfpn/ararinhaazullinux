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

/// Posição e tamanho antes de maximizar, guardados na própria janela.
struct GeometriaAnterior(RefCell<Option<(Point<i32, Logical>, Option<Size<i32, Logical>>)>>);

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

    /// Encaixa a janela ativa (Win+setas): metade esquerda/direita, maximizar,
    /// e para baixo restaura ou minimiza, como no Windows.
    pub fn encaixar(&mut self, lado: Encaixe) {
        let Some(w) = self.space.elements().last().cloned() else { return };
        match lado {
            Encaixe::Cima => {
                if !self.esta_maximizada(&w) {
                    self.maximizar(&w);
                }
                return;
            }
            Encaixe::Baixo => {
                if self.esta_maximizada(&w) || self.esta_encaixada(&w) {
                    self.restaurar(&w);
                } else {
                    self.minimizar(&w);
                }
                return;
            }
            _ => {}
        }
        let Some(t) = w.0.toplevel().cloned() else { return };
        let output = self.space.outputs_for_element(&w).first().cloned().or_else(|| self.space.outputs().next().cloned());
        let Some(area) = output.and_then(|o| area_util(&self.space, &o)) else { return };
        // Guarda o tamanho de antes (se ainda não está maximizada/encaixada).
        if !self.esta_maximizada(&w) && !self.esta_encaixada(&w) {
            if let Some(loc) = self.space.element_location(&w) {
                let tamanho = t.current_state().size;
                w.0.user_data().insert_if_missing(|| GeometriaAnterior(RefCell::new(None)));
                if let Some(g) = w.0.user_data().get::<GeometriaAnterior>() {
                    *g.0.borrow_mut() = Some((loc, tamanho));
                }
            }
        }
        let meia = area.size.w / 2;
        let x = if lado == Encaixe::Esquerda { area.loc.x } else { area.loc.x + meia };
        let mut tamanho: Size<i32, Logical> = (meia, area.size.h).into();
        let y = area.loc.y;
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
        w.0.toplevel()
            .map(|t| t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::TiledLeft)))
            .unwrap_or(false)
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
                        "#area" => self.mostrar_area_de_trabalho(),
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
            if matches!(alvo, Alvo::Barra | Alvo::Tarefa(_) | Alvo::Iniciar | Alvo::Fixado(_)) {
                let p = (pos - geo.loc.to_f64()).to_i32_round();
                self.painel.contexto_barra(p, geo.size);
            }
            self.painel.segurando = true;
            return true;
        }
        if !matches!(alvo, Alvo::Som | Alvo::SomBarra(_) | Alvo::SomMudo | Alvo::SomPainel) {
            self.painel.som_aberto = false;
        }
        if !matches!(alvo, Alvo::Rede | Alvo::RedePainel | Alvo::RedeConfig) {
            self.painel.rede_aberta = false;
        }
        self.painel.segurando = true;
        if alvo != Alvo::Energia {
            self.painel.energia_aberta = false;
        }

        match alvo {
            Alvo::Iniciar => self.alternar_menu(),
            Alvo::Fixado(i) => {
                self.painel.menu_aberto = false;
                let cmd = self
                    .painel
                    .app_fixado(i)
                    .map(|a| a.exec.clone())
                    .unwrap_or_else(|| painel::FIXADOS[i].to_string());
                self.executar_programa(&cmd);
            }
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
                if let Some(exec) = self.painel.apps_visiveis().get(i).map(|a| a.exec.clone()) {
                    self.executar_programa(&exec);
                }
            }
            Alvo::Energia => self.painel.energia_aberta = !self.painel.energia_aberta,
            Alvo::Dormir => {
                self.painel.menu_aberto = false;
                // O gancho do system-sleep bloqueia a tela antes de dormir.
                pedir_sistema("dormir", None);
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
            Alvo::Barra | Alvo::Menu | Alvo::SomPainel | Alvo::Contexto(_) => {}
            Alvo::CantoArea => self.mostrar_area_de_trabalho(),
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
    pub fn alt_tab(&mut self) {
        let janelas: Vec<WindowElement> = self.space.elements().cloned().collect();
        if janelas.len() >= 2 {
            let w = janelas[janelas.len() - 2].clone();
            self.focar_janela(&w);
        } else if let Some((w, _)) = self.painel.minimizadas.last().cloned() {
            self.desminimizar(&w);
        }
    }

    /// Alt+F4: pede para a janela ativa fechar.
    pub fn fechar_janela_ativa(&mut self) {
        if let Some(w) = self.space.elements().last() {
            if let Some(t) = w.0.toplevel() {
                t.send_close();
            }
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
pub fn pedir_sistema(pedido: &'static str, som: Option<&'static str>) {
    std::thread::spawn(move || {
        if let Some(s) = som {
            let _ = std::process::Command::new("ararinha-som").arg(s).status();
        }
        use std::io::{Read, Write};
        match std::os::unix::net::UnixStream::connect("/run/ararinha/sistema.sock") {
            Ok(mut c) => {
                let _ = c.write_all(format!("{pedido}\n").as_bytes());
                let _ = c.shutdown(std::net::Shutdown::Write);
                let mut r = String::new();
                let _ = c.read_to_string(&mut r);
                if !r.starts_with("OK") {
                    tracing::warn!("ararinha-sistema recusou {pedido}: {}", r.trim());
                }
            }
            Err(e) => tracing::warn!("ararinha-sistema indisponível: {e}"),
        }
    });
}

impl<BackendData: crate::state::Backend + 'static> crate::state::AnvilState<BackendData> {
    /// Sai do modo de recorte da captura.
    pub fn cancelar_recorte(&mut self) {
        self.painel.recorte = None;
        crate::render::RECORTE.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
