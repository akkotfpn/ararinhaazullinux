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
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use smithay_client_toolkit::{
    data_device_manager::{
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{CopyPasteSource, DataSourceHandler},
        DataDeviceManagerState, WritePipe,
    },
    delegate_data_device,
    reexports::client::protocol::{wl_data_device, wl_data_device_manager::DndAction, wl_data_source},
};

const TIPOS_TEXTO: [&str; 4] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "STRING"];

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
    /// Texto que chegou da área de transferência (depois de `pedir_colar`).
    Colado(String),
    Outra(Keysym),
}

/// Pedidos dos apps à área de transferência, atendidos depois do quadro.
enum PedidoArea {
    Copiar(String),
    Colar,
}

thread_local! {
    static PEDIDOS_AREA: std::cell::RefCell<Vec<PedidoArea>> = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    static CTRL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Ctrl apertado agora (os componentes usam para Ctrl+setas).
pub fn mods_ctrl() -> bool {
    CTRL.with(|c| c.get())
}

/// Copia o texto para a área de transferência (Ctrl+C).
pub fn copiar(texto: &str) {
    PEDIDOS_AREA.with(|p| p.borrow_mut().push(PedidoArea::Copiar(texto.to_string())));
}

/// Pede o texto da área de transferência (Ctrl+V); ele chega num dos
/// próximos quadros como `Tecla::Colado`.
pub fn pedir_colar() {
    PEDIDOS_AREA.with(|p| p.borrow_mut().push(PedidoArea::Colar));
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
        self.tela.largura_logica()
    }
    pub fn altura(&self) -> f32 {
        self.tela.altura_logica()
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
/// Grava `chave = valor` na configuração da conta (~/.config/plumagem/
/// plumagem.conf), mantendo o resto. A Plumagem vê a mudança sozinha.
pub fn gravar_chave_config(chave: &str, valor: &str) -> std::io::Result<()> {
    let p = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into())).join(".config/plumagem/plumagem.conf");
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let atual = std::fs::read_to_string(&p).unwrap_or_default();
    let mut achou = false;
    let mut linhas: Vec<String> = atual
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if k.trim() == chave && !l.trim_start().starts_with('#') => {
                achou = true;
                format!("{chave} = {valor}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !achou {
        linhas.push(format!("{chave} = {valor}"));
    }
    std::fs::write(&p, linhas.join("\n") + "\n")
}

/// Usa uma imagem como papel de parede: copia em PNG (no máximo 2560 px de
/// largura) para ~/.local/share/ararinha e grava a chave "papel".
pub fn definir_papel_de_parede(origem: &std::path::Path) -> Result<(), String> {
    let img = image::ImageReader::open(origem)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| format!("Não deu para abrir a imagem: {e}"))?
        .decode()
        .map_err(|_| "Não deu para ler essa imagem (use PNG, JPEG ou WebP).".to_string())?;
    let img = if img.width() > 2560 { img.resize(2560, 2560 * img.height() / img.width().max(1), image::imageops::FilterType::Lanczos3) } else { img };
    let pasta = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/ararinha");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    if let Ok(rd) = std::fs::read_dir(&pasta) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("papel-de-parede-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let destino = pasta.join(format!("papel-de-parede-{agora}.png"));
    img.to_rgba8().save_with_format(&destino, image::ImageFormat::Png).map_err(|e| format!("Não deu para salvar: {e}"))?;
    gravar_chave_config("papel", &destino.to_string_lossy()).map_err(|e| e.to_string())
}

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

/// Escala da tela (Configurações › Tela): "125" vira 1.25.
fn ler_escala() -> f32 {
    ler_chave_config("escala")
        .and_then(|v| v.trim_end_matches('%').parse::<f32>().ok())
        .filter(|p| (100.0..=300.0).contains(p))
        .map(|p| p / 100.0)
        .unwrap_or(1.0)
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

/// Pasta dos relatórios de falhas da conta.
pub fn pasta_falhas() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".local/state/ararinha/falhas")
}

/// Se o app quebrar (panic), grava um relatório local (nada sai do
/// computador) e avisa com uma notificação. "relatorios = nao" desliga.
fn instalar_relatorio(app_id: String, titulo: String) {
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        anterior(info);
        if ler_chave_config("relatorios").as_deref() == Some("nao") {
            return;
        }
        let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let pasta = pasta_falhas();
        let _ = std::fs::create_dir_all(&pasta);
        let pilha = std::backtrace::Backtrace::force_capture();
        let texto = format!(
            "App: {titulo} ({app_id})\nVersão: {}\nQuando: {agora} (segundos desde 1970)\nErro: {info}\n\nPilha:\n{pilha}\n",
            env!("CARGO_PKG_VERSION")
        );
        let arq = pasta.join(format!("{app_id}-{agora}.txt"));
        if std::fs::write(&arq, texto).is_ok() {
            let _ = std::process::Command::new("ararinha-notificar")
                .args([format!("{titulo} fechou por um erro"), "O relatório ficou em Configurações › Privacidade › Relatórios de falhas.".into()])
                .status();
        }
    }));
}

