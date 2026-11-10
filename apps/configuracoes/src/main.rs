// SPDX-License-Identifier: GPL-3.0-or-later
//! Configurações do Ararinha: personalização (barra de tarefas) e Sobre.
//! As escolhas vão para ~/.config/plumagem/plumagem.conf, que a Plumagem
//! relê sozinha.

use std::{fs, path::PathBuf};

use penas::{desenho, tema, App, Cor, Opcoes, Ui};

const LATERAL_L: f32 = 220.0;

#[derive(Clone, Copy, PartialEq)]
enum Pagina {
    Personalizacao,
    Sobre,
}

struct Configuracoes {
    pagina: Pagina,
    barra: String,
    sobre: Vec<(String, String)>,
    aviso: Option<String>,
}

fn arquivo_config() -> PathBuf {
    let casa = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(casa).join(".config/plumagem/plumagem.conf")
}

fn ler_chave(chave: &str) -> Option<String> {
    for c in [arquivo_config(), PathBuf::from("/etc/plumagem/plumagem.conf")] {
        if let Ok(t) = fs::read_to_string(&c) {
            for l in t.lines() {
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
            Some((k, _)) if k.trim() == chave => {
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

fn ler_sobre() -> Vec<(String, String)> {
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string();
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| "Desconhecido".into());
    let nucleos = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();
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

impl Configuracoes {
    fn novo() -> Configuracoes {
        Configuracoes {
            pagina: Pagina::Personalizacao,
            barra: ler_chave("barra").unwrap_or_else(|| "media".into()),
            sobre: ler_sobre(),
            aviso: None,
        }
    }

    fn personalizacao(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO, "Personalização");
        ui.tela.texto(x0 + 32.0, 100.0, 15.0, tema::TEXTO, "Tamanho da barra de tarefas");
        ui.tela.texto(x0 + 32.0, 122.0, 12.5, tema::TEXTO_SECUNDARIO, "Muda na hora, sem precisar sair da sessão.");
        let opcoes = [("pequena", "Pequena", 38.0), ("media", "Média", 44.0), ("grande", "Grande", 50.0)];
        let cl = ((l - 64.0 - 2.0 * 16.0) / 3.0).min(220.0);
        let mut escolhida = None;
        for (i, (id, nome, alt)) in opcoes.iter().enumerate() {
            let x = x0 + 32.0 + i as f32 * (cl + 16.0);
            let y = 140.0;
            let sel = self.barra == *id;
            let sobre = ui.sobre(x, y, cl, 150.0);
            let borda = if sel { tema::DESTAQUE } else if sobre { tema::TEXTO_SECUNDARIO } else { tema::BORDA };
            let esp = if sel { 2.0 } else { 1.0 };
            ui.tela.retangulo(x, y, cl, 150.0, 10.0, borda);
            ui.tela.retangulo(x + esp, y + esp, cl - 2.0 * esp, 150.0 - 2.0 * esp, 10.0 - esp, tema::BRANCO);
            // Prévia: um "monitor" com a barra na altura relativa.
            let (px, py, pl, pa) = (x + 16.0, y + 14.0, cl - 32.0, 86.0);
            ui.tela.retangulo(px, py, pl, pa, 6.0, Cor::hex(0x3E6FA8));
            let ba = pa * alt / 400.0 * 2.2;
            ui.tela.retangulo(px, py + pa - ba, pl, ba, 0.0, Cor(255, 255, 255, 210));
            ui.tela.retangulo(px + 4.0, py + pa - ba + ba * 0.2, ba * 0.6, ba * 0.6, ba * 0.3, tema::DESTAQUE);
            let lt = desenho::medir(14.0, nome);
            ui.tela.texto(x + cl / 2.0 - lt / 2.0, y + 128.0, 14.0, tema::TEXTO, nome);
            if ui.clicou(x, y, cl, 150.0) {
                escolhida = Some(*id);
            }
        }
        if let Some(id) = escolhida {
            match gravar_chave("barra", id) {
                Ok(()) => {
                    self.barra = id.to_string();
                    self.aviso = None;
                }
                Err(e) => self.aviso = Some(format!("Não deu para salvar: {e}")),
            }
        }
    }

    fn sobre(&mut self, ui: &mut Ui, x0: f32) {
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO, "Sobre este computador");
        let mut y = 100.0;
        for (k, v) in &self.sobre {
            ui.tela.texto(x0 + 32.0, y, 13.0, tema::TEXTO_SECUNDARIO, k);
            ui.tela.texto(x0 + 210.0, y, 13.5, tema::TEXTO, v);
            y += 30.0;
        }
        ui.tela.texto(x0 + 32.0, y + 20.0, 12.0, tema::TEXTO_SECUNDARIO, "Software livre, sob a GPL-3.0. Feito do zero, com carinho, pelo Mingau.");
    }
}

impl App for Configuracoes {
    fn desenhar(&mut self, ui: &mut Ui) {
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::BRANCO);
        ui.tela.retangulo(0.0, 0.0, LATERAL_L, a, 0.0, tema::FUNDO);
        ui.tela.retangulo(LATERAL_L - 1.0, 0.0, 1.0, a, 0.0, tema::BORDA);
        ui.tela.texto(20.0, 40.0, 17.0, tema::TEXTO, "Configurações");
        let paginas = [(Pagina::Personalizacao, "Personalização"), (Pagina::Sobre, "Sobre")];
        for (i, (p, nome)) in paginas.iter().enumerate() {
            let y = 64.0 + i as f32 * 36.0;
            if self.pagina == *p {
                ui.tela.retangulo(10.0, y, LATERAL_L - 20.0, 32.0, 6.0, tema::DESTAQUE_SUAVE);
                ui.tela.retangulo(10.0, y + 8.0, 3.0, 16.0, 1.5, tema::DESTAQUE);
            } else if ui.sobre(10.0, y, LATERAL_L - 20.0, 32.0) {
                ui.tela.retangulo(10.0, y, LATERAL_L - 20.0, 32.0, 6.0, tema::SUPERFICIE_ALTA);
            }
            ui.tela.texto(26.0, y + 21.0, 13.5, tema::TEXTO, nome);
            if ui.clicou(10.0, y, LATERAL_L - 20.0, 32.0) {
                self.pagina = *p;
            }
        }
        match self.pagina {
            Pagina::Personalizacao => self.personalizacao(ui, LATERAL_L, l - LATERAL_L),
            Pagina::Sobre => self.sobre(ui, LATERAL_L),
        }
        if let Some(av) = &self.aviso {
            ui.tela.texto(LATERAL_L + 32.0, a - 20.0, 12.5, tema::ERRO, av);
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Configurações".into(),
            app_id: "configuracoes".into(),
            tamanho: (900, 560),
            minimo: (640, 400),
        },
        Box::new(Configuracoes::novo()),
    );
}
