// SPDX-License-Identifier: GPL-3.0-or-later
//! Músicas: toca as músicas da pasta Músicas (ou os arquivos abertos pelo
//! Explorador). MP3, FLAC, OGG/Vorbis, WAV e M4A/AAC, decodificados pelo
//! Symphonia; o som sai pelo ALSA para o PipeWire.

mod tocador;

use std::{
    path::{Path, PathBuf},
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    time::Duration,
};

use penas::{desenho, tema, App, Cor, Keysym, Opcoes, Tecla, Tela, Ui};
use tocador::{Comando, Estado};

const EXTENSOES: [&str; 9] = ["mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "mp4"];
const BARRA_A: f32 = 96.0;
const CAB_A: f32 = 64.0;
const LINHA_A: f32 = 34.0;

#[derive(Clone)]
struct Faixa {
    caminho: PathBuf,
    nome: String,
    pasta: String,
}

struct Musicas {
    faixas: Vec<Faixa>,
    carregando: Option<Receiver<Vec<Faixa>>>,
    atual: Option<usize>,
    selecionada: Option<usize>,
    rolagem: f32,
    tx: Sender<Comando>,
    estado: Arc<Mutex<Estado>>,
    aleatorio: bool,
    repetir: bool,
    semente: u64,
    arrastando_barra: bool,
}

fn eh_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| EXTENSOES.contains(&e.to_lowercase().as_str())).unwrap_or(false)
}

fn faixa_de(p: PathBuf) -> Faixa {
    Faixa {
        nome: tocador::nome_da_faixa(&p),
        pasta: p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        caminho: p,
    }
}

/// Procura músicas na pasta (e subpastas), em ordem de pasta e nome.
fn varrer(raiz: &Path) -> Vec<Faixa> {
    let mut achadas = Vec::new();
    let mut pilha = vec![raiz.to_path_buf()];
    while let Some(d) = pilha.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                pilha.push(p);
            } else if eh_audio(&p) {
                achadas.push(p);
            }
        }
        if achadas.len() > 20_000 {
            break;
        }
    }
    achadas.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    achadas.into_iter().map(faixa_de).collect()
}

fn tempo(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

impl Musicas {
    fn novo() -> Musicas {
        let (tx, rx) = channel();
        let estado = Arc::new(Mutex::new(Estado::default()));
        let e2 = estado.clone();
        std::thread::spawn(move || tocador::rodar(rx, e2));
        let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
        let mut m = Musicas {
            faixas: Vec::new(),
            carregando: None,
            atual: None,
            selecionada: None,
            rolagem: 0.0,
            tx,
            estado,
            aleatorio: false,
            repetir: false,
            semente: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7)
                | 1,
            arrastando_barra: false,
        };
        if !args.is_empty() {
            // Abertos pelo Explorador: a fila são esses arquivos (ou as
            // músicas de uma pasta passada).
            let mut v = Vec::new();
            for a in args {
                if a.is_dir() {
                    v.extend(varrer(&a));
                } else if eh_audio(&a) {
                    v.push(faixa_de(a));
                }
            }
            m.faixas = v;
            if !m.faixas.is_empty() {
                m.tocar(0);
            }
        } else {
            let (txf, rxf) = channel();
            std::thread::spawn(move || {
                let casa = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
                let _ = txf.send(varrer(&Path::new(&casa).join("Músicas")));
            });
            m.carregando = Some(rxf);
        }
        m
    }

    fn tocar(&mut self, i: usize) {
        if let Some(f) = self.faixas.get(i) {
            let _ = self.tx.send(Comando::Tocar(f.caminho.clone()));
            self.atual = Some(i);
            self.selecionada = Some(i);
            let mut e = self.estado.lock().unwrap();
            e.terminou = false;
            e.tocando = true;
            e.pausado = false;
            e.titulo = f.nome.clone();
            e.artista.clear();
            e.posicao = 0.0;
            e.duracao = None;
        }
    }

    fn proxima(&mut self, pelo_usuario: bool) {
        let n = self.faixas.len();
        if n == 0 {
            return;
        }
        let i = match self.atual {
            _ if self.aleatorio => {
                self.semente ^= self.semente << 13;
                self.semente ^= self.semente >> 7;
                self.semente ^= self.semente << 17;
                (self.semente % n as u64) as usize
            }
            Some(i) if i + 1 < n => i + 1,
            Some(_) if self.repetir || pelo_usuario => 0,
            Some(_) => {
                let _ = self.tx.send(Comando::Parar);
                return;
            }
            None => 0,
        };
        self.tocar(i);
    }

    fn anterior(&mut self) {
        let pos = self.estado.lock().unwrap().posicao;
        match self.atual {
            // Como nos outros tocadores: depois de 3 s, volta ao começo.
            Some(i) if pos < 3.0 && i > 0 => self.tocar(i - 1),
            Some(i) => self.tocar(i),
            None => {}
        }
    }

    fn tocar_pausar(&mut self) {
        let e = self.estado.lock().unwrap().clone();
        if !e.tocando {
            let i = self.selecionada.or(self.atual).unwrap_or(0);
            drop(e);
            self.tocar(i);
        } else if e.pausado {
            let _ = self.tx.send(Comando::Continuar);
        } else {
            let _ = self.tx.send(Comando::Pausar);
        }
    }
}

