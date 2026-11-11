// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Som": saída e entrada (caixas, fone, microfone), volume de cada
//! uma e de cada app tocando agora. Lê e muda pelo `wpctl` (PipeWire).

use std::{process::Command, time::Duration};

use penas::{componentes as c, desenho, tema, Ui};

#[derive(Clone, Debug, Default)]
struct Dispositivo {
    id: u32,
    nome: String,
    volume: f32,
    mudo: bool,
    padrao: bool,
}

#[derive(Default)]
struct Estado {
    saidas: Vec<Dispositivo>,
    entradas: Vec<Dispositivo>,
    apps: Vec<Dispositivo>,
}

/// Lê o `wpctl status` (seção Audio).
fn ler() -> Estado {
    let t = Command::new("wpctl").arg("status").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let mut e = Estado::default();
    let mut secao = "";
    let mut no_audio = false;
    for bruta in t.lines() {
        let l: String = bruta.chars().map(|c| if "│├└─".contains(c) { ' ' } else { c }).collect();
        let s = l.trim();
        if s == "Audio" {
            no_audio = true;
            continue;
        }
        if s == "Video" || s == "Settings" {
            no_audio = false;
        }
        if !no_audio {
            continue;
        }
        if let Some(h) = s.strip_suffix(':') {
            secao = match h {
                "Sinks" => "saida",
                "Sources" => "entrada",
                "Streams" => "apps",
                _ => "",
            };
            continue;
        }
        if secao.is_empty() || s.contains('>') || s.contains('<') {
            continue;
        }
        let padrao = s.starts_with('*');
        let s = s.trim_start_matches('*').trim();
        let Some((id, resto)) = s.split_once(". ") else { continue };
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        let (nome, extra) = match resto.find('[') {
            Some(i) => (resto[..i].trim(), &resto[i..]),
            None => (resto.trim(), ""),
        };
        let volume = extra
            .split("vol:")
            .nth(1)
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.trim_end_matches(']').parse::<f32>().ok())
            .unwrap_or(1.0);
        let d = Dispositivo { id, nome: nome.to_string(), volume, mudo: extra.contains("MUTED"), padrao };
        match secao {
            "saida" => e.saidas.push(d),
            "entrada" => e.entradas.push(d),
            _ => e.apps.push(d),
        }
    }
    // Volume de cada app (o status não mostra para streams).
    for a in &mut e.apps {
        if let Ok(o) = Command::new("wpctl").args(["get-volume", &a.id.to_string()]).output() {
            let t = String::from_utf8_lossy(&o.stdout);
            if let Some(v) = t.split_whitespace().nth(1).and_then(|v| v.parse::<f32>().ok()) {
                a.volume = v;
            }
            a.mudo = t.contains("MUTED");
        }
    }
    e
}

#[derive(Default)]
pub struct PaginaSom {
    estado: Estado,
    lido: Option<std::time::Instant>,
    /// Barra sendo arrastada: (id do nó).
    arrastando: Option<u32>,
}

