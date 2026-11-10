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
            Kind,
        },
        ImportMem, Renderer,
    },
    desktop::Space,
    output::Output,
    utils::{IsAlive, Logical, Point, Size},
    wayland::{compositor, shell::xdg::XdgToplevelSurfaceData},
};

use crate::{
    desenho::{self, tema, Tela},
    shell::WindowElement,
};

/// Altura da barra de tarefas, em pixels lógicos.
pub const ALTURA: i32 = 48;

const MENU_L: i32 = 380;
const MENU_A: i32 = 470;
const MENU_MARGEM: i32 = 8;
const APPS_Y: i32 = 112;
const APP_A: i32 = 44;
const RODAPE_A: i32 = 64;
const INICIAR_X: i32 = 6;
const INICIAR_L: i32 = 40;
const TAREFAS_X: i32 = 56;
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
}

#[derive(Debug, Clone)]
pub struct App {
    pub nome: String,
    pub exec: String,
}

pub struct Painel {
    pub menu_aberto: bool,
    /// Um clique começou na interface: a soltura também é dela.
    pub segurando: bool,
    /// Janelas na ordem em que apareceram (é a ordem dos botões).
    pub ordem: Vec<WindowElement>,
    /// Janelas minimizadas e onde estavam.
    pub minimizadas: Vec<(WindowElement, Point<i32, Logical>)>,
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
            apps: None,
            relogio: (0, String::new(), String::new()),
            barra: None,
            menu: None,
            logo: desenho::icone(LOGO_PNG, 30, 30),
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

    fn largura_tarefa(&self, largura_tela: i32) -> i32 {
        let n = self.ordem.len().max(1) as i32;
        let livre = largura_tela - TAREFAS_X - RELOGIO_L - 8;
        ((livre / n) - 4).clamp(48, TAREFA_MAX)
    }

    fn menu_origem(tela: Size<i32, Logical>) -> Point<i32, Logical> {
        (MENU_MARGEM, tela.h - ALTURA - MENU_MARGEM - MENU_A).into()
    }

    fn max_apps() -> usize {
        ((MENU_A - RODAPE_A - APPS_Y) / APP_A).max(0) as usize
    }

