// SPDX-License-Identifier: GPL-3.0-or-later
//! Central de Apps do Ararinha: busca, instala, atualiza e remove aplicativos
//! do Flathub (pelo comando flatpak, instalando para o usuário).

use std::sync::{Arc, Mutex};
use std::{
    process::Command,
    sync::mpsc::{channel, Receiver},
    thread,
};

use penas::{desenho, tema, App, Keysym, Opcoes, Tecla, Ui};

const BARRA_A: f32 = 64.0;
const LINHA_A: f32 = 64.0;

/// Sugestões da página inicial: id do Flathub, nome e descrição.
const DESTAQUES: [(&str, &str, &str); 12] = [
    ("org.chromium.Chromium", "Chromium", "Navegador de internet rápido e seguro"),
    ("org.mozilla.firefox", "Firefox", "Navegador livre da Mozilla"),
    ("org.videolan.VLC", "VLC", "Toca qualquer vídeo e música"),
    ("com.spotify.Client", "Spotify", "Músicas e podcasts"),
    ("com.discordapp.Discord", "Discord", "Conversas por voz, vídeo e texto"),
    ("org.telegram.desktop", "Telegram", "Mensagens rápidas e seguras"),
    ("org.libreoffice.LibreOffice", "LibreOffice", "Escritório: textos, planilhas e apresentações"),
    ("org.gimp.GIMP", "GIMP", "Editor de imagens"),
    ("com.obsproject.Studio", "OBS Studio", "Gravação de tela e transmissões ao vivo"),
    ("com.valvesoftware.Steam", "Steam", "Jogos da Steam"),
    ("org.kde.krita", "Krita", "Pintura digital"),
    ("com.visualstudio.code", "Visual Studio Code", "Editor de código"),
];

#[derive(Debug, Clone, PartialEq)]
struct AppInfo {
    id: String,
    nome: String,
    descricao: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Aba {
    Inicio,
    Instalados,
}

enum Resultado {
    Busca(Vec<AppInfo>),
    Instalados(Vec<AppInfo>),
    Feito(String),
    Erro(String),
}

struct Central {
    aba: Aba,
    busca: String,
    resultados: Option<Vec<AppInfo>>,
    instalados: Vec<AppInfo>,
    /// Trabalho em andamento: descrição e canal da resposta.
    trabalho: Option<(String, Receiver<Resultado>)>,
    aviso: Option<String>,
    rolagem: f32,
    quadro: u64,
    /// Progresso da instalação/atualização em andamento (o flatpak informa).
    progresso: Arc<Mutex<Progresso>>,
    /// Ícones já lidos do catálogo do Flathub (None = não tem).
    icones: std::cell::RefCell<std::collections::HashMap<String, Option<penas::desenho::Imagem>>>,
}

/// Ícone do app: o catálogo (appstream) do Flathub traz um PNG de cada um;
/// o app instalado também exporta o seu.
fn achar_icone(id: &str) -> Option<penas::desenho::Imagem> {
    let casa = std::env::var("HOME").unwrap_or_default();
    let arq = std::env::consts::ARCH;
    let mut caminhos = Vec::new();
    for raiz in [format!("{casa}/.local/share/flatpak"), "/var/lib/flatpak".to_string()] {
        for t in ["128x128", "64x64"] {
            caminhos.push(format!("{raiz}/appstream/flathub/{arq}/active/icons/{t}/{id}.png"));
            caminhos.push(format!("{raiz}/exports/share/icons/hicolor/{t}/apps/{id}.png"));
        }
    }
    caminhos.iter().find_map(|c| std::fs::read(c).ok().and_then(|b| penas::desenho::icone(&b, 40, 40)))
}

fn flatpak(args: &[&str]) -> Result<String, String> {
    // Prioridade baixa (nice/ionice): instalar não trava o resto do sistema.
    match Command::new("nice").arg("-n").arg("15").arg("flatpak").args(args).output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("").to_string()),
        Err(e) => Err(format!("Flatpak não encontrado: {e}")),
    }
}