impl PaginaSom {
    /// Uma linha com nome, barra de volume e mudo. Devolve o volume novo.
    fn barra(ui: &mut Ui, x: f32, y: f32, w: f32, d: &Dispositivo, arrastando: &mut Option<u32>) -> Option<f32> {
        let bx = x + w * 0.45;
        let bw = w * 0.55 - 70.0;
        ui.tela.texto(x, y + 18.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &d.nome, w * 0.45 - 12.0));
        ui.tela.retangulo(bx, y + 11.0, bw, 6.0, 3.0, tema::BORDA());
        let v = d.volume.clamp(0.0, 1.0);
        ui.tela.retangulo(bx, y + 11.0, bw * v, 6.0, 3.0, if d.mudo { tema::TEXTO_SECUNDARIO() } else { tema::DESTAQUE() });
        ui.tela.retangulo(bx + bw * v - 8.0, y + 6.0, 16.0, 16.0, 8.0, tema::JANELA());
        ui.tela.retangulo(bx + bw * v - 6.0, y + 8.0, 12.0, 12.0, 6.0, if d.mudo { tema::TEXTO_SECUNDARIO() } else { tema::DESTAQUE() });
        ui.tela.texto(bx + bw + 14.0, y + 18.0, 12.5, tema::TEXTO_SECUNDARIO(), &if d.mudo { "mudo".to_string() } else { format!("{:.0}%", v * 100.0) });
        let mut novo = None;
        if ui.pressionado && ui.sobre(bx - 8.0, y, bw + 16.0, 28.0) && arrastando.is_none() {
            *arrastando = Some(d.id);
        }
        if *arrastando == Some(d.id) {
            if let Some((mx, _)) = ui.mouse {
                novo = Some(((mx - bx) / bw).clamp(0.0, 1.0));
            }
            if !ui.pressionado {
                *arrastando = None;
            }
        }
        // Clique no "%"/"mudo": liga e desliga o mudo.
        if ui.clicou(bx + bw + 8.0, y, 62.0, 28.0) {
            let _ = Command::new("wpctl").args(["set-mute", &d.id.to_string(), "toggle"]).status();
        }
        novo
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Relê a cada 2 s (apps começam e param de tocar), menos arrastando.
        if self.arrastando.is_none() && self.lido.map(|t| t.elapsed() > Duration::from_secs(2)).unwrap_or(true) {
            self.estado = ler();
            self.lido = Some(std::time::Instant::now());
        }
        ui.acordar_em = Some(Duration::from_millis(if self.arrastando.is_some() { 30 } else { 2000 }));
        let x = x0 + 32.0;
        let w = (l - 64.0).min(640.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Som");
        let mut y = 76.0;
        let mut mudar: Vec<(u32, f32)> = Vec::new();
        let mut padrao: Option<u32> = None;
        for (titulo, lista, vazio) in [
            ("Saída (onde o som toca)", &self.estado.saidas, "Nenhuma saída de som encontrada."),
            ("Entrada (microfone)", &self.estado.entradas, "Nenhum microfone encontrado."),
            ("Apps tocando agora", &self.estado.apps, "Nenhum app tocando som agora."),
        ] {
            y += 26.0;
            ui.tela.texto(x, y, 15.0, tema::TEXTO(), titulo);
            y += 12.0;
            if lista.is_empty() {
                ui.tela.texto(x, y + 18.0, 12.5, tema::TEXTO_SECUNDARIO(), vazio);
                y += 32.0;
                continue;
            }
            for d in lista.clone() {
                let ehapp = titulo.starts_with("Apps");
                if !ehapp {
                    // Bolinha para escolher a saída/entrada padrão.
                    let sel = d.padrao;
                    ui.tela.retangulo(x, y + 7.0, 14.0, 14.0, 7.0, if sel { tema::DESTAQUE() } else { tema::BORDA() });
                    ui.tela.retangulo(x + 2.0, y + 9.0, 10.0, 10.0, 5.0, tema::JANELA());
                    if sel {
                        ui.tela.retangulo(x + 4.0, y + 11.0, 6.0, 6.0, 3.0, tema::DESTAQUE());
                    }
                    if ui.clicou(x - 4.0, y, 22.0, 28.0) && !sel {
                        padrao = Some(d.id);
                    }
                }
                let dx = if ehapp { 0.0 } else { 24.0 };
                if let Some(v) = Self::barra(ui, x + dx, y, w - dx, &d, &mut self.arrastando) {
                    mudar.push((d.id, v));
                }
                y += 34.0;
            }
        }
        for (id, v) in mudar {
            let _ = Command::new("wpctl").args(["set-volume", &id.to_string(), &format!("{v:.2}")]).status();
            for d in self.estado.saidas.iter_mut().chain(self.estado.entradas.iter_mut()).chain(self.estado.apps.iter_mut()) {
                if d.id == id {
                    d.volume = v;
                }
            }
        }
        if let Some(id) = padrao {
            let _ = Command::new("wpctl").args(["set-default", &id.to_string()]).status();
            self.lido = None;
        }
        y += 16.0;
        if c::botao(ui, x, y, "Testar o som", c::Estilo::Comum, true) {
            let _ = Command::new("ararinha-som").arg("/usr/share/sounds/ararinha/sessao-iniciada.wav").spawn();
        }
    }
}
