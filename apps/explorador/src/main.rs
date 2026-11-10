// SPDX-License-Identifier: GPL-3.0-or-later
//! Explorador de Arquivos do Ararinha.
//!
//! Por dentro é Linux; por fora fala a língua de quem vem do Windows:
//! "Este Computador", Disco Local (C:), unidades D:, E:..., pastas do
//! usuário com os nomes de sempre e navegação com Voltar, Avançar e Subir.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use penas::{desenho, tema, App, Cor, Opcoes, Tecla, Tela, Ui};

const BARRA_A: f32 = 52.0;
const LATERAL_L: f32 = 210.0;
const LINHA_A: f32 = 30.0;

/// Pastas do usuário, com o nome em português que aparece no disco.
const PASTAS_USUARIO: [(&str, &str); 6] = [
    ("Área de Trabalho", "Área de Trabalho"),
    ("Documentos", "Documentos"),
    ("Downloads", "Downloads"),
    ("Imagens", "Imagens"),
    ("Músicas", "Músicas"),
    ("Vídeos", "Vídeos"),
];

#[derive(Debug, Clone, PartialEq)]
enum Local {
    EsteComputador,
    Pasta(PathBuf),
}

#[derive(Debug, Clone)]
struct Unidade {
    letra: char,
    nome: String,
    ponto: PathBuf,
    total: u64,
    livre: u64,
}

#[derive(Debug, Clone)]
struct Entrada {
    nome: String,
    caminho: PathBuf,
    pasta: bool,
    tamanho: u64,
    modificado: Option<SystemTime>,
}

struct Explorador {
    local: Local,
    voltar: Vec<Local>,
    avancar: Vec<Local>,
    entradas: Vec<Entrada>,
    unidades: Vec<Unidade>,
    selecionado: Option<usize>,
    rolagem: f32,
    aviso: Option<String>,
    casa: PathBuf,
    mostrar_ocultos: bool,
}

/// Lê as unidades: a raiz é C:, as outras partições montadas ganham D:, E:...
fn ler_unidades() -> Vec<Unidade> {
    let montagens = fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut pontos: Vec<(String, PathBuf)> = vec![("Disco Local".into(), PathBuf::from("/"))];
    for linha in montagens.lines() {
        let c: Vec<&str> = linha.split_whitespace().collect();
        if c.len() < 3 || !c[0].starts_with("/dev/") {
            continue;
        }
        let ponto = c[1].replace("\\040", " ");
        if ponto == "/" || ponto.starts_with("/boot") || ponto.starts_with("/efi") || ponto.starts_with("/sysroot") {
            continue;
        }
        let nome = Path::new(&ponto)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unidade".into());
        if !pontos.iter().any(|(_, p)| p == Path::new(&ponto)) {
            pontos.push((nome, PathBuf::from(ponto)));
        }
    }
    pontos
        .into_iter()
        .enumerate()
        .map(|(i, (nome, ponto))| {
            let (total, livre) = espaco(&ponto);
            Unidade { letra: (b'C' + i as u8) as char, nome, ponto, total, livre }
        })
        .collect()
}

/// Tamanho total e livre (em bytes) de uma unidade, pelo `df`.
fn espaco(ponto: &Path) -> (u64, u64) {
    let saida = Command::new("df")
        .args(["-B1", "--output=size,avail"])
        .arg(ponto)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let n: Vec<u64> = saida
        .lines()
        .nth(1)
        .map(|l| l.split_whitespace().filter_map(|v| v.parse().ok()).collect())
        .unwrap_or_default();
    (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0))
}

fn tamanho_legivel(b: u64) -> String {
    const U: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} bytes")
    } else {
        format!("{:.1} {}", v, U[i]).replace('.', ",")
    }
}

/// Deslocamento do fuso local em segundos (lido uma vez, com o `date`).
fn fuso() -> i64 {
    static FUSO: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *FUSO.get_or_init(|| {
        let z = Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let z = z.trim();
        if z.len() != 5 {
            return 0;
        }
        let sinal = if z.starts_with('-') { -1 } else { 1 };
        let h: i64 = z[1..3].parse().unwrap_or(0);
        let m: i64 = z[3..5].parse().unwrap_or(0);
        sinal * (h * 3600 + m * 60)
    })
}