/// O que o flatpak está fazendo agora.
#[derive(Default)]
struct Progresso {
    /// 0 a 100, quando o flatpak mostra porcentagem.
    pct: Option<f32>,
    /// Última linha de estado ("Instalando 2/3… 45% 1,2 MB/s").
    texto: String,
    /// Grupo de processos do flatpak (para cancelar).
    grupo: Option<i32>,
    cancelado: bool,
}

extern "C" {
    fn kill(pid: i32, sinal: i32) -> i32;
}

/// Tira as sequências de cor/cursor (ESC [ ... letra) de uma linha de terminal.
fn sem_escapes(s: &str) -> String {
    let mut r = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                while let Some(d) = it.next() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_control() {
            r.push(c);
        }
    }
    r
}

/// Roda o flatpak num terminal falso (o `script`), para ele mostrar o
/// progresso, e vai lendo a porcentagem. Pode ser cancelado.
fn flatpak_com_progresso(args: &[&str], prog: &Arc<Mutex<Progresso>>) -> Result<(), String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    // Só ids e opções simples chegam aqui; nada vai para o sh sem conferir.
    if args.iter().any(|a| !a.chars().all(|c| c.is_ascii_alphanumeric() || "._-/=".contains(c))) {
        return Err("nome de app inválido".into());
    }
    let linha = format!("nice -n 15 flatpak {}", args.join(" "));
    let mut filho = Command::new("script")
        .args(["-qfec", &linha, "/dev/null"])
        .env("COLUMNS", "120")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("não deu para rodar o flatpak: {e}"))?;
    {
        let mut p = prog.lock().unwrap();
        *p = Progresso { grupo: Some(filho.id() as i32), texto: "Preparando…".into(), ..Default::default() };
    }
    let mut saida = filho.stdout.take().ok_or("sem saída")?;
    let mut buf = [0u8; 4096];
    let mut atual = String::new();
    let mut ultimas: Vec<String> = Vec::new();
    loop {
        let n = match saida.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for ch in String::from_utf8_lossy(&buf[..n]).chars() {
            if ch == '\r' || ch == '\n' {
                let limpa = sem_escapes(&atual).trim().to_string();
                atual.clear();
                if limpa.is_empty() {
                    continue;
                }
                let pct = limpa
                    .split_whitespace()
                    .filter_map(|t| t.strip_suffix('%').and_then(|n| n.parse::<f32>().ok()))
                    .last();
                let mut p = prog.lock().unwrap();
                if let Some(v) = pct {
                    p.pct = Some(v.clamp(0.0, 100.0));
                }
                // Sem as barrinhas de blocos do terminal.
                p.texto = limpa.chars().filter(|c| !('\u{2580}'..='\u{259F}').contains(c)).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
                drop(p);
                ultimas.push(limpa);
                if ultimas.len() > 8 {
                    ultimas.remove(0);
                }
            } else {
                atual.push(ch);
            }
        }
    }
    let status = filho.wait().map_err(|e| e.to_string())?;
    let cancelado = prog.lock().unwrap().cancelado;
    *prog.lock().unwrap() = Progresso::default();
    if cancelado {
        return Err("cancelado".into());
    }
    if status.success() {
        Ok(())
    } else {
        let erro = ultimas.iter().rev().find(|l| l.to_lowercase().contains("error") || l.contains("erro")).cloned();
        Err(erro.or_else(|| ultimas.last().cloned()).unwrap_or_else(|| "o flatpak parou com erro".into()))
    }
}

