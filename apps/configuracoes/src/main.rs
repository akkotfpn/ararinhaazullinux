// SPDX-License-Identifier: GPL-3.0-or-later
//! Configurações do Ararinha: personalização (barra de tarefas e papel de
//! parede), energia e Sobre. As escolhas de aparência vão para
//! ~/.config/plumagem/plumagem.conf, que a Plumagem relê sozinha; o plano
//! de energia vai pelo ajudante `ararinha-energia`.

mod rede;

use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{desenho, tema, App, Cor, Opcoes, Ui};

const LATERAL_L: f32 = 220.0;
const PASTA_PAPEIS: &str = "/usr/share/backgrounds/ararinha";
static MINGAU_PNG: &[u8] = include_bytes!("../recursos/mingau.png");

/// Nomes bonitos dos papéis de parede que vêm com o sistema.
const NOMES_PAPEIS: [(&str, &str); 5] = [
    ("spix", "Spix"),
    ("noite-no-sertao", "Noite no Sertão"),
    ("velho-chico", "Velho Chico"),
    ("aero-azul", "Aero Azul"),
    ("caatinga-em-flor", "Caatinga em Flor"),
];

#[derive(Clone, Copy, PartialEq)]
enum Pagina {
    Personalizacao,
    Rede,
    Energia,
    Sobre,
}

struct Papel {
    id: String,
    nome: String,
    miniatura: Option<penas::desenho::Imagem>,
}

struct Configuracoes {
    pagina: Pagina,
    barra: String,
    papel: String,
    tema: String,
    sons: String,
    papeis: Vec<Papel>,
    miniaturas: Option<Receiver<(usize, penas::desenho::Imagem)>>,
    energia: String,
    sobre: Vec<(String, String)>,
    aviso: Option<String>,
    rolagem: f32,
    rede: Option<rede::PaginaRede>,
    mingau: Option<penas::desenho::Imagem>,
}

fn arquivo_config() -> PathBuf {
    let casa = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(casa).join(".config/plumagem/plumagem.conf")
}

fn ler_chave(chave: &str) -> Option<String> {
    for c in [arquivo_config(), PathBuf::from("/etc/plumagem/plumagem.conf")] {
        if let Ok(t) = fs::read_to_string(&c) {
            for l in t.lines() {
                if l.trim_start().starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = l.split_once('=') {
                    if k.trim() == chave {
                        return Some(v.trim().trim_matches('"').to_string());
                    }
                }
            }
        }
    }
    None
}

/// Grava `chave = valor` no arquivo do usuário, mantendo o resto.
fn gravar_chave(chave: &str, valor: &str) -> std::io::Result<()> {
    let p = arquivo_config();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d)?;
    }
    let atual = fs::read_to_string(&p).unwrap_or_default();
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
    fs::write(&p, linhas.join("\n") + "\n")
}

fn listar_papeis() -> Vec<Papel> {
    let mut ids: Vec<String> = fs::read_dir(PASTA_PAPEIS)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".png")).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    // Os do sistema na ordem conhecida; os outros depois, em ordem alfabética.
    ids.sort_by_key(|id| (NOMES_PAPEIS.iter().position(|(n, _)| n == id).unwrap_or(usize::MAX), id.clone()));
    if ids.is_empty() {
        ids.push("spix".into());
    }
    ids.into_iter()
        .map(|id| {
            let nome = NOMES_PAPEIS
                .iter()
                .find(|(n, _)| *n == id)
                .map(|(_, nome)| nome.to_string())
                .unwrap_or_else(|| id.replace('-', " "));
            Papel { id, nome, miniatura: None }
        })
        .collect()
}

fn ler_sobre() -> Vec<(String, String)> {
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string();
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| "Desconhecido".into());
    let nucleos = cpuinfo.lines().filter(|l| l.starts_with("processor")).count();
    let mem_kb: u64 = fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("MemTotal"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let nome = fs::read_to_string("/etc/hostname").unwrap_or_default().trim().to_string();
    let usuario = std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_default();
    vec![
        ("Sistema".into(), "Ararinha Azul Linux 0.1 (Primeiro Pio)".into()),
        ("Nome do computador".into(), nome),
        ("Usuário".into(), usuario),
        ("Processador".into(), format!("{cpu} ({nucleos} núcleos)")),
        ("Memória".into(), format!("{:.1} GB", mem_kb as f64 / 1024.0 / 1024.0).replace('.', ",")),
        ("Kernel".into(), format!("Spix Kernel {kernel}")),
        ("Interface".into(), "Plumagem".into()),
    ]
}