fn icone_tocar(t: &mut Tela, cx: f32, cy: f32, r: f32, cor: Cor) {
    t.poligono(&[(cx - r * 0.4, cy - r * 0.6), (cx + r * 0.6, cy), (cx - r * 0.4, cy + r * 0.6)], cor);
}

fn icone_pausar(t: &mut Tela, cx: f32, cy: f32, r: f32, cor: Cor) {
    t.retangulo(cx - r * 0.45, cy - r * 0.55, r * 0.3, r * 1.1, 1.5, cor);
    t.retangulo(cx + r * 0.15, cy - r * 0.55, r * 0.3, r * 1.1, 1.5, cor);
}

fn icone_pular(t: &mut Tela, cx: f32, cy: f32, r: f32, cor: Cor, frente: bool) {
    let s = if frente { 1.0 } else { -1.0 };
    t.poligono(&[(cx - s * r * 0.5, cy - r * 0.5), (cx + s * r * 0.3, cy), (cx - s * r * 0.5, cy + r * 0.5)], cor);
    let bx = if frente { cx + r * 0.3 } else { cx - r * 0.3 - r * 0.18 };
    t.retangulo(bx, cy - r * 0.5, r * 0.18, r, 1.0, cor);
}

fn nota(t: &mut Tela, cx: f32, cy: f32, s: f32, cor: Cor) {
    t.retangulo(cx - s * 0.35, cy + s * 0.15, s * 0.32, s * 0.26, s * 0.13, cor);
    t.linha(cx - s * 0.05, cy + s * 0.28, cx - s * 0.05, cy - s * 0.4, s * 0.08, cor);
    t.linha(cx - s * 0.05, cy - s * 0.4, cx + s * 0.3, cy - s * 0.25, s * 0.08, cor);
}