fn ler_tabela(saida: &str) -> Vec<AppInfo> {
    saida
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            (c.len() >= 2 && c[0].contains('.')).then(|| AppInfo {
                id: c[0].trim().to_string(),
                nome: c[1].trim().to_string(),
                descricao: c.get(2).map(|d| d.trim().to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

fn listar_instalados() -> Resultado {
    match flatpak(&["list", "--user", "--app", "--columns=application,name,description"]) {
        Ok(s) => Resultado::Instalados(ler_tabela(&s)),
        Err(e) => Resultado::Erro(e),
    }
}

impl Central {
    fn nova() -> Central {
        let mut c = Central {
            aba: Aba::Inicio,
            busca: String::new(),
            resultados: None,
            instalados: Vec::new(),
            trabalho: None,
            aviso: None,
            rolagem: 0.0,
            quadro: 0,
            icones: Default::default(),
            progresso: Default::default(),
        };
        c.rodar("Preparando o Flathub…", || {
            let _ = flatpak(&["remote-add", "--user", "--if-not-exists", "flathub", "/usr/share/flatpak/flathub.flatpakrepo"]);
            // Catálogo com os ícones (rápido se já estiver em dia).
            let _ = flatpak(&["update", "--user", "--appstream", "flathub"]);
            listar_instalados()
        });
        c
    }

    fn rodar(&mut self, descricao: &str, f: impl FnOnce() -> Resultado + Send + 'static) {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let _ = tx.send(f());
        });
        self.trabalho = Some((descricao.to_string(), rx));
    }

    fn buscar(&mut self) {
        let termo = self.busca.trim().to_string();
        if termo.is_empty() {
            self.resultados = None;
            return;
        }
        self.rodar(&format!("Procurando \"{termo}\"…"), move || {
            // Atualiza o catálogo (rápido se já estiver em dia) e procura.
            let _ = flatpak(&["update", "--user", "--appstream", "flathub"]);
            match flatpak(&["search", "--columns=application,name,description", &termo]) {
                Ok(s) => Resultado::Busca(ler_tabela(&s)),
                Err(e) => Resultado::Erro(e),
            }
        });
    }

    fn instalar(&mut self, app: &AppInfo) {
        let id = app.id.clone();
        let nome = app.nome.clone();
        let prog = self.progresso.clone();
        self.rodar(&format!("Instalando {nome}…"), move || {
            match flatpak_com_progresso(&["install", "--user", "-y", "flathub", &id], &prog) {
                Ok(_) => {
                    criar_atalho(&id);
                    Resultado::Feito(format!("{nome} instalado: está no menu Iniciar e na área de trabalho."))
                }
                Err(e) if e == "cancelado" => {
                    // Desfaz o que ficou pela metade.
                    let _ = flatpak(&["uninstall", "--user", "-y", "--noninteractive", &id]);
                    Resultado::Feito(format!("Instalação de {nome} cancelada."))
                }
                Err(e) => Resultado::Erro(format!("Não deu para instalar {nome}: {e}")),
            }
        });
    }

    fn remover(&mut self, app: &AppInfo) {
        let id = app.id.clone();
        let nome = app.nome.clone();
        self.rodar(&format!("Removendo {nome}…"), move || {
            match flatpak(&["uninstall", "--user", "-y", "--noninteractive", &id]) {
                Ok(_) => {
                    if let Ok(casa) = std::env::var("HOME") {
                        let _ = std::fs::remove_file(format!("{casa}/Área de Trabalho/{id}.desktop"));
                    }
                    Resultado::Feito(format!("{nome} removido."))
                }
                Err(e) => Resultado::Erro(format!("Não deu para remover {nome}: {e}")),
            }
        });
    }

    fn atualizar_tudo(&mut self) {
        let prog = self.progresso.clone();
        self.rodar("Atualizando os aplicativos…", move || {
            match flatpak_com_progresso(&["update", "--user", "-y"], &prog) {
                Ok(_) => Resultado::Feito("Tudo atualizado.".into()),
                Err(e) if e == "cancelado" => Resultado::Feito("Atualização cancelada.".into()),
                Err(e) => Resultado::Erro(format!("Não deu para atualizar: {e}")),
            }
        });
    }

    fn abrir(app: &AppInfo) {
        let _ = Command::new("flatpak").args(["run", &app.id]).spawn();
    }

    fn esta_instalado(&self, id: &str) -> bool {
        self.instalados.iter().any(|a| a.id == id)
    }

    fn receber(&mut self) {
        let Some((_, rx)) = &self.trabalho else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.trabalho = None;
        // O catálogo pode ter chegado: tenta de novo os ícones que faltavam.
        self.icones.borrow_mut().retain(|_, v| v.is_some());
        match r {
            Resultado::Busca(v) => self.resultados = Some(v),
            Resultado::Instalados(v) => self.instalados = v,
            Resultado::Feito(msg) => {
                self.aviso = Some(msg);
                self.rodar("Atualizando a lista…", listar_instalados);
            }
            Resultado::Erro(e) => self.aviso = Some(if e.is_empty() { "Algo deu errado.".into() } else { e }),
        }
    }

    /// Uma linha de app com o botão de ação. Devolve a ação clicada.
    fn linha(&self, ui: &mut Ui, app: &AppInfo, x: f32, y: f32, l: f32) -> Option<u8> {
        if ui.sobre(x, y, l, LINHA_A - 6.0) {
            ui.tela.retangulo(x, y, l, LINHA_A - 6.0, 8.0, tema::SUPERFICIE_ALTA());
        }
        // Ícone do catálogo do Flathub; sem ele, a inicial num quadrado azul.
        let mut cache = self.icones.borrow_mut();
        let icone = cache.entry(app.id.clone()).or_insert_with(|| achar_icone(&app.id));
        if let Some(img) = icone {
            ui.tela.imagem((x + 10.0) as i32, (y + 9.0) as i32, img);
        } else {
            ui.tela.retangulo(x + 10.0, y + 9.0, 40.0, 40.0, 10.0, tema::DESTAQUE());
            let ini: String = app.nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let li = desenho::medir(20.0, &ini);
            ui.tela.texto(x + 30.0 - li / 2.0, y + 36.0, 20.0, tema::BRANCO(), &ini);
        }
        drop(cache);
        let largura_texto = l - 260.0;
        ui.tela.texto(x + 64.0, y + 26.0, 15.0, tema::TEXTO(), &desenho::encurtar(15.0, &app.nome, largura_texto));
        ui.tela.texto(x + 64.0, y + 46.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, &app.descricao, largura_texto));

        let ocupado = self.trabalho.is_some();
        let instalado = self.esta_instalado(&app.id);
        let botoes: Vec<(&str, u8, bool)> =
            if instalado { vec![("Abrir", 1, true), ("Remover", 2, false)] } else { vec![("Instalar", 0, true)] };
        let mut bx = x + l - 12.0;
        let mut acao = None;
        for (nome, codigo, principal) in botoes.iter().rev() {
            let bl = desenho::medir(13.0, nome) + 32.0;
            bx -= bl;
            let sobre = !ocupado && ui.sobre(bx, y + 13.0, bl, 32.0);
            let (fundo, cor) = if *principal {
                (if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() }, tema::BRANCO())
            } else {
                (if sobre { tema::SUPERFICIE_ALTA() } else { tema::FUNDO() }, tema::TEXTO())
            };
            let fundo = if ocupado { fundo.com_alfa(120) } else { fundo };
            ui.tela.retangulo(bx, y + 13.0, bl, 32.0, 8.0, fundo);
            ui.tela.texto(bx + 16.0, y + 34.0, 13.0, cor, nome);
            if !ocupado && ui.clicou(bx, y + 13.0, bl, 32.0) {
                acao = Some(*codigo);
            }
            bx -= 8.0;
        }
        acao
    }
}

