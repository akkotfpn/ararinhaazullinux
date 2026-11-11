// SPDX-License-Identifier: GPL-3.0-or-later
//! Barra de tarefas e menu iniciar da Plumagem.
//!
//! A barra fica embaixo, com o botão do Mingau (menu iniciar), um botão por
//! janela aberta e o relógio. O menu lista os aplicativos instalados (arquivos
//! .desktop) e tem Sair, Reiniciar e Desligar.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use smithay::{
    backend::renderer::{
        element::{
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            Kind,
        },
        ImportMem, Renderer,
    },
    desktop::Space,
    output::Output,
    utils::{IsAlive, Logical, Point, Rectangle, Size},
    wayland::{compositor, shell::xdg::XdgToplevelSurfaceData},
};

use crate::{
    desenho::{self, tema, Tela},
    shell::WindowElement,
};

/// Tamanhos da barra de tarefas (em pixels lógicos), escolhidos em
/// /etc/plumagem/plumagem.conf com `barra = pequena | media | grande`.
const ALTURAS: [(&str, i32); 3] = [("pequena", 38), ("media", 44), ("grande", 50)];
const ALTURA_PADRAO: i32 = 44;

static ALTURA_ATUAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn caminhos_config() -> [String; 2] {
    [
        std::env::var("HOME").map(|h| format!("{h}/.config/plumagem/plumagem.conf")).unwrap_or_default(),
        "/etc/plumagem/plumagem.conf".to_string(),
    ]
}

/// Valor de `chave = valor` na configuração do usuário (ou na do sistema).
pub fn ler_chave(chave: &str) -> Option<String> {
    for caminho in caminhos_config() {
        let Ok(texto) = std::fs::read_to_string(&caminho) else { continue };
        for linha in texto.lines() {
            let Some((k, v)) = linha.split_once('=') else { continue };
            if k.trim() == chave && !linha.trim_start().starts_with('#') {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Grava `chave = valor` na configuração do usuário, mantendo o resto.
pub fn gravar_chave(chave: &str, valor: &str) {
    let Ok(casa) = std::env::var("HOME") else { return };
    let p = std::path::PathBuf::from(casa).join(".config/plumagem/plumagem.conf");
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
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
    let _ = std::fs::write(&p, linhas.join("\n") + "\n");
}

fn ler_altura() -> i32 {
    for caminho in caminhos_config() {
        let Ok(texto) = std::fs::read_to_string(&caminho) else { continue };
        for linha in texto.lines() {
            let Some((k, v)) = linha.split_once('=') else { continue };
            if k.trim() != "barra" {
                continue;
            }
            let v = v.trim().trim_matches('"').to_lowercase().replace('é', "e");
            if let Some((_, a)) = ALTURAS.iter().find(|(n, _)| *n == v) {
                return *a;
            }
            if let Ok(a) = v.parse::<i32>() {
                return a.clamp(32, 72);
            }
        }
    }
    ALTURA_PADRAO
}

/// Altura da barra de tarefas (relida quando a configuração muda).
pub fn altura() -> i32 {
    let a = ALTURA_ATUAL.load(std::sync::atomic::Ordering::Relaxed);
    if a > 0 {
        return a;
    }
    let a = ler_altura();
    ALTURA_ATUAL.store(a, std::sync::atomic::Ordering::Relaxed);
    a
}

/// Data de modificação das configurações (para perceber mudanças).
fn versao_config() -> Vec<Option<std::time::SystemTime>> {
    caminhos_config()
        .iter()
        .map(|c| std::fs::metadata(c).and_then(|m| m.modified()).ok())
        .collect()
}

const MENU_L: i32 = 400;
const MENU_A: i32 = 540;
const MENU_MARGEM: i32 = 8;
/// Campo de busca no topo do menu Iniciar.
const BUSCA_Y: i32 = 16;
const BUSCA_A: i32 = 40;
const APPS_Y: i32 = 98;
const APP_A: i32 = 42;
const RODAPE_A: i32 = 64;
/// Botões de energia no rodapé do menu (quadrados).
const ENERGIA_L: i32 = 40;
/// Opções do botão de energia, numa janelinha acima dele.
fn opcoes_energia() -> &'static [(Alvo, &'static str)] {
    const SEM: [(Alvo, &str); 4] = [(Alvo::Dormir, "Dormir"), (Alvo::Sair, "Sair da sessão"), (Alvo::Reiniciar, "Reiniciar"), (Alvo::Desligar, "Desligar")];
    const COM: [(Alvo, &str); 5] =
        [(Alvo::Dormir, "Dormir"), (Alvo::Hibernar, "Hibernar"), (Alvo::Sair, "Sair da sessão"), (Alvo::Reiniciar, "Reiniciar"), (Alvo::Desligar, "Desligar")];
    if pode_hibernar() {
        &COM
    } else {
        &SEM
    }
}

/// Hibernar precisa de uma área de troca (swap) do tamanho da memória.
fn pode_hibernar() -> bool {
    static PODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PODE.get_or_init(|| {
        let kb = |arq: &str, chave: &str| -> u64 {
            std::fs::read_to_string(arq)
                .unwrap_or_default()
                .lines()
                .find(|l| l.starts_with(chave))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };
        let swap = kb("/proc/meminfo", "SwapTotal:");
        let disco = std::fs::read_to_string("/sys/power/state").unwrap_or_default().contains("disk");
        disco && swap > 0 && swap >= kb("/proc/meminfo", "MemTotal:")
    })
}
const POP_L: i32 = 190;
const POP_ITEM: i32 = 38;
fn pop_a() -> i32 {
    opcoes_energia().len() as i32 * POP_ITEM + 12
}
const INICIAR_X: i32 = 6;
const TAREFA_MAX: i32 = 220;
const RELOGIO_L: i32 = 120;
/// Central de notificações (clique no relógio).
const CENTRAL_L: i32 = 380;
/// Histórico da área de transferência (Win+V).
const AREA_L: i32 = 380;
const AREA_A: i32 = 460;
const AREA_ITEM: i32 = 58;
const CENTRAL_A: i32 = 520;
const CENTRAL_ITEM: i32 = 64;
/// Ações rápidas no pé da central: (rótulo, ação).
const ACOES_RAPIDAS: [&str; 4] = ["Não perturbe", "Modo escuro", "Recortar tela", "Configurações"];

static LOGO_PNG: &[u8] = include_bytes!("../resources/logo-64.png");
static ESTE_PC_PNG: &[u8] = include_bytes!("../resources/este-computador-48.png");
static LIXEIRA_PNG: &[u8] = include_bytes!("../resources/lixeira-48.png");

/// Ícone da área de trabalho: nome, comando ao abrir, imagem e o arquivo
/// de atalho (para os da pasta Área de Trabalho).
pub struct IconeArea {
    pub nome: String,
    pub cmd: String,
    img: Option<image::RgbaImage>,
    pub arquivo: Option<std::path::PathBuf>,
}

fn icones_fixos() -> Vec<IconeArea> {
    let imgs = [
        desenho::icone(ESTE_PC_PNG, 48, 48),
        desenho::icone(include_bytes!("../resources/logo-64.png"), 48, 48),
        desenho::icone(LIXEIRA_PNG, 48, 48),
    ];
    let mut v: Vec<IconeArea> = ICONES_AREA
        .iter()
        .zip(imgs)
        .map(|((n, c), img)| IconeArea { nome: n.to_string(), cmd: c.to_string(), img, arquivo: None })
        .collect();
    // Rodando do pendrive (ainda não instalado): atalho do instalador.
    if !std::path::Path::new("/etc/ararinha/instalado").exists() && std::path::Path::new("/usr/bin/instalador").exists() {
        let img = std::fs::read("/usr/share/icons/hicolor/128x128/apps/instalador.png")
            .or_else(|_| std::fs::read("/usr/share/icons/hicolor/64x64/apps/instalador.png"))
            .ok()
            .and_then(|b| desenho::icone(&b, 48, 48));
        v.push(IconeArea { nome: "Instalar o Ararinha".into(), cmd: "instalador".into(), img, arquivo: None });
    }
    v
}

/// Pasta "Área de Trabalho" do usuário.
pub fn pasta_area() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join("Área de Trabalho"))
}

/// Aspas simples para o sh.
fn aspas(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Ícones da pasta Área de Trabalho: atalhos (.desktop) e pastas.
fn icones_da_pasta(pasta: &std::path::Path) -> Vec<IconeArea> {
    let Ok(rd) = std::fs::read_dir(pasta) else { return Vec::new() };
    let mut v: Vec<(String, IconeArea)> = Vec::new();
    for e in rd.flatten() {
        let caminho = e.path();
        let nome_arq = e.file_name().to_string_lossy().to_string();
        if nome_arq.starts_with('.') {
            continue;
        }
        if caminho.extension().and_then(|x| x.to_str()) == Some("desktop") {
            let id = caminho.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if let Some(app) = std::fs::read_to_string(&caminho).ok().and_then(|t| ler_desktop(&t, &id)) {
                let img = app.icone.map(|i| image::imageops::resize(&i, 48, 48, image::imageops::FilterType::Triangle));
                v.push((app.nome.to_lowercase(), IconeArea { nome: app.nome, cmd: app.exec, img, arquivo: Some(caminho) }));
            }
        } else if caminho.is_dir() {
            let img = achar_icone("explorador")
                .map(|i| image::imageops::resize(&i, 48, 48, image::imageops::FilterType::Triangle));
            let cmd = format!("explorador {}", aspas(&caminho.to_string_lossy()));
            v.push((nome_arq.to_lowercase(), IconeArea { nome: nome_arq, cmd, img, arquivo: Some(caminho) }));
        }
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.into_iter().map(|(_, i)| i).collect()
}

/// Ícones da área de trabalho: nome e comando ao abrir (clique duplo).
const ICONES_AREA: [(&str, &str); 3] = [
    ("Este Computador", "explorador"),
    ("Pasta pessoal", "sh -c 'exec explorador \"$HOME\"'"),
    (
        "Lixeira",
        "sh -c 'mkdir -p \"$HOME/.local/share/Trash/files\" && exec explorador \"$HOME/.local/share/Trash/files\"'",
    ),
];
/// Apps fixados na barra de tarefas: id do .desktop.
pub const FIXADOS: [&str; 1] = ["explorador"];
const FIXADO_L: i32 = 40;
const ICONE_L: i32 = 92;
const ICONE_A: i32 = 98;
const AREA_X: i32 = 10;
const AREA_Y: i32 = 12;

/// O que está sob o ponteiro na interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alvo {
    Iniciar,
    Tarefa(usize),
    Barra,
    App(usize),
    Sair,
    Reiniciar,
    Desligar,
    Bloquear,
    /// Botão de energia do menu (abre Dormir, Sair, Reiniciar, Desligar).
    Energia,
    Dormir,
    Hibernar,
    /// Avatar e nome do usuário no rodapé do menu (abre a Conta).
    Usuario,
    Menu,
    /// Ícone de som na bandeja.
    Som,
    /// Barra de volume no painel de som (posição em milésimos).
    SomBarra(u16),
    SomMudo,
    SomPainel,
    /// App fixado na barra (atalho ao lado do botão iniciar).
    Fixado(usize),
    /// Ícone de rede na bandeja e o painel dele.
    Rede,
    RedePainel,
    /// "Configurações de rede e Wi-Fi" no painel de rede.
    RedeConfig,
    /// Uma notificação (o clique fecha).
    Aviso(usize),
    /// Item do menu do botão direito (área de trabalho ou barra).
    Contexto(usize),
    /// O cantinho depois do relógio: mostra a área de trabalho.
    CantoArea,
    /// Ícone de app na bandeja.
    ItemBandeja(usize),
    /// Relógio: abre a central de notificações.
    Relogio,
    CentralPainel,
    CentralItem(usize),
    CentralLimpar,
    CentralAcao(usize),
    /// Histórico da área de transferência (Win+V).
    AreaPainel,
    AreaItem(usize),
    AreaLimpar,
}

/// Largura do cantinho "Mostrar área de trabalho", como no Windows.
const CANTO_L: i32 = 10;

/// Menu do botão direito: onde abriu e os itens (rótulo, ação). A ação é um
/// comando ou, começando com '#', algo da própria Plumagem.
#[derive(Debug, Clone)]
pub struct MenuContexto {
    pub origem: Point<i32, Logical>,
    pub itens: Vec<(String, String)>,
}

const CTX_L: i32 = 250;
const CTX_ITEM: i32 = 32;

/// Notificação mostrada no canto da tela por alguns segundos.
#[derive(Debug, Clone)]
pub struct Aviso {
    pub titulo: String,
    pub texto: String,
    /// Comando para o clique (por exemplo, abrir a captura).
    pub acao: Option<String>,
    ate: std::time::Instant,
}

const AVISO_L: i32 = 360;
const AVISO_A: i32 = 78;
const AVISO_ESPACO: i32 = 8;

/// Pasta onde programas deixam notificações (ver `ararinha-notificar`).
fn pasta_avisos() -> Option<std::path::PathBuf> {
    std::env::var("XDG_RUNTIME_DIR").ok().map(|d| std::path::PathBuf::from(d).join("ararinha-avisos"))
}

/// Toca um som do sistema sem esperar (o ararinha-som respeita "sons = nao").
pub fn tocar_som(nome: &str) {
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("ararinha-som /usr/share/sounds/ararinha/{nome}.wav >/dev/null 2>&1 &"))
        .status();
}

/// Estado da rede (lido de /sys e /proc, a cada 5 s).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rede {
    pub conectado: bool,
    pub interface: String,
    pub ip: String,
    pub sem_fio: bool,
    lido_em: u64,
}

/// Estado do som (lido com o wpctl, do WirePlumber).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Som {
    /// Volume de 0.0 a 1.5; `None` sem saída de som.
    pub volume: Option<f32>,
    pub mudo: bool,
    lido_em: u64,
}

const ICONE_BANDEJA: i32 = 38;
const BANDEJA_L: i32 = 2 * ICONE_BANDEJA;
/// Ícones dos apps na bandeja (Steam, Discord...), à esquerda de rede e som.
const ITEM_BANDEJA: i32 = 30;

/// Um ícone de app na bandeja (vem do ararinha-bandeja).
#[derive(Debug, Clone)]
pub struct ItemBandeja {
    pub n: usize,
    pub titulo: String,
    pub icone: Option<image::RgbaImage>,
}

fn pasta_bandeja() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or_default()).join("ararinha-bandeja")
}

/// Manda um pedido ao ararinha-bandeja (sem travar se ele não estiver lá).
pub fn pedir_bandeja(pedido: &str) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).custom_flags(0o4000).open(pasta_bandeja().join("ctl")) {
        let _ = f.write_all(format!("{pedido}\n").as_bytes());
    }
}