impl App for Musicas {
    fn desenhar(&mut self, ui: &mut Ui) {
        if let Some(rx) = &self.carregando {
            if let Ok(v) = rx.try_recv() {
                self.faixas = v;
                self.carregando = None;
            } else {
                ui.acordar_em = Some(Duration::from_millis(150));
            }
        }
        let estado = self.estado.lock().unwrap().clone();
        if estado.terminou && estado.tocando == false && estado.erro.is_none() && self.atual.is_some() {
            self.estado.lock().unwrap().terminou = false;
            self.proxima(false);
        } else if estado.terminou && estado.erro.is_some() {
            // Faixa com problema: pula para a próxima.
            self.estado.lock().unwrap().terminou = false;
            if self.faixas.len() > 1 {
                self.proxima(false);
            }
        }
        if estado.tocando && !estado.pausado {
            ui.acordar_em = Some(Duration::from_millis(250));
        }

        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Texto(s) if s == " " => self.tocar_pausar(),
                Tecla::Enter => {
                    if let Some(i) = self.selecionada {
                        self.tocar(i);
                    }
                }
                Tecla::Baixo => {
                    let n = self.faixas.len();
                    if n > 0 {
                        self.selecionada = Some(self.selecionada.map(|i| (i + 1).min(n - 1)).unwrap_or(0));
                    }
                }
                Tecla::Cima => self.selecionada = self.selecionada.map(|i| i.saturating_sub(1)),
                Tecla::Direita => {
                    let _ = self.tx.send(Comando::Buscar(estado.posicao + 10.0));
                }
                Tecla::Esquerda => {
                    let _ = self.tx.send(Comando::Buscar((estado.posicao - 10.0).max(0.0)));
                }
                Tecla::Outra(k) if k == Keysym::XF86_AudioPlay || k == Keysym::XF86_AudioPause => self.tocar_pausar(),
                Tecla::Outra(k) if k == Keysym::XF86_AudioNext => self.proxima(true),
                Tecla::Outra(k) if k == Keysym::XF86_AudioPrev => self.anterior(),
                _ => {}
            }
        }

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());

        // Cabeçalho
        ui.tela.retangulo(0.0, 0.0, l, CAB_A, 0.0, tema::FUNDO());
        ui.tela.texto(24.0, 30.0, 20.0, tema::TEXTO(), "Músicas");
        let sub = if self.carregando.is_some() {
            "Procurando músicas na pasta Músicas…".to_string()
        } else {
            format!("{} faixas", self.faixas.len())
        };
        ui.tela.texto(24.0, 50.0, 12.5, tema::TEXTO_SECUNDARIO(), &sub);
        // Aleatório e Repetir
        let mut x = l - 16.0;
        for (rot, ligado, k) in [("Repetir", self.repetir, 1), ("Aleatório", self.aleatorio, 0)] {
            let w = desenho::medir(13.0, rot) + 26.0;
            x -= w;
            let s = ui.sobre(x, 16.0, w, 32.0);
            let fundo = if ligado { tema::DESTAQUE_SUAVE() } else if s { tema::SUPERFICIE_ALTA() } else { tema::FUNDO() };
            ui.tela.retangulo(x, 16.0, w, 32.0, 16.0, if ligado { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(x + 1.0, 17.0, w - 2.0, 30.0, 15.0, fundo);
            ui.tela.texto(x + 13.0, 37.0, 13.0, if ligado { tema::DESTAQUE() } else { tema::TEXTO() }, rot);
            if ui.clicou(x, 16.0, w, 32.0) {
                if k == 0 {
                    self.aleatorio = !self.aleatorio;
                } else {
                    self.repetir = !self.repetir;
                }
            }
            x -= 8.0;
        }
        ui.tela.retangulo(0.0, CAB_A - 1.0, l, 1.0, 0.0, tema::BORDA());

        // Lista
        let topo = CAB_A + 6.0;
        let visivel = a - topo - BARRA_A;
        let total = self.faixas.len() as f32 * LINHA_A;
        self.rolagem = (self.rolagem + ui.rolagem * 2.0).clamp(0.0, (total - visivel + 10.0).max(0.0));
        if let Some(i) = self.selecionada {
            let iy = i as f32 * LINHA_A;
            if iy < self.rolagem {
                self.rolagem = iy;
            } else if iy + LINHA_A > self.rolagem + visivel {
                self.rolagem = iy + LINHA_A - visivel;
            }
        }
        if self.faixas.is_empty() && self.carregando.is_none() {
            nota(ui.tela, l / 2.0, topo + 90.0, 64.0, tema::BORDA());
            let t1 = "Nenhuma música na pasta Músicas.";
            let t2 = "Copie seus arquivos MP3, FLAC, OGG ou M4A para lá e abra o app de novo.";
            let l1 = desenho::medir(15.0, t1);
            let l2 = desenho::medir(12.5, t2);
            ui.tela.texto(l / 2.0 - l1 / 2.0, topo + 160.0, 15.0, tema::TEXTO(), t1);
            ui.tela.texto(l / 2.0 - l2 / 2.0, topo + 184.0, 12.5, tema::TEXTO_SECUNDARIO(), t2);
        }
        let primeiro = (self.rolagem / LINHA_A) as usize;
        let mut tocar = None;
        for i in primeiro..self.faixas.len() {
            let y = topo + i as f32 * LINHA_A - self.rolagem;
            if y > topo + visivel {
                break;
            }
            let f = &self.faixas[i];
            let atual = self.atual == Some(i);
            let sel = self.selecionada == Some(i);
            let sobre = ui.sobre(8.0, y, l - 16.0, LINHA_A - 2.0) && ui.mouse.is_some_and(|(_, my)| my < topo + visivel);
            if sel {
                ui.tela.retangulo(8.0, y, l - 16.0, LINHA_A - 2.0, 5.0, tema::DESTAQUE_SUAVE());
            } else if sobre {
                ui.tela.retangulo(8.0, y, l - 16.0, LINHA_A - 2.0, 5.0, tema::SUPERFICIE_ALTA());
            }
            let cor = if atual { tema::DESTAQUE() } else { tema::TEXTO() };
            if atual && estado.tocando {
                if estado.pausado {
                    icone_pausar(ui.tela, 30.0, y + LINHA_A / 2.0 - 1.0, 12.0, cor);
                } else {
                    icone_tocar(ui.tela, 30.0, y + LINHA_A / 2.0 - 1.0, 12.0, cor);
                }
            } else {
                let n = format!("{}", i + 1);
                let ln = desenho::medir(12.0, &n);
                ui.tela.texto(36.0 - ln, y + 21.0, 12.0, tema::TEXTO_SECUNDARIO(), &n);
            }
            let max_nome = (l - 80.0) * 0.62;
            ui.tela.texto(56.0, y + 21.0, 13.5, cor, &desenho::encurtar(13.5, &f.nome, max_nome));
            ui.tela.texto(56.0 + max_nome + 16.0, y + 21.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, &f.pasta, l - max_nome - 100.0));
            if sobre && ui.clique {
                self.selecionada = Some(i);
                if ui.duplo {
                    tocar = Some(i);
                }
            }
        }
        if let Some(i) = tocar {
            self.tocar(i);
        }

        // Barra do tocador
        let by = a - BARRA_A;
        ui.tela.retangulo(0.0, by, l, BARRA_A, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, by, l, 1.0, 0.0, tema::BORDA());
        // Capa (nota) e título
        ui.tela.retangulo(16.0, by + 14.0, 56.0, 56.0, 8.0, tema::DESTAQUE_SUAVE());
        nota(ui.tela, 44.0, by + 42.0, 34.0, tema::DESTAQUE());
        let largura_info = (l * 0.3).max(160.0);
        if estado.tocando || estado.arquivo.is_some() {
            ui.tela.texto(84.0, by + 38.0, 14.0, tema::TEXTO(), &desenho::encurtar(14.0, &estado.titulo, largura_info));
            let art = if estado.artista.is_empty() { "Artista desconhecido".to_string() } else { estado.artista.clone() };
            ui.tela.texto(84.0, by + 58.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, &art, largura_info));
        } else if let Some(e) = &estado.erro {
            ui.tela.texto(84.0, by + 46.0, 12.5, tema::ERRO(), &desenho::encurtar(12.5, e, largura_info));
        } else {
            ui.tela.texto(84.0, by + 46.0, 13.0, tema::TEXTO_SECUNDARIO(), "Clique duas vezes numa música para tocar");
        }
        // Botões no centro
        let cx = l / 2.0 + 40.0;
        let cy = by + 34.0;
        let s_ant = ui.sobre(cx - 76.0, cy - 18.0, 36.0, 36.0);
        let s_toc = ui.sobre(cx - 22.0, cy - 22.0, 44.0, 44.0);
        let s_prox = ui.sobre(cx + 40.0, cy - 18.0, 36.0, 36.0);
        if s_ant {
            ui.tela.retangulo(cx - 76.0, cy - 18.0, 36.0, 36.0, 18.0, tema::SUPERFICIE_ALTA());
        }
        if s_prox {
            ui.tela.retangulo(cx + 40.0, cy - 18.0, 36.0, 36.0, 18.0, tema::SUPERFICIE_ALTA());
        }
        icone_pular(ui.tela, cx - 58.0, cy, 14.0, tema::TEXTO(), false);
        icone_pular(ui.tela, cx + 58.0, cy, 14.0, tema::TEXTO(), true);
        ui.tela.retangulo(cx - 22.0, cy - 22.0, 44.0, 44.0, 22.0, if s_toc { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        if estado.tocando && !estado.pausado {
            icone_pausar(ui.tela, cx, cy, 16.0, tema::BRANCO());
        } else {
            icone_tocar(ui.tela, cx + 2.0, cy, 18.0, tema::BRANCO());
        }
        if ui.clicou(cx - 76.0, cy - 18.0, 36.0, 36.0) {
            self.anterior();
        }
        if ui.clicou(cx - 22.0, cy - 22.0, 44.0, 44.0) {
            self.tocar_pausar();
        }
        if ui.clicou(cx + 40.0, cy - 18.0, 36.0, 36.0) {
            self.proxima(true);
        }
        // Barra de posição
        let (px0, px1) = (l * 0.3 + 100.0, l - 90.0);
        let (px0, px1) = if px1 - px0 < 120.0 { (cx - 160.0, cx + 160.0) } else { (px0.min(cx - 160.0), px1.max(cx + 160.0)) };
        let py = by + 74.0;
        let dur = estado.duracao.unwrap_or(0.0);
        let frac = if dur > 0.0 { (estado.posicao / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
        ui.tela.texto(px0 - 46.0, py + 5.0, 11.5, tema::TEXTO_SECUNDARIO(), &tempo(estado.posicao));
        if dur > 0.0 {
            ui.tela.texto(px1 + 10.0, py + 5.0, 11.5, tema::TEXTO_SECUNDARIO(), &tempo(dur));
        }
        ui.tela.retangulo(px0, py - 2.0, px1 - px0, 4.0, 2.0, tema::BORDA());
        ui.tela.retangulo(px0, py - 2.0, (px1 - px0) * frac, 4.0, 2.0, tema::DESTAQUE());
        let sobre_barra = ui.sobre(px0 - 6.0, py - 10.0, px1 - px0 + 12.0, 20.0);
        if sobre_barra || self.arrastando_barra {
            ui.tela.retangulo(px0 + (px1 - px0) * frac - 6.0, py - 6.0, 12.0, 12.0, 6.0, tema::DESTAQUE());
        }
        if ui.clicou(px0 - 6.0, py - 10.0, px1 - px0 + 12.0, 20.0) && dur > 0.0 {
            self.arrastando_barra = true;
        }
        if self.arrastando_barra && (ui.soltou || !ui.pressionado) {
            self.arrastando_barra = false;
            if let Some((mx, _)) = ui.mouse {
                let f = ((mx - px0) / (px1 - px0)).clamp(0.0, 1.0) as f64;
                let _ = self.tx.send(Comando::Buscar(f * dur));
            }
        }
        ui.titulo = Some(if estado.tocando { format!("{} – Músicas", estado.titulo) } else { "Músicas".into() });
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Músicas".into(),
            app_id: "musicas".into(),
            tamanho: (860, 600),
            minimo: (560, 380),
        },
        Box::new(Musicas::novo()),
    );
}