/// Data e hora local no formato dd/mm/aaaa hh:mm.
fn data_legivel(t: SystemTime) -> String {
    let s = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + fuso();
    let (dias, resto) = (s.div_euclid(86400), s.rem_euclid(86400));
    // Conversão de dias desde 1970 para data civil (algoritmo de H. Hinnant).
    let z = dias + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + if mes <= 2 { 1 } else { 0 };
    format!("{dia:02}/{mes:02}/{ano} {:02}:{:02}", resto / 3600, (resto % 3600) / 60)
}

impl Explorador {
    fn novo() -> Explorador {
        let casa = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"));
        for (_, pasta) in PASTAS_USUARIO {
            let _ = fs::create_dir_all(casa.join(pasta));
        }
        let mut e = Explorador {
            local: Local::EsteComputador,
            voltar: Vec::new(),
            avancar: Vec::new(),
            entradas: Vec::new(),
            unidades: Vec::new(),
            selecionado: None,
            rolagem: 0.0,
            aviso: None,
            casa,
            mostrar_ocultos: false,
        };
        // Abre numa pasta passada como argumento, se houver.
        if let Some(p) = std::env::args().nth(1).map(PathBuf::from).filter(|p| p.is_dir()) {
            e.local = Local::Pasta(p);
        }
        e.recarregar();
        e
    }

    fn recarregar(&mut self) {
        self.selecionado = None;
        self.rolagem = 0.0;
        match &self.local {
            Local::EsteComputador => {
                self.unidades = ler_unidades();
                self.entradas.clear();
            }
            Local::Pasta(p) => {
                let mut v: Vec<Entrada> = match fs::read_dir(p) {
                    Ok(d) => d
                        .flatten()
                        .filter_map(|e| {
                            let nome = e.file_name().to_string_lossy().to_string();
                            if !self.mostrar_ocultos && nome.starts_with('.') {
                                return None;
                            }
                            let meta = e.metadata().ok();
                            let caminho = e.path();
                            Some(Entrada {
                                pasta: caminho.is_dir(),
                                tamanho: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                                modificado: meta.and_then(|m| m.modified().ok()),
                                nome,
                                caminho,
                            })
                        })
                        .collect(),
                    Err(e) => {
                        self.aviso = Some(format!("Não deu para abrir a pasta: {e}"));
                        Vec::new()
                    }
                };
                v.sort_by(|a, b| b.pasta.cmp(&a.pasta).then(a.nome.to_lowercase().cmp(&b.nome.to_lowercase())));
                self.entradas = v;
            }
        }
    }

    fn ir(&mut self, novo: Local) {
        if novo == self.local {
            return;
        }
        let antigo = std::mem::replace(&mut self.local, novo);
        self.voltar.push(antigo);
        self.avancar.clear();
        self.aviso = None;
        self.recarregar();
    }

    fn ir_voltar(&mut self) {
        if let Some(l) = self.voltar.pop() {
            let atual = std::mem::replace(&mut self.local, l);
            self.avancar.push(atual);
            self.recarregar();
        }
    }

    fn ir_avancar(&mut self) {
        if let Some(l) = self.avancar.pop() {
            let atual = std::mem::replace(&mut self.local, l);
            self.voltar.push(atual);
            self.recarregar();
        }
    }

    fn subir(&mut self) {
        if let Local::Pasta(p) = &self.local {
            let destino = match p.parent() {
                Some(pai) => Local::Pasta(pai.to_path_buf()),
                None => Local::EsteComputador,
            };
            self.ir(destino);
        }
    }

    /// Unidade que contém o caminho (a de ponto de montagem mais longo).
    fn unidade_de(&self, p: &Path) -> Option<&Unidade> {
        self.unidades
            .iter()
            .filter(|u| p.starts_with(&u.ponto))
            .max_by_key(|u| u.ponto.as_os_str().len())
    }

    /// Partes do caminho para a barra de endereço, no jeito do Windows.
    fn migalhas(&self) -> Vec<(String, Local)> {
        let mut v = vec![("Este Computador".to_string(), Local::EsteComputador)];
        if let Local::Pasta(p) = &self.local {
            if self.unidades.is_empty() {
                return v;
            }
            let u = self.unidade_de(p).cloned().unwrap_or_else(|| self.unidades[0].clone());
            v.push((format!("{} ({}:)", u.nome, u.letra), Local::Pasta(u.ponto.clone())));
            let mut atual = u.ponto.clone();
            if let Ok(resto) = p.strip_prefix(&u.ponto) {
                for parte in resto.components() {
                    atual.push(parte);
                    v.push((parte.as_os_str().to_string_lossy().to_string(), Local::Pasta(atual.clone())));
                }
            }
        }
        v
    }

