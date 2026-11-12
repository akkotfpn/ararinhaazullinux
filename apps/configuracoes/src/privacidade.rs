// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Privacidade": o que o sistema guarda (histórico do Win+V,
//! arquivos na busca do Iniciar) e os relatórios de falhas — tudo local,
//! nada sai do computador.

use std::{path::PathBuf, process::Command, time::Duration};

use penas::{componentes as c, desenho, tema, Ui};

use crate::{gravar_chave, ler_chave};

struct Relatorio {
    titulo: String,
    quando: String,
    /// Arquivo do relatório (os das Penas/Plumagem) ou o PID (coredump).
    arquivo: Option<PathBuf>,
    pid: Option<String>,
}

fn quando(seg: u64) -> String {
    Command::new("date")
        .args(["-d", &format!("@{seg}"), "+%d/%m/%Y %H:%M"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Relatórios dos apps do Ararinha e falhas de outros programas
/// (systemd-coredump, quando o journal deixa ler).
fn ler() -> Vec<Relatorio> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(penas::pasta_falhas()) {
        let mut arqs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
        arqs.sort();
        arqs.reverse();
        for a in arqs.into_iter().take(30) {
            let nome = a.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let (app, seg) = nome.rsplit_once('-').unwrap_or((nome.as_str(), "0"));
            let titulo = std::fs::read_to_string(&a)
                .ok()
                .and_then(|t| t.lines().next().map(|l| l.trim_start_matches("App: ").to_string()))
                .unwrap_or_else(|| app.to_string());
            v.push(Relatorio { titulo, quando: quando(seg.parse().unwrap_or(0)), arquivo: Some(a.clone()), pid: None });
        }
    }
    // coredumpctl: "TIME PID UID GID SIG COREFILE EXE SIZE"
    if let Ok(o) = Command::new("coredumpctl").args(["list", "--no-pager", "--no-legend", "-r", "-n", "20"]).output() {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() < 9 {
                continue;
            }
            let exe = c[c.len() - 2];
            let prog = exe.rsplit('/').next().unwrap_or(exe);
            v.push(Relatorio { titulo: format!("{prog} (sinal {})", c[c.len() - 5]), quando: format!("{} {}", c[1], c[2]), arquivo: None, pid: Some(c[4].to_string()) });
        }
    }
    v
}

#[derive(Default)]
pub struct PaginaPrivacidade {
    relatorios: Option<Vec<Relatorio>>,
    lido: Option<std::time::Instant>,
    rolagem: f32,
}

impl PaginaPrivacidade {
    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if self.lido.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true) {
            self.relatorios = Some(ler());
            self.lido = Some(std::time::Instant::now());
        }
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        let n = self.relatorios.as_ref().map(|r| r.len()).unwrap_or(0);
        let total = 420.0 + n as f32 * 52.0;
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (total - ui.altura()).max(0.0));
        let dy = -self.rolagem;
        ui.tela.texto(x, 52.0 + dy, 22.0, tema::TEXTO(), "Privacidade");
        ui.tela.texto(x, 76.0 + dy, 12.5, tema::TEXTO_SECUNDARIO(), "Tudo aqui fica só neste computador.");

        let opcoes = [
            ("historico_area", "Histórico da área de transferência (Win+V)", "Guarda os últimos textos copiados enquanto a sessão está aberta."),
            ("busca_arquivos", "Arquivos na busca do menu Iniciar", "Procura nomes de arquivos da sua pasta pessoal ao digitar."),
            ("relatorios", "Guardar relatórios de falhas", "Quando um app fecha por erro, guarda o motivo para ajudar a corrigir."),
        ];
        let mut y = 112.0 + dy;
        for (chave, titulo, sub) in opcoes {
            ui.tela.texto(x, y, 14.5, tema::TEXTO(), titulo);
            ui.tela.texto(x, y + 19.0, 12.0, tema::TEXTO_SECUNDARIO(), sub);
            let mut v = ler_chave(chave).as_deref() != Some("nao");
            if c::chave(ui, x + w - 44.0, y - 10.0, &mut v) {
                let _ = gravar_chave(chave, if v { "sim" } else { "nao" });
            }
            y += 54.0;
        }

        y += 20.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Relatórios de falhas");
        y += 12.0;
        let relatorios = self.relatorios.as_ref().map(|r| r.iter().map(|r| (r.titulo.clone(), r.quando.clone(), r.arquivo.clone(), r.pid.clone())).collect::<Vec<_>>()).unwrap_or_default();
        if relatorios.is_empty() {
            ui.tela.texto(x, y + 22.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhuma falha registrada. Ótimo!");
            return;
        }
        for (titulo, quando, arquivo, pid) in relatorios {
            ui.tela.retangulo(x, y + 6.0, w, 46.0, 8.0, tema::SUPERFICIE_ALTA());
            ui.tela.texto(x + 14.0, y + 26.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &titulo, w - 140.0));
            ui.tela.texto(x + 14.0, y + 44.0, 11.5, tema::TEXTO_SECUNDARIO(), &quando);
            if c::botao_direita(ui, x + w - 8.0, y + 11.0, "Ver", c::Estilo::Comum, true) {
                match (&arquivo, &pid) {
                    (Some(a), _) => {
                        let _ = Command::new("notas").arg(a).spawn();
                    }
                    (None, Some(p)) => {
                        // Salva o resumo do coredump num arquivo e abre.
                        let destino = penas::pasta_falhas().join(format!("sistema-{p}.log"));
                        let _ = std::fs::create_dir_all(penas::pasta_falhas());
                        if let Ok(o) = Command::new("coredumpctl").args(["info", "--no-pager", p]).output() {
                            let _ = std::fs::write(&destino, &o.stdout);
                            let _ = Command::new("notas").arg(&destino).spawn();
                        }
                    }
                    _ => {}
                }
            }
            y += 52.0;
        }
        if c::botao(ui, x, y + 16.0, "Apagar os relatórios", c::Estilo::Comum, true) {
            if let Ok(rd) = std::fs::read_dir(penas::pasta_falhas()) {
                for e in rd.flatten() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
            self.lido = None;
        }
    }
}