/// Ícone da bandeja: PNG pelo nome ou os pixels ARGB que o app mandou.
fn ler_icone_bandeja(caminho: &str, lado: u32) -> Option<image::RgbaImage> {
    let bytes = std::fs::read(caminho).ok()?;
    let img = if caminho.ends_with(".argb") {
        let l = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
        let a = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
        let px = bytes.get(8..8 + (l * a * 4) as usize)?;
        let mut rgba = Vec::with_capacity(px.len());
        for c in px.chunks_exact(4) {
            // ARGB em ordem de rede.
            rgba.extend_from_slice(&[c[1], c[2], c[3], c[0]]);
        }
        image::RgbaImage::from_raw(l, a, rgba)?
    } else {
        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8()
    };
    Some(image::imageops::resize(&img, lado, lado, image::imageops::FilterType::Triangle))
}
const SOM_L: i32 = 280;
const SOM_A: i32 = 104;
const REDE_A: i32 = 150;
const SOM_BARRA_X0: f32 = 64.0;
const SOM_BARRA_X1: f32 = 220.0;

#[derive(Debug, Clone)]
pub struct App {
    pub nome: String,
    pub exec: String,
    /// Ícone (PNG) do aplicativo, se encontrado.
    pub icone: Option<image::RgbaImage>,
    /// Nomes pelos quais as janelas dele se identificam (app_id), em minúsculas.
    pub ids: Vec<String>,
}

pub struct Painel {
    pub menu_aberto: bool,
    /// Um clique começou na interface: a soltura também é dela.
    pub segurando: bool,
    /// Janelas na ordem em que apareceram (é a ordem dos botões).
    pub ordem: Vec<WindowElement>,
    /// Janelas minimizadas e onde estavam.
    pub minimizadas: Vec<(WindowElement, Point<i32, Logical>)>,
    /// Retângulo de seleção na área de trabalho: onde o arrasto começou.
    pub selecao: Option<Point<f64, Logical>>,
    sel: [SolidColorBuffer; 9],
    /// Captura com recorte (Win+Shift+S): None = desligado; Some(None) =
    /// esperando o arraste; Some(Some(p)) = arrastando desde `p`.
    pub recorte: Option<Option<Point<f64, Logical>>>,
    /// Bordas da janela sendo redimensionada (para a forma do cursor).
    pub cursor_redim: Option<crate::shell::ResizeEdge>,
    pub som: Som,
    pub som_aberto: bool,
    painel_som: Option<(String, MemoryRenderBuffer)>,
    pub rede: Rede,
    pub rede_aberta: bool,
    /// Há uma placa Wi-Fi (mesmo desconectada).
    wifi_disponivel: bool,
    painel_rede: Option<(String, MemoryRenderBuffer)>,
    pub avisos: Vec<Aviso>,
    pub menu_contexto: Option<MenuContexto>,
    ctx_buf: Option<(String, MemoryRenderBuffer)>,
    avisos_buf: Option<(String, MemoryRenderBuffer)>,
    avisos_visto: std::time::Instant,
    /// Ícone selecionado na área de trabalho e o último clique (clique duplo).
    pub icone_sel: Option<usize>,
    ultimo_clique_icone: Option<(usize, std::time::Instant)>,
    area_buf: Option<(String, MemoryRenderBuffer)>,
    /// Ícones da área de trabalho: os três fixos e os da pasta
    /// "Área de Trabalho" (atalhos .desktop e pastas).
    icones_area: Vec<IconeArea>,
    area_visto: (std::time::Instant, Option<std::time::SystemTime>),
    /// Quantos ícones cabem numa coluna (calculado ao desenhar).
    area_linhas: usize,
    pub apps: Option<Vec<App>>,
    /// Texto digitado no menu Iniciar (filtra os apps).
    pub busca: String,
    /// Primeira linha visível da lista de apps.
    pub menu_rolagem: usize,
    /// App escolhido pelo teclado (índice na lista filtrada).
    pub menu_sel: usize,
    /// Foto da conta (~/.face), relida ao abrir o menu.
    foto: Option<image::RgbaImage>,
    /// Resultados da busca além dos apps (conta, configurações, arquivos).
    extra: Vec<App>,
    /// As opções do botão de energia estão abertas.
    pub energia_aberta: bool,
    /// Central de notificações: aberta, histórico e "Não perturbe".
    pub central_aberta: bool,
    pub historico: Vec<(String, String, String, Option<String>)>,
    pub nao_perturbe: bool,
    central_buf: Option<(String, MemoryRenderBuffer)>,
    /// Win+V aberto.
    pub area_aberta: bool,
    transf_buf: Option<(String, MemoryRenderBuffer)>,
    /// Indicador na tela (teclas de volume/brilho): rótulo, nível 0..1 e até quando.
    pub indicador: Option<(&'static str, f32, std::time::Instant)>,
    indicador_buf: Option<(String, MemoryRenderBuffer)>,
    /// Alt+Tab: janelas (título e índice do app) e a escolhida.
    pub alternador: Option<(Vec<(String, Option<usize>)>, usize)>,
    alternador_buf: Option<(String, MemoryRenderBuffer)>,
    /// Ícones dos apps na bandeja e quantas vezes a lista mudou.
    pub bandeja: Vec<ItemBandeja>,
    bandeja_versao: u32,
    /// Onde foi o clique direito que pediu o menu de um ícone da bandeja.
    pub bandeja_clique: Point<i32, Logical>,
    relogio: (u64, String, String),
    barra: Option<(String, MemoryRenderBuffer)>,
    menu: Option<(String, MemoryRenderBuffer)>,
    logo: Option<image::RgbaImage>,
    /// Última checagem da configuração: (segundo, datas dos arquivos).
    config_visto: (u64, Vec<Option<std::time::SystemTime>>),
}

impl std::fmt::Debug for Painel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Painel")
            .field("menu_aberto", &self.menu_aberto)
            .field("janelas", &self.ordem.len())
            .finish()
    }
}

impl Default for Painel {
    fn default() -> Self {
        desenho::tema::definir_escuro(ler_chave("tema").as_deref() == Some("escuro"));
        desenho::tema::definir_destaque(ler_chave("destaque").as_deref().unwrap_or("azul"));
        Painel {
            menu_aberto: false,
            segurando: false,
            ordem: Vec::new(),
            minimizadas: Vec::new(),
            selecao: None,
            sel: Default::default(),
            recorte: None,
            cursor_redim: None,
            som: Som::default(),
            som_aberto: false,
            painel_som: None,
            rede: Rede::default(),
            rede_aberta: false,
            wifi_disponivel: false,
            painel_rede: None,
            avisos: Vec::new(),
            menu_contexto: None,
            ctx_buf: None,
            avisos_buf: None,
            avisos_visto: std::time::Instant::now(),
            icone_sel: None,
            ultimo_clique_icone: None,
            area_buf: None,
            icones_area: icones_fixos(),
            area_visto: (std::time::Instant::now() - std::time::Duration::from_secs(10), None),
            area_linhas: 6,
            apps: None,
            busca: String::new(),
            menu_rolagem: 0,
            menu_sel: 0,
            foto: None,
            extra: Vec::new(),
            energia_aberta: false,
            central_aberta: false,
            historico: Vec::new(),
            nao_perturbe: ler_chave("nao_perturbe").as_deref() == Some("sim"),
            central_buf: None,
            area_aberta: false,
            transf_buf: None,
            alternador: None,
            indicador: None,
            indicador_buf: None,
            alternador_buf: None,
            bandeja: Vec::new(),
            bandeja_versao: 0,
            bandeja_clique: (0, 0).into(),
            relogio: (0, String::new(), String::new()),
            barra: None,
            menu: None,
            logo: desenho::icone(LOGO_PNG, (altura() - 16).max(16) as u32, (altura() - 16).max(16) as u32),
            config_visto: (0, versao_config()),
        }
    }
}