/// Abre a janela e roda o app até ela ser fechada.
pub fn executar(opcoes: Opcoes, app: Box<dyn App>) {
    instalar_relatorio(opcoes.app_id.clone(), opcoes.titulo.clone());
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

    // wp_viewporter: com escala 125%, a janela manda um buffer 1,25× maior
    // e diz ao compositor o tamanho lógico; o texto sai nítido, sem esticar.
    let viewporter: Option<wp_viewporter::WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
    let area = DataDeviceManagerState::bind(&globals, &qh).ok();
    // O texto colado chega por uma thread (lendo o pipe) e entra no laço.
    let (colar_tx, colar_rx) = smithay_client_toolkit::reexports::calloop::channel::channel::<String>();
    let surface = compositor.create_surface(&qh);
    let viewport = viewporter.as_ref().map(|v| v.get_viewport(&surface, &qh, ()));
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
        z: if viewport.is_some() { ler_escala() } else { 1.0 },
        viewport,
        area,
        dispositivo: None,
        fonte: None,
        copiado: String::new(),
        serial: 0,
        colar_tx,
    };
    let _ = event_loop.handle().insert_source(colar_rx, |ev, _, janela: &mut Janela| {
        if let smithay_client_toolkit::reexports::calloop::channel::Event::Msg(t) = ev {
            janela.teclas.push(Tecla::Colado(t));
            janela.sujo = true;
        }
    });

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
                if janela.viewport.is_some() {
                    janela.z = ler_escala();
                }
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
    /// Escala da tela (1.0 = 100%).
    z: f32,
    viewport: Option<wp_viewport::WpViewport>,
    /// Área de transferência (Ctrl+C / Ctrl+V).
    area: Option<DataDeviceManagerState>,
    dispositivo: Option<DataDevice>,
    fonte: Option<CopyPasteSource>,
    copiado: String,
    /// Serial do último clique/tecla (o compositor exige para copiar).
    serial: u32,
    colar_tx: smithay_client_toolkit::reexports::calloop::channel::Sender<String>,
}

impl Janela {
    fn desenhar(&mut self, qh: &QueueHandle<Self>) {
        // Tamanho lógico (o da janela) e em pixels de verdade (com a escala).
        let (ll, la) = (self.largura.max(1), self.altura.max(1));
        let z = self.z;
        let (l, a) = (((ll as f32) * z).round().max(1.0) as u32, ((la as f32) * z).round().max(1.0) as u32);
        if self.tela.largura != l as i32 || self.tela.altura != a as i32 || (self.tela.z - z).abs() > 0.001 {
            self.tela = Tela::nova_com_escala(l as i32, a as i32, z);
        }
        CTRL.with(|c| c.set(self.mods.ctrl));
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
        for p in PEDIDOS_AREA.with(|p| std::mem::take(&mut *p.borrow_mut())) {
            match p {
                PedidoArea::Copiar(t) => self.copiar(qh, t),
                PedidoArea::Colar => self.colar(),
            }
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
        if let Some(v) = &self.viewport {
            if (z - 1.0).abs() > 0.001 {
                v.set_destination(ll as i32, la as i32);
            } else {
                v.set_destination(-1, -1);
            }
        }
        surface.damage_buffer(0, 0, l as i32, a as i32);
        if buffer.attach_to(surface).is_ok() {
            self.window.commit();
        }
        self.buffer = Some(buffer);
        let _ = qh;
    }
}

impl Janela {
    fn copiar(&mut self, qh: &QueueHandle<Self>, texto: String) {
        let (Some(area), Some(disp)) = (&self.area, &self.dispositivo) else { return };
        let fonte = area.create_copy_paste_source(qh, TIPOS_TEXTO);
        fonte.set_selection(disp, self.serial);
        self.copiado = texto;
        self.fonte = Some(fonte);
    }

    fn colar(&mut self) {
        // A própria janela é a dona: não precisa do pipe.
        if self.fonte.is_some() {
            let _ = self.colar_tx.send(self.copiado.clone());
            return;
        }
        let Some(disp) = &self.dispositivo else { return };
        let Some(oferta) = disp.data().selection_offer() else { return };
        let tipo = oferta.with_mime_types(|m| TIPOS_TEXTO.iter().find(|t| m.iter().any(|x| x == **t)).map(|t| t.to_string()));
        let Some(tipo) = tipo else { return };
        let Ok(mut pipe) = oferta.receive(tipo) else { return };
        let tx = self.colar_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut b = Vec::new();
            let _ = (&mut pipe).take(4 * 1024 * 1024).read_to_end(&mut b);
            if let Ok(t) = String::from_utf8(b) {
                if !t.is_empty() {
                    let _ = tx.send(t.replace("\r\n", "\n"));
                }
            }
        });
    }
}

impl DataDeviceHandler for Janela {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice, _: f64, _: f64, _: &wl_surface::WlSurface) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {}
}

impl DataOfferHandler for Janela {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for Janela {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: Option<String>) {}
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: String, mut fd: WritePipe) {
        let t = self.copiado.clone();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = fd.write_all(t.as_bytes());
        });
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {
        // Outro app copiou algo: a área não é mais nossa.
        self.fonte = None;
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: DndAction) {}
}

delegate_data_device!(Janela);

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
            if self.dispositivo.is_none() {
                self.dispositivo = self.area.as_ref().map(|a| a.get_data_device(qh, &seat));
            }
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
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, ev: KeyEvent) {
        self.serial = serial;
        let alt = self.mods.alt;
        let tecla = match ev.keysym {
            Keysym::Return | Keysym::KP_Enter => Tecla::Enter,
            Keysym::BackSpace if self.mods.ctrl => Tecla::Outra(Keysym::BackSpace),
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
                PointerEventKind::Press { button, serial, .. } => {
                    self.serial = serial;
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

// O viewporter não manda eventos.
impl smithay_client_toolkit::reexports::client::Dispatch<wp_viewporter::WpViewporter, ()> for Janela {
    fn event(
        _: &mut Self,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl smithay_client_toolkit::reexports::client::Dispatch<wp_viewport::WpViewport, ()> for Janela {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl ProvidesRegistryState for Janela {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