fn ler_energia() -> String {
    std::process::Command::new("ararinha-energia")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "padrao".into())
}

impl Configuracoes {
    fn novo() -> Configuracoes {
        let papeis = listar_papeis();
        // As miniaturas saem numa linha separada para a janela abrir na hora.
        let (tx, rx) = channel();
        let ids: Vec<String> = papeis.iter().map(|p| p.id.clone()).collect();
        std::thread::spawn(move || {
            for (i, id) in ids.iter().enumerate() {
                if let Ok(b) = fs::read(format!("{PASTA_PAPEIS}/{id}.png")) {
                    if let Some(m) = desenho::icone(&b, 176, 99) {
                        if tx.send((i, m)).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Configuracoes {
            pagina: Pagina::Personalizacao,
            barra: ler_chave("barra").unwrap_or_else(|| "media".into()),
            papel: ler_chave("papel").unwrap_or_else(|| "spix".into()),
            tema: ler_chave("tema").unwrap_or_else(|| "claro".into()),
            sons: ler_chave("sons").unwrap_or_else(|| "sim".into()),
            papeis,
            miniaturas: Some(rx),
            energia: ler_energia(),
            sobre: ler_sobre(),
            aviso: None,
            rolagem: 0.0,
            rede: None,
            mingau: desenho::icone(MINGAU_PNG, 120, 141),
        }
    }

    fn salvar(&mut self, chave: &str, valor: &str) -> bool {
        match gravar_chave(chave, valor) {
            Ok(()) => {
                self.aviso = None;
                true
            }
            Err(e) => {
                self.aviso = Some(format!("Não deu para salvar: {e}"));
                false
            }
        }
    }

    fn personalizacao(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Conteúdo maior que a janela: rola com a roda do mouse.
        let altura_total = 410.0 + ((self.papeis.len() + 2) / 3) as f32 * 140.0 + 40.0;
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (altura_total - ui.altura()).max(0.0));
        let oy = -self.rolagem;

        ui.tela.texto(x0 + 32.0, oy + 52.0, 22.0, tema::TEXTO(), "Personalização");
        ui.tela.texto(x0 + 32.0, oy + 96.0, 15.0, tema::TEXTO(), "Tamanho da barra de tarefas");
        let opcoes = [("pequena", "Pequena", 38.0), ("media", "Média", 44.0), ("grande", "Grande", 50.0)];
        let cl = ((l - 64.0 - 2.0 * 16.0) / 3.0).min(200.0);
        let ca = 112.0;
        let mut escolhida = None;
        for (i, (id, nome, alt)) in opcoes.iter().enumerate() {
            let x = x0 + 32.0 + i as f32 * (cl + 16.0);
            let y = oy + 112.0;
            let sel = self.barra == *id;
            let sobre = ui.sobre(x, y, cl, ca);
            let borda = if sel { tema::DESTAQUE() } else if sobre { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() };
            let esp = if sel { 2.0 } else { 1.0 };
            ui.tela.retangulo(x, y, cl, ca, 10.0, borda);
            ui.tela.retangulo(x + esp, y + esp, cl - 2.0 * esp, ca - 2.0 * esp, 10.0 - esp, tema::JANELA());
            let (px, py, pl, pa) = (x + 14.0, y + 12.0, cl - 28.0, 64.0);
            ui.tela.retangulo(px, py, pl, pa, 6.0, Cor::hex(0x3E6FA8));
            let ba = pa * alt / 400.0 * 2.2;
            ui.tela.retangulo(px, py + pa - ba, pl, ba, 0.0, Cor(255, 255, 255, 210));
            ui.tela.retangulo(px + 4.0, py + pa - ba + ba * 0.2, ba * 0.6, ba * 0.6, ba * 0.3, tema::DESTAQUE());
            let lt = desenho::medir(13.5, nome);
            ui.tela.texto(x + cl / 2.0 - lt / 2.0, y + 98.0, 13.5, tema::TEXTO(), nome);
            if ui.clicou(x, y, cl, ca) {
                escolhida = Some(*id);
            }
        }
        if let Some(id) = escolhida {
            if self.salvar("barra", id) {
                self.barra = id.to_string();
            }
        }

        // Modo de cor
        let y_modo = oy + 270.0;
        ui.tela.texto(x0 + 32.0, y_modo, 15.0, tema::TEXTO(), "Modo de cor");
        let mut novo_modo = None;
        for (i, (id, nome)) in [("claro", "Claro"), ("escuro", "Escuro")].iter().enumerate() {
            let (x, y, w, h) = (x0 + 32.0 + i as f32 * 132.0, y_modo + 14.0, 120.0, 36.0);
            let sel = (self.tema == *id) || (self.tema.is_empty() && *id == "claro");
            let sobre = ui.sobre(x, y, w, h);
            ui.tela.retangulo(x, y, w, h, 8.0, if sel { tema::DESTAQUE() } else if sobre { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
            let esp = if sel { 2.0 } else { 1.0 };
            ui.tela.retangulo(x + esp, y + esp, w - 2.0 * esp, h - 2.0 * esp, 8.0 - esp, if sel { tema::DESTAQUE_SUAVE() } else { tema::JANELA() });
            // Amostra do modo
            let amostra = if *id == "escuro" { Cor::hex(0x0B1A2E) } else { Cor::hex(0xF4F7FA) };
            ui.tela.retangulo(x + 12.0, y + 10.0, 16.0, 16.0, 8.0, tema::BORDA());
            ui.tela.retangulo(x + 13.0, y + 11.0, 14.0, 14.0, 7.0, amostra);
            ui.tela.texto(x + 38.0, y + 23.0, 13.5, tema::TEXTO(), nome);
            if ui.clicou(x, y, w, h) && !sel {
                novo_modo = Some(*id);
            }
        }
        // Sons do sistema (ligados/desligados), na mesma linha.
        {
            let (x, y, w, h) = (x0 + 32.0 + 2.0 * 132.0 + 24.0, y_modo + 14.0, 190.0, 36.0);
            let ligados = self.sons != "nao";
            let sobre = ui.sobre(x, y, w, h);
            ui.tela.retangulo(x, y, w, h, 8.0, if sobre { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
            ui.tela.retangulo(x + 1.0, y + 1.0, w - 2.0, h - 2.0, 7.0, tema::JANELA());
            // Chavinha
            let (cx, cy) = (x + 12.0, y + 9.0);
            ui.tela.retangulo(cx, cy, 34.0, 18.0, 9.0, if ligados { tema::DESTAQUE() } else { tema::BORDA() });
            let bx = if ligados { cx + 18.0 } else { cx + 2.0 };
            ui.tela.retangulo(bx, cy + 2.0, 14.0, 14.0, 7.0, tema::BRANCO());
            ui.tela.texto(x + 56.0, y + 23.0, 13.5, tema::TEXTO(), if ligados { "Sons do sistema" } else { "Sons desligados" });
            if ui.clicou(x, y, w, h) {
                let novo = if ligados { "nao" } else { "sim" };
                if self.salvar("sons", novo) {
                    self.sons = novo.to_string();
                }
            }
        }
        if let Some(id) = novo_modo {
            if self.salvar("tema", id) {
                self.tema = id.to_string();
                tema::definir_escuro(id == "escuro");
                ui.redesenhar = true;
            }
        }

        let y_papel = oy + 350.0;
        ui.tela.texto(x0 + 32.0, y_papel, 15.0, tema::TEXTO(), "Papel de parede");
        ui.tela.texto(x0 + 32.0, y_papel + 22.0, 12.5, tema::TEXTO_SECUNDARIO(), "Vale também para o vidro da barra e das janelas.");
        let (ml, ma) = (176.0, 99.0);
        let mut escolhido = None;
        for (i, p) in self.papeis.iter().enumerate() {
            let x = x0 + 32.0 + (i % 3) as f32 * (ml + 24.0);
            let y = y_papel + 40.0 + (i / 3) as f32 * 140.0;
            let sel = self.papel == p.id;
            let sobre = ui.sobre(x - 4.0, y - 4.0, ml + 8.0, ma + 8.0);
            if sel {
                ui.tela.retangulo(x - 4.0, y - 4.0, ml + 8.0, ma + 8.0, 9.0, tema::DESTAQUE());
            } else if sobre {
                ui.tela.retangulo(x - 3.0, y - 3.0, ml + 6.0, ma + 6.0, 8.0, tema::BORDA());
            }
            match &p.miniatura {
                Some(m) => ui.tela.imagem(x as i32, y as i32, m),
                None => ui.tela.retangulo(x, y, ml, ma, 6.0, tema::SUPERFICIE_ALTA()),
            }
            let cor = if sel { tema::DESTAQUE() } else { tema::TEXTO() };
            ui.tela.texto(x, y + ma + 22.0, 13.0, cor, &desenho::encurtar(13.0, &p.nome, ml));
            if ui.clicou(x - 4.0, y - 4.0, ml + 8.0, ma + 8.0) {
                escolhido = Some(p.id.clone());
            }
        }
        if let Some(id) = escolhido {
            if self.salvar("papel", &id) {
                self.papel = id;
            }
        }
    }

    fn energia(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO(), "Energia");
        ui.tela.texto(x0 + 32.0, 84.0, 13.0, tema::TEXTO_SECUNDARIO(), "Escolha como o computador equilibra rapidez e consumo.");
        let planos = [
            ("padrao", "Padrão", "Equilibrado: rápido quando precisa, econômico quando está parado."),
            ("economico", "Econômico", "Gasta menos energia e esquenta menos. Bom para notebook na bateria."),
            ("desempenho", "Desempenho máximo", "O processador sempre no máximo. Gasta mais energia."),
        ];
        let mut escolhido = None;
        for (i, (id, nome, desc)) in planos.iter().enumerate() {
            let y = 110.0 + i as f32 * 78.0;
            let (x, w, h) = (x0 + 32.0, (l - 64.0).min(620.0), 66.0);
            let sel = self.energia == *id;
            let sobre = ui.sobre(x, y, w, h);
            ui.tela.retangulo(x, y, w, h, 8.0, if sel { tema::DESTAQUE() } else if sobre { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
            let esp = if sel { 2.0 } else { 1.0 };
            ui.tela.retangulo(x + esp, y + esp, w - 2.0 * esp, h - 2.0 * esp, 8.0 - esp, if sel { tema::DESTAQUE_SUAVE() } else { tema::JANELA() });
            // Botão de rádio
            ui.tela.retangulo(x + 18.0, y + 24.0, 18.0, 18.0, 9.0, if sel { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() });
            ui.tela.retangulo(x + 20.0, y + 26.0, 14.0, 14.0, 7.0, tema::JANELA());
            if sel {
                ui.tela.retangulo(x + 23.0, y + 29.0, 8.0, 8.0, 4.0, tema::DESTAQUE());
            }
            ui.tela.texto(x + 50.0, y + 28.0, 14.5, tema::TEXTO(), nome);
            ui.tela.texto(x + 50.0, y + 49.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, desc, w - 64.0));
            if ui.clicou(x, y, w, h) && !sel {
                escolhido = Some(*id);
            }
        }
        if let Some(id) = escolhido {
            match std::process::Command::new("ararinha-energia").arg(id).output() {
                Ok(o) if o.status.success() => {
                    self.energia = id.to_string();
                    self.aviso = Some(String::from_utf8_lossy(&o.stdout).trim().to_string());
                }
                Ok(o) => self.aviso = Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
                Err(e) => self.aviso = Some(format!("Não deu para mudar o plano: {e}")),
            }
        }
    }

    fn sobre(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO(), "Sobre o Ararinha Azul Linux");
        if let Some(m) = &self.mingau {
            ui.tela.imagem((x0 + l - 32.0 - 120.0) as i32, 80, m);
        }
        let mut y = 100.0;
        for (k, v) in &self.sobre {
            ui.tela.texto(x0 + 32.0, y, 13.0, tema::TEXTO_SECUNDARIO(), k);
            ui.tela.texto(x0 + 200.0, y, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, v, l - 200.0 - 170.0));
            y += 28.0;
        }
        y += 18.0;
        ui.tela.retangulo(x0 + 32.0, y, l - 64.0, 1.0, 0.0, tema::BORDA());
        y += 30.0;
        let textos: [(f32, Cor, &str); 6] = [
            (15.0, tema::TEXTO(), "Um projeto sem fins lucrativos"),
            (13.0, tema::TEXTO_SECUNDARIO(), "O Ararinha existe para facilitar a vida de quem usa o computador: um sistema"),
            (13.0, tema::TEXTO_SECUNDARIO(), "livre, em português, com cara de casa. Não vende nada, não mostra anúncios e"),
            (13.0, tema::TEXTO_SECUNDARIO(), "não coleta seus dados. O nome homenageia a ararinha-azul (Cyanopsitta spixii)."),
            (15.0, tema::TEXTO(), "Créditos"),
            (13.0, tema::TEXTO_SECUNDARIO(), "Criado por Davi Souza, com a ajuda do Claude (Anthropic). Mascote: Mingau."),
        ];
        for (i, (tam, cor, t)) in textos.iter().enumerate() {
            if i == 4 {
                y += 14.0;
            }
            ui.tela.texto(x0 + 32.0, y, *tam, *cor, &desenho::encurtar(*tam, t, l - 64.0));
            y += if *tam > 14.0 { 26.0 } else { 20.0 };
        }
        let rodape = [
            "Feito sobre o trabalho de milhares de pessoas: Linux, GNU, systemd, Mesa, Wayland,",
            "Smithay, Rust, LLVM, PipeWire, Flatpak e muitos outros projetos livres. Obrigado!",
            "Software livre sob a GPL-3.0; artes sob a CC BY-SA 4.0.",
        ];
        for t in rodape {
            ui.tela.texto(x0 + 32.0, y, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, t, l - 64.0));
            y += 19.0;
        }
    }
}

impl App for Configuracoes {
    fn desenhar(&mut self, ui: &mut Ui) {
        // Miniaturas que já ficaram prontas.
        if let Some(rx) = &self.miniaturas {
            loop {
                match rx.try_recv() {
                    Ok((i, m)) => {
                        if let Some(p) = self.papeis.get_mut(i) {
                            p.miniatura = Some(m);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ui.acordar_em = Some(Duration::from_millis(60));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.miniaturas = None;
                        break;
                    }
                }
            }
        }

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());
        match self.pagina {
            Pagina::Personalizacao => self.personalizacao(ui, LATERAL_L, l - LATERAL_L),
            Pagina::Rede => {
                let r = self.rede.get_or_insert_with(rede::PaginaRede::nova);
                r.desenhar(ui, LATERAL_L, l - LATERAL_L);
                if let Some(av) = r.aviso.take() {
                    self.aviso = Some(av);
                }
            }
            Pagina::Energia => self.energia(ui, LATERAL_L, l - LATERAL_L),
            Pagina::Sobre => self.sobre(ui, LATERAL_L, l - LATERAL_L),
        }
        // A lateral por cima (o conteúdo rolado não passa por ela).
        ui.tela.retangulo(0.0, 0.0, LATERAL_L, a, 0.0, tema::FUNDO());
        ui.tela.retangulo(LATERAL_L - 1.0, 0.0, 1.0, a, 0.0, tema::BORDA());
        ui.tela.texto(20.0, 40.0, 17.0, tema::TEXTO(), "Configurações");
        let paginas = [
            (Pagina::Personalizacao, "Personalização"),
            (Pagina::Rede, "Rede e Internet"),
            (Pagina::Energia, "Energia"),
            (Pagina::Sobre, "Sobre"),
        ];
        for (i, (p, nome)) in paginas.iter().enumerate() {
            let y = 64.0 + i as f32 * 36.0;
            if self.pagina == *p {
                ui.tela.retangulo(10.0, y, LATERAL_L - 20.0, 32.0, 6.0, tema::DESTAQUE_SUAVE());
                ui.tela.retangulo(10.0, y + 8.0, 3.0, 16.0, 1.5, tema::DESTAQUE());
            } else if ui.sobre(10.0, y, LATERAL_L - 20.0, 32.0) {
                ui.tela.retangulo(10.0, y, LATERAL_L - 20.0, 32.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            ui.tela.texto(26.0, y + 21.0, 13.5, tema::TEXTO(), nome);
            if ui.clicou(10.0, y, LATERAL_L - 20.0, 32.0) {
                self.pagina = *p;
                self.aviso = None;
                self.rolagem = 0.0;
            }
        }
        if let Some(av) = &self.aviso {
            ui.tela.retangulo(LATERAL_L, a - 36.0, l - LATERAL_L, 36.0, 0.0, tema::FUNDO());
            let cor = if av.starts_with("Não") { tema::ERRO() } else { tema::TEXTO_SECUNDARIO() };
            ui.tela.texto(LATERAL_L + 32.0, a - 13.0, 12.5, cor, &desenho::encurtar(12.5, av, l - LATERAL_L - 64.0));
        }
    }
}

fn main() {
    // "configuracoes sobre" abre direto no Sobre (como o winver).
    let mut app = Configuracoes::novo();
    match std::env::args().nth(1).as_deref() {
        Some("sobre") => app.pagina = Pagina::Sobre,
        Some("energia") => app.pagina = Pagina::Energia,
        Some("rede") => {
            app.pagina = Pagina::Rede;
            let mut r = rede::PaginaRede::nova();
            r.pedida = std::env::args().nth(2);
            app.rede = Some(r);
        }
        _ => {}
    }
    penas::executar(
        Opcoes {
            titulo: "Configurações".into(),
            app_id: "configuracoes".into(),
            tamanho: (900, 620),
            minimo: (720, 480),
        },
        Box::new(app),
    );
}