/// Título da janela (ou o app_id, se não tiver título).
pub fn titulo(w: &WindowElement) -> String {
    w.0.toplevel()
        .and_then(|t| {
            compositor::with_states(t.wl_surface(), |s| {
                s.data_map.get::<XdgToplevelSurfaceData>().and_then(|d| {
                    let d = d.lock().unwrap();
                    d.title.clone().or_else(|| d.app_id.clone())
                })
            })
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Janela".into())
}

fn letra(texto: &str) -> String {
    texto
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "•".into())
}

impl Painel {
    pub fn esta_minimizada(&self, w: &WindowElement) -> bool {
        self.minimizadas.iter().any(|(m, _)| m == w)
    }

    /// Acompanha as janelas abertas: novas entram no fim, fechadas saem.
    pub fn atualizar(&mut self, space: &Space<WindowElement>) {
        self.minimizadas.retain(|(w, _)| w.0.alive());
        let minimizadas: Vec<WindowElement> = self.minimizadas.iter().map(|(w, _)| w.clone()).collect();
        self.ordem
            .retain(|w| w.0.alive() && (minimizadas.contains(w) || space.elements().any(|e| e == w)));
        for w in space.elements() {
            if !self.ordem.contains(w) {
                self.ordem.push(w.clone());
            }
        }
    }

    fn iniciar_l() -> i32 {
        altura() - 8
    }

    fn fixados_x() -> i32 {
        INICIAR_X + Self::iniciar_l() + 6
    }

    fn tarefas_x() -> i32 {
        Self::fixados_x() + FIXADOS.len() as i32 * FIXADO_L + 10
    }

    /// App (em `apps`) de um fixado, pelo id do .desktop.
    pub fn app_fixado(&self, i: usize) -> Option<&App> {
        let id = FIXADOS.get(i)?;
        self.apps.as_ref()?.iter().find(|a| a.ids.first().map(|x| x == id).unwrap_or(false))
    }

    fn largura_tarefa(&self, largura_tela: i32) -> i32 {
        let n = self.ordem.len().max(1) as i32;
        let livre = largura_tela - Self::tarefas_x() - RELOGIO_L - BANDEJA_L - self.bandeja_l() - 8;
        ((livre / n) - 4).clamp(48, TAREFA_MAX)
    }

    fn menu_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (MENU_MARGEM, tela.h - altura() - MENU_MARGEM - MENU_A).into()
    }

    fn max_apps() -> usize {
        ((MENU_A - RODAPE_A - APPS_Y - 4) / APP_A).max(0) as usize
    }

    /// Apps que combinam com a busca (sem diferenciar acentos e maiúsculas).
    pub fn apps_visiveis(&self) -> Vec<App> {
        let apps = self.apps.as_deref().unwrap_or(&[]);
        if self.busca.trim().is_empty() {
            return apps.to_vec();
        }
        let b = sem_acento(&self.busca.trim().to_lowercase());
        let mut achados: Vec<(u8, &App)> = apps
            .iter()
            .filter_map(|a| {
                let nome = sem_acento(&a.nome.to_lowercase());
                if nome.starts_with(&b) {
                    Some((0, a))
                } else if nome.split_whitespace().any(|p| p.starts_with(&b)) {
                    Some((1, a))
                } else if nome.contains(&b) || a.exec.to_lowercase().contains(&b) {
                    Some((2, a))
                } else {
                    None
                }
            })
            .collect();
        achados.sort_by_key(|(r, a)| (*r, a.nome.to_lowercase()));
        let mut v: Vec<App> = achados.into_iter().map(|(_, a)| a.clone()).collect();
        // Depois dos apps: conta, configurações e arquivos.
        v.extend(self.extra.iter().cloned());
        v
    }

    /// Resultados além dos apps, refeitos quando a busca muda: conta,
    /// páginas das Configurações e arquivos da pasta pessoal.
    fn atualizar_extra(&mut self) {
        self.extra.clear();
        let b = sem_acento(&self.busca.trim().to_lowercase());
        if b.is_empty() {
            return;
        }
        let icone_de = |trecho: &str| {
            self.apps
                .as_ref()
                .and_then(|v| v.iter().find(|a| a.exec.starts_with(trecho)))
                .and_then(|a| a.icone.clone())
        };
        // Conta: "2+2*3", "100/7".
        if let Some(r) = calcular(&self.busca) {
            let r = if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r:.6}").trim_end_matches('0').trim_end_matches('.').to_string() };
            self.extra.push(App {
                nome: format!("{} = {}", self.busca.trim(), r.replace('.', ",")),
                exec: "calculadora".into(),
                icone: icone_de("calculadora"),
                ids: Vec::new(),
            });
        }
        const PAGINAS: [(&str, &str, &str); 13] = [
            ("Atualizações", "atualizacoes", "atualizacao atualizacoes atualizar update versao novidades"),
            ("Idioma e teclado", "idioma", "idioma teclado layout abnt abnt2 lingua ingles portugues"),
            ("Tela", "tela", "tela monitor resolucao escala tamanho zoom dpi"),
            ("Armazenamento", "armazenamento", "armazenamento disco espaco lixeira limpar hd ssd"),
            ("Rede e Wi-Fi", "rede", "wifi wi-fi rede internet ip cabo"),
            ("Firewall", "firewall", "firewall porta seguranca protecao"),
            ("Som", "som", "som volume audio microfone fone caixa"),
            ("Energia e bloqueio da tela", "energia", "energia bateria bloquear bloqueio suspender plano desempenho"),
            ("Data e hora", "hora", "data hora fuso relogio ntp"),
            ("Sua conta (senha e foto)", "conta", "senha conta foto perfil avatar"),
            ("Outros usuários", "contas", "usuario usuarios conta nova admin administrador familia"),
            ("Personalização", "personalizacao", "tema escuro claro papel parede fundo cor destaque barra visual"),
            ("Sobre o computador", "sobre", "sobre versao sistema computador memoria processador"),
        ];
        for (nome, pagina, chaves) in PAGINAS {
            if chaves.split_whitespace().any(|k| k.starts_with(&b) || b.starts_with(k)) || sem_acento(&nome.to_lowercase()).contains(&b) {
                self.extra.push(App {
                    nome: format!("Configurações › {nome}"),
                    exec: format!("configuracoes {pagina}"),
                    icone: icone_de("configuracoes"),
                    ids: Vec::new(),
                });
            }
        }
        // Arquivos: até 6, procurando até 4 níveis na pasta pessoal.
        if b.chars().count() >= 3 {
            if let Ok(casa) = std::env::var("HOME") {
                let mut pilha = vec![(std::path::PathBuf::from(&casa), 0)];
                let (mut vistos, mut achados) = (0, 0);
                while let Some((d, nivel)) = pilha.pop() {
                    let Ok(rd) = std::fs::read_dir(&d) else { continue };
                    for e in rd.flatten() {
                        vistos += 1;
                        let nome = e.file_name().to_string_lossy().to_string();
                        if nome.starts_with('.') {
                            continue;
                        }
                        let pasta = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        if sem_acento(&nome.to_lowercase()).contains(&b) && achados < 6 {
                            achados += 1;
                            let p = e.path();
                            self.extra.push(App {
                                nome: format!("{nome}  ·  {}", p.parent().map(|x| x.display().to_string().replace(&casa, "~")).unwrap_or_default()),
                                exec: format!("explorador {}", aspas(&p.display().to_string())),
                                icone: icone_de("explorador"),
                                ids: Vec::new(),
                            });
                        }
                        if pasta && nivel < 4 {
                            pilha.push((e.path(), nivel + 1));
                        }
                    }
                    if vistos > 4000 || achados >= 6 {
                        break;
                    }
                }
            }
        }
    }

    /// Abre o menu Iniciar limpo.
    pub fn abrir_menu(&mut self) {
        self.menu_aberto = true;
        self.busca.clear();
        self.menu_rolagem = 0;
        self.menu_sel = 0;
        self.extra.clear();
        self.energia_aberta = false;
        self.foto = std::env::var("HOME").ok().and_then(|h| desenho::foto_da_conta(&h, 32));
        self.menu = None;
        self.carregar_apps();
    }

    /// Relê os ícones da bandeja (o ararinha-bandeja avisa quando mudam).
    pub fn ler_bandeja(&mut self) {
        let texto = std::fs::read_to_string(pasta_bandeja().join("itens")).unwrap_or_default();
        let lado = 20;
        self.bandeja = texto
            .lines()
            .filter_map(|l| {
                let c: Vec<&str> = l.split('\t').collect();
                let n = c.first()?.parse().ok()?;
                let icone = c.get(2).filter(|p| !p.is_empty()).and_then(|p| ler_icone_bandeja(p, lado));
                Some(ItemBandeja { n, titulo: c.get(1).unwrap_or(&"").to_string(), icone })
            })
            .take(12)
            .collect();
        self.bandeja_versao = self.bandeja_versao.wrapping_add(1);
        self.barra = None;
    }

    /// O menu de um ícone da bandeja chegou: abre onde foi o clique.
    pub fn abrir_menu_bandeja(&mut self, n: usize, tela: Size<i32, Logical>) {
        let texto = std::fs::read_to_string(pasta_bandeja().join(format!("menu-{n}"))).unwrap_or_default();
        let mut itens: Vec<(String, String)> = Vec::new();
        for l in texto.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            let (id, rotulo, flags) = (c.first().copied().unwrap_or(""), c.get(1).copied().unwrap_or(""), c.get(2).copied().unwrap_or(""));
            if flags.contains('s') && rotulo.is_empty() {
                continue;
            }
            let acao = if flags.contains('d') { "#nada".to_string() } else { format!("#bandeja:{n}:{id}") };
            let rotulo = if flags.contains('m') { format!("✓ {rotulo}") } else { rotulo.to_string() };
            itens.push((rotulo, acao));
        }
        if itens.is_empty() {
            return;
        }
        let refs: Vec<(&str, &str)> = itens.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let p = self.bandeja_clique;
        self.abrir_contexto(p, tela, refs);
    }

    fn bandeja_l(&self) -> i32 {
        if self.bandeja.is_empty() {
            0
        } else {
            self.bandeja.len() as i32 * ITEM_BANDEJA + 6
        }
    }

    /// Rola a lista do menu (positivo = para baixo).
    pub fn rolar_menu(&mut self, linhas: i32) {
        let n = self.apps_visiveis().len();
        let max = n.saturating_sub(Self::max_apps());
        self.menu_rolagem = (self.menu_rolagem as i32 + linhas).clamp(0, max as i32) as usize;
    }

    /// Tecla com o menu aberto. Devolve o comando a rodar, se houver, e se
    /// o menu deve fechar.
    pub fn tecla_menu(&mut self, sym: smithay::input::keyboard::Keysym) -> (Option<String>, bool) {
        use smithay::input::keyboard::Keysym as K;
        let n = self.apps_visiveis().len();
        match sym {
            K::Escape | K::Super_L | K::Super_R => return (None, true),
            K::Return | K::KP_Enter => {
                let cmd = self.apps_visiveis().get(self.menu_sel).map(|a| a.exec.clone());
                return (cmd, true);
            }
            K::BackSpace => {
                self.busca.pop();
                self.menu_sel = 0;
                self.menu_rolagem = 0;
                self.atualizar_extra();
            }
            K::Down => {
                if n > 0 {
                    self.menu_sel = (self.menu_sel + 1).min(n - 1);
                }
            }
            K::Up => self.menu_sel = self.menu_sel.saturating_sub(1),
            s => {
                if let Some(c) = s.key_char() {
                    if !c.is_control() && self.busca.chars().count() < 40 {
                        self.busca.push(c);
                        self.menu_sel = 0;
                        self.menu_rolagem = 0;
                        self.atualizar_extra();
                    }
                }
            }
        }
        // A seleção sempre visível.
        let m = Self::max_apps();
        if self.menu_sel < self.menu_rolagem {
            self.menu_rolagem = self.menu_sel;
        } else if m > 0 && self.menu_sel >= self.menu_rolagem + m {
            self.menu_rolagem = self.menu_sel + 1 - m;
        }
        (None, false)
    }

    /// O mouse está sobre o menu Iniciar aberto?
    pub fn sobre_menu(&self, p: Point<f64, Logical>, tela: Size<i32, Logical>) -> bool {
        let o = Self::menu_origem(tela);
        self.menu_aberto
            && p.x >= o.x as f64
            && p.y >= o.y as f64
            && p.x < (o.x + MENU_L) as f64
            && p.y < (o.y + MENU_A) as f64
    }

    /// O que está em `p` (coordenadas da tela), se for a interface.
    pub fn alvo(&self, p: Point<f64, Logical>, tela: Size<i32, Logical>) -> Option<Alvo> {
        let (x, y) = (p.x as i32, p.y as i32);
        if let Some(m) = &self.menu_contexto {
            let a = m.itens.len() as i32 * CTX_ITEM + 12;
            if x >= m.origem.x && x < m.origem.x + CTX_L && y >= m.origem.y && y < m.origem.y + a {
                let i = (y - m.origem.y - 6).max(0) / CTX_ITEM;
                return Some(Alvo::Contexto(i as usize));
            }
        }
        if !self.avisos.is_empty() {
            let o = Self::avisos_origem(tela, self.avisos.len());
            if x >= o.x && x < o.x + AVISO_L && y >= o.y {
                let i = ((y - o.y) / (AVISO_A + AVISO_ESPACO)) as usize;
                if i < self.avisos.len() && (y - o.y) % (AVISO_A + AVISO_ESPACO) < AVISO_A {
                    return Some(Alvo::Aviso(i));
                }
            }
        }
        if self.area_aberta {
            let o = Self::area_origem(tela);
            let (cx, cy) = (x - o.x, y - o.y);
            if cx >= 0 && cy >= 0 && cx < AREA_L && cy < AREA_A {
                if cy < 52 && cx >= AREA_L - 100 {
                    return Some(Alvo::AreaLimpar);
                }
                if cy >= 52 {
                    let i = ((cy - 52) / AREA_ITEM) as usize;
                    if i < crate::area::itens().len() && (cy - 52) % AREA_ITEM < AREA_ITEM - 6 {
                        return Some(Alvo::AreaItem(i));
                    }
                }
                return Some(Alvo::AreaPainel);
            }
        }
        if self.central_aberta {
            let o = Self::central_origem(tela);
            let (cx, cy) = (x - o.x, y - o.y);
            if cx >= 0 && cy >= 0 && cx < CENTRAL_L && cy < CENTRAL_A {
                if cy < 52 && cx >= CENTRAL_L - 100 {
                    return Some(Alvo::CentralLimpar);
                }
                // Ações rápidas: 4 quadradinhos no pé.
                let ay = CENTRAL_A - 96;
                if cy >= ay && cy < CENTRAL_A - 12 {
                    let w = (CENTRAL_L - 24 - 3 * 8) / 4;
                    let i = ((cx - 12) / (w + 8)) as usize;
                    if cx >= 12 && i < ACOES_RAPIDAS.len() && (cx - 12) % (w + 8) < w {
                        return Some(Alvo::CentralAcao(i));
                    }
                    return Some(Alvo::CentralPainel);
                }
                if cy >= 52 && cy < ay - 8 {
                    let i = ((cy - 52) / CENTRAL_ITEM) as usize;
                    if i < self.historico.len() {
                        return Some(Alvo::CentralItem(i));
                    }
                }
                return Some(Alvo::CentralPainel);
            }
        }
        if self.rede_aberta {
            let o = Self::rede_origem(tela);
            if x >= o.x && y >= o.y && x < o.x + SOM_L && y < o.y + REDE_A {
                if y >= o.y + REDE_A - 46 {
                    return Some(Alvo::RedeConfig);
                }
                return Some(Alvo::RedePainel);
            }
        }
        if self.som_aberto {
            let o = Self::som_origem(tela);
            let (sx, sy) = ((x - o.x) as f32, (y - o.y) as f32);
            if sx >= 0.0 && sy >= 0.0 && sx < SOM_L as f32 && sy < SOM_A as f32 {
                if (12.0..52.0).contains(&sx) && (44.0..84.0).contains(&sy) {
                    return Some(Alvo::SomMudo);
                }
                if (SOM_BARRA_X0 - 8.0..SOM_BARRA_X1 + 8.0).contains(&sx) && (48.0..80.0).contains(&sy) {
                    let f = ((sx - SOM_BARRA_X0) / (SOM_BARRA_X1 - SOM_BARRA_X0)).clamp(0.0, 1.0);
                    return Some(Alvo::SomBarra((f * 1000.0) as u16));
                }
                return Some(Alvo::SomPainel);
            }
        }
        if self.menu_aberto {
            let o = Self::menu_origem(tela);
            let (mx, my) = (x - o.x, y - o.y);
            if mx >= 0 && my >= 0 && mx < MENU_L && my < MENU_A {
                if self.energia_aberta {
                    let (px, py) = (MENU_L - 12 - POP_L, MENU_A - RODAPE_A - pop_a() + 4);
                    if mx >= px && mx < px + POP_L && my >= py && my < py + pop_a() {
                        let i = ((my - py - 6).max(0) / POP_ITEM) as usize;
                        return Some(opcoes_energia().get(i).map(|o| o.0).unwrap_or(Alvo::Menu));
                    }
                }
                if my >= MENU_A - RODAPE_A + 12 && my < MENU_A - 12 {
                    // Botões do rodapé, da direita para a esquerda.
                    let botoes = [Alvo::Energia, Alvo::Bloquear];
                    let dx = MENU_L - 12 - mx;
                    if dx >= 0 {
                        let i = (dx / (ENERGIA_L + 4)) as usize;
                        if i < botoes.len() && dx % (ENERGIA_L + 4) < ENERGIA_L {
                            return Some(botoes[i]);
                        }
                    }
                    if mx >= 12 && mx < MENU_L - 12 - 2 * (ENERGIA_L + 4) {
                        return Some(Alvo::Usuario);
                    }
                    return Some(Alvo::Menu);
                }
                if my >= APPS_Y && my < MENU_A - RODAPE_A && mx >= 8 && mx < MENU_L - 8 {
                    let i = ((my - APPS_Y) / APP_A) as usize;
                    let n = self.apps_visiveis().len().saturating_sub(self.menu_rolagem).min(Self::max_apps());
                    if i < n {
                        return Some(Alvo::App(self.menu_rolagem + i));
                    }
                }
                return Some(Alvo::Menu);
            }
        }
        if y < tela.h - altura() || y >= tela.h || x < 0 || x >= tela.w {
            return None;
        }
        if (INICIAR_X..INICIAR_X + Self::iniciar_l()).contains(&x) {
            return Some(Alvo::Iniciar);
        }
        let fx = Self::fixados_x();
        if x >= fx && x < fx + FIXADOS.len() as i32 * FIXADO_L {
            return Some(Alvo::Fixado(((x - fx) / FIXADO_L) as usize));
        }
        if x >= tela.w - CANTO_L {
            return Some(Alvo::CantoArea);
        }
        let bx = tela.w - RELOGIO_L - BANDEJA_L;
        let ix = bx - self.bandeja_l();
        if x >= ix && x < bx - 6 {
            let i = ((x - ix) / ITEM_BANDEJA) as usize;
            if i < self.bandeja.len() {
                return Some(Alvo::ItemBandeja(i));
            }
        }
        if (bx..bx + ICONE_BANDEJA).contains(&x) {
            return Some(Alvo::Rede);
        }
        if (bx + ICONE_BANDEJA..bx + BANDEJA_L).contains(&x) {
            return Some(Alvo::Som);
        }
        if (bx + BANDEJA_L..tela.w - CANTO_L).contains(&x) {
            return Some(Alvo::Relogio);
        }
        let lt = self.largura_tarefa(tela.w);
        let tx = Self::tarefas_x();
        if x >= tx {
            let i = ((x - tx) / (lt + 4)) as usize;
            if i < self.ordem.len() && (x - tx) % (lt + 4) < lt {
                return Some(Alvo::Tarefa(i));
            }
        }
        Some(Alvo::Barra)
    }

    /// Abre o menu do botão direito em `p`, sem sair da tela.
    fn abrir_contexto(&mut self, p: Point<i32, Logical>, tela: Size<i32, Logical>, itens: Vec<(&str, &str)>) {
        let a = itens.len() as i32 * CTX_ITEM + 12;
        let x = p.x.min(tela.w - CTX_L - 4).max(0);
        let y = if p.y + a > tela.h - 4 { (p.y - a).max(0) } else { p.y };
        self.menu_contexto = Some(MenuContexto {
            origem: (x, y).into(),
            itens: itens.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        });
        self.menu_aberto = false;
        self.som_aberto = false;
        self.rede_aberta = false;
    }

    /// Botão direito na área de trabalho (num ícone ou no fundo).
    pub fn contexto_area(&mut self, p: Point<i32, Logical>, tela: Size<i32, Logical>, icone: Option<usize>) {
        let itens: Vec<(&str, &str)> = match icone {
            Some(0) => vec![("Abrir", ICONES_AREA[0].1), ("Propriedades", "configuracoes sobre")],
            Some(1) => vec![
                ("Abrir", ICONES_AREA[1].1),
                ("Abrir no terminal", "sh -c 'cd \"$HOME\" && exec alacritty'"),
            ],
            Some(2) => vec![("Abrir", ICONES_AREA[2].1)],
            Some(i) => {
                // Atalho ou pasta da Área de Trabalho.
                if let Some(ic) = self.icones_area.get(i) {
                    let cmd = ic.cmd.clone();
                    let apagar = ic.arquivo.as_ref().map(|a| format!("#apagar:{}", a.display()));
                    self.icone_sel = Some(i);
                    let mut v: Vec<(String, String)> = vec![("Abrir".into(), cmd)];
                    if let Some(a) = apagar {
                        v.push(("Remover da área de trabalho".into(), a));
                    }
                    let v2: Vec<(&str, &str)> = v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                    self.abrir_contexto(p, tela, v2);
                }
                return;
            }
            None => vec![
                ("Atualizar", "#atualizar"),
                ("Próximo papel de parede", "#papel"),
                ("Abrir o terminal", "alacritty"),
                ("Central de Apps", "central"),
                ("Gerenciador de Tarefas", "tarefas"),
                ("Personalizar", "configuracoes"),
            ],
        };
        if let Some(i) = icone {
            self.icone_sel = Some(i);
        }
        self.abrir_contexto(p, tela, itens);
    }

    /// Botão direito na barra de tarefas.
    pub fn contexto_barra(&mut self, p: Point<i32, Logical>, tela: Size<i32, Logical>) {
        self.abrir_contexto(
            p,
            tela,
            vec![
                ("Gerenciador de Tarefas", "tarefas"),
                ("Mostrar a área de trabalho", "#area"),
                ("Configurações da barra de tarefas", "configuracoes"),
            ],
        );
    }

    /// Troca para o próximo papel de parede da pasta do sistema.
    pub fn proximo_papel(&mut self) {
        let mut ids: Vec<String> = std::fs::read_dir(crate::fundo::PASTA_PAPEIS)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".png")).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        ids.sort();
        let atual = ler_chave("papel").unwrap_or_else(|| "spix".into());
        let i = ids.iter().position(|x| *x == atual).map(|i| (i + 1) % ids.len()).unwrap_or(0);
        gravar_chave("papel", &ids[i]);
    }

    fn desenhar_contexto(&self, m: &MenuContexto, hover: Option<Alvo>) -> Tela {
        let a = m.itens.len() as i32 * CTX_ITEM + 12;
        let mut t = Tela::nova(CTX_L, a);
        let (l, af) = (CTX_L as f32, a as f32);
        t.vidro(0.0, 0.0, l, af, 10.0, (m.origem.x, m.origem.y), tema::FUNDO().com_alfa(200));
        for (i, (rotulo, _)) in m.itens.iter().enumerate() {
            let y = 6.0 + (i as i32 * CTX_ITEM) as f32;
            if hover == Some(Alvo::Contexto(i)) {
                t.retangulo(6.0, y, l - 12.0, CTX_ITEM as f32, 6.0, tema::DESTAQUE().com_alfa(60));
            }
            t.texto(18.0, y + 21.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, rotulo, l - 30.0));
        }
        t
    }

    fn avisos_origem(tela: Size<i32, Logical>, n: usize) -> Point<i32, Logical> {
        let n = n.min(3) as i32;
        let h = n * AVISO_A + (n - 1).max(0) * AVISO_ESPACO;
        (tela.w - AVISO_L - 8, tela.h - altura() - 8 - h).into()
    }

    /// Mostra uma notificação (com o som de notificação).
    pub fn avisar(&mut self, titulo: &str, texto: &str) {
        self.avisar_com(titulo, texto, None);
    }

    pub fn avisar_com(&mut self, titulo: &str, texto: &str, acao: Option<String>) {
        // Tudo vai para o histórico da central; com "Não perturbe", só ele.
        self.historico.insert(0, (titulo.to_string(), texto.to_string(), hora_local().0, acao.clone()));
        self.historico.truncate(30);
        if self.nao_perturbe {
            return;
        }
        self.avisos.push(Aviso {
            titulo: titulo.to_string(),
            texto: texto.to_string(),
            acao,
            ate: std::time::Instant::now() + std::time::Duration::from_secs(6),
        });
        while self.avisos.len() > 3 {
            self.avisos.remove(0);
        }
        tocar_som("notificacao");
    }

    /// Lê as notificações deixadas na pasta (a cada meio segundo) e tira as
    /// que já passaram do tempo.
    fn ler_avisos(&mut self) {
        let agora = std::time::Instant::now();
        self.avisos.retain(|a| a.ate > agora);
        if agora.duration_since(self.avisos_visto) < std::time::Duration::from_millis(500) {
            return;
        }
        self.avisos_visto = agora;
        let Some(pasta) = pasta_avisos() else { return };
        let Ok(rd) = std::fs::read_dir(&pasta) else { return };
        let mut arquivos: Vec<std::path::PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "txt").unwrap_or(false))
            .collect();
        arquivos.sort();
        for p in arquivos {
            let texto = std::fs::read_to_string(&p).unwrap_or_default();
            let _ = std::fs::remove_file(&p);
            let mut linhas = texto.lines();
            let titulo = linhas.next().unwrap_or("").trim().to_string();
            let corpo = linhas.collect::<Vec<_>>().join(" ");
            if !titulo.is_empty() {
                self.avisar(&titulo, corpo.trim());
            }
        }
    }

    fn desenhar_avisos(&self, origem: Point<i32, Logical>, hover: Option<Alvo>) -> Tela {
        let n = self.avisos.len().min(3);
        let h = n as i32 * AVISO_A + (n as i32 - 1).max(0) * AVISO_ESPACO;
        let mut t = Tela::nova(AVISO_L, h.max(1));
        for (i, a) in self.avisos.iter().take(3).enumerate() {
            let y = i as f32 * (AVISO_A + AVISO_ESPACO) as f32;
            let (l, al) = (AVISO_L as f32, AVISO_A as f32);
            t.vidro(0.0, y, l, al, 12.0, (origem.x, origem.y + y as i32), tema::FUNDO().com_alfa(190));
            // Faixa de destaque e o Mingau (bolinha azul) à esquerda.
            t.retangulo(14.0, y + 18.0, 40.0, 40.0, 20.0, tema::DESTAQUE());
            t.retangulo(24.0, y + 26.0, 20.0, 20.0, 10.0, tema::BRANCO().com_alfa(220));
            t.retangulo(30.0, y + 32.0, 8.0, 8.0, 4.0, tema::DESTAQUE());
            t.texto(66.0, y + 32.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &a.titulo, l - 100.0));
            t.texto(66.0, y + 54.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, &a.texto, l - 84.0));
            // "x" para fechar
            let cor = if hover == Some(Alvo::Aviso(i)) { tema::TEXTO() } else { tema::TEXTO_SECUNDARIO().com_alfa(140) };
            t.linha(l - 24.0, y + 14.0, l - 16.0, y + 22.0, 1.4, cor);
            t.linha(l - 24.0, y + 22.0, l - 16.0, y + 14.0, 1.4, cor);
        }
        t
    }

    /// Alt+Tab: cartões com ícone e título, a escolhida em destaque.
    fn desenhar_alternador(&self, lista: &[(String, Option<usize>)], sel: usize, origem: Point<i32, Logical>) -> Tela {
        let n = lista.len().min(7);
        let (cw, ch) = (132.0, 120.0);
        let l = (n as f32 * cw + 24.0) as i32;
        let a = (ch + 24.0) as i32;
        let mut t = Tela::nova(l, a);
        t.vidro(0.0, 0.0, l as f32, a as f32, 14.0, (origem.x, origem.y), tema::FUNDO().com_alfa(200));
        t.contorno(0.5, 0.5, l as f32 - 1.0, a as f32 - 1.0, 1.0, tema::BORDA().com_alfa(160));
        // Janela da escolhida sempre visível (rola se tiver mais de 7).
        let ini = sel.saturating_sub(6);
        for (k, (titulo, app)) in lista.iter().enumerate().skip(ini).take(7) {
            let x = 12.0 + (k - ini) as f32 * cw;
            if k == sel {
                t.retangulo(x + 2.0, 12.0, cw - 4.0, ch, 10.0, tema::DESTAQUE_SUAVE());
                t.contorno(x + 2.0, 12.0, cw - 4.0, ch, 2.0, tema::DESTAQUE());
            }
            let app_ref = app.and_then(|i| self.apps.as_ref().and_then(|v| v.get(i)));
            self.desenhar_icone(&mut t, app_ref, titulo, x + cw / 2.0 - 24.0, 26.0, 48.0);
            let nome = desenho::encurtar(12.5, titulo, cw - 16.0);
            let ln = desenho::medir(12.5, &nome);
            t.texto(x + cw / 2.0 - ln / 2.0, 104.0, 12.5, tema::TEXTO(), &nome);
        }
        t
    }

    fn area_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        ((tela.w - AREA_L) / 2, tela.h - altura() - 12 - AREA_A).into()
    }

    fn desenhar_area(&self, origem: Point<i32, Logical>, hover: Option<Alvo>, itens: &[String]) -> Tela {
        let mut t = Tela::nova(AREA_L, AREA_A);
        let (l, a) = (AREA_L as f32, AREA_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO().com_alfa(215));
        t.contorno(0.5, 0.5, l - 1.0, a - 1.0, 1.0, tema::BORDA().com_alfa(160));
        t.texto(18.0, 34.0, 16.0, tema::TEXTO(), "Área de transferência");
        if !itens.is_empty() {
            let cor = if hover == Some(Alvo::AreaLimpar) { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() };
            let lt = desenho::medir(13.0, "Limpar tudo");
            t.texto(l - 18.0 - lt, 33.0, 13.0, cor, "Limpar tudo");
        } else {
            for (i, linha) in ["Nada copiado ainda.", "Copie um texto (Ctrl+C) e ele aparece aqui;", "clique num item para colar de novo."].iter().enumerate() {
                let lm = desenho::medir(13.0, linha);
                let cor = if i == 0 { tema::TEXTO() } else { tema::TEXTO_SECUNDARIO() };
                t.texto(l / 2.0 - lm / 2.0, a / 2.0 - 10.0 + i as f32 * 22.0, 13.0, cor, linha);
            }
        }
        let max = ((a - 52.0 - 8.0) / AREA_ITEM as f32) as usize;
        for (i, texto) in itens.iter().take(max).enumerate() {
            let y = 52.0 + i as f32 * AREA_ITEM as f32;
            let sobre = hover == Some(Alvo::AreaItem(i));
            t.retangulo(10.0, y, l - 20.0, AREA_ITEM as f32 - 6.0, 8.0, if sobre { tema::SUPERFICIE() } else { tema::SUPERFICIE().com_alfa(150) });
            // Até duas linhas do texto copiado.
            let linhas: Vec<&str> = texto.lines().map(str::trim).filter(|l| !l.is_empty()).take(2).collect();
            for (k, li) in linhas.iter().enumerate() {
                let li: String = li.chars().take(200).collect();
                t.texto(22.0, y + 22.0 + k as f32 * 18.0, 12.5, if k == 0 { tema::TEXTO() } else { tema::TEXTO_SECUNDARIO() }, &desenho::encurtar(12.5, &li, l - 44.0));
            }
        }
        t
    }

    fn central_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (tela.w - CENTRAL_L - 8, tela.h - altura() - 8 - CENTRAL_A).into()
    }

    fn desenhar_central(&self, origem: Point<i32, Logical>, hover: Option<Alvo>) -> Tela {
        let mut t = Tela::nova(CENTRAL_L, CENTRAL_A);
        let (l, a) = (CENTRAL_L as f32, CENTRAL_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO().com_alfa(200));
        t.contorno(0.5, 0.5, l - 1.0, a - 1.0, 1.0, tema::BORDA().com_alfa(160));
        t.texto(18.0, 34.0, 16.0, tema::TEXTO(), "Notificações");
        if !self.historico.is_empty() {
            let cor = if hover == Some(Alvo::CentralLimpar) { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() };
            let lt = desenho::medir(13.0, "Limpar tudo");
            t.texto(l - 18.0 - lt, 33.0, 13.0, cor, "Limpar tudo");
        }
        let ay = a - 96.0;
        if self.historico.is_empty() {
            let msg = if self.nao_perturbe { "Nenhuma notificação. Não perturbe está ligado." } else { "Nenhuma notificação nova." };
            let lm = desenho::medir(13.0, msg);
            t.texto(l / 2.0 - lm / 2.0, ay / 2.0 + 20.0, 13.0, tema::TEXTO_SECUNDARIO(), msg);
        }
        let max = ((ay - 8.0 - 52.0) / CENTRAL_ITEM as f32) as usize;
        for (i, (titulo, texto, hora, _)) in self.historico.iter().take(max).enumerate() {
            let y = 52.0 + i as f32 * CENTRAL_ITEM as f32;
            let sobre = hover == Some(Alvo::CentralItem(i));
            t.retangulo(10.0, y, l - 20.0, CENTRAL_ITEM as f32 - 6.0, 8.0, if sobre { tema::SUPERFICIE() } else { tema::SUPERFICIE().com_alfa(150) });
            t.texto(22.0, y + 23.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, titulo, l - 110.0));
            let lh = desenho::medir(11.5, hora);
            t.texto(l - 22.0 - lh, y + 22.0, 11.5, tema::TEXTO_SECUNDARIO(), hora);
            t.texto(22.0, y + 43.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, texto, l - 44.0));
        }
        // Ações rápidas.
        t.retangulo(0.0, ay - 4.0, l, 1.0, 0.0, tema::BORDA());
        let w = (l - 24.0 - 3.0 * 8.0) / 4.0;
        for (i, rot) in ACOES_RAPIDAS.iter().enumerate() {
            let x = 12.0 + i as f32 * (w + 8.0);
            let ligado = match i {
                0 => self.nao_perturbe,
                1 => desenho::tema::escuro(),
                _ => false,
            };
            let sobre = hover == Some(Alvo::CentralAcao(i));
            let fundo = if ligado { tema::DESTAQUE() } else if sobre { tema::SUPERFICIE() } else { tema::SUPERFICIE().com_alfa(170) };
            t.retangulo(x, ay + 8.0, w, 72.0, 8.0, fundo);
            let cor = if ligado { tema::BRANCO() } else { tema::TEXTO() };
            // Ícones simples.
            let (cx, cy) = (x + w / 2.0, ay + 34.0);
            match i {
                0 => {
                    // Lua (não perturbe).
                    arco(&mut t, cx, cy, 8.0, 0.18, 0.95, 1.8, cor);
                    arco(&mut t, cx + 4.0, cy - 3.0, 6.0, 0.62, 1.12, 1.8, cor);
                }
                1 => {
                    // Meio círculo cheio (tema escuro).
                    arco(&mut t, cx, cy, 8.0, 0.0, 1.0, 1.8, cor);
                    t.retangulo(cx - 1.0, cy - 8.0, 9.0, 16.0, 8.0, cor);
                }
                2 => {
                    // Tesoura/recorte: retângulo tracejado.
                    for k in 0..4 {
                        let d = k as f32 * 5.0;
                        t.linha(cx - 9.0 + d, cy - 7.0, cx - 6.5 + d, cy - 7.0, 1.6, cor);
                        t.linha(cx - 9.0 + d, cy + 7.0, cx - 6.5 + d, cy + 7.0, 1.6, cor);
                    }
                    t.linha(cx - 10.0, cy - 7.0, cx - 10.0, cy + 7.0, 1.6, cor);
                    t.linha(cx + 10.0, cy - 7.0, cx + 10.0, cy + 7.0, 1.6, cor);
                }
                _ => {
                    // Engrenagem simples.
                    arco(&mut t, cx, cy, 7.0, 0.0, 1.0, 2.2, cor);
                    arco(&mut t, cx, cy, 2.5, 0.0, 1.0, 1.6, cor);
                    for k in 0..6 {
                        let ang = k as f32 * std::f32::consts::TAU / 6.0;
                        t.linha(cx + 7.0 * ang.cos(), cy - 7.0 * ang.sin(), cx + 10.0 * ang.cos(), cy - 10.0 * ang.sin(), 2.2, cor);
                    }
                }
            }
            let lr = desenho::medir(11.5, rot);
            t.texto(cx - lr / 2.0, ay + 66.0, 11.5, cor, &desenho::encurtar(11.5, rot, w - 6.0));
        }
        t
    }

    fn rede_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (tela.w - SOM_L - 8, tela.h - altura() - 8 - REDE_A).into()
    }

    fn som_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (tela.w - SOM_L - 8, tela.h - altura() - 8 - SOM_A).into()
    }

    /// Lê volume e mudo com o wpctl: na hora quando `agora` (painel aberto,
    /// volume mudado aqui); fora isso, a cada 30 s (mudança feita por outro
    /// programa). Assim a barra não cria um processo a cada poucos segundos.
    pub fn ler_som(&mut self, agora: bool) {
        let seg = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let intervalo = if self.som_aberto { 3 } else { 30 };
        if !agora && seg < self.som.lido_em + intervalo {
            return;
        }
        self.som.lido_em = seg;
        let saida = std::process::Command::new("wpctl")
            .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok());
        // Formato: "Volume: 0.40" ou "Volume: 0.40 [MUTED]".
        match saida {
            Some(s) => {
                self.som.volume = s.split_whitespace().nth(1).and_then(|v| v.parse().ok());
                self.som.mudo = s.contains("MUTED");
            }
            None => {
                self.som.volume = None;
                self.som.mudo = false;
            }
        }
    }

    /// A cada segundo, vê se a configuração mudou (o app Configurações
    /// grava nela) e aplica o novo tamanho da barra na hora.
    fn ver_config(&mut self) {
        let seg = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if seg == self.config_visto.0 {
            return;
        }
        let v = versao_config();
        if v != self.config_visto.1 {
            let a = ler_altura();
            ALTURA_ATUAL.store(a, std::sync::atomic::Ordering::Relaxed);
            let lado = (a - 16).max(16) as u32;
            self.logo = desenho::icone(LOGO_PNG, lado, lado);
            self.barra = None;
            self.menu = None;
            // Papel de parede (e o vidro feito dele) podem ter mudado.
            crate::fundo::VERSAO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            desenho::tema::definir_escuro(ler_chave("tema").as_deref() == Some("escuro"));
            desenho::tema::definir_destaque(ler_chave("destaque").as_deref().unwrap_or("azul"));
        }
        self.config_visto = (seg, v);
    }

    /// Lê o estado da rede: interface com rota padrão, IP e se é sem fio.
    pub fn ler_rede(&mut self) {
        let seg = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if seg < self.rede.lido_em + 5 {
            return;
        }
        let mut r = Rede { lido_em: seg, ..Default::default() };
        // Rota padrão: destino 00000000 em /proc/net/route.
        let rotas = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
        if let Some(iface) = rotas
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().collect::<Vec<_>>())
            .find(|c| c.len() > 2 && c[1] == "00000000")
            .map(|c| c[0].to_string())
        {
            r.conectado = true;
            r.sem_fio = std::path::Path::new(&format!("/sys/class/net/{iface}/wireless")).exists();
            r.ip = ipv4_de(&iface).unwrap_or_default();
            r.interface = iface;
        } else if let Ok(dir) = std::fs::read_dir("/sys/class/net") {
            // Sem rota: mostra a primeira interface que não seja a lo.
            r.interface = dir
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .find(|n| n != "lo")
                .unwrap_or_default();
            r.sem_fio = std::path::Path::new(&format!("/sys/class/net/{}/wireless", r.interface)).exists();
        }
        self.wifi_disponivel = std::fs::read_dir("/sys/class/net")
            .map(|d| d.flatten().any(|e| e.path().join("wireless").exists()))
            .unwrap_or(false);
        self.rede = r;
    }

    fn desenhar_rede(&self, origem: Point<i32, Logical>, sobre_config: bool) -> Tela {
        let mut t = Tela::nova(SOM_L, REDE_A);
        let (l, a) = (SOM_L as f32, REDE_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO().com_alfa(175));
        let tipo = if self.rede.sem_fio { "Wi-Fi" } else { "Rede cabeada" };
        t.texto(16.0, 28.0, 13.0, tema::TEXTO(), tipo);
        icone_rede(&mut t, 16.0, 44.0, 32.0, &self.rede, tema::TEXTO());
        if self.rede.conectado {
            t.texto(60.0, 60.0, 14.0, tema::TEXTO(), "Conectado");
            let det = format!("{} · {}", self.rede.interface, self.rede.ip);
            let det = desenho::encurtar(12.0, &det, l - 76.0);
            t.texto(60.0, 80.0, 12.0, tema::TEXTO_SECUNDARIO(), &det);
        } else {
            t.texto(60.0, 60.0, 14.0, tema::TEXTO(), "Sem conexão");
            let det = if self.rede.interface.is_empty() {
                "Nenhuma placa de rede encontrada.".to_string()
            } else {
                format!("Verifique o cabo ({}).", self.rede.interface)
            };
            t.texto(60.0, 80.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, &det, l - 76.0));
        }
        // Atalho para a página de rede das Configurações (lista de Wi-Fi).
        let by = a - 42.0;
        t.retangulo(12.0, by - 6.0, l - 24.0, 1.0, 0.0, tema::BORDA().com_alfa(160));
        if sobre_config {
            t.retangulo(8.0, by, l - 16.0, 34.0, 8.0, tema::BRANCO().com_alfa(90));
        }
        let rotulo = if self.rede.sem_fio || self.wifi_disponivel { "Redes Wi-Fi e configurações" } else { "Configurações de rede" };
        t.texto(18.0, by + 22.0, 13.0, tema::DESTAQUE(), rotulo);
        t
    }

    pub fn definir_volume(&mut self, f: f32) {
        let _ = std::process::Command::new("wpctl")
            .args(["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", f.clamp(0.0, 1.0))])
            .status();
        if self.som.mudo {
            let _ = std::process::Command::new("wpctl").args(["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]).status();
        }
        self.ler_som(true);
    }

    /// Tecla de volume: +/− 5% (ou mudo) e o indicador na tela.
    pub fn tecla_volume(&mut self, delta: f32, mudo: bool) {
        self.ler_som(true);
        if mudo {
            self.alternar_mudo();
        } else {
            let v = (self.som.volume.unwrap_or(0.5) + delta).clamp(0.0, 1.0);
            self.definir_volume(v);
        }
        let v = if self.som.mudo { 0.0 } else { self.som.volume.unwrap_or(0.0) };
        self.indicador = Some(("Volume", v.min(1.0), std::time::Instant::now() + std::time::Duration::from_millis(1500)));
    }

    /// Tecla de brilho (notebook): o ararinha-sistema muda o /sys.
    pub fn tecla_brilho(&mut self, delta: f32) {
        let base = std::fs::read_dir("/sys/class/backlight").ok().and_then(|mut r| r.next()).and_then(|e| e.ok()).map(|e| e.path());
        let Some(base) = base else { return };
        let ler = |n: &str| std::fs::read_to_string(base.join(n)).ok().and_then(|s| s.trim().parse::<f32>().ok());
        let (Some(atual), Some(max)) = (ler("brightness"), ler("max_brightness")) else { return };
        let novo = ((atual / max) + delta).clamp(0.05, 1.0);
        let pedido = format!("brilho {}", (novo * 100.0).round() as u32);
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok(mut c) = std::os::unix::net::UnixStream::connect("/run/ararinha/sistema.sock") {
                let _ = c.write_all(format!("{pedido}\n").as_bytes());
                let _ = c.shutdown(std::net::Shutdown::Write);
                let mut r = String::new();
                let _ = c.read_to_string(&mut r);
            }
        });
        self.indicador = Some(("Brilho", novo, std::time::Instant::now() + std::time::Duration::from_millis(1500)));
    }

    fn desenhar_indicador(&self, rotulo: &str, nivel: f32, origem: Point<i32, Logical>) -> Tela {
        let (l, a) = (260, 64);
        let mut t = Tela::nova(l, a);
        t.vidro(0.0, 0.0, l as f32, a as f32, 14.0, (origem.x, origem.y), tema::FUNDO().com_alfa(205));
        t.contorno(0.5, 0.5, l as f32 - 1.0, a as f32 - 1.0, 1.0, tema::BORDA().com_alfa(160));
        t.texto(18.0, 26.0, 13.0, tema::TEXTO(), rotulo);
        let pct = format!("{:.0}%", nivel * 100.0);
        let lp = desenho::medir(13.0, &pct);
        t.texto(l as f32 - 18.0 - lp, 26.0, 13.0, tema::TEXTO_SECUNDARIO(), &pct);
        t.retangulo(18.0, 40.0, l as f32 - 36.0, 6.0, 3.0, tema::BORDA());
        t.retangulo(18.0, 40.0, (l as f32 - 36.0) * nivel.clamp(0.0, 1.0), 6.0, 3.0, tema::DESTAQUE());
        t
    }

    pub fn alternar_mudo(&mut self) {
        let _ = std::process::Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
            .status();
        self.ler_som(true);
    }

    fn desenhar_som(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(SOM_L, SOM_A);
        let (l, a) = (SOM_L as f32, SOM_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO().com_alfa(175));
        t.texto(16.0, 28.0, 13.0, tema::TEXTO(), "Volume");
        let Some(vol) = self.som.volume else {
            t.texto(16.0, 70.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhuma saída de som encontrada.");
            return t;
        };
        if hover == Some(Alvo::SomMudo) {
            t.retangulo(12.0, 44.0, 40.0, 40.0, 8.0, tema::BRANCO().com_alfa(150));
        }
        alto_falante(&mut t, 18.0, 50.0, 28.0, vol, self.som.mudo, tema::TEXTO());
        let v = vol.clamp(0.0, 1.0);
        let y = 64.0;
        t.retangulo(SOM_BARRA_X0, y - 2.0, SOM_BARRA_X1 - SOM_BARRA_X0, 4.0, 2.0, tema::TEXTO_SECUNDARIO().com_alfa(90));
        let cor = if self.som.mudo { tema::TEXTO_SECUNDARIO() } else { tema::DESTAQUE() };
        let xk = SOM_BARRA_X0 + v * (SOM_BARRA_X1 - SOM_BARRA_X0);
        t.retangulo(SOM_BARRA_X0, y - 2.0, xk - SOM_BARRA_X0, 4.0, 2.0, cor);
        let r = if matches!(hover, Some(Alvo::SomBarra(_))) { 9.0 } else { 8.0 };
        t.retangulo(xk - r, y - r, 2.0 * r, 2.0 * r, r, tema::BRANCO());
        t.retangulo(xk - r + 3.0, y - r + 3.0, 2.0 * r - 6.0, 2.0 * r - 6.0, r - 3.0, cor);
        let pct = format!("{}", (vol * 100.0).round() as i32);
        let lp = desenho::medir(13.0, &pct);
        t.texto(l - 18.0 - lp, y + 5.0, 13.0, tema::TEXTO(), &pct);
        t
    }

    fn atualizar_relogio(&mut self) {
        let minuto = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() / 60)
            .unwrap_or(0);
        if minuto == self.relogio.0 {
            return;
        }
        let (hora, data) = hora_local();
        self.relogio = (minuto, hora, data);
    }

    /// Lê os aplicativos de /usr/share/applications (uma vez por abertura do menu).
    pub fn carregar_apps(&mut self) {
        let mut apps = Vec::new();
        let mut vistos = std::collections::HashSet::new();
        for dados in pastas_de_dados() {
            let pasta = format!("{dados}/applications");
            let Ok(dir) = std::fs::read_dir(pasta) else { continue };
            for entrada in dir.flatten() {
                let caminho = entrada.path();
                if caminho.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let id = caminho.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
                // O primeiro com o mesmo nome vence (como no XDG_DATA_DIRS).
                if !vistos.insert(id.clone()) {
                    continue;
                }
                if let Some(app) = std::fs::read_to_string(&caminho).ok().and_then(|t| ler_desktop(&t, &id)) {
                    apps.push(app);
                }
            }
        }
        apps.sort_by_key(|a| a.nome.to_lowercase());
        self.apps = Some(apps);
    }

    /// Retângulo azul de seleção ao arrastar na área de trabalho.
    pub fn elementos_selecao(
        &mut self,
        geo: Rectangle<i32, Logical>,
        ponteiro: Point<f64, Logical>,
        escala: f64,
    ) -> Vec<SolidColorRenderElement> {
        if let Some(r) = self.recorte {
            return self.elementos_recorte(r, geo, ponteiro, escala);
        }
        let Some(inicio) = self.selecao else { return Vec::new() };
        let (x0, y0) = (inicio.x.min(ponteiro.x), inicio.y.min(ponteiro.y));
        let (x1, y1) = (inicio.x.max(ponteiro.x), inicio.y.max(ponteiro.y));
        let loc = Point::<f64, Logical>::from((x0, y0)) - geo.loc.to_f64();
        let (w, h) = ((x1 - x0).round() as i32, (y1 - y0).round() as i32);
        if w < 2 && h < 2 {
            return Vec::new();
        }
        // Cores pré-multiplicadas: azul-ararinha translúcido e borda mais forte.
        let (r, g, b) = (0x2B as f32 / 255.0, 0x7B as f32 / 255.0, 0xC4 as f32 / 255.0);
        let preenchimento = [r * 0.25, g * 0.25, b * 0.25, 0.25];
        let borda = [r * 0.9, g * 0.9, b * 0.9, 0.9];
        let partes: [((i32, i32), (i32, i32), [f32; 4]); 5] = [
            ((0, 0), (w, h), preenchimento),
            ((0, 0), (w, 1), borda),
            ((0, h - 1), (w, 1), borda),
            ((0, 0), (1, h), borda),
            ((w - 1, 0), (1, h), borda),
        ];
        let mut v = Vec::new();
        for (buf, (desl, tam, cor)) in self.sel.iter_mut().zip(partes) {
            buf.update(tam, cor);
            let pos = (loc + Point::from((desl.0 as f64, desl.1 as f64))).to_physical(escala).to_i32_round();
            v.push(SolidColorRenderElement::from_buffer(buf, pos, escala, 1.0, Kind::Unspecified));
        }
        // A borda vem antes (fica por cima do preenchimento).
        v.rotate_left(1);
        v
    }

    /// Captura com recorte: escurece a tela fora do retângulo escolhido.
    fn elementos_recorte(
        &mut self,
        inicio: Option<Point<f64, Logical>>,
        geo: Rectangle<i32, Logical>,
        ponteiro: Point<f64, Logical>,
        escala: f64,
    ) -> Vec<SolidColorRenderElement> {
        let (tw, th) = (geo.size.w, geo.size.h);
        let p = ponteiro - geo.loc.to_f64();
        let (x0, y0, x1, y1) = match inicio {
            Some(i) => {
                let i = i - geo.loc.to_f64();
                (i.x.min(p.x) as i32, i.y.min(p.y) as i32, i.x.max(p.x) as i32, i.y.max(p.y) as i32)
            }
            None => (0, 0, 0, 0),
        };
        let escuro = [0.0, 0.0, 0.0, 0.45];
        let borda = [1.0, 1.0, 1.0, 1.0];
        let (w, h) = (x1 - x0, y1 - y0);
        let mut partes: Vec<((i32, i32), (i32, i32), [f32; 4])> = vec![
            ((0, 0), (tw, y0), escuro),
            ((0, y1), (tw, th - y1), escuro),
            ((0, y0), (x0, h), escuro),
            ((x1, y0), (tw - x1, h), escuro),
        ];
        if inicio.is_none() {
            // Antes de arrastar: a tela toda escura.
            partes = vec![((0, 0), (tw, th), escuro)];
        } else if w > 1 && h > 1 {
            partes.extend([
                ((x0, y0), (w, 1), borda),
                ((x0, y1 - 1), (w, 1), borda),
                ((x0, y0), (1, h), borda),
                ((x1 - 1, y0), (1, h), borda),
            ]);
        }
        let mut v = Vec::new();
        for (buf, (desl, tam, cor)) in self.sel.iter_mut().zip(partes) {
            if tam.0 <= 0 || tam.1 <= 0 {
                continue;
            }
            buf.update(tam, cor);
            let pos = Point::<f64, Logical>::from((desl.0 as f64, desl.1 as f64)).to_physical(escala).to_i32_round();
            v.push(SolidColorRenderElement::from_buffer(buf, pos, escala, 1.0, Kind::Unspecified));
        }
        // Bordas por cima.
        v.reverse();
        v
    }

    /// Ícone da área de trabalho em `p` (coordenadas da tela).
    pub fn icone_em(&self, p: Point<f64, Logical>) -> Option<usize> {
        let (x, y) = (p.x as i32 - AREA_X, p.y as i32 - AREA_Y);
        if x < 0 || y < 0 || x % (ICONE_L + 6) >= ICONE_L {
            return None;
        }
        let coluna = (x / (ICONE_L + 6)) as usize;
        let linha = (y / (ICONE_A + 6)) as usize;
        if linha >= self.area_linhas {
            return None;
        }
        let i = coluna * self.area_linhas + linha;
        (i < self.icones_area.len() && y % (ICONE_A + 6) < ICONE_A).then_some(i)
    }

    /// Clique num ícone da área de trabalho: seleciona; no segundo clique
    /// rápido devolve o comando para abrir.
    pub fn clicar_icone(&mut self, i: usize) -> Option<String> {
        let agora = std::time::Instant::now();
        let duplo = self
            .ultimo_clique_icone
            .map(|(j, t)| j == i && agora.duration_since(t).as_millis() < 450)
            .unwrap_or(false);
        self.icone_sel = Some(i);
        self.ultimo_clique_icone = if duplo { None } else { Some((i, agora)) };
        if duplo {
            self.icones_area.get(i).map(|c| c.cmd.clone())
        } else {
            None
        }
    }

    /// Ícones da área de trabalho (ficam por baixo das janelas).
    pub fn elementos_area<R>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        space: &Space<WindowElement>,
        ponteiro: Point<f64, Logical>,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        let Some(geo) = space.output_geometry(output) else { return Vec::new() };
        let escala = output.current_scale().fractional_scale();
        // Hover só quando não há janela por cima do ponteiro.
        let sobre = if space.element_under(ponteiro).is_none() {
            self.icone_em(ponteiro - geo.loc.to_f64())
        } else {
            None
        };
        // A pasta Área de Trabalho mudou? (olha a cada 2 s)
        let mut mudou = false;
        if self.area_visto.0.elapsed() >= std::time::Duration::from_secs(2) {
            let pasta = pasta_area();
            let m = pasta.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
            if m != self.area_visto.1 {
                let mut v = icones_fixos();
                if let Some(p) = &pasta {
                    v.extend(icones_da_pasta(p));
                }
                self.icones_area = v;
                mudou = true;
            }
            self.area_visto = (std::time::Instant::now(), m);
        }
        let linhas = (((geo.size.h - altura() - AREA_Y) / (ICONE_A + 6)).max(1)) as usize;
        if linhas != self.area_linhas {
            self.area_linhas = linhas;
            mudou = true;
        }
        let chave = format!("{sobre:?}|{:?}|{}|{}", self.icone_sel, self.icones_area.len(), linhas);
        if mudou || self.area_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
            let n = self.icones_area.len();
            let colunas = n.div_ceil(linhas).max(1) as i32;
            let mut t = Tela::nova(colunas * (ICONE_L + 6), linhas.min(n.max(1)) as i32 * (ICONE_A + 6));
            for (i, icone) in self.icones_area.iter().enumerate() {
                let nome = icone.nome.as_str();
                let x0 = ((i / linhas) as i32 * (ICONE_L + 6)) as f32;
                let y = ((i % linhas) as i32 * (ICONE_A + 6)) as f32;
                if self.icone_sel == Some(i) {
                    t.retangulo(x0, y, ICONE_L as f32, ICONE_A as f32, 6.0, tema::DESTAQUE().com_alfa(110));
                    t.retangulo(x0 + 1.0, y + 1.0, ICONE_L as f32 - 2.0, ICONE_A as f32 - 2.0, 5.0, tema::BRANCO().com_alfa(40));
                } else if sobre == Some(i) {
                    t.retangulo(x0, y, ICONE_L as f32, ICONE_A as f32, 6.0, tema::BRANCO().com_alfa(55));
                }
                if let Some(img) = icone.img.as_ref() {
                    t.imagem(x0 as i32 + (ICONE_L - 48) / 2, y as i32 + 8, img);
                }
                // Nome em branco com sombra, em até duas linhas (como no Windows).
                let max = ICONE_L as f32 - 6.0;
                let rotulo: Vec<String> = if desenho::medir(12.0, nome) <= max {
                    vec![nome.to_string()]
                } else {
                    match nome.split_once(' ') {
                        Some((a, b)) => vec![desenho::encurtar(12.0, a, max), desenho::encurtar(12.0, b, max)],
                        None => vec![desenho::encurtar(12.0, nome, max)],
                    }
                };
                for (k, linha) in rotulo.iter().enumerate() {
                    let ln = desenho::medir(12.0, linha);
                    let x = x0 + ICONE_L as f32 / 2.0 - ln / 2.0;
                    let yl = y + 72.0 + k as f32 * 14.0;
                    t.texto(x + 1.0, yl + 1.0, 12.0, desenho::Cor(0, 0, 0, 170), linha);
                    t.texto(x, yl, 12.0, tema::BRANCO(), linha);
                }
            }
            self.area_buf = Some((chave, t.em_buffer()));
        }
        let Some((_, buf)) = &self.area_buf else { return Vec::new() };
        let pos = geo.loc + Point::<i32, Logical>::from((AREA_X, AREA_Y));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (pos - geo.loc).to_f64().to_physical(escala),
            buf,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .map(|e| vec![e])
        .unwrap_or_default()
    }

    /// Elementos de desenho da interface para esta saída (menu por cima).
    pub fn elementos<R>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        space: &Space<WindowElement>,
        ponteiro: Point<f64, Logical>,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        let Some(geo) = space.output_geometry(output) else { return Vec::new() };
        let escala = output.current_scale().fractional_scale();
        let tela = geo.size;
        let local = ponteiro - geo.loc.to_f64();
        let hover = if geo.to_f64().contains(ponteiro) { self.alvo(local, tela) } else { None };

        self.atualizar(space);
        self.atualizar_relogio();
        desenho::preparar_vidro(tela.w, tela.h);

        let ativa = space.elements().last().cloned();
        if self.apps.is_none() {
            self.carregar_apps();
        }
        let janelas: Vec<(String, bool, bool, Option<usize>)> = self
            .ordem
            .iter()
            .map(|w| (titulo(w), Some(w) == ativa.as_ref(), self.esta_minimizada(w), self.app_da_janela(w)))
            .collect();

        self.ler_som(false);
        self.ler_rede();
        self.ver_config();
        self.ler_avisos();
        let mut elementos = Vec::new();

        // Menu do botão direito: o primeiro da lista fica por cima de tudo.
        if let Some(m) = self.menu_contexto.clone() {
            let chave = format!("{:?}|{:?}|{hover:?}|{}", m.origem, m.itens, desenho::tema::escuro());
            if self.ctx_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_contexto(&m, hover);
                self.ctx_buf = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.ctx_buf {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    m.origem.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if let Some((rot, nivel, ate)) = self.indicador {
            if std::time::Instant::now() > ate {
                self.indicador = None;
            } else {
                let o: Point<i32, Logical> = ((tela.w - 260) / 2, tela.h - altura() - 100).into();
                let chave = format!("{rot}|{nivel}|{o:?}|{}", desenho::tema::escuro());
                if self.indicador_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                    let t = self.desenhar_indicador(rot, nivel, o);
                    self.indicador_buf = Some((chave, t.em_buffer()));
                }
                if let Some((_, buf)) = &self.indicador_buf {
                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                        renderer,
                        o.to_f64().to_physical(escala),
                        buf,
                        None,
                        None,
                        None,
                        Kind::Unspecified,
                    ) {
                        elementos.insert(0, e);
                    }
                }
            }
        }

        if let Some((lista, sel)) = self.alternador.clone() {
            let n = lista.len().min(7) as i32;
            let (l, a) = (n * 132 + 24, 144);
            let o: Point<i32, Logical> = ((tela.w - l) / 2, (tela.h - a) / 2).into();
            let chave = format!("{lista:?}|{sel}|{o:?}|{}", desenho::tema::escuro());
            if self.alternador_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_alternador(&lista, sel, o);
                self.alternador_buf = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.alternador_buf {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.insert(0, e);
                }
            }
        }

        if self.area_aberta {
            let o = Self::area_origem(tela);
            let chave = format!(
                "{hover:?}|{o:?}|{}|{}|{}",
                crate::area::VERSAO.load(std::sync::atomic::Ordering::Relaxed),
                desenho::tema::escuro(),
                desenho::tema::destaque_atual()
            );
            if self.transf_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_area(o, hover, &crate::area::itens());
                self.transf_buf = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.transf_buf {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if self.central_aberta {
            let o = Self::central_origem(tela);
            let chave = format!(
                "{hover:?}|{o:?}|{:?}|{}|{}|{}",
                self.historico,
                self.nao_perturbe,
                desenho::tema::escuro(),
                desenho::tema::destaque_atual()
            );
            if self.central_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_central(o, hover);
                self.central_buf = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.central_buf {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if !self.avisos.is_empty() && !self.central_aberta {
            let o = Self::avisos_origem(tela, self.avisos.len());
            let chave = format!(
                "{:?}|{o:?}|{hover:?}|{}",
                self.avisos.iter().map(|a| (&a.titulo, &a.texto)).collect::<Vec<_>>(),
                desenho::tema::escuro()
            );
            if self.avisos_buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_avisos(o, hover);
                self.avisos_buf = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.avisos_buf {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if self.rede_aberta {
            let o = Self::rede_origem(tela);
            let sobre_config = hover == Some(Alvo::RedeConfig);
            let chave = format!("{:?}|{o:?}|{sobre_config}|{}", self.rede, desenho::tema::escuro());
            if self.painel_rede.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_rede(o, sobre_config);
                self.painel_rede = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.painel_rede {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if self.som_aberto {
            let o = Self::som_origem(tela);
            let chave = format!("{hover:?}|{:?}|{o:?}", self.som);
            if self.painel_som.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_som(hover, o);
                self.painel_som = Some((chave, t.em_buffer()));
            }
            if let Some((_, buf)) = &self.painel_som {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        if self.menu_aberto {
            let chave = format!(
                "{hover:?}|{}|{:?}|{}|{}|{}",
                self.energia_aberta,
                self.apps.as_ref().map(|a| a.len()),
                self.busca,
                self.menu_rolagem,
                self.menu_sel
            );
            if self.menu.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let tela_menu = self.desenhar_menu(hover, Self::menu_origem(tela));
                self.menu = Some((chave, tela_menu.em_buffer()));
            }
            let o = Self::menu_origem(tela);
            if let Some((_, buf)) = &self.menu {
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    o.to_f64().to_physical(escala),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    elementos.push(e);
                }
            }
        }

        let chave = format!(
            "{}|{}|{hover:?}|{}|{}|{}|{janelas:?}|{:?}|{}",
            tela.w,
            altura(), self.menu_aberto, self.relogio.1, self.relogio.2, self.som, self.som_aberto
        ) + &format!("|{:?}|{}|{}|{}", self.rede, self.rede_aberta, self.bandeja_versao, self.central_aberta);
        if self.barra.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
            let t = self.desenhar_barra(tela.w, tela.h, hover, &janelas);
            self.barra = Some((chave, t.em_buffer()));
        }
        if let Some((_, buf)) = &self.barra {
            let pos = Point::<i32, Logical>::from((0, tela.h - altura()));
            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                pos.to_f64().to_physical(escala),
                buf,
                None,
                None,
                None,
                Kind::Unspecified,
            ) {
                elementos.push(e);
            }
        }
        elementos
    }

    /// Índice do aplicativo (em `apps`) dono da janela, pelo app_id.
    pub fn app_de(&self, w: &WindowElement) -> Option<usize> {
        self.app_da_janela(w)
    }

    fn app_da_janela(&self, w: &WindowElement) -> Option<usize> {
        let id = w.0.toplevel().and_then(|t| {
            compositor::with_states(t.wl_surface(), |s| {
                s.data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().unwrap().app_id.clone())
            })
        })?;
        let id = id.to_lowercase();
        self.apps.as_ref()?.iter().position(|a| a.ids.contains(&id))
    }

    /// Desenha o ícone do app ou, sem ícone, a inicial num quadrado azul.
    fn desenhar_icone(&self, t: &mut Tela, app: Option<&App>, nome: &str, x: f32, y: f32, lado: f32) {
        if let Some(img) = app.and_then(|a| a.icone.as_ref()) {
            let l = lado.round() as u32;
            let r = image::imageops::resize(img, l, l, image::imageops::FilterType::Triangle);
            t.imagem(x.round() as i32, y.round() as i32, &r);
            return;
        }
        t.retangulo(x, y, lado, lado, lado * 0.25, tema::DESTAQUE());
        let ini = letra(nome);
        let ti = lado * 0.56;
        let li = desenho::medir(ti, &ini);
        t.texto(x + lado / 2.0 - li / 2.0, y + lado / 2.0 + ti * 0.36, ti, tema::BRANCO(), &ini);
    }

    fn desenhar_barra(
        &self,
        largura: i32,
        altura_tela: i32,
        hover: Option<Alvo>,
        janelas: &[(String, bool, bool, Option<usize>)],
    ) -> Tela {
        let h = altura();
        let hf = h as f32;
        let mut t = Tela::nova(largura, h);
        let l = largura as f32;
        // Vidro (Aero): papel de parede desfocado com tinta clara.
        t.vidro(-1.0, 0.0, l + 2.0, hf + 1.0, 0.0, (0, altura_tela - h), tema::SUPERFICIE().com_alfa(120));

        let bh = hf - 8.0; // altura dos botões
        let meio = hf / 2.0;

        // Botão iniciar com o Mingau.
        let il = Self::iniciar_l() as f32;
        if self.menu_aberto || hover == Some(Alvo::Iniciar) {
            t.retangulo(INICIAR_X as f32, 4.0, il, bh, 8.0, tema::BRANCO().com_alfa(150));
            t.retangulo(INICIAR_X as f32, 4.0, il, bh, 8.0, tema::DESTAQUE().com_alfa(40));
        }
        if let Some(logo) = &self.logo {
            let lado = logo.width() as i32;
            t.imagem(INICIAR_X + (Self::iniciar_l() - lado) / 2, (h - lado) / 2, logo);
        } else {
            t.retangulo(INICIAR_X as f32 + 6.0, meio - 14.0, 28.0, 28.0, 14.0, tema::DESTAQUE());
        }

        // Apps fixados (ícone só).
        for i in 0..FIXADOS.len() {
            let x = (Self::fixados_x() + i as i32 * FIXADO_L) as f32;
            if hover == Some(Alvo::Fixado(i)) {
                t.retangulo(x + 2.0, 4.0, FIXADO_L as f32 - 4.0, bh, 6.0, tema::BRANCO().com_alfa(130));
            }
            let app = self.app_fixado(i);
            let nome = app.map(|a| a.nome.clone()).unwrap_or_else(|| FIXADOS[i].to_string());
            let lado = (hf * 0.55).round().clamp(18.0, 30.0);
            self.desenhar_icone(&mut t, app, &nome, x + (FIXADO_L as f32 - lado) / 2.0, meio - lado / 2.0, lado);
        }
        t.retangulo(
            (Self::tarefas_x() - 6) as f32,
            hf * 0.28,
            1.0,
            hf * 0.44,
            0.0,
            tema::TEXTO_SECUNDARIO().com_alfa(70),
        );

        // Um botão por janela.
        let lt = self.largura_tarefa(largura);
        let icone = (hf * 0.5).round().clamp(18.0, 28.0);
        let tam_txt = if h < 42 { 12.0 } else { 13.0 };
        for (i, (nome, ativa, minimizada, app)) in janelas.iter().enumerate() {
            let x = (Self::tarefas_x() + i as i32 * (lt + 4)) as f32;
            if *ativa && !*minimizada {
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::BRANCO().com_alfa(170));
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::DESTAQUE().com_alfa(45));
            } else if hover == Some(Alvo::Tarefa(i)) {
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::BRANCO().com_alfa(120));
            }
            // Indicador embaixo: longo na ativa, curto nas outras.
            let (il, cor) = if *ativa && !*minimizada {
                (20.0, tema::DESTAQUE())
            } else {
                (8.0, tema::TEXTO_SECUNDARIO().com_alfa(150))
            };
            t.retangulo(x + lt as f32 / 2.0 - il / 2.0, hf - 6.0, il, 3.0, 1.5, cor);

            let app = (*app).and_then(|i| self.apps.as_ref()?.get(i));
            self.desenhar_icone(&mut t, app, nome, x + 8.0, meio - icone / 2.0, icone);
            if lt > 60 {
                let txt = desenho::encurtar(tam_txt, nome, lt as f32 - icone - 24.0);
                let cor = if *minimizada { tema::TEXTO_SECUNDARIO() } else { tema::TEXTO() };
                t.texto_com_brilho(x + icone + 16.0, meio + tam_txt * 0.36, tam_txt, cor, &txt);
            }
        }

        // Bandeja: ícones dos apps, depois rede e som.
        let ib = ICONE_BANDEJA as f32;
        let bx = l - (RELOGIO_L + BANDEJA_L) as f32;
        let mut ix = bx - self.bandeja_l() as f32;
        for (i, item) in self.bandeja.iter().enumerate() {
            let il = ITEM_BANDEJA as f32;
            if hover == Some(Alvo::ItemBandeja(i)) {
                t.retangulo(ix + 1.0, 4.0, il - 2.0, bh, 6.0, tema::BRANCO().com_alfa(150));
            }
            if let Some(img) = &item.icone {
                t.imagem((ix + (il - img.width() as f32) / 2.0) as i32, (meio - img.height() as f32 / 2.0) as i32, img);
            } else {
                let ini = letra(&item.titulo);
                t.retangulo(ix + il / 2.0 - 9.0, meio - 9.0, 18.0, 18.0, 5.0, tema::DESTAQUE());
                let li = desenho::medir(11.0, &ini);
                t.texto(ix + il / 2.0 - li / 2.0, meio + 4.0, 11.0, tema::BRANCO(), &ini);
            }
            ix += il;
        }
        let lado = (hf * 0.45).clamp(16.0, 24.0);
        if self.rede_aberta || hover == Some(Alvo::Rede) {
            t.retangulo(bx + 2.0, 4.0, ib - 4.0, bh, 6.0, tema::BRANCO().com_alfa(150));
        }
        icone_rede(&mut t, bx + (ib - lado) / 2.0, meio - lado / 2.0, lado, &self.rede, tema::TEXTO());
        let bx = bx + ib;
        if self.som_aberto || hover == Some(Alvo::Som) {
            t.retangulo(bx + 2.0, 4.0, ib - 4.0, bh, 6.0, tema::BRANCO().com_alfa(150));
        }
        alto_falante(
            &mut t,
            bx + (ib - lado) / 2.0,
            meio - lado / 2.0,
            lado,
            self.som.volume.unwrap_or(0.0),
            self.som.mudo || self.som.volume.is_none(),
            tema::TEXTO(),
        );

        // Cantinho "Mostrar área de trabalho" e o tracinho que o separa.
        let cx = l - CANTO_L as f32;
        t.retangulo(cx, 8.0, 1.0, hf - 16.0, 0.0, tema::TEXTO_SECUNDARIO().com_alfa(90));
        if hover == Some(Alvo::CantoArea) {
            t.retangulo(cx + 1.0, 0.0, CANTO_L as f32 - 1.0, hf, 0.0, tema::BRANCO().com_alfa(120));
        }
        let l = cx;
        // Relógio à direita (uma linha só na barra pequena).
        let (hora, data) = (&self.relogio.1, &self.relogio.2);
        if h < 42 {
            let txt = format!("{hora}  {data}");
            let lx = desenho::medir(12.0, &txt);
            t.texto(l - 12.0 - lx, meio + 4.5, 12.0, tema::TEXTO(), &txt);
        } else {
            let lh = desenho::medir(13.0, hora);
            let ld = desenho::medir(12.0, data);
            t.texto_com_brilho(l - 14.0 - lh, meio - 3.0, 13.0, tema::TEXTO(), hora);
            t.texto(l - 14.0 - ld, meio + 14.0, 12.0, tema::TEXTO(), data);
        }
        t
    }

    fn desenhar_menu(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(MENU_L, MENU_A);
        let (l, a) = (MENU_L as f32, MENU_A as f32);
        // Vidro com tinta mais forte que a da barra: o menu tem muito texto.
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO().com_alfa(185));
        t.contorno(0.5, 0.5, l - 1.0, a - 1.0, 1.0, tema::BORDA().com_alfa(160));

        // Busca: digitar com o menu aberto já filtra.
        let (bx, by, bl, ba) = (16.0, BUSCA_Y as f32, l - 32.0, BUSCA_A as f32);
        t.retangulo(bx, by, bl, ba, ba / 2.0, tema::BORDA());
        t.retangulo(bx + 1.0, by + 1.0, bl - 2.0, ba - 2.0, ba / 2.0 - 1.0, tema::SUPERFICIE());
        if !self.busca.is_empty() {
            t.retangulo(bx + 1.0, by + ba - 3.0, bl - 2.0, 2.0, 1.0, tema::DESTAQUE());
        }
        // Lupa
        let (lx, ly) = (bx + 22.0, by + ba / 2.0 - 2.0);
        t.retangulo(lx - 7.0, ly - 7.0, 14.0, 14.0, 7.0, tema::TEXTO_SECUNDARIO());
        t.retangulo(lx - 5.5, ly - 5.5, 11.0, 11.0, 5.5, tema::SUPERFICIE());
        t.linha(lx + 4.5, ly + 4.5, lx + 9.0, ly + 9.0, 2.0, tema::TEXTO_SECUNDARIO());
        if self.busca.is_empty() {
            t.texto(bx + 42.0, by + 25.0, 13.0, tema::TEXTO_SECUNDARIO(), "Digite para pesquisar aplicativos");
        } else {
            let fim = t.texto(bx + 42.0, by + 25.0, 14.0, tema::TEXTO(), &desenho::encurtar(14.0, &self.busca, bl - 60.0));
            t.retangulo(bx + 43.0 + fim, by + 11.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
        }

        let visiveis = self.apps_visiveis();
        let titulo = if self.busca.is_empty() { "Todos os aplicativos".to_string() } else { format!("Resultados ({})", visiveis.len()) };
        t.texto(20.0, 84.0, 12.0, tema::TEXTO_SECUNDARIO(), &titulo);
        if visiveis.is_empty() {
            let msg = if self.apps.as_ref().map(|a| a.is_empty()).unwrap_or(true) {
                "Nenhum aplicativo instalado."
            } else {
                "Nenhum aplicativo com esse nome."
            };
            t.texto(20.0, (APPS_Y + 26) as f32, 13.0, tema::TEXTO_SECUNDARIO(), msg);
        }
        let max = Self::max_apps();
        for (i, app) in visiveis.iter().skip(self.menu_rolagem).take(max).enumerate() {
            let idx = self.menu_rolagem + i;
            let y = (APPS_Y + i as i32 * APP_A) as f32;
            let sel = idx == self.menu_sel && !self.busca.is_empty();
            if hover == Some(Alvo::App(idx)) || sel {
                t.retangulo(8.0, y, l - 24.0, (APP_A - 4) as f32, 8.0, if sel { tema::DESTAQUE_SUAVE() } else { tema::SUPERFICIE_ALTA().com_alfa(200) });
            }
            if sel {
                t.retangulo(8.0, y + 10.0, 3.0, (APP_A - 24) as f32, 1.5, tema::DESTAQUE());
            }
            self.desenhar_icone(&mut t, Some(app), &app.nome, 18.0, y + 6.0, 28.0);
            let nome = desenho::encurtar(14.0, &app.nome, l - 90.0);
            t.texto(58.0, y + 25.0, 14.0, tema::TEXTO(), &nome);
        }
        // Barra de rolagem fina quando não cabe tudo.
        if visiveis.len() > max && max > 0 {
            let area = (MENU_A - RODAPE_A - APPS_Y - 8) as f32;
            let h = (area * max as f32 / visiveis.len() as f32).max(24.0);
            let topo = APPS_Y as f32 + (area - h) * self.menu_rolagem as f32 / (visiveis.len() - max) as f32;
            t.retangulo(l - 10.0, topo, 4.0, h, 2.0, tema::TEXTO_SECUNDARIO().com_alfa(140));
        }

        // Rodapé: usuário à esquerda, energia à direita.
        let ry = a - RODAPE_A as f32;
        t.retangulo(0.0, ry, l, RODAPE_A as f32, 0.0, tema::SUPERFICIE_ALTA().com_alfa(110));
        t.retangulo(0.0, ry, l, 1.0, 0.0, tema::BORDA());
        let usuario = std::env::var("USER").unwrap_or_else(|_| "você".into());
        let nome_conta = nome_completo(&usuario).unwrap_or_else(|| usuario.clone());
        let fim_usuario = l - 12.0 - 2.0 * (ENERGIA_L + 4) as f32;
        if hover == Some(Alvo::Usuario) {
            t.retangulo(10.0, ry + 12.0, fim_usuario - 14.0, 40.0, 8.0, tema::SUPERFICIE().com_alfa(200));
        }
        if let Some(f) = &self.foto {
            t.imagem(18, (ry + 16.0) as i32, f);
        } else {
            t.retangulo(18.0, ry + 16.0, 32.0, 32.0, 16.0, tema::DESTAQUE());
            let ini = letra(&nome_conta);
            let li = desenho::medir(15.0, &ini);
            t.texto(34.0 - li / 2.0, ry + 37.5, 15.0, tema::BRANCO(), &ini);
        }
        let rotulo_hover = match hover {
            Some(Alvo::Bloquear) => Some("Bloquear (Win+L)"),
            Some(Alvo::Energia) => Some("Energia"),
            _ => None,
        };
        let texto_usuario = rotulo_hover.map(str::to_string).unwrap_or(nome_conta);
        t.texto(60.0, ry + 37.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &texto_usuario, fim_usuario - 70.0));

        for (i, alvo) in [Alvo::Energia, Alvo::Bloquear].iter().enumerate() {
            let x = l - 12.0 - ((i + 1) as i32 * (ENERGIA_L + 4) - 4) as f32;
            let y = ry + 12.0;
            let sobre = hover == Some(*alvo) || (*alvo == Alvo::Energia && self.energia_aberta);
            if sobre {
                t.retangulo(x, y, ENERGIA_L as f32, 40.0, 8.0, tema::SUPERFICIE().com_alfa(220));
            }
            icone_energia(&mut t, *alvo, x + ENERGIA_L as f32 / 2.0, y + 20.0, tema::TEXTO());
        }

        // Opções do botão de energia, numa janelinha acima dele.
        if self.energia_aberta {
            let (px, py) = ((MENU_L - 12 - POP_L) as f32, (MENU_A - RODAPE_A - pop_a() + 4) as f32);
            t.retangulo(px - 1.0, py - 1.0, POP_L as f32 + 2.0, pop_a() as f32 + 2.0, 10.0, tema::BORDA());
            t.retangulo(px, py, POP_L as f32, pop_a() as f32, 9.0, tema::SUPERFICIE());
            for (i, (alvo, nome)) in opcoes_energia().iter().enumerate() {
                let y = py + 6.0 + i as f32 * POP_ITEM as f32;
                let sobre = hover == Some(*alvo);
                if sobre {
                    let cor = if *alvo == Alvo::Desligar { tema::ERRO() } else { tema::DESTAQUE_SUAVE() };
                    t.retangulo(px + 6.0, y, POP_L as f32 - 12.0, (POP_ITEM - 2) as f32, 6.0, cor);
                }
                let cor = if sobre && *alvo == Alvo::Desligar { tema::BRANCO() } else { tema::TEXTO() };
                icone_energia(&mut t, *alvo, px + 26.0, y + POP_ITEM as f32 / 2.0 - 1.0, cor);
                t.texto(px + 48.0, y + 24.0, 13.5, cor, nome);
            }
        }
        t
    }
}