    /// O que está em `p` (coordenadas da tela), se for a interface.
    pub fn alvo(&self, p: Point<f64, Logical>, tela: Size<i32, Logical>) -> Option<Alvo> {
        let (x, y) = (p.x as i32, p.y as i32);
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
        if y < tela.h - ALTURA || y >= tela.h || x < 0 || x >= tela.w {
            return None;
        }
        if (INICIAR_X..INICIAR_X + INICIAR_L).contains(&x) {
            return Some(Alvo::Iniciar);
        }
        let lt = self.largura_tarefa(tela.w);
        if x >= TAREFAS_X {
            let i = ((x - TAREFAS_X) / (lt + 4)) as usize;
            if i < self.ordem.len() && (x - TAREFAS_X) % (lt + 4) < lt {
                return Some(Alvo::Tarefa(i));
            }
        }
        Some(Alvo::Barra)
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
                if let Some(app) = std::fs::read_to_string(&caminho).ok().and_then(|t| ler_desktop(&t)) {
                    apps.push(app);
                }
            }
        }
        apps.sort_by_key(|a| a.nome.to_lowercase());
        self.apps = Some(apps);
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

        let ativa = space.elements().last().cloned();
        let janelas: Vec<(String, bool, bool)> = self
            .ordem
            .iter()
            .map(|w| (titulo(w), Some(w) == ativa.as_ref(), self.esta_minimizada(w)))
            .collect();

        let mut elementos = Vec::new();

        if self.menu_aberto {
            let chave = format!("{hover:?}|{:?}", self.apps.as_ref().map(|a| a.len()));
            if self.menu.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
                let tela_menu = self.desenhar_menu(hover);
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
            "{}|{hover:?}|{}|{}|{}|{janelas:?}",
            tela.w, self.menu_aberto, self.relogio.1, self.relogio.2
        );
        if self.barra.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
            let t = self.desenhar_barra(tela.w, hover, &janelas);
            self.barra = Some((chave, t.em_buffer()));
        }
        if let Some((_, buf)) = &self.barra {
            let pos = Point::<i32, Logical>::from((0, tela.h - ALTURA));
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

    fn desenhar_barra(&self, largura: i32, hover: Option<Alvo>, janelas: &[(String, bool, bool)]) -> Tela {
        let mut t = Tela::nova(largura, ALTURA);
        let l = largura as f32;
        t.retangulo(0.0, 0.0, l, ALTURA as f32, 0.0, tema::SUPERFICIE.com_alfa(225));
        t.retangulo(0.0, 0.0, l, 1.0, 0.0, tema::BORDA);

        // Botão iniciar com o Mingau.
        if self.menu_aberto || hover == Some(Alvo::Iniciar) {
            t.retangulo(INICIAR_X as f32, 4.0, INICIAR_L as f32, 40.0, 8.0, tema::DESTAQUE_SUAVE);
        }
        if let Some(logo) = &self.logo {
            t.imagem(INICIAR_X + 5, 9, logo);
        } else {
            t.retangulo(INICIAR_X as f32 + 6.0, 10.0, 28.0, 28.0, 14.0, tema::DESTAQUE);
        }

        // Um botão por janela.
        let lt = self.largura_tarefa(largura);
        for (i, (nome, ativa, minimizada)) in janelas.iter().enumerate() {
            let x = (TAREFAS_X + i as i32 * (lt + 4)) as f32;
            if *ativa && !*minimizada {
                t.retangulo(x, 4.0, lt as f32, 40.0, 6.0, tema::DESTAQUE_SUAVE);
            } else if hover == Some(Alvo::Tarefa(i)) {
                t.retangulo(x, 4.0, lt as f32, 40.0, 6.0, tema::SUPERFICIE_ALTA);
            }
            // Indicador embaixo: longo na ativa, curto nas outras.
            let (il, cor) = if *ativa && !*minimizada {
                (20.0, tema::DESTAQUE)
            } else {
                (8.0, tema::TEXTO_SECUNDARIO.com_alfa(150))
            };
            t.retangulo(x + lt as f32 / 2.0 - il / 2.0, 41.0, il, 3.0, 1.5, cor);

            // Ícone provisório: a inicial do programa num quadrado azul.
            t.retangulo(x + 8.0, 12.0, 24.0, 24.0, 6.0, tema::DESTAQUE);
            let ini = letra(nome);
            let li = desenho::medir(14.0, &ini);
            t.texto(x + 20.0 - li / 2.0, 29.0, 14.0, tema::BRANCO, &ini);
            if lt > 60 {
                let txt = desenho::encurtar(13.0, nome, lt as f32 - 48.0);
                let cor = if *minimizada { tema::TEXTO_SECUNDARIO } else { tema::TEXTO };
                t.texto(x + 40.0, 29.0, 13.0, cor, &txt);
            }
        }

        // Relógio à direita.
        let (hora, data) = (&self.relogio.1, &self.relogio.2);
        let lh = desenho::medir(13.0, hora);
        let ld = desenho::medir(12.0, data);
        t.texto(l - 14.0 - lh, 21.0, 13.0, tema::TEXTO, hora);
        t.texto(l - 14.0 - ld, 38.0, 12.0, tema::TEXTO_SECUNDARIO, data);
        t
    }

    fn desenhar_menu(&self, hover: Option<Alvo>) -> Tela {
        let mut t = Tela::nova(MENU_L, MENU_A);
        let (l, a) = (MENU_L as f32, MENU_A as f32);
        t.retangulo(0.0, 0.0, l, a, 12.0, tema::BORDA);
        t.retangulo(1.0, 1.0, l - 2.0, a - 2.0, 11.0, tema::FUNDO);

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
            t.retangulo(16.0, y + 6.0, 28.0, 28.0, 7.0, tema::DESTAQUE);
            let ini = letra(&app.nome);
            let li = desenho::medir(15.0, &ini);
            t.texto(30.0 - li / 2.0, y + 26.0, 15.0, tema::BRANCO, &ini);
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

/// Lê Name e Exec de um arquivo .desktop (preferindo o nome em pt_BR).
fn ler_desktop(texto: &str) -> Option<App> {
    let mut na_secao = false;
    let (mut nome, mut nome_br, mut exec) = (None, None, None);
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
        match k.trim() {
            "Name" => nome = Some(v.trim().to_string()),
            "Name[pt_BR]" => nome_br = Some(v.trim().to_string()),
            "Exec" => exec = Some(v.trim().to_string()),
            "NoDisplay" | "Hidden" if v.trim() == "true" => return None,
            "Type" if v.trim() != "Application" => return None,
            _ => {}
        }
    }
    let exec = exec?
        .split_whitespace()
        .filter(|p| !(p.len() == 2 && p.starts_with('%')))
        .collect::<Vec<_>>()
        .join(" ");
    Some(App { nome: nome_br.or(nome)?, exec })
}
