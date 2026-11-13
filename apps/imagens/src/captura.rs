// SPDX-License-Identifier: GPL-3.0-or-later
//! Ferramenta de Captura (como a do Windows 11): "+ Novo" captura um
//! retângulo, uma janela ou a tela inteira (com atraso opcional), e a
//! imagem abre aqui para marcar com caneta e marca-texto, recortar, copiar
//! e salvar. Win+Shift+S faz o mesmo direto; a notificação abre aqui.
//!
//! Também é a janela "Permitir compartilhar a tela?" do portal
//! (`captura --permitir NomeDoApp`): sai com 0 se a pessoa permitir.
//!
//! A captura em si é da Plumagem: o app escreve o pedido em
//! $XDG_RUNTIME_DIR/ararinha-captura.pedido e lê a resposta em
//! ararinha-captura.resultado (caminho do PNG ou "cancelado").

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use penas::{componentes as c, desenho, tema, App, Cor, Keysym, Opcoes, Tecla, Ui};

const BARRA_A: f32 = 56.0;

static PERMITIDO: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq)]
enum Modo {
    Retangulo,
    Janela,
    Tela,
}

#[derive(Clone, Copy, PartialEq)]
enum Ferramenta {
    Nenhuma,
    Caneta,
    Marca,
    Borracha,
    Recortar,
}

#[derive(Clone)]
struct Traco {
    marca: bool,
    cor: [u8; 3],
    largura: f32,
    pontos: Vec<(f32, f32)>,
}

enum Acao {
    Traco,
    Recorte(image::RgbaImage, Vec<Traco>),
    Apagou(usize, Traco),
}

const CORES: [[u8; 3]; 5] = [[20, 24, 30], [220, 40, 40], [30, 110, 220], [30, 160, 80], [255, 255, 255]];
const CORES_MARCA: [[u8; 3]; 3] = [[255, 230, 0], [120, 255, 120], [255, 140, 220]];

struct Captura {
    imagem: Option<image::RgbaImage>,
    caminho: Option<PathBuf>,
    reduzida: Option<(u32, u32, image::RgbaImage)>,
    tracos: Vec<Traco>,
    historico: Vec<Acao>,
    ferramenta: Ferramenta,
    cor: usize,
    cor_marca: usize,
    modo: Modo,
    atraso: u64,
    /// Contagem do atraso (quando dispara) e espera da resposta.
    disparar_em: Option<Instant>,
    esperando: Option<Instant>,
    /// Arraste em andamento (traço novo ou retângulo do recorte), em
    /// coordenadas da imagem.
    recorte: Option<((f32, f32), (f32, f32))>,
    desenhando: bool,
    salvar_rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    aviso: Option<(String, Instant)>,
}

fn arquivo_runtime(nome: &str) -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR").ok().map(|d| PathBuf::from(d).join(nome))
}

fn abrir_png(p: &std::path::Path) -> Option<image::RgbaImage> {
    image::open(p).ok().map(|i| i.to_rgba8())
}