/// Ícones de traço simples do rodapé do menu, centrados em (cx, cy).
fn icone_energia(t: &mut Tela, alvo: Alvo, cx: f32, cy: f32, cor: desenho::Cor) {
    let e = 1.8;
    match alvo {
        Alvo::Desligar | Alvo::Energia => {
            // Arco com abertura em cima + traço vertical.
            arco(t, cx, cy + 1.0, 7.0, 0.32, 1.18, e, cor);
            t.linha(cx, cy - 8.0, cx, cy - 1.0, e, cor);
        }
        Alvo::Reiniciar => {
            arco(t, cx, cy, 7.0, 0.12, 1.0, e, cor);
            // Ponta da seta.
            let (px, py) = (cx + 7.0 * (0.12f32 * std::f32::consts::TAU).cos(), cy - 7.0 * (0.12f32 * std::f32::consts::TAU).sin());
            t.linha(px, py, px - 4.0, py - 1.0, e, cor);
            t.linha(px, py, px + 1.0, py - 4.5, e, cor);
        }
        Alvo::Dormir | Alvo::Hibernar => {
            // Lua crescente: um círculo com outro "mordendo".
            arco(t, cx, cy, 7.0, 0.18, 0.95, e, cor);
            arco(t, cx + 3.5, cy - 2.5, 5.5, 0.62, 1.12, e, cor);
        }
        Alvo::Sair => {
            // Porta + seta saindo.
            t.linha(cx - 6.0, cy - 7.0, cx - 6.0, cy + 7.0, e, cor);
            t.linha(cx - 6.0, cy - 7.0, cx + 1.0, cy - 7.0, e, cor);
            t.linha(cx - 6.0, cy + 7.0, cx + 1.0, cy + 7.0, e, cor);
            t.linha(cx - 2.0, cy, cx + 8.0, cy, e, cor);
            t.linha(cx + 8.0, cy, cx + 4.5, cy - 3.5, e, cor);
            t.linha(cx + 8.0, cy, cx + 4.5, cy + 3.5, e, cor);
        }
        _ => {
            // Cadeado.
            t.retangulo(cx - 6.5, cy - 1.0, 13.0, 9.0, 2.0, cor);
            arco(t, cx, cy - 3.0, 4.0, 0.0, 0.5, e, cor);
            t.linha(cx - 4.0, cy - 3.0, cx - 4.0, cy - 1.0, e, cor);
            t.linha(cx + 4.0, cy - 3.0, cx + 4.0, cy - 1.0, e, cor);
        }
    }
}

