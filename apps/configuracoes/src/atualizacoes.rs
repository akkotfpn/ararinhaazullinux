// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Atualizações": os apps do Flathub se atualizam sozinhos uma vez
//! por dia (a sessão cuida disso); aqui liga/desliga, mostra a última vez
//! e procura agora.

use std::{
    process::Command,
    sync::mpsc::{channel, Receiver},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use penas::{componentes as c, tema, Ui};

use crate::{gravar_chave, ler_chave};

fn estado_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state/ararinha")
}

fn quando(seg: u64) -> String {
    let agora = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let d = agora.saturating_sub(seg);
    match d {
        0..=119 => "agora há pouco".into(),
        120..=7199 => format!("há {} minutos", d / 60),
        7200..=172_799 => format!("há {} horas", d / 3600),
        _ => format!("há {} dias", d / 86400),
    }
}

#[derive(Default)]
pub struct PaginaAtualizacoes {
    /// Apps com atualização: (nome/id).
    pendentes: Option<Vec<String>>,
    rx: Option<Receiver<Result<Vec<String>, String>>>,
    pub aviso: Option<String>,
}

/// Apps com atualização disponível (sem instalar nada).
fn procurar() -> Result<Vec<String>, String> {
    let _ = Command::new("flatpak").args(["update", "--user", "--appstream"]).output();
    let o = Command::new("flatpak")
        .args(["remote-ls", "--user", "--updates", "--columns=name,application"])
        .output()
        .map_err(|e| format!("flatpak: {e}"))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("falhou").to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.split('\t').next()).filter(|n| !n.trim().is_empty()).map(|n| n.trim().to_string()).collect())
}

impl PaginaAtualizacoes {
    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx = None;
                    match r {
                        Ok(v) => self.pendentes = Some(v),
                        Err(e) => self.aviso = Some(format!("Não deu para procurar: {e}")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(200)),
            }
        }
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Atualizações");

        let mut y = 96.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Atualizar os apps sozinho");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Uma vez por dia, em segundo plano, sem atrapalhar o que você faz.");
        let mut auto = ler_chave("atualizar_auto").as_deref() != Some("nao");
        if c::chave(ui, x + w - 44.0, y - 8.0, &mut auto) {
            let _ = gravar_chave("atualizar_auto", if auto { "sim" } else { "nao" });
        }
        y += 44.0;
        let ultima = std::fs::read_to_string(estado_dir().join("ultima-atualizacao")).ok().and_then(|s| s.trim().parse::<u64>().ok());
        let txt = match ultima {
            Some(s) => format!("Última atualização automática: {}.", quando(s)),
            None => "Ainda não houve atualização automática.".into(),
        };
        ui.tela.texto(x, y + 10.0, 12.5, tema::TEXTO_SECUNDARIO(), &txt);

        y += 44.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Apps");
        y += 12.0;
        match (&self.pendentes, self.rx.is_some()) {
            (_, true) => {
                ui.tela.texto(x, y + 20.0, 13.0, tema::TEXTO_SECUNDARIO(), "Procurando atualizações…");
                y += 36.0;
            }
            (None, false) => {
                ui.tela.texto(x, y + 20.0, 13.0, tema::TEXTO_SECUNDARIO(), "Clique em \"Procurar agora\" para ver se há novidades.");
                y += 36.0;
            }
            (Some(v), false) if v.is_empty() => {
                ui.tela.texto(x, y + 20.0, 13.0, tema::TEXTO(), "Tudo em dia.");
                y += 36.0;
            }
            (Some(v), false) => {
                ui.tela.texto(x, y + 20.0, 13.0, tema::TEXTO(), &format!("{} com atualização:", if v.len() == 1 { "1 app".to_string() } else { format!("{} apps", v.len()) }));
                y += 30.0;
                for n in v.iter().take(8) {
                    ui.tela.texto(x + 12.0, y + 16.0, 12.5, tema::TEXTO_SECUNDARIO(), &format!("• {n}"));
                    y += 22.0;
                }
                if v.len() > 8 {
                    ui.tela.texto(x + 12.0, y + 16.0, 12.5, tema::TEXTO_SECUNDARIO(), &format!("e mais {}", v.len() - 8));
                    y += 22.0;
                }
                y += 8.0;
            }
        }
        if c::botao(ui, x, y + 8.0, "Procurar agora", c::Estilo::Comum, self.rx.is_none()) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(procurar());
            });
            self.rx = Some(rx);
        }
        let tem = self.pendentes.as_ref().is_some_and(|v| !v.is_empty());
        if tem && c::botao(ui, x + c::largura_botao("Procurar agora") + 10.0, y + 8.0, "Atualizar na Central de Apps", c::Estilo::Principal, true) {
            let _ = Command::new("central").arg("atualizar").spawn();
        }

        y += 80.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Sistema");
        c::paragrafo(ui, x, y + 12.0, w, "O Ararinha ainda é atualizado instalando uma imagem nova. As atualizações do sistema por aqui, com volta para a versão anterior, estão a caminho.");
    }
}
