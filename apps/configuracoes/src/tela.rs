// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Tela": resolução de cada monitor ligado e o tamanho de tudo
//! (escala). A escala vai na chave "escala" do plumagem.conf; a Plumagem
//! confere a cada 2 s e aplica sem reiniciar a sessão.

use penas::{tema, Ui};

use crate::{gravar_chave, ler_chave};

const ESCALAS: [(u32, &str); 5] = [(100, "100%"), (125, "125%"), (150, "150% (telas grandes ou de alta definição)"), (175, "175%"), (200, "200% (telas 4K)")];

struct Monitor {
    nome: String,
    modo: String,
}

/// Monitores ligados, pelo /sys/class/drm (o primeiro modo é o preferido,
/// que é o que a Plumagem usa).
fn monitores() -> Vec<Monitor> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return v };
    let mut nomes: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    nomes.sort();
    for p in nomes {
        let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let Some((_, conector)) = n.split_once('-') else { continue };
        if std::fs::read_to_string(p.join("status")).map(|s| s.trim() != "connected").unwrap_or(true) {
            continue;
        }
        let modo = std::fs::read_to_string(p.join("modes")).ok().and_then(|m| m.lines().next().map(str::to_string)).unwrap_or_else(|| "?".into());
        let tipo = if conector.starts_with("eDP") || conector.starts_with("LVDS") {
            "Tela do notebook"
        } else if conector.starts_with("Virtual") {
            "Tela virtual"
        } else {
            "Monitor"
        };
        v.push(Monitor { nome: format!("{tipo} ({conector})"), modo });
    }
    v
}

#[derive(Default)]
pub struct PaginaTela {
    lista: Option<Vec<Monitor>>,
}

impl PaginaTela {
    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Tela");
        let lista = self.lista.get_or_insert_with(monitores);
        let mut y = 76.0;
        if lista.is_empty() {
            ui.tela.texto(x, y + 24.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhum monitor encontrado.");
            y += 40.0;
        }
        for m in lista.iter() {
            ui.tela.retangulo(x, y, w, 56.0, 8.0, tema::SUPERFICIE_ALTA());
            ui.tela.texto(x + 18.0, y + 24.0, 14.5, tema::TEXTO(), &m.nome);
            ui.tela.texto(x + 18.0, y + 44.0, 12.0, tema::TEXTO_SECUNDARIO(), &format!("Resolução: {} (a melhor que o monitor informa)", m.modo.replace('x', " × ")));
            y += 64.0;
        }

        y += 24.0;
        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Tamanho do texto, apps e itens");
        ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Muda na hora. Apps abertos podem ficar desfocados até serem reabertos.");
        y += 36.0;
        let atual: u32 = ler_chave("escala").and_then(|v| v.trim_end_matches('%').parse().ok()).unwrap_or(100);
        for (pct, nome) in ESCALAS {
            let sel = atual == pct;
            if sel {
                ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::DESTAQUE_SUAVE());
            } else if ui.sobre(x, y, w, 30.0) {
                ui.tela.retangulo(x, y, w, 30.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            ui.tela.retangulo(x + 10.0, y + 8.0, 14.0, 14.0, 7.0, if sel { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(x + 12.0, y + 10.0, 10.0, 10.0, 5.0, tema::JANELA());
            if sel {
                ui.tela.retangulo(x + 14.0, y + 12.0, 6.0, 6.0, 3.0, tema::DESTAQUE());
            }
            ui.tela.texto(x + 34.0, y + 20.0, 13.5, tema::TEXTO(), nome);
            if ui.clicou(x, y, w, 30.0) && !sel {
                let _ = gravar_chave("escala", &pct.to_string());
            }
            y += 34.0;
        }
    }
}