/// Arco de circunferência de `ini` a `fim` voltas (0 = direita, 0,25 = em
/// cima, sentido anti-horário), feito com segmentos de reta.
#[allow(clippy::too_many_arguments)]
fn arco(t: &mut Tela, cx: f32, cy: f32, r: f32, ini: f32, fim: f32, esp: f32, cor: desenho::Cor) {
    let passos = 24;
    let tau = std::f32::consts::TAU;
    let mut ant = None;
    for k in 0..=passos {
        let ang = (ini + (fim - ini) * k as f32 / passos as f32) * tau;
        let p = (cx + r * ang.cos(), cy - r * ang.sin());
        if let Some((x0, y0)) = ant {
            t.linha(x0, y0, p.0, p.1, esp, cor);
        }
        ant = Some(p);
    }
}

/// Nome completo do usuário (campo GECOS do /etc/passwd), se houver.
fn nome_completo(usuario: &str) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    let linha = passwd.lines().find(|l| l.split(':').next() == Some(usuario))?;
    let gecos = linha.split(':').nth(4)?.split(',').next()?.trim().to_string();
    (!gecos.is_empty()).then_some(gecos)
}

#[repr(C)]
struct IfAddrs {
    ifa_next: *mut IfAddrs,
    ifa_name: *const std::os::raw::c_char,
    ifa_flags: u32,
    ifa_addr: *const SockAddrIn,
    ifa_netmask: *const SockAddrIn,
    ifa_ifu: *const std::ffi::c_void,
    ifa_data: *const std::ffi::c_void,
}

