// SPDX-License-Identifier: GPL-3.0-or-later
//! Explorador de Arquivos do Ararinha.
//!
//! Por dentro é Linux; por fora fala a língua de quem vem do Windows:
//! "Este Computador", Disco Local (C:), unidades D:, E:..., pastas do
//! usuário com os nomes de sempre e navegação com Voltar, Avançar e Subir.

mod abrir_com;
mod tipos;

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use penas::{desenho, tema, App, Cor, Keysym, Opcoes, Tecla, Tela, Ui};
use tipos::Propriedades;

const BARRA_A: f32 = 52.0;
const LATERAL_L: f32 = 210.0;
/// Largura do painel de visualização.
const PREVIA_L: f32 = 280.0;
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
    /// Unidade de rede (SMB), conectada pelo ararinha-unidade.
    rede: bool,
    /// Pendrive/HD externo: o dispositivo (sdb1) para abrir e ejetar.
    usb: Option<String>,
}

/// Diálogo "Mapear unidade de rede".
#[derive(Debug, Clone)]
struct Mapear {
    letra: char,
    /// Campos: pasta (\\servidor\pasta), usuário e senha.
    campos: [String; 3],
    foco: usize,
    reconectar: bool,
    erro: Option<String>,
}

/// Arquivo com as unidades a reconectar ao entrar (só do usuário, 0600).
fn arquivo_unidades() -> PathBuf {
    let casa = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    PathBuf::from(casa).join(".config/ararinha/unidades")
}

/// Conecta pelo ajudante; devolve a mensagem de erro, se houver.
fn conectar_unidade(letra: char, pasta: &str, usuario: &str, senha: &str) -> Result<(), String> {
    use std::io::Write as _;
    let mut filho = Command::new("ararinha-unidade")
        .args(["montar", &letra.to_string(), pasta, usuario])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não deu para chamar o ararinha-unidade: {e}"))?;
    if let Some(mut entrada) = filho.stdin.take() {
        let _ = writeln!(entrada, "{senha}");
    }
    let saida = filho.wait_with_output().map_err(|e| e.to_string())?;
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).trim().to_string())
    }
}

/// Lido no início da sessão: reconecta as unidades marcadas.
fn reconectar_tudo() {
    let Ok(texto) = fs::read_to_string(arquivo_unidades()) else { return };
    for linha in texto.lines() {
        let c: Vec<&str> = linha.splitn(4, '\t').collect();
        if c.len() == 4 {
            if let Some(l) = c[0].chars().next() {
                if let Err(e) = conectar_unidade(l, c[1], c[2], c[3]) {
                    eprintln!("explorador: {l}: {e}");
                }
            }
        }
    }
}

fn lembrar_unidade(letra: char, pasta: &str, usuario: &str, senha: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let p = arquivo_unidades();
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let atual = fs::read_to_string(&p).unwrap_or_default();
    let mut linhas: Vec<String> = atual
        .lines()
        .filter(|l| !l.starts_with(&format!("{letra}\t")))
        .map(String::from)
        .collect();
    linhas.push(format!("{letra}\t{pasta}\t{usuario}\t{senha}"));
    let _ = fs::remove_file(&p);
    if let Ok(mut f) = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&p) {
        use std::io::Write as _;
        let _ = f.write_all((linhas.join("\n") + "\n").as_bytes());
    }
}

fn esquecer_unidade(letra: char) {
    let p = arquivo_unidades();
    if let Ok(atual) = fs::read_to_string(&p) {
        let resto: Vec<&str> = atual.lines().filter(|l| !l.starts_with(&format!("{letra}\t"))).collect();
        let _ = fs::write(&p, resto.join("\n") + if resto.is_empty() { "" } else { "\n" });
    }
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
    mapear: Option<Mapear>,
    /// Menu do botão direito numa unidade de rede: posição e letra.
    menu_unidade: Option<(f32, f32, char)>,
    /// Itens marcados (Ctrl/Shift+clique, Ctrl+A); `selecionado` é o foco.
    selecionados: BTreeSet<usize>,
    /// Todas as entradas da pasta, antes do filtro da busca.
    todas: Vec<Entrada>,
    busca: String,
    busca_ativa: bool,
    /// Os resultados são de uma busca nas subpastas.
    busca_funda: bool,
    /// Arraste: onde o botão foi apertado e se já virou arraste.
    arraste: Option<(f32, f32, bool)>,
    /// Ao soltar sem arrastar, a seleção vira só este item.
    colapsar: Option<usize>,
    /// Pasta sob o mouse durante o arraste.
    alvo_soltar: Option<PathBuf>,
    propriedades: Option<Propriedades>,
    /// Modo "escolher arquivo" (chamado pelo portal dos apps Flatpak).
    escolha: Option<Escolha>,
    /// Fecha a janela no fim do quadro (escolha feita).
    sair: bool,
    /// "Deseja executar este aplicativo?" (AppImage baixado).
    executar_app: Option<PathBuf>,
    /// Extração ou compactação em andamento (resposta para o aviso).
    tarefa: Option<std::sync::mpsc::Receiver<String>>,
    /// Painel de visualização (à direita): o arquivo mostrado e a prévia.
    previa: Option<(PathBuf, Previa)>,
    previa_rx: Option<std::sync::mpsc::Receiver<(PathBuf, Previa)>>,
    /// Janela "Abrir com…".
    abrir_com: Option<AbrirCom>,
    /// Quando as unidades foram lidas (pendrives entram e saem).
    unidades_em: Option<std::time::Instant>,
}

/// Janela "Como você quer abrir este arquivo?".
struct AbrirCom {
    arquivo: PathBuf,
    tipo: String,
    ext: String,
    apps: Vec<abrir_com::AppDesktop>,
    outros: Vec<abrir_com::AppDesktop>,
    ver_todos: bool,
    sel: usize,
    sempre: bool,
    rolagem: f32,
}

impl AbrirCom {
    fn novo(arquivo: &Path) -> AbrirCom {
        let nome = arquivo.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let tipo = abrir_com::tipo_mime(&nome);
        let (apps, outros) = abrir_com::para_tipo(&tipo);
        let ext = arquivo.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ver_todos = apps.is_empty();
        AbrirCom { arquivo: arquivo.to_path_buf(), tipo, ext, apps, outros, ver_todos, sel: 0, sempre: false, rolagem: 0.0 }
    }

    fn visiveis(&self) -> Vec<&abrir_com::AppDesktop> {
        let mut v: Vec<&abrir_com::AppDesktop> = self.apps.iter().collect();
        if self.ver_todos {
            v.extend(self.outros.iter());
        }
        v
    }

    /// (x, y, largura, altura) da caixa e o topo/fundo da lista.
    fn caixa(l: f32, a: f32) -> (f32, f32, f32, f32, f32, f32) {
        let (cl, ca) = (460.0_f32.min(l - 20.0), 480.0_f32.min(a - 20.0));
        let (x, y) = ((l - cl) / 2.0, (a - ca) / 2.0);
        (x, y, cl, ca, y + 96.0, y + ca - 108.0)
    }
}
const ABRIR_ITEM: f32 = 40.0;

/// O que o painel de visualização mostra de um arquivo.
enum Previa {
    Carregando,
    Imagem(image::RgbaImage, (u32, u32)),
    Texto(Vec<String>),
    Nada,
}

