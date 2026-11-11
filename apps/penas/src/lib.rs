// SPDX-License-Identifier: GPL-3.0-or-later
//! Penas: o kit de interface dos aplicativos do Ararinha.
//!
//! Cada app abre uma janela Wayland (pelo smithay-client-toolkit), desenha
//! tudo pela CPU com o mesmo desenho da Plumagem e recebe a cada quadro um
//! [`Ui`] com o estado do mouse e do teclado. É um modelo "imediato": o app
//! redesenha a janela inteira a cada evento e pergunta ao `Ui` se a área de
//! um botão foi clicada.

pub mod componentes;
pub mod sistema;
pub mod desenho;

use std::time::{Duration, Instant};

pub use desenho::{tema, Cor, Tela};
pub use smithay_client_toolkit::seat::keyboard::Keysym;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::EventLoop,
        calloop_wayland_source::WaylandSource,
        client::{
            globals::registry_queue_init,
            protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
            Connection, QueueHandle,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT, BTN_RIGHT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};

/// Tecla recebida neste quadro.
#[derive(Debug, Clone, PartialEq)]
pub enum Tecla {
    Texto(String),
    Enter,
    Apagar,
    Esc,
    Cima,
    Baixo,
    Esquerda,
    Direita,
    /// Alt+Esquerda (voltar).
    Voltar,
    /// Alt+Direita (avançar).
    Avancar,
    /// Alt+Cima (pasta de cima).
    Subir,
    Outra(Keysym),
}

/// O que o app recebe a cada quadro para desenhar e reagir.
pub struct Ui<'a> {
    pub tela: &'a mut Tela,
    /// Posição do mouse dentro da janela, se ele estiver nela.
    pub mouse: Option<(f32, f32)>,
    /// O botão esquerdo foi apertado neste quadro.
    pub clique: bool,
    /// Foi o segundo clique rápido (clique duplo).
    pub duplo: bool,
    pub clique_direito: bool,
    /// O botão esquerdo está apertado agora (para arrastar).
    pub pressionado: bool,
    /// O botão esquerdo foi solto neste quadro (fim de um arraste).
    pub soltou: bool,
    /// Rolagem vertical, em pixels (positivo = para baixo).
    pub rolagem: f32,
    pub teclas: Vec<Tecla>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// O app pede para fechar a janela.
    pub fechar: bool,
    /// O app pede outro quadro logo em seguida (animação).
    pub redesenhar: bool,
    /// Novo título da janela, se o app quiser mudar.
    pub titulo: Option<String>,
    /// Pede um quadro novo daqui a esse tempo, mesmo sem eventos
    /// (relógios, gráficos que se atualizam sozinhos).
    pub acordar_em: Option<Duration>,
}

impl Ui<'_> {
    pub fn largura(&self) -> f32 {
        self.tela.largura as f32
    }
    pub fn altura(&self) -> f32 {
        self.tela.altura as f32
    }
    /// O mouse está sobre o retângulo?
    pub fn sobre(&self, x: f32, y: f32, l: f32, a: f32) -> bool {
        self.mouse.map(|(mx, my)| mx >= x && my >= y && mx < x + l && my < y + a).unwrap_or(false)
    }
    /// O retângulo foi clicado neste quadro?
    pub fn clicou(&self, x: f32, y: f32, l: f32, a: f32) -> bool {
        self.clique && self.sobre(x, y, l, a)
    }
    /// O retângulo recebeu um clique duplo neste quadro?
    pub fn clicou_duplo(&self, x: f32, y: f32, l: f32, a: f32) -> bool {
        self.duplo && self.sobre(x, y, l, a)
    }
    pub fn clicou_direito(&self, x: f32, y: f32, l: f32, a: f32) -> bool {
        self.clique_direito && self.sobre(x, y, l, a)
    }
}

/// Um aplicativo das Penas.
pub trait App {
    fn desenhar(&mut self, ui: &mut Ui);
}

pub struct Opcoes {
    pub titulo: String,
    pub app_id: String,
    pub tamanho: (u32, u32),
    pub minimo: (u32, u32),
}

fn arquivos_config() -> [std::path::PathBuf; 2] {
    let casa = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    [
        std::path::PathBuf::from(casa).join(".config/plumagem/plumagem.conf"),
        std::path::PathBuf::from("/etc/plumagem/plumagem.conf"),
    ]
}