#[repr(C)]
struct SockAddrIn {
    familia: u16,
    porta: u16,
    endereco: [u8; 4],
}

#[repr(C)]
#[derive(Default)]
struct Tm {
    seg: i32,
    min: i32,
    hora: i32,
    dia: i32,
    mes: i32,
    ano: i32,
    dia_semana: i32,
    dia_ano: i32,
    verao: i32,
    gmtoff: i64,
    zona: usize,
}

extern "C" {
    fn getifaddrs(lista: *mut *mut IfAddrs) -> i32;
    fn freeifaddrs(lista: *mut IfAddrs);
    fn time(t: *mut i64) -> i64;
    fn localtime_r(t: *const i64, saida: *mut Tm) -> *mut Tm;
    fn tzset();
}

/// IPv4 da interface pelo getifaddrs (sem chamar o `ip`).
fn ipv4_de(iface: &str) -> Option<String> {
    let mut lista: *mut IfAddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs preenche a lista, liberada com freeifaddrs abaixo;
    // cada ponteiro é conferido antes de ser lido.
    unsafe {
        if getifaddrs(&mut lista) != 0 {
            return None;
        }
        let mut achado = None;
        let mut p = lista;
        while !p.is_null() {
            let item = &*p;
            if !item.ifa_addr.is_null()
                && !item.ifa_name.is_null()
                && (*item.ifa_addr).familia == 2
                && std::ffi::CStr::from_ptr(item.ifa_name).to_bytes() == iface.as_bytes()
            {
                let [a, b, c, d] = (*item.ifa_addr).endereco;
                achado = Some(format!("{a}.{b}.{c}.{d}"));
                break;
            }
            p = item.ifa_next;
        }
        freeifaddrs(lista);
        achado
    }
}