/// Lê a prévia (roda numa thread: imagens grandes demoram).
fn fazer_previa(c: &Path, cat: tipos::Categoria, lado: u32) -> Previa {
    match cat {
        tipos::Categoria::Imagem => match image::open(c) {
            Ok(img) => {
                let tam = (img.width(), img.height());
                Previa::Imagem(img.thumbnail(lado, lado).to_rgba8(), tam)
            }
            Err(_) => Previa::Nada,
        },
        tipos::Categoria::Texto | tipos::Categoria::Codigo => {
            use std::io::Read;
            let mut b = vec![0u8; 4096];
            let n = std::fs::File::open(c).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
            b.truncate(n);
            let t = String::from_utf8_lossy(&b).replace('\t', "    ");
            Previa::Texto(t.lines().take(40).map(str::to_string).collect())
        }
        _ => Previa::Nada,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ModoEscolha {
    Abrir,
    Salvar,
    Pasta,
}

/// Janela de "Abrir"/"Salvar como": o resultado sai no stdout, um caminho
/// por linha; nada no stdout é "Cancelar".
struct Escolha {
    modo: ModoEscolha,
    varios: bool,
    nome: String,
    nome_ativo: bool,
    confirmar: bool,
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
    Propriedades,
    Restaurar,
    EsvaziarLixeira,
    SelecionarTudo,
    Extrair,
    Compactar,
    AbrirCom,
}

/// Arquivos que o bsdtar (libarchive) sabe abrir.
fn compactado(nome: &str) -> bool {
    let n = nome.to_lowercase();
    [".zip", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tar.zst", ".7z", ".rar", ".iso", ".cab"]
        .iter()
        .any(|e| n.ends_with(e))
}

/// Nome sem a extensão de pacote ("fotos.tar.gz" → "fotos").
fn sem_extensao(nome: &str) -> String {
    let n = nome.to_lowercase();
    for e in [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tgz", ".txz", ".tbz2", ".zip", ".tar", ".7z", ".rar", ".iso", ".cab"] {
        if n.ends_with(e) {
            return nome[..nome.len() - e.len()].to_string();
        }
    }
    nome.to_string()
}

/// Caminho livre: "x", "x (2)", "x (3)"...
fn livre(pasta: &std::path::Path, nome: &str, ext: &str) -> PathBuf {
    let mut p = pasta.join(format!("{nome}{ext}"));
    let mut n = 2;
    while p.exists() {
        p = pasta.join(format!("{nome} ({n}){ext}"));
        n += 1;
    }
    p
}

const MENU_L: f32 = 210.0;
const MENU_ITEM_A: f32 = 30.0;

/// Lê as unidades: a raiz é C:, as outras partições montadas ganham D:, E:...
fn ler_unidades() -> Vec<Unidade> {
    let montagens = fs::read_to_string("/proc/mounts").unwrap_or_default();
    // Unidades de rede: /run/media/<usuário>/<LETRA>, com a letra escolhida.
    let mut rede = Vec::new();
    for linha in montagens.lines() {
        let c: Vec<&str> = linha.split_whitespace().collect();
        if c.len() < 3 || !(c[2] == "cifs" || c[2] == "smb3") {
            continue;
        }
        let ponto = PathBuf::from(c[1].replace("\\040", " "));
        let letra = ponto
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| n.len() == 1)
            .and_then(|n| n.chars().next());
        if let Some(letra) = letra {
            let unc = c[0].replace("\\040", " ");
            let mut partes = unc.trim_start_matches('/').splitn(2, '/');
            let servidor = partes.next().unwrap_or("").to_string();
            let pasta = partes.next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            let (total, livre) = espaco(&ponto);
            rede.push(Unidade {
                letra,
                nome: format!("{pasta} (\\\\{servidor})"),
                ponto,
                total,
                livre,
                rede: true,
                usb: None,
            });
        }
    }
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
    let usadas: Vec<char> = rede.iter().map(|u| u.letra).collect();
    let mut livres = ('C'..='Z').filter(|l| !usadas.contains(l));
    let mut v: Vec<Unidade> = pontos
        .into_iter()
        .filter_map(|(nome, ponto)| {
            let letra = livres.next()?;
            let (total, livre) = espaco(&ponto);
            Some(Unidade { letra, nome, ponto, total, livre, rede: false, usb: None })
        })
        .collect();
    v.extend(rede);
    // Pendrives e HDs externos (montados ou não), pelo ararinha-sistema.
    let lista = penas::sistema::pedir("usb listar").unwrap_or_default();
    for item in lista.split(';').filter(|i| !i.is_empty()) {
        let c: Vec<&str> = item.split('|').collect();
        let (dev, bytes, ponto) = (c[0].to_string(), c.get(2).and_then(|b| b.parse::<u64>().ok()).unwrap_or(0), c.get(3).copied().unwrap_or(""));
        let rotulo = rotulo_dispositivo(&dev).or_else(|| c.get(1).filter(|r| !r.is_empty()).map(|r| r.to_string()));
        let nome = rotulo.unwrap_or_else(|| format!("Dispositivo removível ({})", tamanho_legivel(bytes)));
        if !ponto.is_empty() {
            if let Some(u) = v.iter_mut().find(|u| u.ponto == Path::new(ponto)) {
                u.usb = Some(dev);
                u.nome = nome;
            }
            continue;
        }
        let usadas: Vec<char> = v.iter().map(|u| u.letra).collect();
        if let Some(letra) = ('D'..='Z').find(|l| !usadas.contains(l)) {
            v.push(Unidade { letra, nome, ponto: PathBuf::new(), total: bytes, livre: 0, rede: false, usb: Some(dev) });
        }
    }
    v
}

/// Rótulo do dispositivo pelo /dev/disk/by-label.
fn rotulo_dispositivo(dev: &str) -> Option<String> {
    for e in fs::read_dir("/dev/disk/by-label").ok()?.flatten() {
        if fs::read_link(e.path()).ok()?.file_name()?.to_str()? == dev {
            return Some(e.file_name().to_string_lossy().replace("\\x20", " "));
        }
    }
    None
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
            mapear: None,
            menu_unidade: None,
            selecionados: BTreeSet::new(),
            todas: Vec::new(),
            busca: String::new(),
            busca_ativa: false,
            busca_funda: false,
            arraste: None,
            colapsar: None,
            alvo_soltar: None,
            propriedades: None,
            escolha: None,
            sair: false,
            executar_app: None,
            tarefa: None,
            previa: None,
            previa_rx: None,
            unidades_em: None,
            abrir_com: None,
        };
        // Abre numa pasta passada como argumento (ou na pasta de um arquivo,
        // já selecionado: "Mostrar na pasta" do navegador). Aceita file://.
        let arg = std::env::args().nth(1).filter(|a| !a.starts_with("--")).map(|a| {
            let a = a.strip_prefix("file://").map(desfazer_uri).unwrap_or(a);
            PathBuf::from(a)
        });
        let mut selecionar = None;
        if let Some(p) = arg {
            if p.is_dir() {
                e.local = Local::Pasta(p);
            } else if let (Some(pai), true) = (p.parent(), p.exists()) {
                e.local = Local::Pasta(pai.to_path_buf());
                selecionar = Some(p.clone());
            }
        }
        e.recarregar();
        if let Some(alvo) = selecionar {
            if let Some(i) = e.entradas.iter().position(|x| x.caminho == alvo) {
                e.selecionar_so(i);
            }
        }
        e
    }

    fn recarregar(&mut self) {
        self.selecionado = None;
        self.selecionados.clear();
        self.rolagem = 0.0;
        self.busca_funda = false;
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
                self.todas = v;
                self.filtrar();
            }
        }
    }

    /// Aplica o texto da busca às entradas da pasta (filtro na hora).
    fn filtrar(&mut self) {
        let termo = self.busca.to_lowercase();
        self.entradas = if termo.is_empty() {
            self.todas.clone()
        } else {
            self.todas.iter().filter(|e| e.nome.to_lowercase().contains(&termo)).cloned().collect()
        };
        self.selecionado = None;
        self.selecionados.clear();
    }

    /// Enter na busca: procura também nas subpastas.
    fn buscar_fundo(&mut self) {
        let Some(raiz) = self.pasta_atual() else { return };
        if self.busca.trim().is_empty() {
            return;
        }
        let achados = tipos::buscar(&raiz, self.busca.trim(), self.mostrar_ocultos, 500);
        self.entradas = achados
            .into_iter()
            .map(|caminho| {
                let meta = fs::metadata(&caminho).ok();
                Entrada {
                    nome: caminho.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    pasta: caminho.is_dir(),
                    tamanho: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    modificado: meta.and_then(|m| m.modified().ok()),
                    caminho,
                }
            })
            .collect();
        self.busca_funda = true;
        self.selecionado = None;
        self.selecionados.clear();
        self.rolagem = 0.0;
        self.aviso = Some(if self.entradas.is_empty() {
            format!("Nada encontrado para \"{}\".", self.busca.trim())
        } else {
            format!("{} resultado(s) para \"{}\" nesta pasta e nas subpastas.", self.entradas.len(), self.busca.trim())
        });
    }

    /// Itens marcados (ou o item em foco, se nada estiver marcado).
    fn marcados(&self) -> Vec<Entrada> {
        let mut idx: BTreeSet<usize> = self.selecionados.clone();
        if let Some(i) = self.selecionado {
            if idx.is_empty() {
                idx.insert(i);
            }
        }
        idx.into_iter().filter_map(|i| self.entradas.get(i).cloned()).collect()
    }

    fn selecionar_so(&mut self, i: usize) {
        self.selecionado = Some(i);
        self.selecionados.clear();
        self.selecionados.insert(i);
    }

    fn na_lixeira(&self) -> bool {
        matches!(&self.local, Local::Pasta(p) if *p == self.lixeira().join("files"))
    }

    /// Move (ou copia, com Ctrl) os itens marcados para outra pasta.
    fn soltar_em(&mut self, destino: PathBuf, copiar: bool) {
        let itens = self.marcados();
        if itens.is_empty() || !destino.is_dir() {
            return;
        }
        let mut feitos = 0;
        let mut falhas = 0;
        for e in &itens {
            // Não deixa mover uma pasta para dentro dela mesma.
            if destino.starts_with(&e.caminho) || e.caminho.parent() == Some(destino.as_path()) && !copiar {
                continue;
            }
            let alvo = Self::nome_livre(&destino, &e.nome, if copiar { " - Cópia" } else { "" });
            let ok = if copiar {
                Command::new("cp").arg("-a").arg("--").arg(&e.caminho).arg(&alvo).status().map(|s| s.success()).unwrap_or(false)
            } else {
                fs::rename(&e.caminho, &alvo).is_ok()
                    || Command::new("mv").arg("--").arg(&e.caminho).arg(&alvo).status().map(|s| s.success()).unwrap_or(false)
            };
            if ok {
                feitos += 1;
            } else {
                falhas += 1;
            }
        }
        let nome_destino = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
        self.aviso = Some(if falhas > 0 {
            format!("{falhas} item(ns) não puderam ser {}.", if copiar { "copiados" } else { "movidos" })
        } else {
            format!("{feitos} item(ns) {} para \"{nome_destino}\".", if copiar { "copiados" } else { "movidos" })
        });
        self.recarregar();
    }

    /// Escreve a escolha no stdout e fecha.
    fn entregar(&mut self, caminhos: Vec<PathBuf>) {
        use std::io::Write;
        let mut saida = std::io::stdout().lock();
        for c in caminhos {
            let _ = writeln!(saida, "{}", c.display());
        }
        let _ = saida.flush();
        self.sair = true;
    }

    /// Barra de baixo do modo escolher: nome (ao salvar) e os botões.
    fn desenhar_escolha(&mut self, ui: &mut Ui, x0: f32, y: f32, l: f32) {
        let Some(modo) = self.escolha.as_ref().map(|e| e.modo) else { return };
        ui.tela.retangulo(x0, y, l, 64.0, 0.0, tema::FUNDO());
        ui.tela.retangulo(x0, y, l, 1.0, 0.0, tema::BORDA());
        let by = y + 14.0;
        let mut bx = x0 + l - 16.0;
        let mut botao = |ui: &mut Ui, rot: &str, principal: bool, ativo: bool| -> bool {
            let w = desenho::medir(13.5, rot) + 34.0;
            bx -= w;
            let s = ativo && ui.sobre(bx, by, w, 36.0);
            let fundo = if !ativo {
                tema::SUPERFICIE_ALTA()
            } else if principal {
                if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() }
            } else if s {
                tema::SUPERFICIE_ALTA()
            } else {
                tema::JANELA()
            };
            ui.tela.retangulo(bx, by, w, 36.0, 7.0, if principal { fundo } else { tema::BORDA() });
            if !principal {
                ui.tela.retangulo(bx + 1.0, by + 1.0, w - 2.0, 34.0, 6.0, fundo);
            }
            let cor = if !ativo { tema::TEXTO_SECUNDARIO() } else if principal { tema::BRANCO() } else { tema::TEXTO() };
            ui.tela.texto(bx + 17.0, by + 23.0, 13.5, cor, rot);
            let r = ativo && ui.clicou(bx, by, w, 36.0);
            bx -= 10.0;
            r
        };
        let cancelar = botao(ui, "Cancelar", false, true);
        let marcados: Vec<Entrada> = self.marcados();
        let pasta = self.pasta_atual();
        match modo {
            ModoEscolha::Abrir => {
                let so_arquivos: Vec<PathBuf> = marcados.iter().filter(|e| !e.pasta).map(|e| e.caminho.clone()).collect();
                let varios = self.escolha.as_ref().map(|e| e.varios).unwrap_or(false);
                let ok = !so_arquivos.is_empty() && (varios || so_arquivos.len() == 1);
                if botao(ui, "Abrir", true, ok) {
                    self.entregar(so_arquivos);
                }
            }
            ModoEscolha::Pasta => {
                let alvo = marcados.iter().find(|e| e.pasta).map(|e| e.caminho.clone()).or(pasta.clone());
                if botao(ui, "Selecionar pasta", true, alvo.is_some()) {
                    if let Some(p) = alvo {
                        self.entregar(vec![p]);
                    }
                }
            }
            ModoEscolha::Salvar => {
                let (nome, ativo, confirmar) = {
                    let e = self.escolha.as_ref().unwrap();
                    (e.nome.clone(), e.nome_ativo, e.confirmar)
                };
                let pode = !nome.trim().is_empty() && !nome.contains('/') && pasta.is_some();
                let salvar = botao(ui, if confirmar { "Substituir" } else { "Salvar" }, true, pode);
                // Campo do nome
                let (cx, cl) = (x0 + 70.0, (bx - x0 - 86.0).max(120.0));
                ui.tela.texto(x0 + 16.0, by + 23.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nome:");
                ui.tela.retangulo(cx, by, cl, 36.0, 6.0, if ativo { tema::DESTAQUE() } else { tema::BORDA() });
                ui.tela.retangulo(cx + 1.0, by + 1.0, cl - 2.0, 34.0, 5.0, tema::JANELA());
                let fim = ui.tela.texto(cx + 10.0, by + 23.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &nome, cl - 24.0));
                if ativo {
                    ui.tela.retangulo(cx + 11.0 + fim, by + 9.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
                }
                if ui.clique {
                    let dentro = ui.sobre(cx, by, cl, 36.0);
                    if let Some(e) = self.escolha.as_mut() {
                        e.nome_ativo = dentro;
                    }
                }
                if salvar {
                    self.salvar_escolha();
                }
            }
        }
        if cancelar {
            self.sair = true;
        }
    }

    /// "Salvar" do modo escolher: pede confirmação se o arquivo já existe.
    fn salvar_escolha(&mut self) {
        let Some(pasta) = self.pasta_atual() else { return };
        let Some(e) = self.escolha.as_mut() else { return };
        let destino = pasta.join(e.nome.trim());
        if destino.exists() && !e.confirmar {
            e.confirmar = true;
            self.aviso = Some(format!("\"{}\" já existe. Clique em Substituir para trocar.", e.nome.trim()));
            return;
        }
        self.entregar(vec![destino]);
    }

    fn ir(&mut self, novo: Local) {
        if novo == self.local {
            return;
        }
        self.busca.clear();
        self.busca_ativa = false;
        let antigo = std::mem::replace(&mut self.local, novo);
        self.voltar.push(antigo);
        self.avancar.clear();
        self.aviso = None;
        self.recarregar();
    }

    fn ir_voltar(&mut self) {
        self.busca.clear();
        if let Some(l) = self.voltar.pop() {
            let atual = std::mem::replace(&mut self.local, l);
            self.avancar.push(atual);
            self.recarregar();
        }
    }

    fn ir_avancar(&mut self) {
        self.busca.clear();
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
            .filter(|u| !u.ponto.as_os_str().is_empty() && p.starts_with(&u.ponto))
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

    fn abrir_mapear(&mut self) {
        let usadas: Vec<char> = self.unidades.iter().map(|u| u.letra).collect();
        let letra = ('D'..='Z').rev().find(|l| !usadas.contains(l)).unwrap_or('Z');
        self.mapear = Some(Mapear {
            letra,
            campos: [String::new(), String::new(), String::new()],
            foco: 0,
            reconectar: true,
            erro: None,
        });
    }

    fn trocar_letra(&mut self, passo: i32) {
        let usadas: Vec<char> = self.unidades.iter().map(|u| u.letra).collect();
        let livres: Vec<char> = ('D'..='Z').filter(|l| !usadas.contains(l)).collect();
        if let Some(m) = self.mapear.as_mut() {
            if livres.is_empty() {
                return;
            }
            let i = livres.iter().position(|l| *l == m.letra).unwrap_or(0) as i32;
            m.letra = livres[(i + passo).rem_euclid(livres.len() as i32) as usize];
        }
    }

    fn tecla_mapear(&mut self, tecla: Tecla) {
        let Some(m) = self.mapear.as_mut() else { return };
        match tecla {
            Tecla::Texto(t) => {
                if m.campos[m.foco].chars().count() < 200 {
                    m.campos[m.foco].push_str(&t);
                }
                m.erro = None;
            }
            Tecla::Apagar => {
                m.campos[m.foco].pop();
            }
            Tecla::Outra(Keysym::Tab) | Tecla::Baixo => m.foco = (m.foco + 1) % 3,
            Tecla::Cima => m.foco = (m.foco + 2) % 3,
            Tecla::Esc => self.mapear = None,
            Tecla::Enter => {
                if m.foco < 2 {
                    m.foco += 1;
                } else {
                    self.conectar_mapear();
                }
            }
            _ => {}
        }
    }

    fn conectar_mapear(&mut self) {
        let Some(m) = self.mapear.clone() else { return };
        let pasta = m.campos[0].trim().to_string();
        if pasta.is_empty() {
            if let Some(mm) = self.mapear.as_mut() {
                mm.erro = Some("Digite a pasta: \\\\servidor\\pasta".into());
                mm.foco = 0;
            }
            return;
        }
        match conectar_unidade(m.letra, &pasta, m.campos[1].trim(), &m.campos[2]) {
            Ok(()) => {
                if m.reconectar {
                    lembrar_unidade(m.letra, &pasta, m.campos[1].trim(), &m.campos[2]);
                }
                self.mapear = None;
                self.aviso = Some(format!("Unidade {}: conectada.", m.letra));
                self.unidades = ler_unidades();
            }
            Err(e) => {
                if let Some(mm) = self.mapear.as_mut() {
                    mm.erro = Some(if e.is_empty() { "Não deu para conectar.".into() } else { e });
                }
            }
        }
    }

    fn desconectar(&mut self, letra: char) {
        let saida = Command::new("ararinha-unidade").args(["desmontar", &letra.to_string()]).output();
        match saida {
            Ok(o) if o.status.success() => {
                esquecer_unidade(letra);
                self.aviso = Some(format!("Unidade {letra}: desconectada."));
            }
            Ok(o) => self.aviso = Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => self.aviso = Some(format!("Não deu para desconectar: {e}")),
        }
        if matches!(&self.local, Local::Pasta(p) if p.starts_with("/run/media")) {
            self.local = Local::EsteComputador;
        }
        self.recarregar();
    }

    /// Posição do diálogo de mapear (centralizado).
    fn caixa_mapear(l: f32, a: f32) -> (f32, f32, f32, f32) {
        let (cl, ca) = (480.0, 380.0);
        ((l - cl) / 2.0, (a - ca) / 2.0, cl, ca)
    }

    fn clique_mapear(&mut self, ui: &mut Ui, l: f32, a: f32) {
        if !ui.clique {
            ui.duplo = false;
            ui.clique_direito = false;
            return;
        }
        let (x, y, cl, ca) = Self::caixa_mapear(l, a);
        let campos_y = [118.0, 190.0, 248.0];
        let mut acao = None;
        for (i, cy) in campos_y.iter().enumerate() {
            if ui.sobre(x + 24.0, y + cy, cl - 48.0, 34.0) {
                acao = Some(i as i32);
            }
        }
        if ui.sobre(x + 24.0, y + 62.0, 30.0, 30.0) {
            self.trocar_letra(1);
        }
        if ui.sobre(x + 110.0, y + 62.0, 30.0, 30.0) {
            self.trocar_letra(-1);
        }
        if ui.sobre(x + 24.0, y + 292.0, 240.0, 24.0) {
            if let Some(m) = self.mapear.as_mut() {
                m.reconectar = !m.reconectar;
            }
        }
        if ui.sobre(x + cl - 228.0, y + ca - 54.0, 100.0, 36.0) {
            self.mapear = None;
        } else if ui.sobre(x + cl - 120.0, y + ca - 54.0, 100.0, 36.0) {
            self.conectar_mapear();
        }
        if let (Some(i), Some(m)) = (acao, self.mapear.as_mut()) {
            m.foco = i as usize;
        }
        ui.clique = false;
        ui.duplo = false;
        ui.clique_direito = false;
    }

    fn desenhar_mapear(&mut self, ui: &mut Ui, l: f32, a: f32) {
        let Some(m) = self.mapear.clone() else { return };
        ui.tela.retangulo(0.0, 0.0, l, a, 0.0, Cor(11, 26, 46, 90));
        let (x, y, cl, ca) = Self::caixa_mapear(l, a);
        ui.tela.retangulo(x + 3.0, y + 6.0, cl, ca, 12.0, Cor(0, 0, 0, 50));
        ui.tela.retangulo(x, y, cl, ca, 12.0, tema::BORDA());
        ui.tela.retangulo(x + 1.0, y + 1.0, cl - 2.0, ca - 2.0, 11.0, tema::JANELA());
        desenhar_icone(ui.tela, "unidade-rede", x + 20.0, y + 14.0, 36.0);
        ui.tela.texto(x + 66.0, y + 38.0, 17.0, tema::TEXTO(), "Mapear unidade de rede");

        // Letra.
        ui.tela.texto(x + 160.0, y + 82.0, 13.0, tema::TEXTO_SECUNDARIO(), "Unidade");
        ui.tela.retangulo(x + 56.0, y + 62.0, 50.0, 30.0, 6.0, tema::DESTAQUE_SUAVE());
        let lt = format!("{}:", m.letra);
        let w = desenho::medir(16.0, &lt);
        ui.tela.texto(x + 81.0 - w / 2.0, y + 83.0, 16.0, tema::TEXTO(), &lt);
        seta(ui.tela, x + 39.0, y + 77.0, 0.0, tema::TEXTO());
        seta(ui.tela, x + 125.0, y + 77.0, 1.0, tema::TEXTO());

        let rotulos = ["Pasta (por exemplo \\\\servidor\\documentos)", "Usuário (vazio = convidado)", "Senha"];
        let campos_y = [118.0, 190.0, 248.0];
        for i in 0..3 {
            let cy = y + campos_y[i];
            ui.tela.texto(x + 26.0, cy - 6.0, 12.0, tema::TEXTO_SECUNDARIO(), rotulos[i]);
            let focado = m.foco == i;
            ui.tela.retangulo(x + 24.0, cy, cl - 48.0, 34.0, 6.0, if focado { tema::DESTAQUE() } else { tema::BORDA() });
            let e = if focado { 2.0 } else { 1.0 };
            ui.tela.retangulo(x + 24.0 + e, cy + e, cl - 48.0 - 2.0 * e, 34.0 - 2.0 * e, 5.0, tema::JANELA());
            let texto = if i == 2 { "•".repeat(m.campos[2].chars().count()) } else { m.campos[i].clone() };
            let texto = desenho::encurtar(14.0, &texto, cl - 80.0);
            let fim = ui.tela.texto(x + 34.0, cy + 23.0, 14.0, tema::TEXTO(), &texto);
            if focado {
                ui.tela.retangulo(x + 35.0 + fim, cy + 8.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
            }
        }
        // Reconectar ao entrar.
        let cx = x + 24.0;
        let cy = y + 294.0;
        ui.tela.retangulo(cx, cy, 18.0, 18.0, 4.0, if m.reconectar { tema::DESTAQUE() } else { tema::BORDA() });
        if m.reconectar {
            ui.tela.linha(cx + 4.0, cy + 9.0, cx + 8.0, cy + 13.0, 2.0, tema::BRANCO());
            ui.tela.linha(cx + 8.0, cy + 13.0, cx + 14.0, cy + 5.0, 2.0, tema::BRANCO());
        } else {
            ui.tela.retangulo(cx + 1.5, cy + 1.5, 15.0, 15.0, 3.0, tema::JANELA());
        }
        ui.tela.texto(cx + 28.0, cy + 14.0, 13.0, tema::TEXTO(), "Reconectar ao entrar");
        if let Some(e) = &m.erro {
            let e = desenho::encurtar(12.5, e, cl - 48.0);
            ui.tela.texto(x + 24.0, y + ca - 66.0, 12.5, tema::ERRO(), &e);
        }
        // Botões.
        let by = y + ca - 54.0;
        let sobre_c = ui.sobre(x + cl - 228.0, by, 100.0, 36.0);
        ui.tela.retangulo(x + cl - 228.0, by, 100.0, 36.0, 8.0, if sobre_c { tema::SUPERFICIE_ALTA() } else { tema::FUNDO() });
        ui.tela.texto(x + cl - 207.0, by + 23.0, 13.5, tema::TEXTO(), "Cancelar");
        let sobre_o = ui.sobre(x + cl - 120.0, by, 100.0, 36.0);
        ui.tela.retangulo(x + cl - 120.0, by, 100.0, 36.0, 8.0, if sobre_o { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        ui.tela.texto(x + cl - 102.0, by + 23.0, 13.5, tema::BRANCO(), "Conectar");
    }

    fn pasta_atual(&self) -> Option<PathBuf> {
        match &self.local {
            Local::Pasta(p) => Some(p.clone()),
            Local::EsteComputador => None,
        }
    }

    fn lixeira(&self) -> PathBuf {
        tipos::lixeira(&self.casa)
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
        let lixeira = self.na_lixeira();
        let varios = self.marcados().len() > 1;
        match alvo {
            Some(i_menu) => {
                let mut v = vec![("Abrir", Acao::Abrir, !varios)];
                let arquivo = !varios && self.entradas.get(i_menu).is_some_and(|e| !e.pasta);
                if arquivo && !lixeira {
                    v.push(("Abrir com…", Acao::AbrirCom, true));
                }
                if lixeira {
                    v.push(("Restaurar", Acao::Restaurar, true));
                }
                let um_compactado = !varios && self.entradas.get(i_menu).is_some_and(|e| !e.pasta && compactado(&e.nome));
                if um_compactado && !lixeira {
                    v.push(("Extrair aqui", Acao::Extrair, self.tarefa.is_none()));
                }
                if !lixeira {
                    v.push(("Compactar em ZIP", Acao::Compactar, self.tarefa.is_none()));
                }
                v.extend([
                    ("Recortar", Acao::Recortar, true),
                    ("Copiar", Acao::Copiar, true),
                    ("Renomear", Acao::Renomear, !varios),
                    (if lixeira { "Excluir de vez" } else { "Excluir" }, Acao::Excluir, true),
                    ("Propriedades", Acao::Propriedades, true),
                ]);
                v
            }
            None => {
                let mut v = vec![
                    ("Nova pasta", Acao::NovaPasta, na_pasta && !lixeira),
                    ("Colar", Acao::Colar, na_pasta && self.transferencia.is_some()),
                    ("Selecionar tudo", Acao::SelecionarTudo, !self.entradas.is_empty()),
                    ("Atualizar", Acao::Atualizar, true),
                    (
                        if self.mostrar_ocultos { "Esconder arquivos ocultos" } else { "Mostrar arquivos ocultos" },
                        Acao::Ocultos,
                        na_pasta,
                    ),
                ];
                if lixeira {
                    v.push(("Esvaziar Lixeira", Acao::EsvaziarLixeira, !self.todas.is_empty()));
                }
                v.push(("Propriedades", Acao::Propriedades, na_pasta));
                v
            }
        }
    }

    fn executar(&mut self, acao: Acao, alvo: Option<usize>) {
        let entrada = alvo.and_then(|i| self.entradas.get(i)).cloned();
        match acao {
            Acao::Extrair => {
                let (Some(e), Some(pasta)) = (entrada, self.pasta_atual()) else { return };
                let destino = livre(&pasta, &sem_extensao(&e.nome), "");
                let (tx, rx) = std::sync::mpsc::channel();
                self.aviso = Some(format!("Extraindo \"{}\"…", e.nome));
                std::thread::spawn(move || {
                    let _ = fs::create_dir_all(&destino);
                    let r = std::process::Command::new("bsdtar").arg("-xf").arg(&e.caminho).arg("-C").arg(&destino).output();
                    let nome = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let _ = tx.send(match r {
                        Ok(o) if o.status.success() => format!("Extraído na pasta \"{nome}\"."),
                        Ok(o) => {
                            let _ = fs::remove_dir(&destino);
                            format!("Não deu para extrair: {}", String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("arquivo danificado"))
                        }
                        Err(e) => format!("Não deu para extrair: {e}"),
                    });
                });
                self.tarefa = Some(rx);
            }
            Acao::Compactar => {
                let Some(pasta) = self.pasta_atual() else { return };
                let itens = self.marcados();
                if itens.is_empty() {
                    return;
                }
                let base = if itens.len() == 1 { sem_extensao(&itens[0].nome) } else { "Arquivos".to_string() };
                let destino = livre(&pasta, &base, ".zip");
                let nomes: Vec<String> = itens.iter().map(|e| e.nome.clone()).collect();
                let (tx, rx) = std::sync::mpsc::channel();
                self.aviso = Some(format!("Compactando {} item(ns)…", nomes.len()));
                std::thread::spawn(move || {
                    let r = std::process::Command::new("bsdtar")
                        .args(["--format", "zip", "-cf"])
                        .arg(&destino)
                        .arg("-C")
                        .arg(&pasta)
                        .arg("--")
                        .args(&nomes)
                        .output();
                    let nome = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let _ = tx.send(match r {
                        Ok(o) if o.status.success() => format!("Criado \"{nome}\"."),
                        Ok(o) => format!("Não deu para compactar: {}", String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("")),
                        Err(e) => format!("Não deu para compactar: {e}"),
                    });
                });
                self.tarefa = Some(rx);
            }
            Acao::Abrir => {
                if let Some(e) = entrada {
                    self.abrir(&e);
                }
            }
            Acao::AbrirCom => {
                if let Some(e) = entrada {
                    self.abrir_com = Some(AbrirCom::novo(&e.caminho));
                }
            }
            Acao::Copiar | Acao::Recortar => {
                let itens = self.marcados();
                if !itens.is_empty() {
                    let corte = acao == Acao::Recortar;
                    self.transferencia = Some((itens.iter().map(|e| e.caminho.clone()).collect(), corte));
                    let verbo = if corte { "recortado" } else { "copiado" };
                    self.aviso = Some(if itens.len() == 1 {
                        format!("\"{}\" {verbo}. Use Colar na pasta de destino.", itens[0].nome)
                    } else {
                        format!("{} itens: {verbo}s. Use Colar na pasta de destino.", itens.len())
                    });
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
                let itens = self.marcados();
                let n = itens.len();
                for e in &itens {
                    self.excluir(e);
                }
                if n > 1 {
                    self.aviso = Some(if self.na_lixeira() {
                        format!("{n} itens apagados de vez.")
                    } else {
                        format!("{n} itens foram para a Lixeira.")
                    });
                }
            }
            Acao::Propriedades => {
                let caminhos: Vec<PathBuf> = if alvo.is_some() {
                    self.marcados().into_iter().map(|e| e.caminho).collect()
                } else {
                    self.pasta_atual().into_iter().collect()
                };
                if !caminhos.is_empty() {
                    self.propriedades = Some(tipos::propriedades(&caminhos, tamanho_legivel, data_legivel));
                }
            }
            Acao::Restaurar => {
                let mut falhas = 0;
                let itens = self.marcados();
                for e in &itens {
                    match tipos::origem_na_lixeira(&self.casa, &e.nome) {
                        Some(orig) => {
                            let pai = orig.parent().map(Path::to_path_buf).unwrap_or_else(|| self.casa.clone());
                            let _ = fs::create_dir_all(&pai);
                            let nome = orig.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(e.nome.clone());
                            let destino = Self::nome_livre(&pai, &nome, "");
                            let ok = fs::rename(&e.caminho, &destino).is_ok()
                                || Command::new("mv").arg("--").arg(&e.caminho).arg(&destino).status().map(|s| s.success()).unwrap_or(false);
                            if ok {
                                let _ = fs::remove_file(self.lixeira().join("info").join(format!("{}.trashinfo", e.nome)));
                            } else {
                                falhas += 1;
                            }
                        }
                        None => falhas += 1,
                    }
                }
                self.aviso = Some(if falhas > 0 {
                    format!("{falhas} item(ns) não puderam ser restaurados.")
                } else {
                    format!("{} item(ns) restaurados para o lugar de antes.", itens.len())
                });
                self.recarregar();
            }
            Acao::EsvaziarLixeira => {
                match tipos::esvaziar_lixeira(&self.casa) {
                    Ok(n) => self.aviso = Some(format!("Lixeira esvaziada ({n} itens apagados de vez).")),
                    Err(e) => self.aviso = Some(format!("Não deu para esvaziar a Lixeira: {e}")),
                }
                self.recarregar();
            }
            Acao::SelecionarTudo => {
                self.selecionados = (0..self.entradas.len()).collect();
                self.selecionado = if self.entradas.is_empty() { None } else { Some(0) };
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
        } else if let Some(esc) = &mut self.escolha {
            // Escolhendo arquivo: clique duplo escolhe (ou põe o nome, ao salvar).
            match esc.modo {
                ModoEscolha::Abrir => self.entregar(vec![e.caminho.clone()]),
                ModoEscolha::Salvar => {
                    esc.nome = e.nome.clone();
                    esc.confirmar = false;
                }
                ModoEscolha::Pasta => {}
            }
        } else {
            let ext = e
                .caminho
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_lowercase())
                .unwrap_or_default();
            const IMAGENS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"];
            const TEXTOS: [&str; 16] = [
                "txt", "md", "log", "conf", "cfg", "ini", "toml", "json", "csv", "sh", "rs", "py", "c", "h",
                "desktop", "xml",
            ];
            const AUDIOS: [&str; 9] = ["mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "mp4"];
            let programa = if IMAGENS.contains(&ext.as_str()) {
                Some("imagens")
            } else if AUDIOS.contains(&ext.as_str()) {
                Some("musicas")
            } else if TEXTOS.contains(&ext.as_str()) || (ext.is_empty() && e.tamanho < 1_000_000) {
                Some("notas")
            } else {
                None
            };
            if ext == "appimage" {
                // Como o Windows com um .exe baixado: pergunta antes.
                self.executar_app = Some(e.caminho.clone());
                return;
            }
            // App padrão escolhido pela pessoa ("Sempre usar este app").
            let tipo = abrir_com::tipo_mime(&e.nome);
            if let Some(app) = abrir_com::padrao(&tipo).and_then(|id| abrir_com::achar(&id)) {
                if let Err(err) = abrir_com::abrir(&app, &e.caminho) {
                    self.aviso = Some(format!("Não deu para abrir: {err}"));
                }
                return;
            }
            match programa {
                Some(p) => {
                    if let Err(err) = Command::new(p).arg(&e.caminho).spawn() {
                        self.aviso = Some(format!("Não deu para abrir: {err}"));
                    }
                }
                None => self.abrir_com = Some(AbrirCom::novo(&e.caminho)),
            }
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
    t.retangulo(x + 0.18 * s + 1.0, y + 0.08 * s + 1.0, 0.64 * s - 2.0, 0.84 * s - 2.0, 0.05 * s, tema::BRANCO());
    for k in 0..3 {
        let ly = y + (0.38 + 0.14 * k as f32) * s;
        t.linha(x + 0.3 * s, ly, x + 0.7 * s, ly, 0.05 * s, borda.com_alfa(160));
    }
}

fn icone_lixeira(t: &mut Tela, x: f32, y: f32, s: f32) {
    let cor = Cor::hex(0x6B8FB5);
    t.retangulo(x + 0.2 * s, y + 0.12 * s, 0.6 * s, 0.1 * s, 0.03 * s, cor);
    t.retangulo(x + 0.4 * s, y + 0.05 * s, 0.2 * s, 0.08 * s, 0.03 * s, cor);
    t.retangulo(x + 0.25 * s, y + 0.26 * s, 0.5 * s, 0.66 * s, 0.06 * s, cor);
    t.retangulo(x + 0.29 * s, y + 0.3 * s, 0.42 * s, 0.58 * s, 0.04 * s, Cor::hex(0xD6E8F7));
    for k in 0..3 {
        let lx = x + (0.38 + 0.12 * k as f32) * s;
        t.linha(lx, y + 0.38 * s, lx, y + 0.8 * s, (0.04 * s).max(1.0), cor);
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
        t.retangulo(x + 0.14 * s, y + 0.4 * s, 0.16 * s, 0.16 * s, 0.03 * s, tema::DESTAQUE());
    }
}

fn icone_computador(t: &mut Tela, x: f32, y: f32, s: f32) {
    if desenhar_icone(t, "este-computador", x, y, s) {
        return;
    }
    t.retangulo(x + 0.08 * s, y + 0.16 * s, 0.84 * s, 0.54 * s, 0.06 * s, Cor::hex(0x3A4A5C));
    t.retangulo(x + 0.12 * s, y + 0.2 * s, 0.76 * s, 0.46 * s, 0.04 * s, tema::DESTAQUE());
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
        // Extração/compactação terminou?
        if let Some(rx) = &self.tarefa {
            match rx.try_recv() {
                Ok(msg) => {
                    self.tarefa = None;
                    self.aviso = Some(msg);
                    self.recarregar();
                }
                Err(_) => ui.acordar_em = Some(std::time::Duration::from_millis(250)),
            }
        }
        // Teclado.
        for tecla in std::mem::take(&mut ui.teclas) {
            if let Some(esc) = self.escolha.as_mut().filter(|e| e.nome_ativo) {
                match tecla {
                    Tecla::Texto(s) => {
                        esc.nome.push_str(&s);
                        esc.confirmar = false;
                    }
                    Tecla::Apagar => {
                        esc.nome.pop();
                        esc.confirmar = false;
                    }
                    Tecla::Enter => self.salvar_escolha(),
                    Tecla::Esc => esc.nome_ativo = false,
                    _ => {}
                }
                continue;
            }
            if self.mapear.is_some() {
                self.tecla_mapear(tecla);
                continue;
            }
            if self.propriedades.is_some() {
                if matches!(tecla, Tecla::Esc | Tecla::Enter) {
                    self.propriedades = None;
                }
                continue;
            }
            if self.busca_ativa {
                match tecla {
                    Tecla::Texto(t) => {
                        self.busca.push_str(&t);
                        if !self.busca_funda {
                            self.filtrar();
                        }
                    }
                    Tecla::Apagar => {
                        self.busca.pop();
                        if self.busca_funda {
                            self.recarregar();
                        } else {
                            self.filtrar();
                        }
                    }
                    Tecla::Enter => self.buscar_fundo(),
                    Tecla::Esc => {
                        self.busca.clear();
                        self.busca_ativa = false;
                        self.recarregar();
                    }
                    Tecla::Baixo => {
                        self.busca_ativa = false;
                        if !self.entradas.is_empty() {
                            self.selecionar_so(0);
                        }
                    }
                    _ => {}
                }
                continue;
            }
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
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::a || k == Keysym::A) => {
                    self.executar(Acao::SelecionarTudo, None)
                }
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::f || k == Keysym::F) => {
                    if self.pasta_atual().is_some() {
                        self.busca_ativa = true;
                    }
                }
                Tecla::Outra(Keysym::F3) => {
                    if self.pasta_atual().is_some() {
                        self.busca_ativa = true;
                    }
                }
                Tecla::Outra(Keysym::Menu) => {
                    self.menu = Some((LATERAL_L + 60.0, BARRA_A + 60.0, self.selecionado));
                }
                Tecla::Esc => {
                    self.menu = None;
                    if !self.busca.is_empty() {
                        self.busca.clear();
                        self.recarregar();
                    }
                }
                Tecla::Voltar | Tecla::Apagar => self.ir_voltar(),
                Tecla::Avancar => self.ir_avancar(),
                Tecla::Subir => self.subir(),
                Tecla::Baixo => {
                    let n = self.entradas.len();
                    if n > 0 {
                        let i = self.selecionado.map(|i| (i + 1).min(n - 1)).unwrap_or(0);
                        self.selecionar_so(i);
                    }
                }
                Tecla::Cima => {
                    if let Some(i) = self.selecionado {
                        self.selecionar_so(i.saturating_sub(1));
                    }
                }
                Tecla::Enter if ui.alt => self.executar(Acao::Propriedades, self.selecionado),
                Tecla::Enter => {
                    if let Some(e) = self.selecionado.and_then(|i| self.entradas.get(i)).cloned() {
                        self.abrir(&e);
                    }
                }
                _ => {}
            }
        }
        if self.escolha.is_none() {
            ui.titulo = Some(self.titulo());
        }

        // Diálogo "Mapear unidade de rede": modal, os cliques são dele.
        let (l_tela, a_tela) = (ui.largura(), ui.altura());
        if self.mapear.is_some() {
            self.clique_mapear(ui, l_tela, a_tela);
        }
        // Propriedades: modal também.
        if self.propriedades.is_some() {
            let (x, y, cl, ca) = self.caixa_propriedades(l_tela, a_tela);
            if ui.clicou(x + cl - 120.0, y + ca - 54.0, 100.0, 36.0) {
                self.propriedades = None;
            }
            ui.clique = false;
            ui.duplo = false;
            ui.clique_direito = false;
            ui.rolagem = 0.0;
        }
        if self.abrir_com.is_some() {
            self.entrada_abrir_com(ui, l_tela, a_tela);
            ui.clique = false;
            ui.duplo = false;
            ui.clique_direito = false;
            ui.rolagem = 0.0;
        }
        if let Some(app) = self.executar_app.clone() {
            let (x, y, cl, ca) = Self::caixa_executar(l_tela, a_tela);
            if ui.clicou(x + cl - 130.0, y + ca - 56.0, 110.0, 36.0) {
                self.executar_app = None;
                self.rodar_appimage(&app);
            } else if ui.clicou(x + cl - 250.0, y + ca - 56.0, 110.0, 36.0) {
                self.executar_app = None;
            }
            ui.clique = false;
            ui.duplo = false;
            ui.clique_direito = false;
            ui.rolagem = 0.0;
        }
        self.alvo_soltar = None;
        // Arraste: vira arraste quando o mouse anda uns pixels apertado.
        if let (Some((ax, ay, ativo)), Some((mx, my))) = (self.arraste, ui.mouse) {
            if !ativo && ui.pressionado && ((mx - ax).abs() > 6.0 || (my - ay).abs() > 6.0) {
                self.arraste = Some((ax, ay, true));
                self.colapsar = None;
            }
        }
        // Menu de uma unidade de rede.
        if let Some((mx, my, letra)) = self.menu_unidade {
            if ui.clique || ui.clique_direito {
                if ui.clique && ui.sobre(mx, my, MENU_L, MENU_ITEM_A + 12.0) {
                    self.desconectar(letra);
                }
                self.menu_unidade = None;
                ui.clique = false;
                ui.duplo = false;
                ui.clique_direito = false;
            }
        }

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
        ui.tela.limpar(tema::SUPERFICIE());

        self.desenhar_lateral(ui, a);
        self.desenhar_barra(ui, l);

        let (cx, cy) = (LATERAL_L, BARRA_A);
        let rodape = if self.escolha.is_some() { 64.0 } else { 0.0 };
        let (cl, ca) = (l - LATERAL_L, a - BARRA_A - rodape);
        match self.local.clone() {
            Local::EsteComputador => self.desenhar_este_computador(ui, cx, cy, cl),
            Local::Pasta(_) if cl >= 820.0 => {
                // Janela larga: painel de visualização à direita.
                let pl = PREVIA_L;
                self.desenhar_pasta(ui, cx, cy, cl - pl, ca);
                self.desenhar_previa(ui, cx + cl - pl, cy, pl, ca);
            }
            Local::Pasta(_) => self.desenhar_pasta(ui, cx, cy, cl, ca),
        }
        if self.escolha.is_some() {
            self.desenhar_escolha(ui, cx, a - rodape, cl);
        }

        // Aviso na parte de baixo.
        if let Some(aviso) = self.aviso.clone() {
            let y = a - 40.0 - rodape;
            ui.tela.retangulo(cx + 12.0, y, cl - 24.0, 30.0, 8.0, tema::DESTAQUE_SUAVE());
            let txt = desenho::encurtar(13.0, &aviso, cl - 80.0);
            ui.tela.texto(cx + 24.0, y + 20.0, 13.0, tema::TEXTO(), &txt);
            let fx = cx + cl - 40.0;
            let cor = if ui.sobre(fx, y, 24.0, 30.0) { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() };
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
            ui.tela.retangulo(mx, my, MENU_L, h, 8.0, tema::BORDA());
            ui.tela.retangulo(mx + 1.0, my + 1.0, MENU_L - 2.0, h - 2.0, 7.0, tema::JANELA());
            for (k, (nome, _, ativo)) in itens.iter().enumerate() {
                let y = my + 6.0 + k as f32 * MENU_ITEM_A;
                if *ativo && ui.sobre(mx, y, MENU_L, MENU_ITEM_A) {
                    ui.tela.retangulo(mx + 4.0, y, MENU_L - 8.0, MENU_ITEM_A, 5.0, tema::DESTAQUE_SUAVE());
                }
                let cor = if *ativo { tema::TEXTO() } else { tema::BORDA() };
                ui.tela.texto(mx + 16.0, y + 20.0, 13.0, cor, nome);
            }
        }

        if let Some((mx, my, letra)) = self.menu_unidade {
            let h = MENU_ITEM_A + 12.0;
            ui.tela.retangulo(mx + 2.0, my + 4.0, MENU_L, h, 8.0, Cor(0, 0, 0, 40));
            ui.tela.retangulo(mx, my, MENU_L, h, 8.0, tema::BORDA());
            ui.tela.retangulo(mx + 1.0, my + 1.0, MENU_L - 2.0, h - 2.0, 7.0, tema::JANELA());
            if ui.sobre(mx, my + 6.0, MENU_L, MENU_ITEM_A) {
                ui.tela.retangulo(mx + 4.0, my + 6.0, MENU_L - 8.0, MENU_ITEM_A, 5.0, tema::DESTAQUE_SUAVE());
            }
            ui.tela.texto(mx + 16.0, my + 26.0, 13.0, tema::TEXTO(), &format!("Desconectar {letra}:"));
        }

        // Fim do arraste: solta na pasta sob o mouse (Ctrl copia).
        if let Some((_, _, ativo)) = self.arraste {
            if ativo {
                if let Some((mx, my)) = ui.mouse {
                    let n = self.marcados().len();
                    let copiar = ui.ctrl;
                    let txt = match &self.alvo_soltar {
                        Some(d) => format!(
                            "{} {n} item(ns) para \"{}\"",
                            if copiar { "Copiar" } else { "Mover" },
                            d.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "/".into())
                        ),
                        None => format!("{n} item(ns)"),
                    };
                    let lt = desenho::medir(12.5, &txt);
                    ui.tela.retangulo(mx + 14.0, my + 16.0, lt + 20.0, 28.0, 6.0, tema::BORDA());
                    ui.tela.retangulo(mx + 15.0, my + 17.0, lt + 18.0, 26.0, 5.0, tema::JANELA());
                    ui.tela.texto(mx + 24.0, my + 35.0, 12.5, tema::TEXTO(), &txt);
                }
            }
            if ui.soltou || !ui.pressionado {
                if ativo {
                    if let Some(d) = self.alvo_soltar.clone() {
                        self.soltar_em(d, ui.ctrl);
                    }
                } else if let Some(i) = self.colapsar.take() {
                    self.selecionar_so(i);
                }
                self.arraste = None;
                ui.redesenhar = true;
            }
        }

        if self.mapear.is_some() {
            self.desenhar_mapear(ui, l, a);
        }
        if self.propriedades.is_some() {
            self.desenhar_propriedades(ui, l, a);
        }
        if let Some(app) = self.executar_app.clone() {
            let (x, y, cl, ca) = Self::caixa_executar(l, a);
            ui.tela.retangulo(0.0, 0.0, l, a, 0.0, Cor(11, 26, 46, 90));
            ui.tela.retangulo(x, y, cl, ca, 12.0, tema::BORDA());
            ui.tela.retangulo(x + 1.0, y + 1.0, cl - 2.0, ca - 2.0, 11.0, tema::JANELA());
            tipos::icone_tipo(ui.tela, tipos::Categoria::Programa, x + 24.0, y + 24.0, 48.0);
            ui.tela.texto(x + 88.0, y + 46.0, 16.0, tema::TEXTO(), "Deseja executar este aplicativo?");
            let nome = app.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui.tela.texto(x + 88.0, y + 70.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, &nome, cl - 110.0));
            ui.tela.texto(x + 24.0, y + 108.0, 12.5, tema::TEXTO_SECUNDARIO(), "Só execute programas de quem você confia: ele terá acesso");
            ui.tela.texto(x + 24.0, y + 126.0, 12.5, tema::TEXTO_SECUNDARIO(), "aos seus arquivos. Apps da Central de Apps são mais seguros.");
            let (bx, by) = (x + cl - 130.0, y + ca - 56.0);
            let s = ui.sobre(bx, by, 110.0, 36.0);
            ui.tela.retangulo(bx, by, 110.0, 36.0, 8.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
            ui.tela.texto(bx + 26.0, by + 23.0, 13.5, tema::BRANCO(), "Executar");
            let cx = x + cl - 250.0;
            let s = ui.sobre(cx, by, 110.0, 36.0);
            ui.tela.retangulo(cx, by, 110.0, 36.0, 8.0, tema::BORDA());
            ui.tela.retangulo(cx + 1.0, by + 1.0, 108.0, 34.0, 7.0, if s { tema::SUPERFICIE_ALTA() } else { tema::JANELA() });
            ui.tela.texto(cx + 24.0, by + 23.0, 13.5, tema::TEXTO(), "Cancelar");
        }
        if self.abrir_com.is_some() {
            self.desenhar_abrir_com(ui, l, a);
        }
        if self.sair {
            ui.fechar = true;
        }
    }
}

impl Explorador {
    fn entrada_abrir_com(&mut self, ui: &mut Ui, l: f32, a: f32) {
        let (x, y, cl, ca, topo, fundo) = AbrirCom::caixa(l, a);
        let Some(ac) = self.abrir_com.as_mut() else { return };
        let n = ac.visiveis().len();
        let mais = !ac.ver_todos && !ac.outros.is_empty();
        let total = (n + mais as usize) as f32 * ABRIR_ITEM;
        if ui.sobre(x, topo, cl, fundo - topo) {
            ac.rolagem = (ac.rolagem + ui.rolagem * 2.0).clamp(0.0, (total - (fundo - topo)).max(0.0));
        }
        let mut abrir = false;
        if let Some((mx, my)) = ui.mouse {
            if (ui.clique || ui.duplo) && mx >= x + 12.0 && mx < x + cl - 12.0 && my >= topo && my < fundo {
                let i = ((my - topo + ac.rolagem) / ABRIR_ITEM) as usize;
                if i < n {
                    ac.sel = i;
                    abrir = ui.duplo;
                } else if mais && i == n {
                    ac.ver_todos = true;
                }
            }
        }
        if ui.clicou(x + 24.0, fundo + 14.0, cl - 48.0, 28.0) {
            ac.sempre = !ac.sempre;
        }
        let by = y + ca - 56.0;
        if ui.clicou(x + cl - 130.0, by, 110.0, 36.0) {
            abrir = true;
        } else if ui.clicou(x + cl - 250.0, by, 110.0, 36.0) {
            self.abrir_com = None;
            return;
        }
        if abrir {
            let Some(ac) = self.abrir_com.take() else { return };
            let Some(app) = ac.visiveis().get(ac.sel).map(|a| (*a).clone()) else { return };
            if ac.sempre {
                if let Err(e) = abrir_com::definir_padrao(&ac.tipo, &app.id) {
                    self.aviso = Some(format!("Não deu para guardar o app padrão: {e}"));
                }
            }
            if let Err(e) = abrir_com::abrir(&app, &ac.arquivo) {
                self.aviso = Some(format!("Não deu para abrir com {}: {e}", app.nome));
            }
        }
    }

    fn desenhar_abrir_com(&mut self, ui: &mut Ui, l: f32, a: f32) {
        let (x, y, cl, ca, topo, fundo) = AbrirCom::caixa(l, a);
        let Some(ac) = self.abrir_com.as_ref() else { return };
        ui.tela.retangulo(0.0, 0.0, l, a, 0.0, Cor(11, 26, 46, 90));
        ui.tela.retangulo(x, y, cl, ca, 12.0, tema::BORDA());
        ui.tela.retangulo(x + 1.0, y + 1.0, cl - 2.0, ca - 2.0, 11.0, tema::JANELA());
        ui.tela.texto(x + 24.0, y + 40.0, 16.0, tema::TEXTO(), "Como você quer abrir este arquivo?");
        let nome = ac.arquivo.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.tela.texto(x + 24.0, y + 66.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, &nome, cl - 48.0));
        let vis = ac.visiveis();
        if vis.is_empty() && ac.outros.is_empty() {
            ui.tela.texto(x + 24.0, topo + 24.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhum app instalado abre este tipo de arquivo.");
            ui.tela.texto(x + 24.0, topo + 44.0, 13.0, tema::TEXTO_SECUNDARIO(), "Procure um na Central de Apps.");
        }
        let mais = !ac.ver_todos && !ac.outros.is_empty();
        let mut linhas: Vec<(String, bool, bool)> = vis.iter().enumerate().map(|(i, a)| (a.nome.clone(), i == ac.sel, i == ac.apps.len() && ac.ver_todos && !ac.apps.is_empty())).collect();
        if mais {
            linhas.push(("Mais apps…".into(), false, false));
        }
        for (i, (nome, sel, separa)) in linhas.iter().enumerate() {
            let iy = topo + i as f32 * ABRIR_ITEM - ac.rolagem;
            // Só os itens inteiros (a lista rola).
            if iy < topo - 1.0 || iy + ABRIR_ITEM > fundo + 1.0 {
                continue;
            }
            if *separa {
                ui.tela.retangulo(x + 24.0, iy - 1.0, cl - 48.0, 1.0, 0.0, tema::BORDA());
            }
            if *sel {
                ui.tela.retangulo(x + 12.0, iy + 2.0, cl - 24.0, ABRIR_ITEM - 4.0, 6.0, tema::DESTAQUE_SUAVE());
            } else if ui.sobre(x + 12.0, iy + 2.0, cl - 24.0, ABRIR_ITEM - 4.0) {
                ui.tela.retangulo(x + 12.0, iy + 2.0, cl - 24.0, ABRIR_ITEM - 4.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            let cor = if mais && i == linhas.len() - 1 { tema::DESTAQUE() } else { tema::TEXTO() };
            ui.tela.texto(x + 28.0, iy + 25.0, 13.5, cor, &desenho::encurtar(13.5, nome, cl - 60.0));
        }
        let rotulo = if ac.ext.is_empty() { "Sempre usar este app para este tipo de arquivo".to_string() } else { format!("Sempre usar este app para arquivos .{}", ac.ext) };
        let mut sempre = ac.sempre;
        penas::componentes::caixa_marcar(ui, x + 28.0, fundo + 18.0, &rotulo, &mut sempre);
        let pode = !vis.is_empty();
        let (bx, by) = (x + cl - 130.0, y + ca - 56.0);
        let s = ui.sobre(bx, by, 110.0, 36.0) && pode;
        ui.tela.retangulo(bx, by, 110.0, 36.0, 8.0, if !pode { tema::BORDA() } else if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        let lt = desenho::medir(13.5, "Abrir");
        ui.tela.texto(bx + 55.0 - lt / 2.0, by + 23.0, 13.5, tema::BRANCO(), "Abrir");
        let cx = x + cl - 250.0;
        let s = ui.sobre(cx, by, 110.0, 36.0);
        ui.tela.retangulo(cx, by, 110.0, 36.0, 8.0, tema::BORDA());
        ui.tela.retangulo(cx + 1.0, by + 1.0, 108.0, 34.0, 7.0, if s { tema::SUPERFICIE_ALTA() } else { tema::JANELA() });
        ui.tela.texto(cx + 24.0, by + 23.0, 13.5, tema::TEXTO(), "Cancelar");
    }

    fn caixa_executar(l: f32, a: f32) -> (f32, f32, f32, f32) {
        let (cl, ca) = (480.0_f32.min(l - 20.0), 210.0);
        ((l - cl) / 2.0, (a - ca) / 2.0, cl, ca)
    }

    /// Dá permissão de execução e roda o AppImage (sem precisar de FUSE).
    fn rodar_appimage(&mut self, app: &Path) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = fs::metadata(app) {
            let mut p = m.permissions();
            p.set_mode(p.mode() | 0o700);
            let _ = fs::set_permissions(app, p);
        }
        match Command::new(app).env("APPIMAGE_EXTRACT_AND_RUN", "1").current_dir(app.parent().unwrap_or(Path::new("/"))).spawn() {
            Ok(_) => self.aviso = Some("Abrindo o aplicativo… (a primeira vez pode demorar alguns segundos)".into()),
            Err(e) => self.aviso = Some(format!("Não deu para executar: {e}")),
        }
    }

    fn caixa_propriedades(&self, l: f32, a: f32) -> (f32, f32, f32, f32) {
        let n = self.propriedades.as_ref().map(|p| p.linhas.len()).unwrap_or(0) as f32;
        let (cl, ca) = (460.0_f32.min(l - 20.0), (150.0 + n * 30.0 + 70.0).min(a - 20.0));
        ((l - cl) / 2.0, (a - ca) / 2.0, cl, ca)
    }

    fn desenhar_propriedades(&mut self, ui: &mut Ui, l: f32, a: f32) {
        let (x, y, cl, ca) = self.caixa_propriedades(l, a);
        let Some(p) = &self.propriedades else { return };
        ui.tela.retangulo(0.0, 0.0, l, a, 0.0, Cor(11, 26, 46, 90));
        ui.tela.retangulo(x + 3.0, y + 6.0, cl, ca, 12.0, Cor(0, 0, 0, 50));
        ui.tela.retangulo(x, y, cl, ca, 12.0, tema::BORDA());
        ui.tela.retangulo(x + 1.0, y + 1.0, cl - 2.0, ca - 2.0, 11.0, tema::JANELA());
        ui.tela.texto(x + 24.0, y + 36.0, 15.0, tema::TEXTO(), "Propriedades");
        // Ícone e nome
        if p.pasta {
            icone_pasta(ui.tela, x + 24.0, y + 56.0, 48.0);
        } else {
            match p.categoria {
                Some(c) => tipos::icone_tipo(ui.tela, c, x + 24.0, y + 56.0, 48.0),
                None => icone_arquivo(ui.tela, x + 24.0, y + 56.0, 48.0),
            }
        }
        ui.tela.retangulo(x + 88.0, y + 64.0, cl - 112.0, 32.0, 5.0, tema::BORDA());
        ui.tela.retangulo(x + 89.0, y + 65.0, cl - 114.0, 30.0, 4.0, tema::SUPERFICIE());
        ui.tela.texto(x + 98.0, y + 85.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &p.titulo, cl - 130.0));
        ui.tela.retangulo(x + 24.0, y + 118.0, cl - 48.0, 1.0, 0.0, tema::BORDA());
        let mut ly = y + 146.0;
        for (k, v) in &p.linhas {
            ui.tela.texto(x + 24.0, ly, 12.5, tema::TEXTO_SECUNDARIO(), &format!("{k}:"));
            ui.tela.texto(x + 140.0, ly, 12.5, tema::TEXTO(), &desenho::encurtar(12.5, v, cl - 164.0));
            ly += 30.0;
        }
        let (bx, by) = (x + cl - 120.0, y + ca - 54.0);
        let sobre = ui.sobre(bx, by, 100.0, 36.0);
        ui.tela.retangulo(bx, by, 100.0, 36.0, 8.0, if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        let lt = desenho::medir(13.5, "OK");
        ui.tela.texto(bx + 50.0 - lt / 2.0, by + 23.0, 13.5, tema::BRANCO(), "OK");
    }

    fn desenhar_barra(&mut self, ui: &mut Ui, l: f32) {
        let t = &mut *ui.tela;
        t.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO());
        t.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA());
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
                ui.tela.retangulo(x, 10.0, 32.0, 32.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            let cor = if *ativo { tema::TEXTO() } else { tema::BORDA() };
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
        // Caixa de busca à direita (só dentro de pastas).
        let busca_l = if matches!(self.local, Local::Pasta(_)) && l > 640.0 { 240.0 } else { 0.0 };
        let el = l - ex - 12.0 - if busca_l > 0.0 { busca_l + 10.0 } else { 0.0 };
        if busca_l > 0.0 {
            let bx = l - 12.0 - busca_l;
            let foco = self.busca_ativa;
            ui.tela.retangulo(bx, 10.0, busca_l, 32.0, 6.0, if foco { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(bx + 1.0, 11.0, busca_l - 2.0, 30.0, 5.0, tema::JANELA());
            // Lupa
            let (lx, ly) = (bx + 18.0, 24.0);
            ui.tela.retangulo(lx - 6.0, ly - 6.0, 12.0, 12.0, 6.0, tema::TEXTO_SECUNDARIO());
            ui.tela.retangulo(lx - 4.5, ly - 4.5, 9.0, 9.0, 4.5, tema::JANELA());
            ui.tela.linha(lx + 4.0, ly + 4.0, lx + 8.0, ly + 8.0, 1.8, tema::TEXTO_SECUNDARIO());
            if self.busca.is_empty() && !foco {
                let nome = format!("Pesquisar em {}", self.titulo());
                ui.tela.texto(bx + 34.0, 31.0, 13.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(13.0, &nome, busca_l - 44.0));
            } else {
                let vis = desenho::encurtar(13.0, &self.busca, busca_l - 50.0);
                let fim = ui.tela.texto(bx + 34.0, 31.0, 13.0, tema::TEXTO(), &vis);
                if foco {
                    ui.tela.retangulo(bx + 35.0 + fim, 16.0, 1.5, 20.0, 0.0, tema::DESTAQUE());
                }
            }
            if ui.clique {
                self.busca_ativa = ui.sobre(bx, 10.0, busca_l, 32.0);
            }
        }
        ui.tela.retangulo(ex, 10.0, el, 32.0, 6.0, tema::BORDA());
        ui.tela.retangulo(ex + 1.0, 11.0, el - 2.0, 30.0, 5.0, tema::JANELA());
        icone_computador(ui.tela, ex + 8.0, 14.0, 22.0);
        let mut x = ex + 36.0;
        let migalhas = self.migalhas();
        let mut ir = None;
        for (i, (nome, destino)) in migalhas.iter().enumerate() {
            if i > 0 {
                seta(ui.tela, x + 4.0, 26.0, 1.0, tema::TEXTO_SECUNDARIO());
                x += 14.0;
            }
            let lt = desenho::medir(13.0, nome);
            if x + lt > ex + el - 10.0 {
                ui.tela.texto(x, 31.0, 13.0, tema::TEXTO_SECUNDARIO(), "…");
                break;
            }
            if ui.sobre(x - 4.0, 13.0, lt + 8.0, 26.0) {
                ui.tela.retangulo(x - 4.0, 14.0, lt + 8.0, 24.0, 4.0, tema::DESTAQUE_SUAVE());
            }
            ui.tela.texto(x, 31.0, 13.0, tema::TEXTO(), nome);
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
        ui.tela.retangulo(0.0, BARRA_A, LATERAL_L, a - BARRA_A, 0.0, tema::FUNDO());
        ui.tela.retangulo(LATERAL_L - 1.0, BARRA_A, 1.0, a - BARRA_A, 0.0, tema::BORDA());
        let mut itens: Vec<(String, Local, u8)> = vec![("Este Computador".into(), Local::EsteComputador, 0)];
        for (nome, pasta) in PASTAS_USUARIO {
            itens.push((nome.into(), Local::Pasta(self.casa.join(pasta)), 1));
        }
        itens.push(("Pasta pessoal".into(), Local::Pasta(self.casa.clone()), 1));
        let arquivos_lixeira = self.lixeira().join("files");
        let _ = fs::create_dir_all(&arquivos_lixeira);
        itens.push(("Lixeira".into(), Local::Pasta(arquivos_lixeira), 2));
        let mut y = BARRA_A + 10.0;
        let mut ir = None;
        let arrastando = matches!(self.arraste, Some((_, _, true)));
        for (i, (nome, destino, tipo)) in itens.iter().enumerate() {
            if i == 1 {
                ui.tela.texto(16.0, y + 18.0, 11.5, tema::TEXTO_SECUNDARIO(), "Acesso rápido");
                y += 26.0;
            }
            if *tipo == 2 {
                y += 10.0;
            }
            let ativo = &self.local == destino;
            let sobre = ui.sobre(8.0, y, LATERAL_L - 16.0, 30.0);
            if arrastando && sobre && *tipo == 1 {
                if let Local::Pasta(p) = destino {
                    self.alvo_soltar = Some(p.clone());
                }
                ui.tela.retangulo(8.0, y, LATERAL_L - 16.0, 30.0, 6.0, tema::DESTAQUE());
            } else if ativo {
                ui.tela.retangulo(8.0, y, LATERAL_L - 16.0, 30.0, 6.0, tema::DESTAQUE_SUAVE());
            } else if sobre {
                ui.tela.retangulo(8.0, y, LATERAL_L - 16.0, 30.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            match tipo {
                0 => icone_computador(ui.tela, 16.0, y + 4.0, 22.0),
                2 => icone_lixeira(ui.tela, 16.0, y + 4.0, 22.0),
                _ => icone_pasta(ui.tela, 16.0, y + 4.0, 22.0),
            }
            ui.tela.texto(46.0, y + 20.0, 13.0, tema::TEXTO(), nome);
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
        // Pendrives entram e saem: relê as unidades a cada 3 s.
        if self.unidades_em.map(|t| t.elapsed().as_secs() >= 3).unwrap_or(true) {
            self.unidades = ler_unidades();
            self.unidades_em = Some(std::time::Instant::now());
        }
        ui.acordar_em = Some(std::time::Duration::from_secs(3));
        let mut ejetar: Option<(String, PathBuf)> = None;
        let mut y = y0 + 34.0;
        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO(), "Pastas");
        y += 14.0;
        let mut ir = None;
        let tile = 150.0;
        let por_linha = ((l - 48.0) / (tile + 10.0)).floor().max(1.0) as usize;
        for (i, (nome, pasta)) in PASTAS_USUARIO.iter().enumerate() {
            let (c, r) = (i % por_linha, i / por_linha);
            let (x, ty) = (x0 + 24.0 + c as f32 * (tile + 10.0), y + r as f32 * 70.0);
            if ui.sobre(x, ty, tile, 62.0) {
                ui.tela.retangulo(x, ty, tile, 62.0, 8.0, tema::DESTAQUE_SUAVE());
            }
            icone_pasta(ui.tela, x + 8.0, ty + 11.0, 40.0);
            let n = desenho::encurtar(13.0, nome, tile - 60.0);
            ui.tela.texto(x + 54.0, ty + 36.0, 13.0, tema::TEXTO(), &n);
            if ui.clicou_duplo(x, ty, tile, 62.0) {
                ir = Some(Local::Pasta(self.casa.join(pasta)));
            }
        }
        let linhas = PASTAS_USUARIO.len().div_ceil(por_linha);
        y += linhas as f32 * 70.0 + 30.0;

        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO(), "Dispositivos e unidades");
        y += 14.0;
        let ul = 300.0;
        let por_linha = ((l - 48.0) / (ul + 12.0)).floor().max(1.0) as usize;
        let locais: Vec<Unidade> = self.unidades.iter().filter(|u| !u.rede).cloned().collect();
        for (i, u) in locais.iter().enumerate() {
            let (c, r) = (i % por_linha, i / por_linha);
            let (x, uy) = (x0 + 24.0 + c as f32 * (ul + 12.0), y + r as f32 * 84.0);
            if ui.sobre(x, uy, ul, 76.0) {
                ui.tela.retangulo(x, uy, ul, 76.0, 8.0, tema::DESTAQUE_SUAVE());
            }
            icone_unidade(ui.tela, x + 8.0, uy + 12.0, 52.0, u.letra == 'C');
            let montada = !u.ponto.as_os_str().is_empty();
            let nome = format!("{} ({}:)", u.nome, u.letra);
            let ln = if u.usb.is_some() && montada { ul - 150.0 } else { ul - 80.0 };
            ui.tela.texto(x + 70.0, uy + 24.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &nome, ln));
            // Pendrive montado: botão "Ejetar" no canto.
            if let (Some(dev), true) = (&u.usb, montada) {
                let (ex, ey, el) = (x + ul - 72.0, uy + 8.0, 64.0);
                let s = ui.sobre(ex, ey, el, 24.0);
                ui.tela.retangulo(ex, ey, el, 24.0, 6.0, if s { tema::DESTAQUE() } else { tema::SUPERFICIE_ALTA() });
                let lt = desenho::medir(12.0, "Ejetar");
                ui.tela.texto(ex + (el - lt) / 2.0, ey + 16.5, 12.0, if s { tema::BRANCO() } else { tema::TEXTO() }, "Ejetar");
                if ui.clicou(ex, ey, el, 24.0) {
                    ejetar = Some((dev.clone(), u.ponto.clone()));
                    ui.duplo = false;
                }
            }
            let bl = ul - 86.0;
            ui.tela.retangulo(x + 70.0, uy + 34.0, bl, 12.0, 2.0, tema::SUPERFICIE_ALTA());
            if !montada {
                ui.tela.texto(x + 70.0, uy + 64.0, 12.0, tema::TEXTO_SECUNDARIO(), &format!("{} · clique duas vezes para abrir", tamanho_legivel(u.total)));
            } else if u.total > 0 {
                let usado = 1.0 - u.livre as f32 / u.total as f32;
                let cor = if usado > 0.9 { tema::ERRO() } else { tema::DESTAQUE() };
                ui.tela.retangulo(x + 70.0, uy + 34.0, bl * usado, 12.0, 2.0, cor);
                let txt = format!("{} livres de {}", tamanho_legivel(u.livre), tamanho_legivel(u.total));
                ui.tela.texto(x + 70.0, uy + 64.0, 12.0, tema::TEXTO_SECUNDARIO(), &txt);
            }
            if ui.clicou_duplo(x, uy, ul, 76.0) {
                if montada {
                    ir = Some(Local::Pasta(u.ponto.clone()));
                } else if let Some(dev) = &u.usb {
                    match penas::sistema::pedir(&format!("usb montar {dev}")) {
                        Ok(p) => {
                            ir = Some(Local::Pasta(PathBuf::from(p)));
                            self.unidades_em = None;
                        }
                        Err(e) => self.aviso = Some(format!("Não deu para abrir: {e}.")),
                    }
                }
            }
        }
        if let Some((dev, ponto)) = ejetar {
            match penas::sistema::pedir(&format!("usb ejetar {dev}")) {
                Ok(_) => {
                    self.aviso = Some("Pode remover o dispositivo com segurança.".into());
                    if matches!(&self.local, Local::Pasta(p) if p.starts_with(&ponto)) {
                        self.local = Local::EsteComputador;
                    }
                }
                Err(e) => self.aviso = Some(format!("Não deu para ejetar: {e}.")),
            }
            self.unidades_em = None;
        }
        let linhas = locais.len().div_ceil(por_linha);
        y += linhas as f32 * 84.0 + 30.0;

        ui.tela.texto(x0 + 24.0, y, 15.0, tema::TEXTO(), "Locais de rede");
        y += 14.0;
        let redes: Vec<Unidade> = self.unidades.iter().filter(|u| u.rede).cloned().collect();
        for (i, u) in redes.iter().enumerate() {
            let (c, r) = (i % por_linha, i / por_linha);
            let (x, uy) = (x0 + 24.0 + c as f32 * (ul + 12.0), y + r as f32 * 84.0);
            if ui.sobre(x, uy, ul, 76.0) {
                ui.tela.retangulo(x, uy, ul, 76.0, 8.0, tema::DESTAQUE_SUAVE());
            }
            desenhar_icone(ui.tela, "unidade-rede", x + 8.0, uy + 12.0, 52.0);
            let nome = format!("{} ({}:)", u.nome, u.letra);
            ui.tela.texto(x + 70.0, uy + 24.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &nome, ul - 80.0));
            let bl = ul - 86.0;
            ui.tela.retangulo(x + 70.0, uy + 34.0, bl, 12.0, 2.0, tema::SUPERFICIE_ALTA());
            if u.total > 0 {
                let usado = 1.0 - u.livre as f32 / u.total as f32;
                ui.tela.retangulo(x + 70.0, uy + 34.0, bl * usado, 12.0, 2.0, tema::DESTAQUE());
                let txt = format!("{} livres de {}", tamanho_legivel(u.livre), tamanho_legivel(u.total));
                ui.tela.texto(x + 70.0, uy + 64.0, 12.0, tema::TEXTO_SECUNDARIO(), &txt);
            }
            if ui.clicou_duplo(x, uy, ul, 76.0) {
                ir = Some(Local::Pasta(u.ponto.clone()));
            }
            if ui.clicou_direito(x, uy, ul, 76.0) {
                if let Some((mx, my)) = ui.mouse {
                    self.menu_unidade = Some((mx, my, u.letra));
                }
            }
        }
        y += redes.len().div_ceil(por_linha) as f32 * 84.0;
        desenhar_icone(ui.tela, "unidade-rede", x0 + 24.0, y + 2.0, 40.0);
        let bx = x0 + 74.0;
        let bl = 230.0;
        let sobre = ui.sobre(bx, y, bl, 40.0);
        ui.tela.retangulo(bx, y, bl, 40.0, 8.0, if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        ui.tela.texto(bx + 18.0, y + 25.0, 13.5, tema::BRANCO(), "Mapear unidade de rede…");
        if ui.clicou(bx, y, bl, 40.0) {
            self.abrir_mapear();
        }
        if let Some(d) = ir {
            self.ir(d);
        }
    }

    fn cabecalho(t: &mut Tela, c: [f32; 6], largo: bool, rotulo_data: &str) {
        let [x0, y0, l, col_data, col_tipo, col_tam] = c;
        t.retangulo(x0, y0, l, 30.0, 0.0, tema::SUPERFICIE());
        t.texto(x0 + 52.0, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO(), "Nome");
        if l > 520.0 {
            t.texto(col_data, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO(), rotulo_data);
            if largo {
                t.texto(col_tipo, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO(), "Tipo");
            }
            t.texto(col_tam, y0 + 20.0, 12.0, tema::TEXTO_SECUNDARIO(), "Tamanho");
        }
        t.retangulo(x0 + 12.0, y0 + 29.0, l - 24.0, 1.0, 0.0, tema::BORDA());
    }

    fn desenhar_previa(&mut self, ui: &mut Ui, x0: f32, y0: f32, l: f32, a: f32) {
        ui.tela.retangulo(x0, y0, l, a, 0.0, tema::FUNDO());
        ui.tela.retangulo(x0, y0, 1.0, a, 0.0, tema::BORDA());
        if let Some(rx) = &self.previa_rx {
            match rx.try_recv() {
                Ok(p) => {
                    self.previa = Some(p);
                    self.previa_rx = None;
                }
                Err(_) => ui.acordar_em = Some(std::time::Duration::from_millis(100)),
            }
        }
        let x = x0 + 20.0;
        let w = l - 40.0;
        let sel = if self.selecionados.len() > 1 { None } else { self.selecionado.and_then(|i| self.entradas.get(i)) };
        let Some(e) = sel else {
            let msg = if self.selecionados.len() > 1 { format!("{} itens selecionados", self.selecionados.len()) } else { "Selecione um arquivo para ver a prévia.".to_string() };
            penas::componentes::paragrafo(ui, x, y0 + 40.0, w, &msg);
            return;
        };
        let (nome, caminho, pasta, tamanho, modificado) = (e.nome.clone(), e.caminho.clone(), e.pasta, e.tamanho, e.modificado);
        let (cat, tipo) = if pasta { (None, "Pasta de arquivos".to_string()) } else {
            let (c, t) = tipos::tipo_de(&nome);
            (Some(c), t)
        };
        // Pede a prévia do arquivo novo.
        if self.previa.as_ref().map(|p| p.0 != caminho).unwrap_or(true) && self.previa_rx.as_ref().is_none() {
            if let Some(c) = cat.filter(|c| matches!(c, tipos::Categoria::Imagem | tipos::Categoria::Texto | tipos::Categoria::Codigo)).filter(|_| tamanho < 64_000_000) {
                let (tx, rx) = std::sync::mpsc::channel();
                let cam = caminho.clone();
                let lado = w as u32;
                std::thread::spawn(move || {
                    let p = fazer_previa(&cam, c, lado);
                    let _ = tx.send((cam, p));
                });
                self.previa = Some((caminho.clone(), Previa::Carregando));
                self.previa_rx = Some(rx);
            } else {
                self.previa = Some((caminho.clone(), Previa::Nada));
            }
        }
        let mut y = y0 + 24.0;
        let mut extra: Option<String> = None;
        match self.previa.as_ref().map(|p| &p.1) {
            Some(Previa::Imagem(img, (iw, ih))) => {
                let ix = x + (w - img.width() as f32) / 2.0;
                ui.tela.imagem(ix.round() as i32, y.round() as i32, img);
                y += img.height() as f32 + 16.0;
                extra = Some(format!("{iw} × {ih} pixels"));
            }
            Some(Previa::Texto(linhas)) => {
                let h = 220.0;
                ui.tela.retangulo(x, y, w, h, 6.0, tema::SUPERFICIE());
                let mut ty = y + 18.0;
                for li in linhas {
                    if ty > y + h - 6.0 {
                        break;
                    }
                    ui.tela.texto(x + 10.0, ty, 11.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(11.0, li, w - 20.0));
                    ty += 15.0;
                }
                y += h + 16.0;
            }
            Some(Previa::Carregando) => {
                ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Carregando a prévia…");
                y += 40.0;
            }
            _ => {
                let s = 96.0;
                match cat {
                    None => icone_pasta(ui.tela, x + (w - s) / 2.0, y, s),
                    Some(c) => tipos::icone_tipo(ui.tela, c, x + (w - s) / 2.0, y, s),
                }
                y += s + 16.0;
            }
        }
        ui.tela.texto(x, y + 16.0, 14.5, tema::TEXTO(), &desenho::encurtar(14.5, &nome, w));
        ui.tela.texto(x, y + 38.0, 12.0, tema::TEXTO_SECUNDARIO(), &tipo);
        y += 56.0;
        let mut linhas: Vec<(&str, String)> = Vec::new();
        if !pasta {
            linhas.push(("Tamanho", tamanho_legivel(tamanho)));
        }
        if let Some(m) = modificado {
            linhas.push(("Modificado", data_legivel(m)));
        }
        if let Some(d) = extra {
            linhas.push(("Dimensões", d));
        }
        for (k, v) in linhas {
            ui.tela.texto(x, y + 16.0, 11.5, tema::TEXTO_SECUNDARIO(), k);
            ui.tela.texto(x, y + 34.0, 12.5, tema::TEXTO(), &desenho::encurtar(12.5, &v, w));
            y += 44.0;
        }
    }

    fn desenhar_pasta(&mut self, ui: &mut Ui, x0: f32, y0: f32, l: f32, a: f32) {
        // Na Lixeira, uma faixa com "Esvaziar Lixeira" em cima da lista.
        let lixeira = self.na_lixeira();
        let (y0, a) = if lixeira {
            ui.tela.retangulo(x0, y0, l, 44.0, 0.0, tema::FUNDO());
            ui.tela.texto(x0 + 20.0, y0 + 27.0, 13.0, tema::TEXTO_SECUNDARIO(), "Itens na Lixeira podem ser restaurados com o botão direito.");
            let (bx, bl) = (x0 + l - 170.0, 154.0);
            let ativo = !self.todas.is_empty();
            let sobre = ativo && ui.sobre(bx, y0 + 7.0, bl, 30.0);
            ui.tela.retangulo(bx, y0 + 7.0, bl, 30.0, 6.0, if !ativo { tema::SUPERFICIE_ALTA() } else if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
            let lt = desenho::medir(13.0, "Esvaziar Lixeira");
            ui.tela.texto(bx + bl / 2.0 - lt / 2.0, y0 + 27.0, 13.0, if ativo { tema::BRANCO() } else { tema::TEXTO_SECUNDARIO() }, "Esvaziar Lixeira");
            if ativo && ui.clicou(bx, y0 + 7.0, bl, 30.0) {
                self.executar(Acao::EsvaziarLixeira, None);
                ui.clique = false;
            }
            (y0 + 44.0, a - 44.0)
        } else {
            (y0, a)
        };
        // Cabeçalho das colunas: Nome | Data (ou Pasta, na busca) | Tipo | Tamanho.
        let largo = l > 760.0;
        let col_tam = x0 + l - 110.0;
        let col_tipo = x0 + l - 290.0;
        let col_data = if largo { x0 + l - 440.0 } else { x0 + l - 260.0 };
        let rotulo_data = if self.busca_funda { "Pasta" } else { "Data de modificação" };
        let colunas = [x0, y0, l, col_data, col_tipo, col_tam];
        Self::cabecalho(ui.tela, colunas, largo, rotulo_data);

        let topo = y0 + 34.0;
        let visivel = a - 34.0;
        let arrastando = matches!(self.arraste, Some((_, _, true)));
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
            let msg = if !self.busca.is_empty() {
                "Nenhum item corresponde à pesquisa. Enter procura nas subpastas."
            } else if lixeira {
                "A Lixeira está vazia."
            } else {
                "Esta pasta está vazia."
            };
            ui.tela.texto(x0 + 52.0, topo + 30.0, 13.0, tema::TEXTO_SECUNDARIO(), msg);
        }
        let mut abrir = None;
        let primeiro = (self.rolagem / LINHA_A).floor() as usize;
        for i in primeiro..self.entradas.len() {
            let y = topo + i as f32 * LINHA_A - self.rolagem;
            if y > y0 + a {
                break;
            }
            let e = &self.entradas[i];
            let sel = self.selecionados.contains(&i) || (self.selecionados.is_empty() && self.selecionado == Some(i));
            let sobre_linha =
                ui.sobre(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) && ui.mouse.map(|m| m.1 > topo).unwrap_or(false);
            if arrastando && sobre_linha && e.pasta && !sel {
                // Pasta que recebe o que está sendo arrastado.
                self.alvo_soltar = Some(e.caminho.clone());
                ui.tela.retangulo(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0, 4.0, tema::DESTAQUE());
            } else if sel {
                ui.tela.retangulo(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0, 4.0, tema::DESTAQUE_SUAVE());
                if self.selecionado == Some(i) && self.selecionados.len() > 1 {
                    ui.tela.retangulo(x0 + 8.0, y, 3.0, LINHA_A - 2.0, 1.5, tema::DESTAQUE());
                }
            } else if sobre_linha {
                ui.tela.retangulo(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0, 4.0, tema::SUPERFICIE_ALTA());
            }
            let (cat, tipo) = if e.pasta { (None, "Pasta de arquivos".to_string()) } else {
                let (c, t) = tipos::tipo_de(&e.nome);
                (Some(c), t)
            };
            match cat {
                None => icone_pasta(ui.tela, x0 + 20.0, y + 3.0, 22.0),
                Some(c) => tipos::icone_tipo(ui.tela, c, x0 + 20.0, y + 3.0, 22.0),
            }
            let max_nome = if l > 520.0 { col_data - x0 - 64.0 } else { l - 70.0 };
            match &self.renomeando {
                Some((j, texto)) if *j == i => {
                    ui.tela.retangulo(x0 + 48.0, y + 2.0, max_nome + 8.0, LINHA_A - 6.0, 4.0, tema::DESTAQUE());
                    ui.tela.retangulo(x0 + 49.5, y + 3.5, max_nome + 5.0, LINHA_A - 9.0, 3.0, tema::JANELA());
                    let fim = ui.tela.texto(x0 + 54.0, y + 19.0, 13.0, tema::TEXTO(), texto);
                    ui.tela.retangulo(x0 + 55.0 + fim, y + 7.0, 1.5, LINHA_A - 16.0, 0.0, tema::DESTAQUE());
                }
                _ => {
                    let corte = self
                        .transferencia
                        .as_ref()
                        .map(|(v, c)| *c && v.contains(&e.caminho))
                        .unwrap_or(false);
                    let cor = if corte { tema::TEXTO_SECUNDARIO() } else { tema::TEXTO() };
                    ui.tela.texto(x0 + 52.0, y + 19.0, 13.0, cor, &desenho::encurtar(13.0, &e.nome, max_nome));
                }
            }
            if l > 520.0 {
                let largura_data = if largo { col_tipo - col_data - 12.0 } else { col_tam - col_data - 12.0 };
                if self.busca_funda {
                    let pai = e.caminho.parent().map(|p| p.display().to_string()).unwrap_or_default();
                    ui.tela.texto(col_data, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, &pai, largura_data));
                } else if let Some(m) = e.modificado {
                    ui.tela.texto(col_data, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO(), &data_legivel(m));
                }
                if largo {
                    ui.tela.texto(col_tipo, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, &tipo, col_tam - col_tipo - 12.0));
                }
                if !e.pasta {
                    ui.tela.texto(col_tam, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO(), &tamanho_legivel(e.tamanho));
                }
            }
            if y >= topo - 1.0 && ui.clicou(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) {
                if self.renomeando.as_ref().map(|(j, _)| *j != i).unwrap_or(false) {
                    self.confirmar_renomear();
                }
                if ui.ctrl {
                    // Ctrl+clique: marca ou desmarca.
                    if self.selecionados.contains(&i) {
                        self.selecionados.remove(&i);
                    } else {
                        if let (true, Some(f)) = (self.selecionados.is_empty(), self.selecionado) {
                            self.selecionados.insert(f);
                        }
                        self.selecionados.insert(i);
                    }
                    self.selecionado = if self.selecionados.is_empty() { None } else { Some(i) };
                } else if ui.shift && self.selecionado.is_some() {
                    // Shift+clique: do foco até aqui.
                    let f = self.selecionado.unwrap_or(i);
                    self.selecionados = (f.min(i)..=f.max(i)).collect();
                } else if sel && self.selecionados.len() > 1 {
                    // Clique num item já marcado: talvez seja um arraste.
                    self.selecionado = Some(i);
                    self.colapsar = Some(i);
                    if let Some((mx, my)) = ui.mouse {
                        self.arraste = Some((mx, my, false));
                    }
                } else {
                    self.selecionar_so(i);
                    if let Some((mx, my)) = ui.mouse {
                        self.arraste = Some((mx, my, false));
                    }
                }
                if ui.duplo && self.renomeando.is_none() && !ui.ctrl && !ui.shift {
                    abrir = Some(self.entradas[i].clone());
                    self.arraste = None;
                }
            }
            if y >= topo - 1.0 && ui.clicou_direito(x0 + 8.0, y, l - 16.0, LINHA_A - 2.0) {
                if !self.selecionados.contains(&i) {
                    self.selecionar_so(i);
                }
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
                self.selecionados.clear();
            }
        }
        // Clique no fundo (fora dos itens) desmarca tudo.
        let fim_lista = topo + self.entradas.len() as f32 * LINHA_A - self.rolagem;
        // (A barra de baixo do modo escolher não conta: o clique em "Abrir"
        // não pode desmarcar o arquivo antes de entregar.)
        let fundo_lista = if self.escolha.is_some() { ui.altura() - 64.0 } else { ui.altura() };
        if ui.clique && !ui.ctrl && !ui.shift && ui.mouse.is_some_and(|(mx, my)| mx > x0 && my > fim_lista.max(topo) && my < fundo_lista) {
            self.selecionado = None;
            self.selecionados.clear();
        }
        if ui.clicou(x0, topo, l, a - 34.0) && self.renomeando.is_some() {
            self.confirmar_renomear();
        }
        // Cobre o que rolou para baixo do cabeçalho.
        Self::cabecalho(ui.tela, colunas, largo, rotulo_data);
        if let Some(e) = abrir {
            self.abrir(&e);
        }
    }
}

fn main() {
    // Chamado pela sessão ao entrar: reconecta as unidades de rede salvas.
    if std::env::args().nth(1).as_deref() == Some("--reconectar") {
        reconectar_tudo();
        return;
    }
    // Modo escolher (portal dos apps Flatpak):
    //   explorador --escolher abrir|salvar|pasta [--varios] [--nome N] [--pasta P] [--titulo T]
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--escolher") {
        let modo = match args.get(2).map(String::as_str) {
            Some("salvar") => ModoEscolha::Salvar,
            Some("pasta") => ModoEscolha::Pasta,
            _ => ModoEscolha::Abrir,
        };
        let valor = |chave: &str| args.iter().position(|a| a == chave).and_then(|i| args.get(i + 1)).cloned();
        let mut app = Explorador::novo();
        let inicio = valor("--pasta").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(|| app.casa.clone());
        app.ir(Local::Pasta(inicio));
        app.voltar.clear();
        app.escolha = Some(Escolha {
            modo,
            varios: args.iter().any(|a| a == "--varios"),
            nome: valor("--nome").unwrap_or_default(),
            nome_ativo: modo == ModoEscolha::Salvar,
            confirmar: false,
        });
        let titulo = valor("--titulo").unwrap_or_else(|| {
            match modo {
                ModoEscolha::Abrir => "Abrir",
                ModoEscolha::Salvar => "Salvar como",
                ModoEscolha::Pasta => "Escolher pasta",
            }
            .into()
        });
        penas::executar(
            Opcoes { titulo, app_id: "explorador-escolha".into(), tamanho: (900, 600), minimo: (560, 380) },
            Box::new(app),
        );
        return;
    }
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

/// Desfaz o %XX de um caminho vindo de uma URI file://.
fn desfazer_uri(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
