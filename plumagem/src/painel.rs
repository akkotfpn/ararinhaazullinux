// SPDX-License-Identifier: GPL-3.0-or-later
//! Barra de tarefas e menu iniciar da Plumagem.
//!
//! A barra fica embaixo, com o botão do Mingau (menu iniciar), um botão por
//! janela aberta e o relógio. O menu lista os aplicativos instalados (arquivos
//! .desktop) e tem Sair, Reiniciar e Desligar.

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

/// Altura da barra de tarefas, lida uma vez da configuração.
pub fn altura() -> i32 {
    static ALTURA: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *ALTURA.get_or_init(|| {
        let caminhos = [
            std::env::var("HOME").map(|h| format!("{h}/.config/plumagem/plumagem.conf")).unwrap_or_default(),
            "/etc/plumagem/plumagem.conf".to_string(),
        ];
        for caminho in caminhos {
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
    })
}

const MENU_L: i32 = 380;
const MENU_A: i32 = 470;
const MENU_MARGEM: i32 = 8;
const APPS_Y: i32 = 112;
const APP_A: i32 = 44;
const RODAPE_A: i32 = 64;
const INICIAR_X: i32 = 6;
const TAREFA_MAX: i32 = 220;
const RELOGIO_L: i32 = 120;

static LOGO_PNG: &[u8] = include_bytes!("../resources/logo-64.png");
static MINGAU_PNG: &[u8] = include_bytes!("../resources/mingau-128.png");

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
    Menu,
    /// Ícone de som na bandeja.
    Som,
    /// Barra de volume no painel de som (posição em milésimos).
    SomBarra(u16),
    SomMudo,
    SomPainel,
    /// Ícone de rede na bandeja e o painel dele.
    Rede,
    RedePainel,
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
const SOM_L: i32 = 280;
const SOM_A: i32 = 104;
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
    sel: [SolidColorBuffer; 5],
    /// Bordas da janela sendo redimensionada (para a forma do cursor).
    pub cursor_redim: Option<crate::shell::ResizeEdge>,
    pub som: Som,
    pub som_aberto: bool,
    painel_som: Option<(String, MemoryRenderBuffer)>,
    pub rede: Rede,
    pub rede_aberta: bool,
    painel_rede: Option<(String, MemoryRenderBuffer)>,
    pub apps: Option<Vec<App>>,
    relogio: (u64, String, String),
    barra: Option<(String, MemoryRenderBuffer)>,
    menu: Option<(String, MemoryRenderBuffer)>,
    logo: Option<image::RgbaImage>,
    mingau: Option<image::RgbaImage>,
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
        Painel {
            menu_aberto: false,
            segurando: false,
            ordem: Vec::new(),
            minimizadas: Vec::new(),
            selecao: None,
            sel: Default::default(),
            cursor_redim: None,
            som: Som::default(),
            som_aberto: false,
            painel_som: None,
            rede: Rede::default(),
            rede_aberta: false,
            painel_rede: None,
            apps: None,
            relogio: (0, String::new(), String::new()),
            barra: None,
            menu: None,
            logo: desenho::icone(LOGO_PNG, (altura() - 16).max(16) as u32, (altura() - 16).max(16) as u32),
            mingau: desenho::icone(MINGAU_PNG, 44, 51),
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

    fn tarefas_x() -> i32 {
        INICIAR_X + Self::iniciar_l() + 10
    }

    fn largura_tarefa(&self, largura_tela: i32) -> i32 {
        let n = self.ordem.len().max(1) as i32;
        let livre = largura_tela - Self::tarefas_x() - RELOGIO_L - BANDEJA_L - 8;
        ((livre / n) - 4).clamp(48, TAREFA_MAX)
    }

    fn menu_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (MENU_MARGEM, tela.h - altura() - MENU_MARGEM - MENU_A).into()
    }

    fn max_apps() -> usize {
        ((MENU_A - RODAPE_A - APPS_Y) / APP_A).max(0) as usize
    }

    /// O que está em `p` (coordenadas da tela), se for a interface.
    pub fn alvo(&self, p: Point<f64, Logical>, tela: Size<i32, Logical>) -> Option<Alvo> {
        let (x, y) = (p.x as i32, p.y as i32);
        if self.rede_aberta {
            let o = Self::som_origem(tela);
            if x >= o.x && y >= o.y && x < o.x + SOM_L && y < o.y + SOM_A {
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
                if my >= MENU_A - RODAPE_A + 12 && my < MENU_A - 12 {
                    let bl = (MENU_L - 24) / 3;
                    return Some(match ((mx - 12) / bl).clamp(0, 2) {
                        0 => Alvo::Sair,
                        1 => Alvo::Reiniciar,
                        _ => Alvo::Desligar,
                    });
                }
                if my >= APPS_Y && mx >= 8 && mx < MENU_L - 8 {
                    let i = ((my - APPS_Y) / APP_A) as usize;
                    let n = self.apps.as_ref().map(|a| a.len()).unwrap_or(0).min(Self::max_apps());
                    if i < n {
                        return Some(Alvo::App(i));
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
        let bx = tela.w - RELOGIO_L - BANDEJA_L;
        if (bx..bx + ICONE_BANDEJA).contains(&x) {
            return Some(Alvo::Rede);
        }
        if (bx + ICONE_BANDEJA..bx + BANDEJA_L).contains(&x) {
            return Some(Alvo::Som);
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

    fn som_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (tela.w - SOM_L - 8, tela.h - altura() - 8 - SOM_A).into()
    }

    /// Lê volume e mudo com o wpctl (no máximo a cada 5 s, ou se `agora`).
    pub fn ler_som(&mut self, agora: bool) {
        let seg = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if !agora && seg < self.som.lido_em + 5 {
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
            r.ip = std::process::Command::new("ip")
                .args(["-4", "-o", "addr", "show", "dev", &iface])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| {
                    s.split_whitespace()
                        .skip_while(|p| *p != "inet")
                        .nth(1)
                        .map(|ip| ip.split('/').next().unwrap_or(ip).to_string())
                })
                .unwrap_or_default();
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
        self.rede = r;
    }

    fn desenhar_rede(&self, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(SOM_L, SOM_A);
        let (l, a) = (SOM_L as f32, SOM_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO.com_alfa(175));
        let tipo = if self.rede.sem_fio { "Wi-Fi" } else { "Rede cabeada" };
        t.texto(16.0, 28.0, 13.0, tema::TEXTO, tipo);
        icone_rede(&mut t, 16.0, 44.0, 32.0, &self.rede, tema::TEXTO);
        if self.rede.conectado {
            t.texto(60.0, 60.0, 14.0, tema::TEXTO, "Conectado");
            let det = format!("{} · {}", self.rede.interface, self.rede.ip);
            let det = desenho::encurtar(12.0, &det, l - 76.0);
            t.texto(60.0, 80.0, 12.0, tema::TEXTO_SECUNDARIO, &det);
        } else {
            t.texto(60.0, 60.0, 14.0, tema::TEXTO, "Sem conexão");
            let det = if self.rede.interface.is_empty() {
                "Nenhuma placa de rede encontrada.".to_string()
            } else {
                format!("Verifique o cabo ({}).", self.rede.interface)
            };
            t.texto(60.0, 80.0, 12.0, tema::TEXTO_SECUNDARIO, &desenho::encurtar(12.0, &det, l - 76.0));
        }
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

    pub fn alternar_mudo(&mut self) {
        let _ = std::process::Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
            .status();
        self.ler_som(true);
    }

    fn desenhar_som(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(SOM_L, SOM_A);
        let (l, a) = (SOM_L as f32, SOM_A as f32);
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO.com_alfa(175));
        t.texto(16.0, 28.0, 13.0, tema::TEXTO, "Volume");
        let Some(vol) = self.som.volume else {
            t.texto(16.0, 70.0, 13.0, tema::TEXTO_SECUNDARIO, "Nenhuma saída de som encontrada.");
            return t;
        };
        if hover == Some(Alvo::SomMudo) {
            t.retangulo(12.0, 44.0, 40.0, 40.0, 8.0, tema::BRANCO.com_alfa(150));
        }
        alto_falante(&mut t, 18.0, 50.0, 28.0, vol, self.som.mudo, tema::TEXTO);
        let v = vol.clamp(0.0, 1.0);
        let y = 64.0;
        t.retangulo(SOM_BARRA_X0, y - 2.0, SOM_BARRA_X1 - SOM_BARRA_X0, 4.0, 2.0, tema::TEXTO_SECUNDARIO.com_alfa(90));
        let cor = if self.som.mudo { tema::TEXTO_SECUNDARIO } else { tema::DESTAQUE };
        let xk = SOM_BARRA_X0 + v * (SOM_BARRA_X1 - SOM_BARRA_X0);
        t.retangulo(SOM_BARRA_X0, y - 2.0, xk - SOM_BARRA_X0, 4.0, 2.0, cor);
        let r = if matches!(hover, Some(Alvo::SomBarra(_))) { 9.0 } else { 8.0 };
        t.retangulo(xk - r, y - r, 2.0 * r, 2.0 * r, r, tema::BRANCO);
        t.retangulo(xk - r + 3.0, y - r + 3.0, 2.0 * r - 6.0, 2.0 * r - 6.0, r - 3.0, cor);
        let pct = format!("{}", (vol * 100.0).round() as i32);
        let lp = desenho::medir(13.0, &pct);
        t.texto(l - 18.0 - lp, y + 5.0, 13.0, tema::TEXTO, &pct);
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
        let saida = std::process::Command::new("date")
            .arg("+%H:%M|%d/%m/%Y")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let mut partes = saida.trim().splitn(2, '|');
        let hora = partes.next().unwrap_or("").to_string();
        let data = partes.next().unwrap_or("").to_string();
        self.relogio = (minuto, hora, data);
    }

    /// Lê os aplicativos de /usr/share/applications (uma vez por abertura do menu).
    pub fn carregar_apps(&mut self) {
        let mut apps = Vec::new();
        for pasta in ["/usr/share/applications", "/usr/local/share/applications"] {
            let Ok(dir) = std::fs::read_dir(pasta) else { continue };
            for entrada in dir.flatten() {
                let caminho = entrada.path();
                if caminho.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let id = caminho.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
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
        let mut elementos = Vec::new();

        if self.rede_aberta {
            let o = Self::som_origem(tela);
            let chave = format!("{:?}|{o:?}", self.rede);
            if self.painel_rede.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let t = self.desenhar_rede(o);
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
            let chave = format!("{hover:?}|{:?}", self.apps.as_ref().map(|a| a.len()));
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
            "{}|{hover:?}|{}|{}|{}|{janelas:?}|{:?}|{}",
            tela.w, self.menu_aberto, self.relogio.1, self.relogio.2, self.som, self.som_aberto
        ) + &format!("|{:?}|{}", self.rede, self.rede_aberta);
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
        t.retangulo(x, y, lado, lado, lado * 0.25, tema::DESTAQUE);
        let ini = letra(nome);
        let ti = lado * 0.56;
        let li = desenho::medir(ti, &ini);
        t.texto(x + lado / 2.0 - li / 2.0, y + lado / 2.0 + ti * 0.36, ti, tema::BRANCO, &ini);
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
        t.vidro(-1.0, 0.0, l + 2.0, hf + 1.0, 0.0, (0, altura_tela - h), tema::SUPERFICIE.com_alfa(120));

        let bh = hf - 8.0; // altura dos botões
        let meio = hf / 2.0;

        // Botão iniciar com o Mingau.
        let il = Self::iniciar_l() as f32;
        if self.menu_aberto || hover == Some(Alvo::Iniciar) {
            t.retangulo(INICIAR_X as f32, 4.0, il, bh, 8.0, tema::BRANCO.com_alfa(150));
            t.retangulo(INICIAR_X as f32, 4.0, il, bh, 8.0, tema::DESTAQUE.com_alfa(40));
        }
        if let Some(logo) = &self.logo {
            let lado = logo.width() as i32;
            t.imagem(INICIAR_X + (Self::iniciar_l() - lado) / 2, (h - lado) / 2, logo);
        } else {
            t.retangulo(INICIAR_X as f32 + 6.0, meio - 14.0, 28.0, 28.0, 14.0, tema::DESTAQUE);
        }

        // Um botão por janela.
        let lt = self.largura_tarefa(largura);
        let icone = (hf * 0.5).round().clamp(18.0, 28.0);
        let tam_txt = if h < 42 { 12.0 } else { 13.0 };
        for (i, (nome, ativa, minimizada, app)) in janelas.iter().enumerate() {
            let x = (Self::tarefas_x() + i as i32 * (lt + 4)) as f32;
            if *ativa && !*minimizada {
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::BRANCO.com_alfa(170));
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::DESTAQUE.com_alfa(45));
            } else if hover == Some(Alvo::Tarefa(i)) {
                t.retangulo(x, 4.0, lt as f32, bh, 6.0, tema::BRANCO.com_alfa(120));
            }
            // Indicador embaixo: longo na ativa, curto nas outras.
            let (il, cor) = if *ativa && !*minimizada {
                (20.0, tema::DESTAQUE)
            } else {
                (8.0, tema::TEXTO_SECUNDARIO.com_alfa(150))
            };
            t.retangulo(x + lt as f32 / 2.0 - il / 2.0, hf - 6.0, il, 3.0, 1.5, cor);

            let app = (*app).and_then(|i| self.apps.as_ref()?.get(i));
            self.desenhar_icone(&mut t, app, nome, x + 8.0, meio - icone / 2.0, icone);
            if lt > 60 {
                let txt = desenho::encurtar(tam_txt, nome, lt as f32 - icone - 24.0);
                let cor = if *minimizada { tema::TEXTO_SECUNDARIO } else { tema::TEXTO };
                t.texto_com_brilho(x + icone + 16.0, meio + tam_txt * 0.36, tam_txt, cor, &txt);
            }
        }

        // Bandeja: rede e som.
        let ib = ICONE_BANDEJA as f32;
        let bx = l - (RELOGIO_L + BANDEJA_L) as f32;
        let lado = (hf * 0.45).clamp(16.0, 24.0);
        if self.rede_aberta || hover == Some(Alvo::Rede) {
            t.retangulo(bx + 2.0, 4.0, ib - 4.0, bh, 6.0, tema::BRANCO.com_alfa(150));
        }
        icone_rede(&mut t, bx + (ib - lado) / 2.0, meio - lado / 2.0, lado, &self.rede, tema::TEXTO);
        let bx = bx + ib;
        if self.som_aberto || hover == Some(Alvo::Som) {
            t.retangulo(bx + 2.0, 4.0, ib - 4.0, bh, 6.0, tema::BRANCO.com_alfa(150));
        }
        alto_falante(
            &mut t,
            bx + (ib - lado) / 2.0,
            meio - lado / 2.0,
            lado,
            self.som.volume.unwrap_or(0.0),
            self.som.mudo || self.som.volume.is_none(),
            tema::TEXTO,
        );

        // Relógio à direita (uma linha só na barra pequena).
        let (hora, data) = (&self.relogio.1, &self.relogio.2);
        if h < 42 {
            let txt = format!("{hora}  {data}");
            let lx = desenho::medir(12.0, &txt);
            t.texto(l - 12.0 - lx, meio + 4.5, 12.0, tema::TEXTO, &txt);
        } else {
            let lh = desenho::medir(13.0, hora);
            let ld = desenho::medir(12.0, data);
            t.texto_com_brilho(l - 14.0 - lh, meio - 3.0, 13.0, tema::TEXTO, hora);
            t.texto(l - 14.0 - ld, meio + 14.0, 12.0, tema::TEXTO, data);
        }
        t
    }

    fn desenhar_menu(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(MENU_L, MENU_A);
        let (l, a) = (MENU_L as f32, MENU_A as f32);
        // Vidro com tinta mais forte que a da barra: o menu tem muito texto.
        t.vidro(0.0, 0.0, l, a, 12.0, (origem.x, origem.y), tema::FUNDO.com_alfa(175));

        // Cabeçalho com o Mingau.
        if let Some(m) = &self.mingau {
            t.imagem(16, 12, m);
        }
        t.texto(72.0, 36.0, 17.0, tema::TEXTO, "Ararinha Azul Linux");
        t.texto(72.0, 56.0, 12.5, tema::TEXTO_SECUNDARIO, "O Mingau te ajuda a começar.");
        t.retangulo(12.0, 76.0, l - 24.0, 1.0, 0.0, tema::BORDA);

        t.texto(16.0, 100.0, 12.0, tema::TEXTO_SECUNDARIO, "Aplicativos");
        let apps = self.apps.as_deref().unwrap_or(&[]);
        if apps.is_empty() {
            t.texto(16.0, (APPS_Y + 26) as f32, 13.0, tema::TEXTO_SECUNDARIO, "Nenhum aplicativo instalado.");
        }
        for (i, app) in apps.iter().take(Self::max_apps()).enumerate() {
            let y = (APPS_Y + i as i32 * APP_A) as f32;
            if hover == Some(Alvo::App(i)) {
                t.retangulo(8.0, y, l - 16.0, (APP_A - 4) as f32, 8.0, tema::DESTAQUE_SUAVE);
            }
            self.desenhar_icone(&mut t, Some(app), &app.nome, 16.0, y + 6.0, 28.0);
            let nome = desenho::encurtar(14.0, &app.nome, l - 80.0);
            t.texto(56.0, y + 26.0, 14.0, tema::TEXTO, &nome);
        }

        // Rodapé: Sair, Reiniciar, Desligar.
        let ry = a - RODAPE_A as f32;
        t.retangulo(12.0, ry, l - 24.0, 1.0, 0.0, tema::BORDA);
        let bl = ((MENU_L - 24) / 3) as f32;
        for (i, (alvo, nome)) in [(Alvo::Sair, "Sair"), (Alvo::Reiniciar, "Reiniciar"), (Alvo::Desligar, "Desligar")]
            .iter()
            .enumerate()
        {
            let x = 12.0 + i as f32 * bl;
            let sobre = hover == Some(*alvo);
            if sobre {
                let cor = if *alvo == Alvo::Desligar { tema::ERRO } else { tema::SUPERFICIE_ALTA };
                t.retangulo(x + 2.0, ry + 12.0, bl - 4.0, 40.0, 8.0, cor);
            }
            let cor_txt = match (*alvo, sobre) {
                (Alvo::Desligar, true) => tema::BRANCO,
                (Alvo::Desligar, false) => tema::ERRO,
                _ => tema::TEXTO,
            };
            let lt = desenho::medir(13.0, nome);
            t.texto(x + bl / 2.0 - lt / 2.0, ry + 37.0, 13.0, cor_txt, nome);
        }
        t
    }
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
        for t in ["256x256", "128x128", "96x96", "64x64", "48x48", "32x32"] {
            caminhos.push(format!("/usr/share/icons/hicolor/{t}/apps/{nome}.png"));
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