/// Desenha um traço (discos ao longo dos segmentos) numa máscara e mistura
/// na imagem: o marca-texto não escurece onde passa duas vezes.
fn pintar(img: &mut image::RgbaImage, t: &Traco) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut mascara = vec![false; (w * h) as usize];
    let r = (t.largura / 2.0).max(0.5);
    let mut carimbo = |cx: f32, cy: f32| {
        let (x0, x1) = ((cx - r).floor() as i32, (cx + r).ceil() as i32);
        let (y0, y1) = ((cy - r).floor() as i32, (cy + r).ceil() as i32);
        for y in y0.max(0)..=y1.min(h - 1) {
            for x in x0.max(0)..=x1.min(w - 1) {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                // O marca-texto é uma ponta reta (retângulo alto).
                let dentro = if t.marca { dx.abs() <= r * 0.45 && dy.abs() <= r } else { dx * dx + dy * dy <= r * r };
                if dentro {
                    mascara[(y * w + x) as usize] = true;
                }
            }
        }
    };
    let pts = &t.pontos;
    if pts.len() == 1 {
        carimbo(pts[0].0, pts[0].1);
    }
    for par in pts.windows(2) {
        let ((ax, ay), (bx, by)) = (par[0], par[1]);
        let passos = (((bx - ax).powi(2) + (by - ay).powi(2)).sqrt() / (r * 0.4).max(0.5)).ceil().max(1.0) as i32;
        for k in 0..=passos {
            let f = k as f32 / passos as f32;
            carimbo(ax + (bx - ax) * f, ay + (by - ay) * f);
        }
    }
    let alfa = if t.marca { 0.4 } else { 1.0 };
    for (i, m) in mascara.iter().enumerate() {
        if !*m {
            continue;
        }
        let p = img.get_pixel_mut((i as i32 % w) as u32, (i as i32 / w) as u32);
        for k in 0..3 {
            let base = p.0[k] as f32;
            // Marca-texto multiplica (como a caneta marca-texto de verdade).
            let novo = if t.marca { base * t.cor[k] as f32 / 255.0 } else { t.cor[k] as f32 };
            p.0[k] = (base + (novo - base) * alfa).round().clamp(0.0, 255.0) as u8;
        }
        p.0[3] = 255;
    }
}

impl Captura {
    fn nova() -> Captura {
        Captura {
            imagem: None,
            caminho: None,
            reduzida: None,
            tracos: Vec::new(),
            historico: Vec::new(),
            ferramenta: Ferramenta::Caneta,
            cor: 1,
            cor_marca: 0,
            modo: Modo::Retangulo,
            atraso: 0,
            disparar_em: None,
            esperando: None,
            recorte: None,
            desenhando: false,
            salvar_rx: None,
            aviso: None,
        }
    }

    fn carregar(&mut self, p: PathBuf) {
        match abrir_png(&p) {
            Some(img) => {
                self.imagem = Some(img);
                self.caminho = Some(p);
                self.reduzida = None;
                self.tracos.clear();
                self.historico.clear();
            }
            None => self.avisar(&format!("Não deu para abrir {}", p.display())),
        }
    }

    fn avisar(&mut self, t: &str) {
        self.aviso = Some((t.to_string(), Instant::now()));
    }

    /// A imagem com os traços aplicados.
    fn final_(&self) -> Option<image::RgbaImage> {
        let mut img = self.imagem.clone()?;
        for t in &self.tracos {
            pintar(&mut img, t);
        }
        Some(img)
    }