/// `tema = escuro` na configuração da Plumagem (a do usuário vale mais).
/// Valor de uma chave do plumagem.conf (o do usuário vale mais).
fn ler_chave_config(chave: &str) -> Option<String> {
    for c in arquivos_config() {
        let Ok(t) = std::fs::read_to_string(&c) else { continue };
        for l in t.lines() {
            if l.trim_start().starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == chave {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

/// Aplica o tema e a cor de destaque da configuração.
fn aplicar_tema() {
    tema::definir_escuro(ler_tema_escuro());
    tema::definir_destaque(ler_chave_config("destaque").as_deref().unwrap_or("azul"));
}

fn ler_tema_escuro() -> bool {
    for c in arquivos_config() {
        let Ok(t) = std::fs::read_to_string(&c) else { continue };
        for l in t.lines() {
            if l.trim_start().starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "tema" {
                    return v.trim().trim_matches('"') == "escuro";
                }
            }
        }
    }
    false
}

fn versao_config() -> Vec<Option<std::time::SystemTime>> {
    arquivos_config().iter().map(|c| std::fs::metadata(c).and_then(|m| m.modified()).ok()).collect()
}

extern "C" {
    fn inotify_init1(flags: i32) -> i32;
    fn inotify_add_watch(fd: i32, caminho: *const std::os::raw::c_char, mascara: u32) -> i32;
}

/// Vigia as pastas da configuração com inotify, dentro do laço de eventos.
/// Devolve false se não deu (aí o laço confere de tempos em tempos).
fn vigiar_config(event_loop: &EventLoop<Janela>) -> bool {
    use smithay_client_toolkit::reexports::calloop::{generic::Generic, Interest, Mode, PostAction};
    use std::os::fd::FromRawFd;
    const IN_NONBLOCK: i32 = 0o4000;
    const IN_CLOEXEC: i32 = 0o2000000;
    // IN_CLOSE_WRITE | IN_MOVED_TO | IN_CREATE | IN_DELETE
    const MASCARA: u32 = 0x8 | 0x80 | 0x100 | 0x200;
    // SAFETY: chamada simples ao sistema; o fd é conferido abaixo.
    let fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
    if fd < 0 {
        return false;
    }
    // SAFETY: o fd acabou de ser criado e passa a ser só deste File.
    let arquivo = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut algum = false;
    for c in arquivos_config() {
        let Some(pasta) = c.parent() else { continue };
        let _ = std::fs::create_dir_all(pasta);
        let Ok(cp) = std::ffi::CString::new(pasta.as_os_str().as_encoded_bytes()) else { continue };
        // SAFETY: fd válido e string C válida durante a chamada.
        if unsafe { inotify_add_watch(fd, cp.as_ptr(), MASCARA) } >= 0 {
            algum = true;
        }
    }
    if !algum {
        return false;
    }
    let Ok(mut leitor) = arquivo.try_clone() else { return false };
    let fonte = Generic::new(arquivo, Interest::READ, Mode::Level);
    event_loop
        .handle()
        .insert_source(fonte, move |_, _, janela: &mut Janela| {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            while matches!(leitor.read(&mut buf), Ok(n) if n > 0) {}
            janela.config_mudou = true;
            Ok(PostAction::Continue)
        })
        .is_ok()
}

/// Abre a janela e roda o app até ela ser fechada.
pub fn executar(opcoes: Opcoes, app: Box<dyn App>) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("penas: sem sessão Wayland ({e}). Abra pela área de trabalho do Ararinha.");
            std::process::exit(1);
        }
    };
    let (globals, event_queue) = registry_queue_init(&conn).expect("registro Wayland");
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<Janela> = EventLoop::try_new().expect("laço de eventos");
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .expect("fonte Wayland");

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title(opcoes.titulo.clone());
    window.set_app_id(opcoes.app_id.clone());
    window.set_min_size(Some(opcoes.minimo));
    window.commit();

    let (l, a) = opcoes.tamanho;
    let pool = SlotPool::new((l * a * 4) as usize, &shm).expect("memória da janela");

    let mut janela = Janela {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        buffer: None,
        window,
        largura: l,
        altura: a,
        configurada: false,
        sair: false,
        app,
        teclado: None,
        ponteiro: None,
        mouse: None,
        clique: false,
        duplo: false,
        clique_direito: false,
        pressionado: false,
        soltou: false,
        rolagem: 0.0,
        teclas: Vec::new(),
        mods: Modifiers::default(),
        ultimo_clique: None,
        sujo: true,
        config_mudou: false,
        acordar: None,
        repetir: None,
        titulo: opcoes.titulo,
        tela: Tela::nova(l as i32, a as i32),
    };

    aplicar_tema();
    // O tema mudou nas Configurações? O inotify avisa; sem ele, confere a
    // cada 2 s. Parado, o laço não acorda à toa.
    let vigia = vigiar_config(&event_loop);
    let mut config = (Instant::now(), versao_config());
    while !janela.sair {
        if janela.config_mudou || (!vigia && config.0.elapsed() >= Duration::from_secs(2)) {
            janela.config_mudou = false;
            let v = versao_config();
            if v != config.1 {
                aplicar_tema();
                janela.sujo = true;
            }
            config = (Instant::now(), v);
        }
        let proximo = match (janela.acordar, janela.repetir.as_ref().map(|r| r.2)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        // Quadro pedido com `redesenhar` (animação contínua): 20 por segundo.
        let teto = if janela.sujo {
            Some(Duration::from_millis(50))
        } else if vigia {
            None
        } else {
            Some(Duration::from_secs(2))
        };
        let espera = match (proximo.map(|t| t.saturating_duration_since(Instant::now())), teto) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if event_loop.dispatch(espera, &mut janela).is_err() {
            break;
        }
        if let Some((_, tecla, quando)) = janela.repetir.as_mut() {
            if Instant::now() >= *quando {
                janela.teclas.push(tecla.clone());
                *quando = Instant::now() + Duration::from_millis(33);
                janela.sujo = true;
            }
        }
        if janela.acordar.is_some_and(|t| Instant::now() >= t) {
            janela.acordar = None;
            janela.sujo = true;
        }
        if janela.configurada && janela.sujo {
            janela.desenhar(&qh);
        }
    }
}

struct Janela {
    /// O inotify viu mudança na pasta da configuração.
    config_mudou: bool,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    buffer: Option<Buffer>,
    window: Window,
    largura: u32,
    altura: u32,
    configurada: bool,
    sair: bool,
    app: Box<dyn App>,
    teclado: Option<wl_keyboard::WlKeyboard>,
    ponteiro: Option<wl_pointer::WlPointer>,
    mouse: Option<(f32, f32)>,
    clique: bool,
    duplo: bool,
    clique_direito: bool,
    pressionado: bool,
    soltou: bool,
    rolagem: f32,
    teclas: Vec<Tecla>,
    mods: Modifiers,
    ultimo_clique: Option<(Instant, (f32, f32))>,
    sujo: bool,
    acordar: Option<Instant>,
    /// Tecla segurada: código, tecla e quando repetir.
    repetir: Option<(u32, Tecla, Instant)>,
    titulo: String,
    tela: Tela,
}

impl Janela {
    fn desenhar(&mut self, qh: &QueueHandle<Self>) {
        let (l, a) = (self.largura.max(1), self.altura.max(1));
        if self.tela.largura != l as i32 || self.tela.altura != a as i32 {
            self.tela = Tela::nova(l as i32, a as i32);
        }
        let mut ui = Ui {
            tela: &mut self.tela,
            mouse: self.mouse,
            clique: std::mem::take(&mut self.clique),
            duplo: std::mem::take(&mut self.duplo),
            clique_direito: std::mem::take(&mut self.clique_direito),
            pressionado: self.pressionado,
            soltou: std::mem::take(&mut self.soltou),
            rolagem: {
                // Rolagem suave: entrega uma parte do que falta a cada quadro.
                let falta = self.rolagem;
                let parte = if falta.abs() <= 2.0 { falta } else { falta * 0.4 };
                self.rolagem -= parte;
                parte
            },
            teclas: std::mem::take(&mut self.teclas),
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            alt: self.mods.alt,
            fechar: false,
            redesenhar: false,
            titulo: None,
            acordar_em: None,
        };
        self.app.desenhar(&mut ui);
        let (fechar, redesenhar, titulo) = (ui.fechar, ui.redesenhar, ui.titulo.take());
        // Ainda há rolagem a entregar: outro quadro logo.
        let redesenhar = redesenhar || self.rolagem.abs() > 0.0;
        if self.rolagem.abs() > 0.0 && ui.acordar_em.is_none() {
            ui.acordar_em = Some(Duration::from_millis(16));
        }
        if let Some(d) = ui.acordar_em {
            self.acordar = Some(Instant::now() + d);
        }
        if fechar {
            self.sair = true;
        }
        if let Some(t) = titulo {
            if t != self.titulo {
                self.window.set_title(t.clone());
                self.titulo = t;
            }
        }
        self.sujo = redesenhar;

        let stride = l as i32 * 4;
        let precisa_novo = self
            .buffer
            .as_ref()
            .map(|b| b.height() != a as i32 || b.stride() != stride)
            .unwrap_or(true);
        if precisa_novo {
            self.buffer = None;
        }
        let buffer = match self.buffer.take() {
            Some(b) => b,
            None => match self.pool.create_buffer(l as i32, a as i32, stride, wl_shm::Format::Argb8888) {
                Ok((b, _)) => b,
                Err(e) => {
                    eprintln!("penas: sem memória para a janela: {e}");
                    return;
                }
            },
        };
        let buffer = match self.pool.canvas(&buffer) {
            Some(_) => buffer,
            // O compositor ainda usa o anterior: cria outro.
            None => match self.pool.create_buffer(l as i32, a as i32, stride, wl_shm::Format::Argb8888) {
                Ok((b, _)) => b,
                Err(_) => return,
            },
        };
        if let Some(canvas) = self.pool.canvas(&buffer) {
            self.tela.copiar_para_argb(canvas);
        }
        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, l as i32, a as i32);
        if buffer.attach_to(surface).is_ok() {
            self.window.commit();
        }
        self.buffer = Some(buffer);
        let _ = qh;
    }
}