    fn titulo(&self) -> String {
        match &self.local {
            Local::EsteComputador => "Este Computador".into(),
            Local::Pasta(p) => {
                if p == Path::new("/") {
                    "Disco Local (C:)".into()
                } else {
                    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                }
            }
        }
    }

    fn abrir(&mut self, e: &Entrada) {
        if e.pasta {
            self.ir(Local::Pasta(e.caminho.clone()));
        } else {
            self.aviso = Some(format!("Abrir \"{}\" chega em breve: os apps padrão estão a caminho.", e.nome));
        }
    }
}

// ---------------------------------------------------------------- ícones

/// Ícones do tema (arte/icones), embutidos e reduzidos sob demanda.
fn icone_png(nome: &'static str, lado: u32) -> Option<image::RgbaImage> {
    use std::{cell::RefCell, collections::HashMap};
    thread_local! {
        static CACHE: RefCell<HashMap<(&'static str, u32), Option<image::RgbaImage>>> = RefCell::new(HashMap::new());
    }
    let png: &[u8] = match nome {
        "pasta" => include_bytes!("../recursos/pasta.png"),
        "este-computador" => include_bytes!("../recursos/este-computador.png"),
        "unidade-sistema" => include_bytes!("../recursos/unidade-sistema.png"),
        "unidade-disco" => include_bytes!("../recursos/unidade-disco.png"),
        "unidade-rede" => include_bytes!("../recursos/unidade-rede.png"),
        _ => return None,
    };
    CACHE.with(|c| c.borrow_mut().entry((nome, lado)).or_insert_with(|| desenho::icone(png, lado, lado)).clone())
}

fn desenhar_icone(t: &mut Tela, nome: &'static str, x: f32, y: f32, s: f32) -> bool {
    match icone_png(nome, s.round() as u32) {
        Some(img) => {
            t.imagem(x.round() as i32, y.round() as i32, &img);
            true
        }
        None => false,
    }
}

fn icone_pasta(t: &mut Tela, x: f32, y: f32, s: f32) {
    if desenhar_icone(t, "pasta", x, y, s) {
        return;
    }
    let aba = Cor::hex(0xE0A93A);
    let corpo = Cor::hex(0xF5C451);
    t.retangulo(x + 0.05 * s, y + 0.18 * s, 0.42 * s, 0.2 * s, 0.06 * s, aba);
    t.retangulo(x + 0.05 * s, y + 0.28 * s, 0.9 * s, 0.6 * s, 0.08 * s, aba);
    t.retangulo(x + 0.05 * s, y + 0.36 * s, 0.9 * s, 0.52 * s, 0.08 * s, corpo);
}

fn icone_arquivo(t: &mut Tela, x: f32, y: f32, s: f32) {
    let borda = Cor::hex(0x9AA8B6);
    t.retangulo(x + 0.18 * s, y + 0.08 * s, 0.64 * s, 0.84 * s, 0.06 * s, borda);
    t.retangulo(x + 0.18 * s + 1.0, y + 0.08 * s + 1.0, 0.64 * s - 2.0, 0.84 * s - 2.0, 0.05 * s, tema::BRANCO);
    for k in 0..3 {
        let ly = y + (0.38 + 0.14 * k as f32) * s;
        t.linha(x + 0.3 * s, ly, x + 0.7 * s, ly, 0.05 * s, borda.com_alfa(160));
    }
}

fn icone_unidade(t: &mut Tela, x: f32, y: f32, s: f32, sistema: bool) {
    if desenhar_icone(t, if sistema { "unidade-sistema" } else { "unidade-disco" }, x, y, s) {
        return;
    }
    t.retangulo(x + 0.05 * s, y + 0.3 * s, 0.9 * s, 0.42 * s, 0.08 * s, Cor::hex(0x6B7C8F));
    t.retangulo(x + 0.08 * s, y + 0.33 * s, 0.84 * s, 0.3 * s, 0.06 * s, Cor::hex(0xC9D3DD));
    t.retangulo(x + 0.75 * s, y + 0.62 * s, 0.1 * s, 0.05 * s, 0.02 * s, Cor::hex(0x3FBF6B));
    if sistema {
        // Bandeirinha azul: é a unidade do sistema.
        t.retangulo(x + 0.14 * s, y + 0.4 * s, 0.16 * s, 0.16 * s, 0.03 * s, tema::DESTAQUE);
    }
}

fn icone_computador(t: &mut Tela, x: f32, y: f32, s: f32) {
    if desenhar_icone(t, "este-computador", x, y, s) {
        return;
    }
    t.retangulo(x + 0.08 * s, y + 0.16 * s, 0.84 * s, 0.54 * s, 0.06 * s, Cor::hex(0x3A4A5C));
    t.retangulo(x + 0.12 * s, y + 0.2 * s, 0.76 * s, 0.46 * s, 0.04 * s, tema::DESTAQUE);
    t.retangulo(x + 0.42 * s, y + 0.7 * s, 0.16 * s, 0.1 * s, 0.0, Cor::hex(0x3A4A5C));
    t.retangulo(x + 0.28 * s, y + 0.8 * s, 0.44 * s, 0.06 * s, 0.03 * s, Cor::hex(0x3A4A5C));
}

fn seta(t: &mut Tela, cx: f32, cy: f32, dir: f32, cor: Cor) {
    // dir: 0 = esquerda, 1 = direita, 2 = cima
    let (a, b, c) = match dir as i32 {
        0 => ((cx + 4.0, cy - 6.0), (cx - 3.0, cy), (cx + 4.0, cy + 6.0)),
        1 => ((cx - 4.0, cy - 6.0), (cx + 3.0, cy), (cx - 4.0, cy + 6.0)),
        _ => ((cx - 6.0, cy + 4.0), (cx, cy - 3.0), (cx + 6.0, cy + 4.0)),
    };
    t.linha(a.0, a.1, b.0, b.1, 1.8, cor);
    t.linha(b.0, b.1, c.0, c.1, 1.8, cor);
}

// ---------------------------------------------------------------- desenho

impl App for Explorador {
    fn desenhar(&mut self, ui: &mut Ui) {
        // Teclado.
        for tecla in std::mem::take(&mut ui.teclas) {
            match tecla {
                Tecla::Voltar | Tecla::Apagar => self.ir_voltar(),
                Tecla::Avancar => self.ir_avancar(),
                Tecla::Subir => self.subir(),
                Tecla::Baixo => {
                    let n = self.entradas.len();
                    if n > 0 {
                        self.selecionado = Some(self.selecionado.map(|i| (i + 1).min(n - 1)).unwrap_or(0));
                    }
                }
                Tecla::Cima => {
                    self.selecionado = self.selecionado.map(|i| i.saturating_sub(1));
                }
                Tecla::Enter => {
                    if let Some(e) = self.selecionado.and_then(|i| self.entradas.get(i)).cloned() {
                        self.abrir(&e);
                    }
                }
                Tecla::Texto(t) if t == "h" && ui.ctrl => {
                    self.mostrar_ocultos = !self.mostrar_ocultos;
                    self.recarregar();
                }
                _ => {}
            }
        }
        ui.titulo = Some(self.titulo());

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::SUPERFICIE);

        self.desenhar_lateral(ui, a);
        self.desenhar_barra(ui, l);

        let (cx, cy) = (LATERAL_L, BARRA_A);
        let (cl, ca) = (l - LATERAL_L, a - BARRA_A);
        match self.local.clone() {
            Local::EsteComputador => self.desenhar_este_computador(ui, cx, cy, cl),
            Local::Pasta(_) => self.desenhar_pasta(ui, cx, cy, cl, ca),
        }

        // Aviso na parte de baixo.
        if let Some(aviso) = self.aviso.clone() {
            let y = a - 40.0;
            ui.tela.retangulo(cx + 12.0, y, cl - 24.0, 30.0, 8.0, tema::DESTAQUE_SUAVE);
            let txt = desenho::encurtar(13.0, &aviso, cl - 80.0);
            ui.tela.texto(cx + 24.0, y + 20.0, 13.0, tema::TEXTO, &txt);
            let fx = cx + cl - 40.0;
            let cor = if ui.sobre(fx, y, 24.0, 30.0) { tema::DESTAQUE } else { tema::TEXTO_SECUNDARIO };
            ui.tela.linha(fx + 7.0, y + 10.0, fx + 17.0, y + 20.0, 1.5, cor);
            ui.tela.linha(fx + 7.0, y + 20.0, fx + 17.0, y + 10.0, 1.5, cor);
            if ui.clicou(fx, y, 24.0, 30.0) {
                self.aviso = None;
            }
        }
    }
}