/// "HH:MM" e "dd/mm/aaaa" no fuso local (localtime_r, sem chamar o `date`).
fn hora_local() -> (String, String) {
    // O fuso mudou (Configurações › Data e hora)? A glibc não relê o
    // /etc/localtime sozinha: aponta o TZ para o arquivo novo.
    static FUSO: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);
    if let Ok(alvo) = std::fs::canonicalize("/etc/localtime") {
        let mut f = FUSO.lock().unwrap();
        if f.as_ref() != Some(&alvo) {
            if f.is_some() {
                std::env::set_var("TZ", format!(":{}", alvo.display()));
            }
            // SAFETY: tzset só relê o fuso da glibc.
            unsafe { tzset() };
            *f = Some(alvo);
        }
    }
    let mut tm = Tm::default();
    // SAFETY: time e localtime_r só escrevem nas variáveis passadas.
    let ok = unsafe {
        let agora = time(std::ptr::null_mut());
        !localtime_r(&agora, &mut tm).is_null()
    };
    if !ok {
        return (String::new(), String::new());
    }
    // Relógio de 12 horas (Configurações › Data e hora).
    let hora = if ler_chave("relogio_12h").as_deref() == Some("sim") {
        let h = if tm.hora % 12 == 0 { 12 } else { tm.hora % 12 };
        format!("{h}:{:02} {}", tm.min, if tm.hora < 12 { "AM" } else { "PM" })
    } else {
        format!("{:02}:{:02}", tm.hora, tm.min)
    };
    (hora, format!("{:02}/{:02}/{}", tm.dia, tm.mes + 1, tm.ano + 1900))
}

