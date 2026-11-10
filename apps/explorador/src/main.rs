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

use penas::{desenho, tema, App, Cor, Keysym, Opcoes, Tecla, Tela, Ui};

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
    /// Menu do botão direito: posição e o item clicado (None = fundo).
    menu: Option<(f32, f32, Option<usize>)>,
    /// Arquivos copiados (false) ou recortados (true).
    transferencia: Option<(Vec<PathBuf>, bool)>,
    /// Item sendo renomeado e o texto digitado.
    renomeando: Option<(usize, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Acao {
    Abrir,
    Recortar,
    Copiar,
    Colar,
    Renomear,
    Excluir,
    NovaPasta,
    Atualizar,
    Ocultos,
}

const MENU_L: f32 = 210.0;
const MENU_ITEM_A: f32 = 30.0;

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
            menu: None,
            transferencia: None,
            renomeando: None,
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

    fn pasta_atual(&self) -> Option<PathBuf> {
        match &self.local {
            Local::Pasta(p) => Some(p.clone()),
            Local::EsteComputador => None,
        }
    }

    fn lixeira(&self) -> PathBuf {
        self.casa.join(".local/share/Trash")
    }

    /// Nome que ainda não existe na pasta: "Nova pasta", "Nova pasta (2)"...
    fn nome_livre(dir: &Path, base: &str, sufixo: &str) -> PathBuf {
        let (raiz, ext) = match base.rsplit_once('.') {
            Some((r, e)) if !r.is_empty() && !base.ends_with('.') => (r.to_string(), format!(".{e}")),
            _ => (base.to_string(), String::new()),
        };
        let mut p = dir.join(base);
        let mut n = 1;
        while p.exists() {
            n += 1;
            let extra = if n == 2 && !sufixo.is_empty() { sufixo.to_string() } else { format!("{sufixo} ({n})") };
            p = dir.join(format!("{raiz}{extra}{ext}"));
        }
        p
    }

    fn itens_menu(&self, alvo: Option<usize>) -> Vec<(&'static str, Acao, bool)> {
        let na_pasta = self.pasta_atual().is_some();
        match alvo {
            Some(_) => vec![
                ("Abrir", Acao::Abrir, true),
                ("Recortar", Acao::Recortar, true),
                ("Copiar", Acao::Copiar, true),
                ("Renomear", Acao::Renomear, true),
                ("Excluir", Acao::Excluir, true),
            ],
            None => vec![
                ("Nova pasta", Acao::NovaPasta, na_pasta),
                ("Colar", Acao::Colar, na_pasta && self.transferencia.is_some()),
                ("Atualizar", Acao::Atualizar, true),
                (
                    if self.mostrar_ocultos { "Esconder arquivos ocultos" } else { "Mostrar arquivos ocultos" },
                    Acao::Ocultos,
                    na_pasta,
                ),
            ],
        }
    }

    fn executar(&mut self, acao: Acao, alvo: Option<usize>) {
        let entrada = alvo.and_then(|i| self.entradas.get(i)).cloned();
        match acao {
            Acao::Abrir => {
                if let Some(e) = entrada {
                    self.abrir(&e);
                }
            }
            Acao::Copiar | Acao::Recortar => {
                if let Some(e) = entrada {
                    let corte = acao == Acao::Recortar;
                    self.transferencia = Some((vec![e.caminho.clone()], corte));
                    let verbo = if corte { "recortado" } else { "copiado" };
                    self.aviso = Some(format!("\"{}\" {verbo}. Use Colar na pasta de destino.", e.nome));
                }
            }
            Acao::Colar => self.colar(),
            Acao::Renomear => {
                if let (Some(i), Some(e)) = (alvo, entrada) {
                    self.selecionado = Some(i);
                    self.renomeando = Some((i, e.nome));
                }
            }
            Acao::Excluir => {
                if let Some(e) = entrada {
                    self.excluir(&e);
                }
            }
            Acao::NovaPasta => {
                if let Some(dir) = self.pasta_atual() {
                    let p = Self::nome_livre(&dir, "Nova pasta", "");
                    match fs::create_dir(&p) {
                        Ok(()) => {
                            self.recarregar();
                            if let Some(i) = self.entradas.iter().position(|e| e.caminho == p) {
                                self.selecionado = Some(i);
                                let nome = self.entradas[i].nome.clone();
                                self.renomeando = Some((i, nome));
                            }
                        }
                        Err(e) => self.aviso = Some(format!("Não deu para criar a pasta: {e}")),
                    }
                }
            }
            Acao::Atualizar => self.recarregar(),
            Acao::Ocultos => {
                self.mostrar_ocultos = !self.mostrar_ocultos;
                self.recarregar();
            }
        }
    }

    fn confirmar_renomear(&mut self) {
        let Some((i, novo)) = self.renomeando.take() else { return };
        let Some(e) = self.entradas.get(i).cloned() else { return };
        let novo = novo.trim().to_string();
        if novo.is_empty() || novo == e.nome {
            return;
        }
        if novo.contains('/') {
            self.aviso = Some("O nome não pode ter \"/\".".into());
            return;
        }
        let destino = e.caminho.with_file_name(&novo);
        if destino.exists() {
            self.aviso = Some(format!("Já existe \"{novo}\" nesta pasta."));
            return;
        }
        match fs::rename(&e.caminho, &destino) {
            Ok(()) => {
                self.recarregar();
                self.selecionado = self.entradas.iter().position(|x| x.caminho == destino);
            }
            Err(err) => self.aviso = Some(format!("Não deu para renomear: {err}")),
        }
    }

    /// Manda para a Lixeira (no padrão freedesktop); dentro da própria
    /// Lixeira, apaga de vez.
    fn excluir(&mut self, e: &Entrada) {
        let lixeira = self.lixeira();
        let arquivos = lixeira.join("files");
        if e.caminho.starts_with(&arquivos) {
            let r = if e.pasta { fs::remove_dir_all(&e.caminho) } else { fs::remove_file(&e.caminho) };
            let _ = fs::remove_file(lixeira.join("info").join(format!("{}.trashinfo", e.nome)));
            match r {
                Ok(()) => self.aviso = Some(format!("\"{}\" foi apagado de vez.", e.nome)),
                Err(err) => self.aviso = Some(format!("Não deu para apagar: {err}")),
            }
            self.recarregar();
            return;
        }
        let _ = fs::create_dir_all(&arquivos);
        let _ = fs::create_dir_all(lixeira.join("info"));
        let destino = Self::nome_livre(&arquivos, &e.nome, "");
        let nome_lixeira = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let agora = SystemTime::now();
        let data = data_legivel(agora); // dd/mm/aaaa hh:mm
        let iso = {
            let p: Vec<&str> = data.split(|c| c == '/' || c == ' ' || c == ':').collect();
            if p.len() == 5 { format!("{}-{}-{}T{}:{}:00", p[2], p[1], p[0], p[3], p[4]) } else { String::new() }
        };
        let info = format!("[Trash Info]\nPath={}\nDeletionDate={iso}\n", e.caminho.display());
        let _ = fs::write(lixeira.join("info").join(format!("{nome_lixeira}.trashinfo")), info);
        let ok = fs::rename(&e.caminho, &destino).is_ok()
            || Command::new("mv").arg("--").arg(&e.caminho).arg(&destino).status().map(|s| s.success()).unwrap_or(false);
        self.aviso = Some(if ok {
            format!("\"{}\" foi para a Lixeira.", e.nome)
        } else {
            format!("Não deu para mandar \"{}\" para a Lixeira.", e.nome)
        });
        self.recarregar();
    }

    fn colar(&mut self) {
        let Some(dir) = self.pasta_atual() else { return };
        let Some((origens, corte)) = self.transferencia.clone() else { return };
        let mut falhas = 0;
        for o in &origens {
            let nome = o.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if corte && o.parent() == Some(dir.as_path()) {
                continue;
            }
            let destino = Self::nome_livre(&dir, &nome, if corte { "" } else { " - Cópia" });
            let ok = if corte {
                fs::rename(o, &destino).is_ok()
                    || Command::new("mv").arg("--").arg(o).arg(&destino).status().map(|s| s.success()).unwrap_or(false)
            } else {
                Command::new("cp").arg("-a").arg("--").arg(o).arg(&destino).status().map(|s| s.success()).unwrap_or(false)
            };
            if !ok {
                falhas += 1;
            }
        }
        if corte {
            self.transferencia = None;
        }
        self.aviso = (falhas > 0).then(|| format!("{falhas} item(ns) não puderam ser colados."));
        self.recarregar();
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
            if self.renomeando.is_some() {
                match tecla {
                    Tecla::Texto(t) => {
                        if let Some((_, nome)) = self.renomeando.as_mut() {
                            nome.push_str(&t);
                        }
                    }
                    Tecla::Apagar => {
                        if let Some((_, nome)) = self.renomeando.as_mut() {
                            nome.pop();
                        }
                    }
                    Tecla::Enter => self.confirmar_renomear(),
                    Tecla::Esc => self.renomeando = None,
                    _ => {}
                }
                continue;
            }
            let sel = self.selecionado;
            match tecla {
                Tecla::Outra(Keysym::Delete) => {
                    if let Some(i) = sel {
                        self.executar(Acao::Excluir, Some(i));
                    }
                }
                Tecla::Outra(Keysym::F2) => {
                    if let Some(i) = sel {
                        self.executar(Acao::Renomear, Some(i));
                    }
                }
                Tecla::Outra(Keysym::F5) => self.recarregar(),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::c || k == Keysym::C) => self.executar(Acao::Copiar, sel),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::x || k == Keysym::X) => self.executar(Acao::Recortar, sel),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::v || k == Keysym::V) => self.executar(Acao::Colar, None),
                Tecla::Outra(k) if ui.ctrl && ui.shift && (k == Keysym::n || k == Keysym::N) => {
                    self.executar(Acao::NovaPasta, None)
                }
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::h || k == Keysym::H) => self.executar(Acao::Ocultos, None),
                Tecla::Esc => self.menu = None,
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
                _ => {}
            }
        }
        ui.titulo = Some(self.titulo());

        // Menu do botão direito aberto: o clique é dele.
        if let Some((mx, my, alvo)) = self.menu {
            if ui.clique || ui.clique_direito {
                let itens = self.itens_menu(alvo);
                let mut escolhida = None;
                for (k, (_, acao, ativo)) in itens.iter().enumerate() {
                    if *ativo && ui.sobre(mx, my + 6.0 + k as f32 * MENU_ITEM_A, MENU_L, MENU_ITEM_A) {
                        escolhida = Some(*acao);
                    }
                }
                self.menu = None;
                ui.clique = false;
                ui.duplo = false;
                if let Some(acao) = escolhida {
                    self.executar(acao, alvo);
                }
                if !ui.clique_direito {
                    // Clique fora do menu só fecha o menu.
                }
            }
        }

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

        if let Some((mx, my, alvo)) = self.menu {
            let itens = self.itens_menu(alvo);
            let h = itens.len() as f32 * MENU_ITEM_A + 12.0;
            // Não deixa o menu sair da janela.
            let mx = mx.min(l - MENU_L - 4.0).max(0.0);
            let my = my.min(a - h - 4.0).max(0.0);
            self.menu = Some((mx, my, alvo));
            ui.tela.retangulo(mx + 2.0, my + 4.0, MENU_L, h, 8.0, Cor(0, 0, 0, 40));
            ui.tela.retangulo(mx, my, MENU_L, h, 8.0, tema::BORDA);
            ui.tela.retangulo(mx + 1.0, my + 1.0, MENU_L - 2.0, h - 2.0, 7.0, tema::BRANCO);
            for (k, (nome, _, ativo)) in itens.iter().enumerate() {
                let y = my + 6.0 + k as f32 * MENU_ITEM_A;
                if *ativo && ui.sobre(mx, y, MENU_L, MENU_ITEM_A) {
                    ui.tela.retangulo(mx + 4.0, y, MENU_L - 8.0, MENU_ITEM_A, 5.0, tema::DESTAQUE_SUAVE);
                }
                let cor = if *ativo { tema::TEXTO } else { tema::BORDA };
                ui.tela.texto(mx + 16.0, y + 20.0, 13.0, cor, nome);
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
            match &self.renomeando {
                Some((j, texto)) if *j == i => {
                    ui.tela.retangulo(x0 + 48.0, y + 2.0, max_nome + 8.0, LINHA_A - 6.0, 4.0, tema::DESTAQUE);
                    ui.tela.retangulo(x0 + 49.5, y + 3.5, max_nome + 5.0, LINHA_A - 9.0, 3.0, tema::BRANCO);
                    let fim = ui.tela.texto(x0 + 54.0, y + 19.0, 13.0, tema::TEXTO, texto);
                    ui.tela.retangulo(x0 + 55.0 + fim, y + 7.0, 1.5, LINHA_A - 16.0, 0.0, tema::DESTAQUE);
                }
                _ => {
                    let corte = self
                        .transferencia
                        .as_ref()
                        .map(|(v, c)| *c && v.contains(&e.caminho))
                        .unwrap_or(false);
                    let cor = if corte { tema::TEXTO_SECUNDARIO } else { tema::TEXTO };
                    ui.tela.texto(x0 + 52.0, y + 19.0, 13.0, cor, &desenho::encurtar(13.0, &e.nome, max_nome));
                }
            }
            if l > 520.0 {
                if let Some(m) = e.modificado {
                    ui.tela.texto(col_data, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO, &data_legivel(m));
                }
                if !e.pasta {
                    ui.tela.texto(col_tam, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO, &tamanho_legivel(e.tamanho));
                }
            }
            if y >= topo - 1.0 && ui.clicou(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) {
                if self.renomeando.as_ref().map(|(j, _)| *j != i).unwrap_or(false) {
                    self.confirmar_renomear();
                }
                self.selecionado = Some(i);
                if ui.duplo && self.renomeando.is_none() {
                    abrir = Some(self.entradas[i].clone());
                }
            }
            if y >= topo - 1.0 && ui.clicou_direito(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) {
                self.selecionado = Some(i);
                if let Some((mx, my)) = ui.mouse {
                    self.menu = Some((mx, my, Some(i)));
                }
                ui.clique_direito = false;
            }
        }
        // Botão direito no fundo da lista: menu da pasta.
        if ui.clicou_direito(x0, topo, l, a - 34.0) {
            if let Some((mx, my)) = ui.mouse {
                self.menu = Some((mx, my, None));
                self.selecionado = None;
            }
        }
        if ui.clicou(x0, topo, l, a - 34.0) && self.renomeando.is_some() {
            self.confirmar_renomear();
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