impl Explorador {
    fn desenhar_barra(&mut self, ui: &mut Ui, l: f32) {
        let t = &mut *ui.tela;
        t.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO);
        t.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA);
        let botoes = [
            (0.0, !self.voltar.is_empty()),
            (1.0, !self.avancar.is_empty()),
            (2.0, matches!(self.local, Local::Pasta(_))),
        ];
        let mut acao = None;
        for (i, (dir, ativo)) in botoes.iter().enumerate() {
            let x = 10.0 + i as f32 * 36.0;
            let sobre = *ativo && ui.sobre(x, 10.0, 32.0, 32.0);
            if sobre {
                ui.tela.retangulo(x, 10.0, 32.0, 32.0, 6.0, tema::SUPERFICIE_ALTA);
            }
            let cor = if *ativo { tema::TEXTO } else { tema::BORDA };
            seta(ui.tela, x + 16.0, 26.0, *dir, cor);
            if *ativo && ui.clicou(x, 10.0, 32.0, 32.0) {
                acao = Some(i);
            }
        }
        match acao {
            Some(0) => self.ir_voltar(),
            Some(1) => self.ir_avancar(),
            Some(2) => self.subir(),
            _ => {}
        }

        // Barra de endereço com as "migalhas" clicáveis.
        let ex = 122.0;
        let el = l - ex - 12.0;
        ui.tela.retangulo(ex, 10.0, el, 32.0, 6.0, tema::BORDA);
        ui.tela.retangulo(ex + 1.0, 11.0, el - 2.0, 30.0, 5.0, tema::BRANCO);
        icone_computador(ui.tela, ex + 8.0, 14.0, 22.0);
        let mut x = ex + 36.0;
        let migalhas = self.migalhas();
        let mut ir = None;
        for (i, (nome, destino)) in migalhas.iter().enumerate() {
            if i > 0 {
                seta(ui.tela, x + 4.0, 26.0, 1.0, tema::TEXTO_SECUNDARIO);
                x += 14.0;
            }
            let lt = desenho::medir(13.0, nome);
            if x + lt > ex + el - 10.0 {
                ui.tela.texto(x, 31.0, 13.0, tema::TEXTO_SECUNDARIO, "…");
                break;
            }
            if ui.sobre(x - 4.0, 13.0, lt + 8.0, 26.0) {
                ui.tela.retangulo(x - 4.0, 14.0, lt + 8.0, 24.0, 4.0, tema::DESTAQUE_SUAVE);
            }
            ui.tela.texto(x, 31.0, 13.0, tema::TEXTO, nome);
            if ui.clicou(x - 4.0, 13.0, lt + 8.0, 26.0) {
                ir = Some(destino.clone());
            }
            x += lt + 8.0;
        }
        if let Some(d) = ir {
            self.ir(d);
        }
    }

    fn desenhar_lateral(&mut self, ui: &mut Ui, a: f32) {
        ui.tela.retangulo(0.0, BARRA_A, LATERAL_L, a - BARRA_A, 0.0, tema::FUNDO);
        ui.tela.retangulo(LATERAL_L - 1.0, BARRA_A, 1.0, a - BARRA_A, 0.0, tema::BORDA);
        let mut itens: Vec<(String, Local, u8)> = vec![("Este Computador".into(), Local::EsteComputador, 0)];
        for (nome, pasta) in PASTAS_USUARIO {
            itens.push((nome.into(), Local::Pasta(self.casa.join(pasta)), 1));
        }
        itens.push(("Pasta pessoal".into(), Local::Pasta(self.casa.clone()), 1));
        let mut y = BARRA_A + 10.0;
        let mut ir = None;
        for (i, (nome, destino, tipo)) in itens.iter().enumerate() {
            if i == 1 {
                ui.tela.texto(16.0, y + 18.0, 11.5, tema::TEXTO_SECUNDARIO, "Acesso rápido");
                y += 26.0;
            }
            let ativo = &self.local == destino;
            if ativo {
                ui.tela.retangulo(8.0, y, LATERAL_L - 16.0, 30.0, 6.0, tema::DESTAQUE_SUAVE);
            } else if ui.sobre(8.0, y, LATERAL_L - 16.0, 30.0) {
                ui.tela.retangulo(8.0, y, LATERAL_L - 16.0, 30.0, 6.0, tema::SUPERFICIE_ALTA);
            }
            if *tipo == 0 {
                icone_computador(ui.tela, 16.0, y + 4.0, 22.0);
            } else {
                icone_pasta(ui.tela, 16.0, y + 4.0, 22.0);
            }
            ui.tela.texto(46.0, y + 20.0, 13.0, tema::TEXTO, nome);
            if ui.clicou(8.0, y, LATERAL_L - 16.0, 30.0) {
                ir = Some(destino.clone());
            }
            y += 32.0;
        }
        if let Some(d) = ir {
            self.ir(d);
        }
    }

    fn desenhar_este_computador(&mut self, ui: &mut Ui, x0: f32, y0: f32, l: f32) {
        let mut y = y0 + 34.0;
        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO, "Pastas");
        y += 14.0;
        let mut ir = None;
        let tile = 150.0;
        let por_linha = ((l - 48.0) / (tile + 10.0)).floor().max(1.0) as usize;
        for (i, (nome, pasta)) in PASTAS_USUARIO.iter().enumerate() {
            let (c, r) = (i % por_linha, i / por_linha);
            let (x, ty) = (x0 + 24.0 + c as f32 * (tile + 10.0), y + r as f32 * 70.0);
            if ui.sobre(x, ty, tile, 62.0) {
                ui.tela.retangulo(x, ty, tile, 62.0, 8.0, tema::DESTAQUE_SUAVE);
            }
            icone_pasta(ui.tela, x + 8.0, ty + 11.0, 40.0);
            let n = desenho::encurtar(13.0, nome, tile - 60.0);
            ui.tela.texto(x + 54.0, ty + 36.0, 13.0, tema::TEXTO, &n);
            if ui.clicou_duplo(x, ty, tile, 62.0) {
                ir = Some(Local::Pasta(self.casa.join(pasta)));
            }
        }
        let linhas = PASTAS_USUARIO.len().div_ceil(por_linha);
        y += linhas as f32 * 70.0 + 30.0;

        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO, "Dispositivos e unidades");
        y += 14.0;
        let ul = 300.0;
        let por_linha = ((l - 48.0) / (ul + 12.0)).floor().max(1.0) as usize;
        for (i, u) in self.unidades.iter().enumerate() {
            let (c, r) = (i % por_linha, i / por_linha);
            let (x, uy) = (x0 + 24.0 + c as f32 * (ul + 12.0), y + r as f32 * 84.0);
            if ui.sobre(x, uy, ul, 76.0) {
                ui.tela.retangulo(x, uy, ul, 76.0, 8.0, tema::DESTAQUE_SUAVE);
            }
            icone_unidade(ui.tela, x + 8.0, uy + 12.0, 52.0, u.letra == 'C');
            let nome = format!("{} ({}:)", u.nome, u.letra);
            ui.tela.texto(x + 70.0, uy + 24.0, 13.5, tema::TEXTO, &desenho::encurtar(13.5, &nome, ul - 80.0));
            let bl = ul - 86.0;
            ui.tela.retangulo(x + 70.0, uy + 34.0, bl, 12.0, 2.0, tema::SUPERFICIE_ALTA);
            if u.total > 0 {
                let usado = 1.0 - u.livre as f32 / u.total as f32;
                let cor = if usado > 0.9 { tema::ERRO } else { tema::DESTAQUE };
                ui.tela.retangulo(x + 70.0, uy + 34.0, bl * usado, 12.0, 2.0, cor);
                let txt = format!("{} livres de {}", tamanho_legivel(u.livre), tamanho_legivel(u.total));
                ui.tela.texto(x + 70.0, uy + 64.0, 12.0, tema::TEXTO_SECUNDARIO, &txt);
            }
            if ui.clicou_duplo(x, uy, ul, 76.0) {
                ir = Some(Local::Pasta(u.ponto.clone()));
            }
        }
        let linhas = self.unidades.len().div_ceil(por_linha);
        y += linhas as f32 * 84.0 + 30.0;

        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO, "Locais de rede");
        y += 14.0;
        desenhar_icone(ui.tela, "unidade-rede", x0 + 24.0, y + 2.0, 40.0);
        let bx = x0 + 74.0;
        let bl = 230.0;
        let sobre = ui.sobre(bx, y, bl, 40.0);
        ui.tela.retangulo(bx, y, bl, 40.0, 8.0, if sobre { tema::DESTAQUE_FORTE } else { tema::DESTAQUE });
        ui.tela.texto(bx + 18.0, y + 25.0, 13.5, tema::BRANCO, "Mapear unidade de rede…");
        if ui.clicou(bx, y, bl, 40.0) {
            self.aviso = Some("Mapear unidade de rede (SMB/NFS) chega na próxima versão do Explorador.".into());
        }
        if let Some(d) = ir {
            self.ir(d);
        }
    }

    fn desenhar_pasta(&mut self, ui: &mut Ui, x0: f32, y0: f32, l: f32, a: f32) {
        // Cabeçalho das colunas.
        let col_data = x0 + l - 330.0;
        let col_tam = x0 + l - 140.0;
        ui.tela.retangulo(x0, y0, l, 30.0, 0.0, tema::SUPERFICIE);
        ui.tela.texto(x0 + 52.0, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Nome");
        if l > 520.0 {
            ui.tela.texto(col_data, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Data de modificação");
            ui.tela.texto(col_tam, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Tamanho");
        }
        ui.tela.retangulo(x0 + 12.0, y0 + 29.0, l - 24.0, 1.0, 0.0, tema::BORDA);

        let topo = y0 + 34.0;
        let visivel = a - 34.0;
        let total = self.entradas.len() as f32 * LINHA_A;
        self.rolagem = (self.rolagem + ui.rolagem * 2.0).clamp(0.0, (total - visivel + 40.0).max(0.0));
        // Mantém o item selecionado (teclado) à vista.
        if let Some(i) = self.selecionado {
            let iy = i as f32 * LINHA_A;
            if iy < self.rolagem {
                self.rolagem = iy;
            } else if iy + LINHA_A > self.rolagem + visivel {
                self.rolagem = iy + LINHA_A - visivel;
            }
        }

        if self.entradas.is_empty() {
            ui.tela.texto(x0 + 52.0, topo + 30.0, 13.0, tema::TEXTO_SECUNDARIO, "Esta pasta está vazia.");
        }
        let mut abrir = None;
        let primeiro = (self.rolagem / LINHA_A).floor() as usize;
        for i in primeiro..self.entradas.len() {
            let y = topo + i as f32 * LINHA_A - self.rolagem;
            if y > y0 + a {
                break;
            }
            let e = &self.entradas[i];
            let sel = self.selecionado == Some(i);
            if sel {
                ui.tela.retangulo(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0, 4.0, tema::DESTAQUE_SUAVE);
            } else if ui.sobre(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) && ui.mouse.map(|m| m.1 > topo).unwrap_or(false) {
                ui.tela.retangulo(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0, 4.0, tema::SUPERFICIE_ALTA);
            }
            if e.pasta {
                icone_pasta(ui.tela, x0 + 20.0, y + 3.0, 22.0);
            } else {
                icone_arquivo(ui.tela, x0 + 20.0, y + 3.0, 22.0);
            }
            let max_nome = if l > 520.0 { col_data - x0 - 64.0 } else { l - 70.0 };
            ui.tela.texto(x0 + 52.0, y + 19.0, 13.0, tema::TEXTO, &desenho::encurtar(13.0, &e.nome, max_nome));
            if l > 520.0 {
                if let Some(m) = e.modificado {
                    ui.tela.texto(col_data, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO, &data_legivel(m));
                }
                if !e.pasta {
                    ui.tela.texto(col_tam, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO, &tamanho_legivel(e.tamanho));
                }
            }
            if y >= topo - 1.0 && ui.clicou(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) {
                self.selecionado = Some(i);
                if ui.duplo {
                    abrir = Some(e.clone());
                }
            }
        }
        // Cobre o que rolou para baixo do cabeçalho.
        ui.tela.retangulo(x0, y0, l, 30.0, 0.0, tema::SUPERFICIE);
        ui.tela.texto(x0 + 52.0, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Nome");
        if l > 520.0 {
            ui.tela.texto(col_data, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Data de modificação");
            ui.tela.texto(col_tam, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Tamanho");
        }
        ui.tela.retangulo(x0 + 12.0, y0 + 29.0, l - 24.0, 1.0, 0.0, tema::BORDA);
        if let Some(e) = abrir {
            self.abrir(&e);
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Este Computador".into(),
            app_id: "explorador".into(),
            tamanho: (980, 640),
            minimo: (560, 380),
        },
        Box::new(Explorador::novo()),
    );
}