    fn png(&self) -> Option<Vec<u8>> {
        let img = self.final_()?;
        let mut b = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png)
            .ok()?;
        Some(b)
    }

    fn novo(&mut self) {
        if self.esperando.is_some() || self.disparar_em.is_some() {
            return;
        }
        self.disparar_em = Some(Instant::now() + Duration::from_secs(self.atraso));
    }

    fn disparar(&mut self) {
        let modo = match self.modo {
            Modo::Retangulo => "recorte",
            Modo::Janela => "janela",
            Modo::Tela => "tela",
        };
        if let Some(r) = arquivo_runtime("ararinha-captura.resultado") {
            let _ = std::fs::remove_file(r);
        }
        match arquivo_runtime("ararinha-captura.pedido").map(|p| std::fs::write(p, modo)) {
            Some(Ok(())) => self.esperando = Some(Instant::now()),
            _ => self.avisar("Não deu para pedir a captura à Plumagem."),
        }
    }

    fn ver_resposta(&mut self) {
        let Some(desde) = self.esperando else { return };
        let Some(p) = arquivo_runtime("ararinha-captura.resultado") else { return };
        if let Ok(t) = std::fs::read_to_string(&p) {
            let _ = std::fs::remove_file(&p);
            self.esperando = None;
            let t = t.trim();
            if t != "cancelado" && !t.is_empty() {
                self.carregar(PathBuf::from(t));
                self.avisar("Captura salva em Capturas de Tela e copiada para a área de transferência.");
            }
        } else if desde.elapsed() > Duration::from_secs(120) {
            self.esperando = None;
        }
    }

    fn copiar(&mut self) {
        if let Some(b) = self.png() {
            penas::copiar_imagem(b);
            self.avisar("Copiada. Cole com Ctrl+V em qualquer app.");
        }
    }

    fn salvar_como(&mut self) {
        if self.salvar_rx.is_some() {
            return;
        }
        let Some(img) = self.final_() else { return };
        let nome = self
            .caminho
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Captura.png".into());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = std::process::Command::new("explorador")
                .args(["--escolher", "salvar", "--nome", &nome, "--titulo", "Salvar captura como"])
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    let destino = String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string();
                    if destino.is_empty() {
                        return Err(String::new());
                    }
                    let mut p = PathBuf::from(&destino);
                    let formato = match p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
                        Some("jpg") | Some("jpeg") => image::ImageFormat::Jpeg,
                        Some("png") => image::ImageFormat::Png,
                        _ => {
                            p.set_extension("png");
                            image::ImageFormat::Png
                        }
                    };
                    let dinamica = image::DynamicImage::ImageRgba8(img);
                    let r = if formato == image::ImageFormat::Jpeg { dinamica.to_rgb8().save_with_format(&p, formato) } else { dinamica.save_with_format(&p, formato) };
                    r.map_err(|e| e.to_string()).map(|_| p.display().to_string())
                });
            let _ = tx.send(r);
        });
        self.salvar_rx = Some(rx);
    }

    fn desfazer(&mut self) {
        match self.historico.pop() {
            Some(Acao::Traco) => {
                self.tracos.pop();
            }
            Some(Acao::Recorte(img, tracos)) => {
                self.imagem = Some(img);
                self.tracos = tracos;
                self.reduzida = None;
            }
            Some(Acao::Apagou(i, t)) => {
                let i = i.min(self.tracos.len());
                self.tracos.insert(i, t);
            }
            None => {}
        }
    }

    /// Recorta a imagem (já com os traços) no retângulo, em pixels.
    fn recortar(&mut self, a: (f32, f32), b: (f32, f32)) {
        let Some(img) = self.final_() else { return };
        let (x0, y0) = (a.0.min(b.0).max(0.0) as u32, a.1.min(b.1).max(0.0) as u32);
        let (x1, y1) = (a.0.max(b.0).min(img.width() as f32) as u32, a.1.max(b.1).min(img.height() as f32) as u32);
        if x1 <= x0 + 4 || y1 <= y0 + 4 {
            return;
        }
        let antiga = self.imagem.take().unwrap_or_else(|| img.clone());
        let tracos = std::mem::take(&mut self.tracos);
        self.historico.push(Acao::Recorte(antiga, tracos));
        self.imagem = Some(image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image());
        self.reduzida = None;
    }

    /// Botão de ícone da barra (com dica). Devolve true no clique.
    fn botao_icone(ui: &mut Ui, x: f32, y: f32, ativo: bool, marcado: bool, dica: &str, desenho_icone: impl FnOnce(&mut Ui, f32, f32, Cor)) -> bool {
        let s = ativo && ui.sobre(x, y, 40.0, 40.0);
        if marcado {
            ui.tela.retangulo(x, y, 40.0, 40.0, 8.0, tema::DESTAQUE_SUAVE());
        } else if s {
            ui.tela.retangulo(x, y, 40.0, 40.0, 8.0, tema::SUPERFICIE_ALTA());
        }
        let cor = if ativo { tema::TEXTO() } else { tema::BORDA() };
        desenho_icone(ui, x + 20.0, y + 20.0, cor);
        if s {
            let lt = desenho::medir(12.0, dica);
            let tx = (x + 20.0 - lt / 2.0 - 8.0).max(4.0);
            ui.tela.retangulo(tx, y + 46.0, lt + 16.0, 24.0, 5.0, tema::BORDA());
            ui.tela.retangulo(tx + 1.0, y + 47.0, lt + 14.0, 22.0, 4.0, tema::JANELA());
            ui.tela.texto(tx + 8.0, y + 63.0, 12.0, tema::TEXTO(), dica);
        }
        ativo && ui.clicou(x, y, 40.0, 40.0)
    }
}

