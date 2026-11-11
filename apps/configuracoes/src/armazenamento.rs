// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Armazenamento": quanto do disco está usado e com o quê (pastas
//! pessoais, apps), e botões para liberar espaço (Lixeira, apps sem uso).

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{componentes as c, tema, Ui};

fn tamanho_humano(b: u64) -> String {
    let g = b as f64 / 1e9;
    if g >= 1.0 {
        format!("{g:.1} GB").replace('.', ",")
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

/// Tamanho de uma pasta (sem seguir links, sem sair do disco).
fn tamanho(p: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.blocks() * 512;
    }
    let dev = m.dev();
    let mut total = 0;
    let mut pilha = vec![p.to_path_buf()];
    while let Some(d) = pilha.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() && m.dev() == dev {
                pilha.push(e.path());
            } else if m.is_file() {
                total += m.blocks() * 512;
            }
        }
    }
    total
}

/// (total, livre) do disco que tem a pasta.
fn disco(p: &str) -> (u64, u64) {
    let o = Command::new("df").args(["-B1", "--output=size,avail", p]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let n: Vec<u64> = o.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
    (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0))
}

type Partes = Vec<(String, u64)>;

#[derive(Default)]
pub struct PaginaArmazenamento {
    partes: Option<Partes>,
    rx: Option<Receiver<Partes>>,
    limpando: Option<Receiver<String>>,
    pub aviso: Option<String>,
}

fn casa() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

fn medir() -> Partes {
    let h = casa();
    let mut v: Partes = ["Documentos", "Downloads", "Imagens", "Músicas", "Vídeos", "Área de Trabalho"]
        .iter()
        .map(|n| (n.to_string(), tamanho(&h.join(n))))
        .collect();
    v.push(("Lixeira".into(), tamanho(&h.join(".local/share/Trash"))));
    v.push(("Apps (Flatpak)".into(), tamanho(Path::new("/var/lib/flatpak")) + tamanho(&h.join(".local/share/flatpak"))));
    v.push(("Dados e cache dos apps".into(), tamanho(&h.join(".var/app")) + tamanho(&h.join(".cache"))));
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

impl PaginaArmazenamento {
    fn medir(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(medir());
        });
        self.rx = Some(rx);
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if self.partes.is_none() && self.rx.is_none() {
            self.medir();
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(p) => {
                    self.partes = Some(p);
                    self.rx = None;
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(200)),
            }
        }
        if let Some(rx) = &self.limpando {
            match rx.try_recv() {
                Ok(m) => {
                    self.aviso = Some(m);
                    self.limpando = None;
                    self.medir();
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(200)),
            }
        }
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Armazenamento");
        let (total, livre) = disco("/");
        let usado = total.saturating_sub(livre);
        let f = if total > 0 { usado as f32 / total as f32 } else { 0.0 };
        ui.tela.texto(x, 96.0, 15.0, tema::TEXTO(), &format!("{} usados de {}", tamanho_humano(usado), tamanho_humano(total)));
        ui.tela.texto(x, 116.0, 12.5, tema::TEXTO_SECUNDARIO(), &format!("{} livres", tamanho_humano(livre)));
        ui.tela.retangulo(x, 128.0, w, 10.0, 5.0, tema::BORDA());
        let cor = if f > 0.9 { tema::ERRO() } else { tema::DESTAQUE() };
        ui.tela.retangulo(x, 128.0, (w * f).max(10.0), 10.0, 5.0, cor);

        let mut y = 170.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "O que ocupa espaço");
        y += 14.0;
        match &self.partes {
            None => {
                ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Medindo…");
                y += 34.0;
            }
            Some(p) => {
                let maior = p.first().map(|a| a.1).unwrap_or(1).max(1);
                for (nome, t) in p {
                    ui.tela.texto(x, y + 20.0, 13.5, tema::TEXTO(), nome);
                    let bx = x + w * 0.42;
                    let bw = w * 0.58 - 90.0;
                    ui.tela.retangulo(bx, y + 12.0, bw, 6.0, 3.0, tema::BORDA());
                    ui.tela.retangulo(bx, y + 12.0, (bw * (*t as f32 / maior as f32)).max(3.0), 6.0, 3.0, tema::DESTAQUE());
                    ui.tela.texto(bx + bw + 12.0, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), &tamanho_humano(*t));
                    y += 30.0;
                }
            }
        }
        y += 20.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Liberar espaço");
        y += 14.0;
        let livre_btn = self.limpando.is_none();
        if c::botao(ui, x, y, "Esvaziar a Lixeira", c::Estilo::Comum, livre_btn) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let t = casa().join(".local/share/Trash");
                let _ = std::fs::remove_dir_all(t.join("files"));
                let _ = std::fs::remove_dir_all(t.join("info"));
                let _ = tx.send("Lixeira esvaziada.".to_string());
            });
            self.limpando = Some(rx);
        }
        let lx = x + c::largura_botao("Esvaziar a Lixeira") + 10.0;
        if c::botao(ui, lx, y, "Remover partes de apps sem uso", c::Estilo::Comum, livre_btn) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let ok = Command::new("flatpak").args(["uninstall", "--unused", "-y", "--noninteractive"]).status().map(|s| s.success()).unwrap_or(false);
                let _ = tx.send(if ok { "Partes sem uso removidas.".to_string() } else { "Não deu para remover as partes sem uso.".to_string() });
            });
            self.limpando = Some(rx);
        }
        if !livre_btn {
            ui.tela.texto(x, y + 56.0, 12.5, tema::TEXTO_SECUNDARIO(), "Limpando…");
        }
    }
}
