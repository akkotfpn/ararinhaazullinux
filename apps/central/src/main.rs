// SPDX-License-Identifier: GPL-3.0-or-later
//! Central de Apps do Ararinha: busca, instala, atualiza e remove aplicativos
//! do Flathub (pelo comando flatpak, instalando para o usuário).

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
}

fn flatpak(args: &[&str]) -> Result<String, String> {
    match Command::new("flatpak").args(args).output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("").to_string()),
        Err(e) => Err(format!("Flatpak não encontrado: {e}")),
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
        };
        c.rodar("Preparando o Flathub…", || {
            let _ = flatpak(&["remote-add", "--user", "--if-not-exists", "flathub", "/usr/share/flatpak/flathub.flatpakrepo"]);
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
        self.rodar(&format!("Instalando {nome}… (pode levar alguns minutos)"), move || {
            match flatpak(&["install", "--user", "-y", "--noninteractive", "flathub", &id]) {
                Ok(_) => Resultado::Feito(format!("{nome} instalado. Ele já está no menu iniciar.")),
                Err(e) => Resultado::Erro(format!("Não deu para instalar {nome}: {e}")),
            }
        });
    }

    fn remover(&mut self, app: &AppInfo) {
        let id = app.id.clone();
        let nome = app.nome.clone();
        self.rodar(&format!("Removendo {nome}…"), move || {
            match flatpak(&["uninstall", "--user", "-y", "--noninteractive", &id]) {
                Ok(_) => Resultado::Feito(format!("{nome} removido.")),
                Err(e) => Resultado::Erro(format!("Não deu para remover {nome}: {e}")),
            }
        });
    }

    fn atualizar_tudo(&mut self) {
        self.rodar("Atualizando os aplicativos…", || {
            match flatpak(&["update", "--user", "-y", "--noninteractive"]) {
                Ok(_) => Resultado::Feito("Tudo atualizado.".into()),
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
            ui.tela.retangulo(x, y, l, LINHA_A - 6.0, 8.0, tema::SUPERFICIE_ALTA);
        }
        // Ícone: a inicial num quadrado azul (o ícone real aparece depois de instalado, no menu).
        ui.tela.retangulo(x + 10.0, y + 9.0, 40.0, 40.0, 10.0, tema::DESTAQUE);
        let ini: String = app.nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
        let li = desenho::medir(20.0, &ini);
        ui.tela.texto(x + 30.0 - li / 2.0, y + 36.0, 20.0, tema::BRANCO, &ini);
        let largura_texto = l - 260.0;
        ui.tela.texto(x + 64.0, y + 26.0, 15.0, tema::TEXTO, &desenho::encurtar(15.0, &app.nome, largura_texto));
        ui.tela.texto(x + 64.0, y + 46.0, 12.5, tema::TEXTO_SECUNDARIO, &desenho::encurtar(12.5, &app.descricao, largura_texto));

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
                (if sobre { tema::DESTAQUE_FORTE } else { tema::DESTAQUE }, tema::BRANCO)
            } else {
                (if sobre { tema::SUPERFICIE_ALTA } else { tema::FUNDO }, tema::TEXTO)
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
        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Texto(s) => self.busca.push_str(&s),
                Tecla::Apagar => {
                    self.busca.pop();
                    if self.busca.is_empty() {
                        self.resultados = None;
                    }
                }
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
        ui.tela.limpar(tema::BRANCO);

        // Barra de cima: abas, busca e atualizar.
        ui.tela.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO);
        ui.tela.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA);
        let mut x = 16.0;
        for (aba, nome) in [(Aba::Inicio, "Descobrir"), (Aba::Instalados, "Instalados")] {
            let lt = desenho::medir(14.0, nome);
            let ativa = self.aba == aba;
            if ativa || ui.sobre(x - 8.0, 14.0, lt + 16.0, 36.0) {
                ui.tela.retangulo(x - 8.0, 14.0, lt + 16.0, 36.0, 8.0, if ativa { tema::DESTAQUE_SUAVE } else { tema::SUPERFICIE_ALTA });
            }
            ui.tela.texto(x, 37.0, 14.0, tema::TEXTO, nome);
            if ui.clicou(x - 8.0, 14.0, lt + 16.0, 36.0) {
                self.aba = aba;
                self.rolagem = 0.0;
            }
            x += lt + 28.0;
        }
        let (bx, bl) = (x + 10.0, (l - x - 190.0).max(160.0));
        ui.tela.retangulo(bx, 14.0, bl, 36.0, 18.0, tema::DESTAQUE);
        ui.tela.retangulo(bx + 2.0, 16.0, bl - 4.0, 32.0, 16.0, tema::BRANCO);
        if self.busca.is_empty() {
            ui.tela.texto(bx + 18.0, 37.0, 14.0, tema::TEXTO_SECUNDARIO, "Procurar aplicativos (Enter)");
        } else {
            let fim = ui.tela.texto(bx + 18.0, 37.0, 14.0, tema::TEXTO, &self.busca);
            ui.tela.retangulo(bx + 19.0 + fim, 23.0, 1.5, 18.0, 0.0, tema::DESTAQUE);
        }
        let ux = l - 160.0;
        let ocupado = self.trabalho.is_some();
        let sobre = !ocupado && ui.sobre(ux, 14.0, 144.0, 36.0);
        ui.tela.retangulo(ux, 14.0, 144.0, 36.0, 8.0, if sobre { tema::SUPERFICIE_ALTA } else { tema::BRANCO });
        ui.tela.texto(ux + 16.0, 37.0, 13.5, tema::TEXTO, "Atualizar tudo");
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
        ui.tela.texto(24.0, topo + 22.0 - self.rolagem, 18.0, tema::TEXTO, &titulo);
        if lista.is_empty() && !ocupado {
            let msg = if self.aba == Aba::Instalados { "Nenhum aplicativo instalado ainda." } else { "Nada encontrado." };
            ui.tela.texto(24.0, topo + 60.0, 13.5, tema::TEXTO_SECUNDARIO, msg);
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
        ui.tela.retangulo(0.0, BARRA_A, l, 8.0, 0.0, tema::BRANCO);

        // Rodapé: andamento ou aviso.
        ui.tela.retangulo(0.0, a - 44.0, l, 44.0, 0.0, tema::FUNDO);
        ui.tela.retangulo(0.0, a - 44.0, l, 1.0, 0.0, tema::BORDA);
        if let Some((desc, _)) = &self.trabalho {
            // Barrinha animada.
            let fase = (self.quadro % 60) as f32 / 60.0;
            ui.tela.retangulo(16.0, a - 26.0, 120.0, 6.0, 3.0, tema::DESTAQUE_SUAVE);
            ui.tela.retangulo(16.0 + fase * 90.0, a - 26.0, 30.0, 6.0, 3.0, tema::DESTAQUE);
            ui.tela.texto(150.0, a - 17.0, 13.0, tema::TEXTO, &desenho::encurtar(13.0, desc, l - 170.0));
            ui.redesenhar = true;
        } else if let Some(av) = &self.aviso {
            ui.tela.texto(16.0, a - 17.0, 13.0, tema::TEXTO, &desenho::encurtar(13.0, av, l - 32.0));
        } else {
            ui.tela.texto(16.0, a - 17.0, 12.5, tema::TEXTO_SECUNDARIO, "Os aplicativos vêm do Flathub e são instalados só para a sua conta.");
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
        Box::new(Central::nova()),
    );
}