impl App for Captura {
    fn desenhar(&mut self, ui: &mut Ui) {
        // Atraso e espera da Plumagem.
        if let Some(t) = self.disparar_em {
            if Instant::now() >= t {
                self.disparar_em = None;
                self.disparar();
            } else {
                ui.acordar_em = Some(Duration::from_millis(100));
            }
        }
        if self.esperando.is_some() {
            self.ver_resposta();
            ui.acordar_em = Some(Duration::from_millis(100));
        }
        if let Some(rx) = &self.salvar_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.salvar_rx = None;
                    match r {
                        Ok(p) => self.avisar(&format!("Salva em {p}")),
                        Err(e) if e.is_empty() => {}
                        Err(e) => self.avisar(&format!("Não deu para salvar: {e}")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(150)),
            }
        }

        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::n || k == Keysym::N) => self.novo(),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::z || k == Keysym::Z) => self.desfazer(),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::c || k == Keysym::C) => self.copiar(),
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::s || k == Keysym::S) => self.salvar_como(),
                Tecla::Esc => {
                    self.recorte = None;
                    if self.ferramenta == Ferramenta::Recortar {
                        self.ferramenta = Ferramenta::Caneta;
                    }
                }
                _ => {}
            }
        }

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::SUPERFICIE());
        let tem = self.imagem.is_some();

        // ---------------------------------------------------------- área
        let (ax, ay, al, aa) = (12.0, BARRA_A + 12.0, l - 24.0, a - BARRA_A - 24.0);
        let mut mapa: Option<(f32, f32, f32)> = None; // (x, y, fator)
        if let Some(img) = &self.imagem {
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let f = (al / iw).min(aa / ih).min(1.0);
            let (w, h) = ((iw * f).max(1.0) as u32, (ih * f).max(1.0) as u32);
            if self.reduzida.as_ref().map(|(rw, rh, _)| *rw != w || *rh != h).unwrap_or(true) {
                let r = if f < 1.0 { image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle) } else { img.clone() };
                self.reduzida = Some((w, h, r));
            }
            if let Some((w, h, r)) = &self.reduzida {
                let x = ax + (al - *w as f32) / 2.0;
                let y = ay + (aa - *h as f32) / 2.0;
                ui.tela.retangulo(x - 1.0, y - 1.0, *w as f32 + 2.0, *h as f32 + 2.0, 0.0, tema::BORDA());
                ui.tela.imagem(x as i32, y as i32, r);
                mapa = Some((x, y, f));
            }
        } else {
            // Tela inicial, como a do Windows.
            let msg = "Clique em \"+ Novo\" (Ctrl+N) para capturar a tela.";
            let lt = desenho::medir(15.0, msg);
            ui.tela.texto(l / 2.0 - lt / 2.0, a / 2.0 - 6.0, 15.0, tema::TEXTO(), msg);
            let dica = "Atalho em qualquer lugar: Win+Shift+S. Print Screen salva a tela inteira.";
            let ld = desenho::medir(12.5, dica);
            ui.tela.texto(l / 2.0 - ld / 2.0, a / 2.0 + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), dica);
        }

        // Traços (por cima da imagem reduzida).
        if let Some((mx, my, f)) = mapa {
            let para_tela = |p: (f32, f32)| (mx + p.0 * f, my + p.1 * f);
            for t in &self.tracos {
                let cor = if t.marca { Cor(t.cor[0], t.cor[1], t.cor[2], 110) } else { Cor(t.cor[0], t.cor[1], t.cor[2], 255) };
                let esp = (t.largura * f).max(1.0);
                if t.pontos.len() == 1 {
                    let (x, y) = para_tela(t.pontos[0]);
                    ui.tela.retangulo(x - esp / 2.0, y - esp / 2.0, esp, esp, esp / 2.0, cor);
                }
                for par in t.pontos.windows(2) {
                    let (a0, b0) = (para_tela(par[0]), para_tela(par[1]));
                    ui.tela.linha(a0.0, a0.1, b0.0, b0.1, esp, cor);
                }
            }
            if let Some((p0, p1)) = self.recorte {
                let (a0, b0) = (para_tela(p0), para_tela(p1));
                let (x, y, w, h) = (a0.0.min(b0.0), a0.1.min(b0.1), (a0.0 - b0.0).abs(), (a0.1 - b0.1).abs());
                ui.tela.contorno(x, y, w, h, 2.0, tema::DESTAQUE());
                ui.tela.retangulo(x, y, w, h, 0.0, tema::DESTAQUE().com_alfa(30));
            }

            // Mouse na imagem: desenha, apaga ou recorta.
            let dentro = |p: (f32, f32)| p.0 >= mx && p.1 >= my && p.1 > BARRA_A;
            let para_img = |p: (f32, f32)| ((p.0 - mx) / f, (p.1 - my) / f);
            if let Some(m) = ui.mouse {
                if ui.pressionado {
                    let pi = para_img(m);
                    match self.ferramenta {
                        Ferramenta::Caneta | Ferramenta::Marca => {
                            if self.desenhando {
                                if let Some(t) = self.tracos.last_mut() {
                                    if t.pontos.last().map(|q| (q.0 - pi.0).abs() + (q.1 - pi.1).abs() > 1.0).unwrap_or(true) {
                                        t.pontos.push(pi);
                                    }
                                }
                            } else if ui.clique && dentro(m) {
                                let marca = self.ferramenta == Ferramenta::Marca;
                                let base = if marca { 22.0 } else { 4.0 };
                                self.tracos.push(Traco {
                                    marca,
                                    cor: if marca { CORES_MARCA[self.cor_marca] } else { CORES[self.cor] },
                                    largura: base / f.max(0.25),
                                    pontos: vec![pi],
                                });
                                self.historico.push(Acao::Traco);
                                self.desenhando = true;
                            }
                        }
                        Ferramenta::Recortar => {
                            if let Some((p0, _)) = self.recorte {
                                self.recorte = Some((p0, pi));
                            } else if ui.clique && dentro(m) {
                                self.recorte = Some((pi, pi));
                            }
                        }
                        Ferramenta::Borracha => {
                            // Apaga o traço que passa perto do ponteiro.
                            let raio = 8.0 / f;
                            if let Some(i) = self.tracos.iter().rposition(|t| {
                                t.pontos.iter().any(|q| (q.0 - pi.0).abs() < raio + t.largura / 2.0 && (q.1 - pi.1).abs() < raio + t.largura / 2.0)
                            }) {
                                let t = self.tracos.remove(i);
                                self.historico.push(Acao::Apagou(i, t));
                            }
                        }
                        Ferramenta::Nenhuma => {}
                    }
                    ui.acordar_em = Some(Duration::from_millis(16));
                } else {
                    self.desenhando = false;
                    if let Some((p0, p1)) = self.recorte.take() {
                        self.recortar(p0, p1);
                        self.ferramenta = Ferramenta::Caneta;
                    }
                }
            }
        }

        // ---------------------------------------------------------- barra
        ui.tela.retangulo(0.0, 0.0, l, BARRA_A, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, BARRA_A - 1.0, l, 1.0, 0.0, tema::BORDA());
        let ocupado = self.esperando.is_some() || self.disparar_em.is_some();
        let rot_novo = match self.disparar_em {
            Some(t) => format!("Em {} s…", t.saturating_duration_since(Instant::now()).as_secs() + 1),
            None if self.esperando.is_some() => "Capturando…".into(),
            None => "+ Novo".into(),
        };
        if c::botao(ui, 10.0, 10.0, &rot_novo, c::Estilo::Principal, !ocupado) {
            self.novo();
        }
        let mut x = 10.0 + c::largura_botao(&rot_novo) + 10.0;
        // Modo: retângulo, janela, tela inteira.
        for (m, dica) in [(Modo::Retangulo, "Retângulo"), (Modo::Janela, "Janela"), (Modo::Tela, "Tela inteira")] {
            let marcado = self.modo == m;
            if Self::botao_icone(ui, x, 8.0, true, marcado, dica, |ui, cx, cy, cor| match m {
                Modo::Retangulo => {
                    for k in 0..4 {
                        let o = k as f32 * 5.0;
                        ui.tela.retangulo(cx - 10.0 + o, cy - 8.0, 3.0, 1.5, 0.0, cor);
                        ui.tela.retangulo(cx - 10.0 + o, cy + 7.0, 3.0, 1.5, 0.0, cor);
                    }
                    for k in 0..3 {
                        let o = k as f32 * 5.0;
                        ui.tela.retangulo(cx - 10.0, cy - 7.0 + o, 1.5, 3.0, 0.0, cor);
                        ui.tela.retangulo(cx + 8.5, cy - 7.0 + o, 1.5, 3.0, 0.0, cor);
                    }
                }
                Modo::Janela => {
                    ui.tela.contorno(cx - 10.0, cy - 8.0, 20.0, 16.0, 1.5, cor);
                    ui.tela.retangulo(cx - 10.0, cy - 8.0, 20.0, 4.0, 0.0, cor);
                }
                Modo::Tela => {
                    ui.tela.contorno(cx - 11.0, cy - 8.0, 22.0, 14.0, 1.5, cor);
                    ui.tela.retangulo(cx - 4.0, cy + 8.0, 8.0, 1.5, 0.0, cor);
                }
            }) {
                self.modo = m;
            }
            x += 42.0;
        }
        // Atraso: cada clique troca (nenhum, 3, 5, 10 s).
        let rot_atraso = if self.atraso == 0 { "Sem atraso".to_string() } else { format!("Atraso: {} s", self.atraso) };
        if c::botao(ui, x + 4.0, 10.0, &rot_atraso, c::Estilo::Comum, true) {
            self.atraso = match self.atraso {
                0 => 3,
                3 => 5,
                5 => 10,
                _ => 0,
            };
        }
        x += c::largura_botao(&rot_atraso) + 16.0;
        ui.tela.retangulo(x, 14.0, 1.0, 28.0, 0.0, tema::BORDA());
        x += 10.0;

        // Ferramentas de marcação (só com uma imagem aberta).
        let ferramentas: [(Ferramenta, &str); 4] = [
            (Ferramenta::Caneta, "Caneta"),
            (Ferramenta::Marca, "Marca-texto"),
            (Ferramenta::Borracha, "Borracha"),
            (Ferramenta::Recortar, "Recortar imagem"),
        ];
        for (f, dica) in ferramentas {
            let marcado = tem && self.ferramenta == f;
            let cor_atual = match f {
                Ferramenta::Caneta => Some(CORES[self.cor]),
                Ferramenta::Marca => Some(CORES_MARCA[self.cor_marca]),
                _ => None,
            };
            if Self::botao_icone(ui, x, 8.0, tem, marcado, dica, |ui, cx, cy, cor| match f {
                Ferramenta::Caneta | Ferramenta::Marca => {
                    let larg = if f == Ferramenta::Marca { 4.0 } else { 2.0 };
                    ui.tela.linha(cx - 7.0, cy + 7.0, cx + 6.0, cy - 6.0, larg + 1.0, cor);
                    if let Some(k) = cor_atual {
                        ui.tela.retangulo(cx - 10.0, cy + 10.0, 20.0, 3.0, 1.0, Cor(k[0], k[1], k[2], 255));
                    }
                }
                Ferramenta::Borracha => {
                    ui.tela.linha(cx - 6.0, cy + 6.0, cx + 5.0, cy - 5.0, 8.0, cor);
                    ui.tela.linha(cx - 6.0, cy + 6.0, cx - 2.0, cy + 2.0, 6.0, tema::FUNDO());
                }
                _ => {
                    ui.tela.linha(cx - 6.0, cy - 10.0, cx - 6.0, cy + 6.0, 1.6, cor);
                    ui.tela.linha(cx - 10.0, cy + 6.0, cx + 8.0, cy + 6.0, 1.6, cor);
                    ui.tela.linha(cx - 10.0, cy - 6.0, cx + 6.0, cy - 6.0, 1.6, cor);
                    ui.tela.linha(cx + 6.0, cy - 6.0, cx + 6.0, cy + 10.0, 1.6, cor);
                }
            }) {
                if self.ferramenta == f {
                    // Segundo clique na caneta troca a cor.
                    match f {
                        Ferramenta::Caneta => self.cor = (self.cor + 1) % CORES.len(),
                        Ferramenta::Marca => self.cor_marca = (self.cor_marca + 1) % CORES_MARCA.len(),
                        _ => {}
                    }
                }
                self.ferramenta = f;
                self.recorte = None;
            }
            x += 42.0;
        }
        x += 4.0;
        ui.tela.retangulo(x, 14.0, 1.0, 28.0, 0.0, tema::BORDA());
        x += 10.0;
        let pode_desfazer = !self.historico.is_empty();
        if Self::botao_icone(ui, x, 8.0, pode_desfazer, false, "Desfazer (Ctrl+Z)", |ui, cx, cy, cor| {
            ui.tela.linha(cx - 8.0, cy - 2.0, cx + 4.0, cy - 2.0, 1.8, cor);
            ui.tela.linha(cx + 4.0, cy - 2.0, cx + 8.0, cy + 2.0, 1.8, cor);
            ui.tela.linha(cx + 8.0, cy + 2.0, cx + 8.0, cy + 7.0, 1.8, cor);
            ui.tela.linha(cx - 8.0, cy - 2.0, cx - 4.0, cy - 6.0, 1.8, cor);
            ui.tela.linha(cx - 8.0, cy - 2.0, cx - 4.0, cy + 2.0, 1.8, cor);
        }) {
            self.desfazer();
        }
        x += 42.0;

        // Copiar e salvar, à direita.
        let xs = l - 10.0 - 40.0;
        if Self::botao_icone(ui, xs, 8.0, tem && self.salvar_rx.is_none(), false, "Salvar como (Ctrl+S)", |ui, cx, cy, cor| {
            ui.tela.contorno(cx - 8.0, cy - 8.0, 16.0, 16.0, 1.6, cor);
            ui.tela.retangulo(cx - 4.0, cy - 8.0, 8.0, 5.0, 0.0, cor);
            ui.tela.retangulo(cx - 5.0, cy + 2.0, 10.0, 6.0, 0.0, cor);
        }) {
            self.salvar_como();
        }
        if xs - 42.0 > x && Self::botao_icone(ui, xs - 42.0, 8.0, tem, false, "Copiar (Ctrl+C)", |ui, cx, cy, cor| {
            ui.tela.contorno(cx - 8.0, cy - 9.0, 11.0, 13.0, 1.5, cor);
            ui.tela.contorno(cx - 3.0, cy - 4.0, 11.0, 13.0, 1.5, cor);
        }) {
            self.copiar();
        }

        // Aviso rápido no pé.
        if let Some((t, quando)) = self.aviso.clone() {
            if quando.elapsed() < Duration::from_secs(4) {
                let lt = desenho::medir(12.5, &t).min(l - 60.0);
                ui.tela.retangulo(l / 2.0 - lt / 2.0 - 14.0, a - 44.0, lt + 28.0, 30.0, 8.0, tema::FUNDO());
                ui.tela.texto(l / 2.0 - lt / 2.0, a - 24.0, 12.5, tema::TEXTO(), &desenho::encurtar(12.5, &t, l - 60.0));
                ui.acordar_em = Some(ui.acordar_em.unwrap_or(Duration::from_millis(500)).min(Duration::from_millis(500)));
            } else {
                self.aviso = None;
            }
        }
        let nome = self.caminho.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        ui.titulo = Some(match nome {
            Some(n) => format!("{n} – Ferramenta de Captura"),
            None => "Ferramenta de Captura".into(),
        });
    }
}