/// Conta simples digitada no menu: + - * / ( ), vírgula ou ponto decimal.
/// Só responde se houver pelo menos uma operação.
fn calcular(texto: &str) -> Option<f64> {
    let t: Vec<char> = texto.chars().filter(|c| !c.is_whitespace()).map(|c| match c {
        ',' => '.',
        'x' | '×' => '*',
        '÷' => '/',
        o => o,
    }).collect();
    if t.is_empty() || !t.iter().all(|c| c.is_ascii_digit() || "+-*/().".contains(*c)) {
        return None;
    }
    if !t.iter().skip(1).any(|c| "+-*/".contains(*c)) {
        return None;
    }
    fn expr(t: &[char], i: &mut usize) -> Option<f64> {
        let mut v = termo(t, i)?;
        while *i < t.len() && (t[*i] == '+' || t[*i] == '-') {
            let op = t[*i];
            *i += 1;
            let d = termo(t, i)?;
            v = if op == '+' { v + d } else { v - d };
        }
        Some(v)
    }
    fn termo(t: &[char], i: &mut usize) -> Option<f64> {
        let mut v = fator(t, i)?;
        while *i < t.len() && (t[*i] == '*' || t[*i] == '/') {
            let op = t[*i];
            *i += 1;
            let d = fator(t, i)?;
            v = if op == '*' { v * d } else { v / d };
        }
        Some(v)
    }
    fn fator(t: &[char], i: &mut usize) -> Option<f64> {
        if *i < t.len() && t[*i] == '-' {
            *i += 1;
            return fator(t, i).map(|v| -v);
        }
        if *i < t.len() && t[*i] == '(' {
            *i += 1;
            let v = expr(t, i)?;
            if *i < t.len() && t[*i] == ')' {
                *i += 1;
            }
            return Some(v);
        }
        let ini = *i;
        while *i < t.len() && (t[*i].is_ascii_digit() || t[*i] == '.') {
            *i += 1;
        }
        t[ini..*i].iter().collect::<String>().parse().ok()
    }
    let mut i = 0;
    let v = expr(&t, &mut i)?;
    (i == t.len() && v.is_finite()).then_some(v)
}

/// Tira os acentos mais comuns do português (para a busca).
fn sem_acento(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            outro => outro,
        })
        .collect()
}

/// Lê Name, Exec, Icon e StartupWMClass de um arquivo .desktop
/// (preferindo o nome em pt_BR). `id` é o nome do arquivo sem extensão.
fn ler_desktop(texto: &str, id: &str) -> Option<App> {
    let mut na_secao = false;
    let (mut nome, mut nome_br, mut exec, mut icone, mut classe) = (None, None, None, None, None);
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.starts_with('[') {
            na_secao = linha == "[Desktop Entry]";
            continue;
        }
        if !na_secao {
            continue;
        }
        let Some((k, v)) = linha.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "Name" => nome = Some(v.to_string()),
            "Name[pt_BR]" => nome_br = Some(v.to_string()),
            "Exec" => exec = Some(v.to_string()),
            "Icon" => icone = Some(v.to_string()),
            "StartupWMClass" => classe = Some(v.to_lowercase()),
            "NoDisplay" | "Hidden" if v == "true" => return None,
            "Type" if v != "Application" => return None,
            _ => {}
        }
    }
    let exec = exec?
        .split_whitespace()
        .filter(|p| !(p.len() == 2 && p.starts_with('%')))
        .collect::<Vec<_>>()
        .join(" ");
    let mut ids = vec![id.to_lowercase()];
    ids.extend(classe);
    if let Some(prog) = exec.split_whitespace().next().and_then(|p| p.rsplit('/').next()) {
        ids.push(prog.to_lowercase());
    }
    Some(App {
        nome: nome_br.or(nome)?,
        exec,
        icone: icone.as_deref().and_then(achar_icone),
        ids,
    })
}

/// Procura o ícone (PNG) pelo nome do tema hicolor ou em /usr/share/pixmaps.
fn achar_icone(nome: &str) -> Option<image::RgbaImage> {
    let mut caminhos = Vec::new();
    if nome.starts_with('/') {
        caminhos.push(nome.to_string());
    } else {
        for dados in pastas_de_dados() {
            for t in ["128x128", "256x256", "96x96", "64x64", "512x512", "48x48", "32x32"] {
                caminhos.push(format!("{dados}/icons/hicolor/{t}/apps/{nome}.png"));
            }
        }
        caminhos.push(format!("/usr/share/pixmaps/{nome}.png"));
    }
    caminhos.into_iter().find_map(|c| {
        let bytes = std::fs::read(&c).ok()?;
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
        // Guarda em 64 px: suficiente para a barra e o menu.
        Some(if img.width() > 64 {
            image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Triangle)
        } else {
            img
        })
    })
}

/// Ícone de alto-falante: corpo, cone e ondas conforme o volume (ou um X,
/// se mudo).
fn alto_falante(t: &mut Tela, x: f32, y: f32, s: f32, volume: f32, mudo: bool, cor: desenho::Cor) {
    t.retangulo(x + 0.08 * s, y + 0.36 * s, 0.2 * s, 0.28 * s, 1.5, cor);
    t.poligono(
        &[
            (x + 0.26 * s, y + 0.36 * s),
            (x + 0.5 * s, y + 0.14 * s),
            (x + 0.5 * s, y + 0.86 * s),
            (x + 0.26 * s, y + 0.64 * s),
        ],
        cor,
    );
    let (cx, cy) = (x + 0.5 * s, y + 0.5 * s);
    if mudo {
        let (a, b) = (x + 0.66 * s, x + 0.94 * s);
        t.linha(a, y + 0.36 * s, b, y + 0.64 * s, 0.09 * s, cor);
        t.linha(a, y + 0.64 * s, b, y + 0.36 * s, 0.09 * s, cor);
        return;
    }
    let ondas = if volume <= 0.001 { 0 } else if volume < 0.34 { 1 } else if volume < 0.67 { 2 } else { 3 };
    for k in 0..ondas {
        let r = (0.2 + 0.14 * k as f32) * s;
        let passos = 10;
        let mut ant: Option<(f32, f32)> = None;
        for i in 0..=passos {
            let ang = (-45.0 + 90.0 * i as f32 / passos as f32).to_radians();
            let p = (cx + r * ang.cos(), cy + r * ang.sin());
            if let Some(a) = ant {
                t.linha(a.0, a.1, p.0, p.1, 0.075 * s, cor);
            }
            ant = Some(p);
        }
    }
}

/// Ícone de rede: monitor com cabo (cabeada) ou leque (Wi-Fi); apagado e
/// com X quando sem conexão.
fn icone_rede(t: &mut Tela, x: f32, y: f32, s: f32, rede: &Rede, cor: desenho::Cor) {
    let c = if rede.conectado { cor } else { cor.com_alfa(120) };
    if rede.sem_fio {
        let (cx, cy) = (x + 0.5 * s, y + 0.86 * s);
        t.retangulo(cx - 0.07 * s, cy - 0.07 * s, 0.14 * s, 0.14 * s, 0.07 * s, c);
        for k in 1..=3 {
            let r = 0.24 * s * k as f32;
            let mut ant: Option<(f32, f32)> = None;
            for i in 0..=12 {
                let ang = (-135.0 + 90.0 * i as f32 / 12.0).to_radians();
                let p = (cx + r * ang.cos(), cy + r * ang.sin());
                if let Some(a) = ant {
                    t.linha(a.0, a.1, p.0, p.1, 0.08 * s, c);
                }
                ant = Some(p);
            }
        }
    } else {
        // Monitor com pé (rede cabeada).
        t.contorno(x + 0.1 * s, y + 0.16 * s, 0.8 * s, 0.52 * s, 0.08 * s, c);
        t.linha(x + 0.5 * s, y + 0.68 * s, x + 0.5 * s, y + 0.84 * s, 0.08 * s, c);
        t.linha(x + 0.3 * s, y + 0.86 * s, x + 0.7 * s, y + 0.86 * s, 0.08 * s, c);
    }
    if !rede.conectado {
        let (a, b) = (x + 0.62 * s, x + 0.92 * s);
        t.linha(a, y + 0.02 * s, b, y + 0.32 * s, 0.08 * s, cor);
        t.linha(a, y + 0.32 * s, b, y + 0.02 * s, 0.08 * s, cor);
    }
}

/// Pastas de dados (XDG): a do usuário, as do Flatpak e as do sistema.
fn pastas_de_dados() -> Vec<String> {
    let casa = std::env::var("HOME").unwrap_or_default();
    let mut v = vec![
        format!("{casa}/.local/share"),
        format!("{casa}/.local/share/flatpak/exports/share"),
        "/var/lib/flatpak/exports/share".to_string(),
    ];
    let sistema = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in sistema.split(':').filter(|d| !d.is_empty()) {
        if !v.iter().any(|x| x == d) {
            v.push(d.to_string());
        }
    }
    for d in ["/usr/local/share", "/usr/share"] {
        if !v.iter().any(|x| x == d) {
            v.push(d.to_string());
        }
    }
    v
}