impl CompositorHandler for Janela {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Janela {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Janela {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.sair = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, c: WindowConfigure, _: u32) {
        if let Some(l) = c.new_size.0 {
            self.largura = l.get();
        }
        if let Some(a) = c.new_size.1 {
            self.altura = a.get();
        }
        self.configurada = true;
        self.sujo = true;
    }
}

impl SeatHandler for Janela {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.teclado.is_none() {
            self.teclado = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.ponteiro.is_none() {
            self.ponteiro = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard {
            if let Some(t) = self.teclado.take() {
                t.release();
            }
        }
        if cap == Capability::Pointer {
            if let Some(p) = self.ponteiro.take() {
                p.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Janela {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {
        self.repetir = None;
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        let alt = self.mods.alt;
        let tecla = match ev.keysym {
            Keysym::Return | Keysym::KP_Enter => Tecla::Enter,
            Keysym::BackSpace => Tecla::Apagar,
            Keysym::Escape => Tecla::Esc,
            Keysym::Up if alt => Tecla::Subir,
            Keysym::Left if alt => Tecla::Voltar,
            Keysym::Right if alt => Tecla::Avancar,
            Keysym::Up => Tecla::Cima,
            Keysym::Down => Tecla::Baixo,
            Keysym::Left => Tecla::Esquerda,
            Keysym::Right => Tecla::Direita,
            outra => match ev.utf8.filter(|t| !t.chars().any(|c| c.is_control())) {
                Some(t) if !self.mods.ctrl && !alt => Tecla::Texto(t),
                _ => Tecla::Outra(outra),
            },
        };
        // Repetição da tecla segurada (atraso de 450 ms, depois 30 por segundo).
        let modificadora = matches!(
            ev.keysym,
            Keysym::Shift_L
                | Keysym::Shift_R
                | Keysym::Control_L
                | Keysym::Control_R
                | Keysym::Alt_L
                | Keysym::Alt_R
                | Keysym::Super_L
                | Keysym::Super_R
                | Keysym::ISO_Level3_Shift
                | Keysym::Caps_Lock
                | Keysym::Num_Lock
        );
        self.repetir = if modificadora || tecla == Tecla::Esc {
            None
        } else {
            Some((ev.raw_code, tecla.clone(), Instant::now() + Duration::from_millis(450)))
        };
        self.teclas.push(tecla);
        self.sujo = true;
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        if self.repetir.as_ref().map(|(c, _, _)| *c == ev.raw_code).unwrap_or(false) {
            self.repetir = None;
        }
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        mods: Modifiers,
        _: u32,
    ) {
        self.mods = mods;
    }
}

impl PointerHandler for Janela {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, eventos: &[PointerEvent]) {
        for ev in eventos {
            if &ev.surface != self.window.wl_surface() {
                continue;
            }
            let pos = (ev.position.0 as f32, ev.position.1 as f32);
            match ev.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => self.mouse = Some(pos),
                PointerEventKind::Leave { .. } => self.mouse = None,
                PointerEventKind::Press { button, .. } => {
                    self.mouse = Some(pos);
                    if button == BTN_LEFT {
                        self.pressionado = true;
                        let agora = Instant::now();
                        self.duplo = self
                            .ultimo_clique
                            .map(|(t, p)| {
                                agora.duration_since(t) < Duration::from_millis(450)
                                    && (p.0 - pos.0).abs() < 5.0
                                    && (p.1 - pos.1).abs() < 5.0
                            })
                            .unwrap_or(false);
                        self.ultimo_clique = if self.duplo { None } else { Some((agora, pos)) };
                        self.clique = true;
                    } else if button == BTN_RIGHT {
                        self.clique_direito = true;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if button == BTN_LEFT {
                        self.pressionado = false;
                        self.soltou = true;
                    }
                }
                PointerEventKind::Axis { vertical, .. } => {
                    self.rolagem += vertical.absolute as f32;
                }
            }
            self.sujo = true;
        }
    }
}

impl ShmHandler for Janela {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(Janela);
delegate_output!(Janela);
delegate_shm!(Janela);
delegate_seat!(Janela);
delegate_keyboard!(Janela);
delegate_pointer!(Janela);
delegate_xdg_shell!(Janela);
delegate_xdg_window!(Janela);
delegate_registry!(Janela);

impl ProvidesRegistryState for Janela {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