/// Janela "Permitir compartilhar a tela?" do portal.
struct Permitir {
    app: String,
}

impl App for Permitir {
    fn desenhar(&mut self, ui: &mut Ui) {
        for t in std::mem::take(&mut ui.teclas) {
            if t == Tecla::Esc {
                ui.fechar = true;
            }
        }
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());
        // Monitor com o símbolo de transmitir.
        let (ix, iy) = (28.0, 30.0);
        ui.tela.retangulo(ix, iy, 52.0, 36.0, 5.0, tema::DESTAQUE());
        ui.tela.retangulo(ix + 3.0, iy + 3.0, 46.0, 30.0, 3.0, tema::DESTAQUE_SUAVE());
        ui.tela.retangulo(ix + 18.0, iy + 38.0, 16.0, 4.0, 1.0, tema::DESTAQUE());
        for k in 0..3 {
            let r = 6.0 + k as f32 * 5.0;
            ui.tela.linha(ix + 14.0, iy + 27.0 - r, ix + 14.0 + r, iy + 27.0, 1.6, tema::DESTAQUE());
        }
        let x = 100.0;
        ui.tela.texto(x, 44.0, 17.0, tema::TEXTO(), "Compartilhar a tela?");
        let y = c::paragrafo(
            ui,
            x,
            70.0,
            l - x - 24.0,
            &format!("{} quer ver e transmitir tudo o que aparece na sua tela, inclusive notificações e senhas digitadas.", self.app),
        );
        c::paragrafo(ui, x, y + 6.0, l - x - 24.0, "Para parar, encerre o compartilhamento no próprio app.");
        let by = a - 56.0;
        if c::botao_direita(ui, l - 20.0, by, "Compartilhar a tela", c::Estilo::Principal, true) {
            PERMITIDO.store(true, Ordering::Relaxed);
            ui.fechar = true;
        }
        let bx = l - 20.0 - c::largura_botao("Compartilhar a tela") - 10.0;
        if c::botao_direita(ui, bx, by, "Cancelar", c::Estilo::Comum, true) {
            ui.fechar = true;
        }
        ui.titulo = Some("Compartilhar a tela".into());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--permitir") => {
            let app = args.get(2).cloned().unwrap_or_else(|| "Um aplicativo".into());
            penas::executar(
                Opcoes { titulo: "Compartilhar a tela".into(), app_id: "captura-permitir".into(), tamanho: (520, 240), minimo: (460, 220) },
                Box::new(Permitir { app }),
            );
            std::process::exit(if PERMITIDO.load(Ordering::Relaxed) { 0 } else { 1 });
        }
        Some("--abrir") => {
            let mut app = Captura::nova();
            if let Some(p) = args.get(2) {
                app.carregar(PathBuf::from(p));
            }
            penas::executar(
                Opcoes { titulo: "Ferramenta de Captura".into(), app_id: "captura".into(), tamanho: (1000, 680), minimo: (720, 420) },
                Box::new(app),
            );
        }
        _ => {
            let mut app = Captura::nova();
            // "captura --novo": já começa capturando (atalho do menu).
            if args.get(1).map(String::as_str) == Some("--novo") {
                app.novo();
            }
            penas::executar(
                Opcoes { titulo: "Ferramenta de Captura".into(), app_id: "captura".into(), tamanho: (1000, 680), minimo: (720, 420) },
                Box::new(app),
            );
        }
    }
}