impl App for Central {
    fn desenhar(&mut self, ui: &mut Ui) {
        self.receber();
        self.quadro += 1;
        let teclas = std::mem::take(&mut ui.teclas);
        let tinha = !self.busca.is_empty();
        let teclas = penas::componentes::editar(&mut self.busca, teclas);
        if tinha && self.busca.is_empty() {
            self.resultados = None;
        }
        for t in teclas {
            match t {
                Tecla::Enter => {
                    self.aba = Aba::Inicio;
                    self.buscar();
                }
                Tecla::Esc => {
                    self.busca.clear();
                    self.resultados = None;
                }
                Tecla::Outra(Keysym::F5) => self.rodar("Atualizando a lista…", listar_instalados),
                _ => {}
            }
        }
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());

        // Barra de cima: abas, busca e atualizar.
        ui.tela.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA());
        let mut x = 16.0;
        for (aba, nome) in [(Aba::Inicio, "Descobrir"), (Aba::Instalados, "Instalados")] {
            let lt = desenho::medir(14.0, nome);
            let ativa = self.aba == aba;
            if ativa || ui.sobre(x - 8.0, 14.0, lt + 16.0, 36.0) {
                ui.tela.retangulo(x - 8.0, 14.0, lt + 16.0, 36.0, 8.0, if ativa { tema::DESTAQUE_SUAVE() } else { tema::SUPERFICIE_ALTA() });
            }
            ui.tela.texto(x, 37.0, 14.0, tema::TEXTO(), nome);
            if ui.clicou(x - 8.0, 14.0, lt + 16.0, 36.0) {
                self.aba = aba;
                self.rolagem = 0.0;
            }
            x += lt + 28.0;
        }
        let (bx, bl) = (x + 10.0, (l - x - 190.0).max(160.0));
        ui.tela.retangulo(bx, 14.0, bl, 36.0, 18.0, tema::DESTAQUE());
        ui.tela.retangulo(bx + 2.0, 16.0, bl - 4.0, 32.0, 16.0, tema::JANELA());
        if self.busca.is_empty() {
            ui.tela.texto(bx + 18.0, 37.0, 14.0, tema::TEXTO_SECUNDARIO(), "Procurar aplicativos (Enter)");
        } else {
            ui.tela.texto(bx + 18.0, 37.0, 14.0, tema::TEXTO(), &self.busca);
            let cx = penas::componentes::x_do_cursor(&self.busca, 14.0, false);
            ui.tela.retangulo(bx + 19.0 + cx, 23.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
        }
        let ux = l - 160.0;
        let ocupado = self.trabalho.is_some();
        let sobre = !ocupado && ui.sobre(ux, 14.0, 144.0, 36.0);
        ui.tela.retangulo(ux, 14.0, 144.0, 36.0, 8.0, if sobre { tema::SUPERFICIE_ALTA() } else { tema::JANELA() });
        ui.tela.texto(ux + 16.0, 37.0, 13.5, tema::TEXTO(), "Atualizar tudo");
        if !ocupado && ui.clicou(ux, 14.0, 144.0, 36.0) {
            self.atualizar_tudo();
        }

        // Conteúdo.
        let topo = BARRA_A + 12.0;
        let lista: Vec<AppInfo>;
        let titulo: String;
        match (self.aba, &self.resultados) {
            (Aba::Inicio, Some(r)) => {
                titulo = format!("Resultados para \"{}\"", self.busca.trim());
                lista = r.clone();
            }
            (Aba::Inicio, None) => {
                titulo = "Destaques do Flathub".into();
                lista = DESTAQUES
                    .iter()
                    .map(|(id, nome, d)| AppInfo { id: id.to_string(), nome: nome.to_string(), descricao: d.to_string() })
                    .collect();
            }
            (Aba::Instalados, _) => {
                titulo = format!("Instalados ({})", self.instalados.len());
                lista = self.instalados.clone();
            }
        }
        let visivel = a - topo - 60.0;
        let total = lista.len() as f32 * LINHA_A + 40.0;
        self.rolagem = (self.rolagem + ui.rolagem * 2.0).clamp(0.0, (total - visivel).max(0.0));
        ui.tela.texto(24.0, topo + 22.0 - self.rolagem, 18.0, tema::TEXTO(), &titulo);
        if lista.is_empty() && !ocupado {
            let msg = if self.aba == Aba::Instalados { "Nenhum aplicativo instalado ainda." } else { "Nada encontrado." };
            ui.tela.texto(24.0, topo + 60.0, 13.5, tema::TEXTO_SECUNDARIO(), msg);
        }
        let mut acao: Option<(u8, AppInfo)> = None;
        for (i, app) in lista.iter().enumerate() {
            let y = topo + 40.0 + i as f32 * LINHA_A - self.rolagem;
            if y + LINHA_A < topo || y > a - 50.0 {
                continue;
            }
            if let Some(c) = self.linha(ui, app, 16.0, y, l - 32.0) {
                acao = Some((c, app.clone()));
            }
        }
        match acao {
            Some((0, app)) => self.instalar(&app),
            Some((1, app)) => Self::abrir(&app),
            Some((2, app)) => self.remover(&app),
            _ => {}
        }
        // A barra de cima fica por cima do que rolou.
        ui.tela.retangulo(0.0, BARRA_A, l, 8.0, 0.0, tema::JANELA());

        // Rodapé: andamento ou aviso.
        ui.tela.retangulo(0.0, a - 44.0, l, 44.0, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, a - 44.0, l, 1.0, 0.0, tema::BORDA());
        if let Some((desc, _)) = &self.trabalho {
            let (pct, texto, cancelavel) = {
                let p = self.progresso.lock().unwrap();
                (p.pct, p.texto.clone(), p.grupo.is_some() && !p.cancelado)
            };
            ui.tela.retangulo(16.0, a - 26.0, 120.0, 6.0, 3.0, tema::DESTAQUE_SUAVE());
            if let Some(v) = pct {
                // Progresso de verdade.
                ui.tela.retangulo(16.0, a - 26.0, 120.0 * v / 100.0, 6.0, 3.0, tema::DESTAQUE());
            } else {
                // Barrinha animada enquanto o flatpak não diz quanto falta.
                let fase = (self.quadro % 60) as f32 / 60.0;
                ui.tela.retangulo(16.0 + fase * 90.0, a - 26.0, 30.0, 6.0, 3.0, tema::DESTAQUE());
            }
            let linha = if texto.is_empty() { desc.clone() } else { format!("{desc}  ·  {texto}") };
            let fim = if cancelavel { l - 300.0 } else { l - 170.0 };
            ui.tela.texto(150.0, a - 17.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, &linha, fim));
            if cancelavel {
                let (bx, by, bw) = (l - 120.0, a - 38.0, 104.0);
                let s = ui.sobre(bx, by, bw, 30.0);
                ui.tela.retangulo(bx, by, bw, 30.0, 6.0, if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
                ui.tela.retangulo(bx + 1.0, by + 1.0, bw - 2.0, 28.0, 5.0, tema::JANELA());
                ui.tela.texto(bx + 22.0, by + 20.0, 13.0, tema::TEXTO(), "Cancelar");
                if ui.clicou(bx, by, bw, 30.0) {
                    let mut p = self.progresso.lock().unwrap();
                    if let Some(g) = p.grupo {
                        p.cancelado = true;
                        // SAFETY: sinal para o grupo de processos do flatpak que criamos.
                        unsafe { kill(-g, 15) };
                        // O `script` põe o flatpak numa sessão própria: avisa
                        // ele também.
                        let _ = Command::new("pkill").args(["-TERM", "-u", &std::env::var("USER").unwrap_or_default(), "-f", "flatpak (install|update) --user -y"]).status();
                    }
                }
            }
            // Animação leve (12 quadros/s): não roubar CPU de quem instala.
            ui.acordar_em = Some(std::time::Duration::from_millis(80));
        } else if let Some(av) = &self.aviso {
            ui.tela.texto(16.0, a - 17.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, av, l - 32.0));
        } else {
            ui.tela.texto(16.0, a - 17.0, 12.5, tema::TEXTO_SECUNDARIO(), "Os aplicativos vêm do Flathub e são instalados só para a sua conta.");
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Central de Apps".into(),
            app_id: "central".into(),
            tamanho: (980, 680),
            minimo: (640, 420),
        },
        {
            let mut c = Central::nova();
            // "central instalados" abre direto em Instalados (Painel de Controle).
            match std::env::args().nth(1).as_deref() {
                Some("instalados") => c.aba = Aba::Instalados,
                // "central atualizar" (Configurações › Atualizações): começa já.
                Some("atualizar") => {
                    c.aba = Aba::Instalados;
                    c.atualizar_tudo();
                }
                _ => {}
            }
            Box::new(c)
        },
    );
}

/// Copia o atalho do app instalado para a Área de Trabalho, como no Windows.
fn criar_atalho(id: &str) {
    let Ok(casa) = std::env::var("HOME") else { return };
    let casa = std::path::PathBuf::from(casa);
    let origem = casa.join(format!(".local/share/flatpak/exports/share/applications/{id}.desktop"));
    let pasta = casa.join("Área de Trabalho");
    let _ = std::fs::create_dir_all(&pasta);
    if origem.exists() {
        let _ = std::fs::copy(&origem, pasta.join(format!("{id}.desktop")));
    }
}
