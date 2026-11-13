// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Bluetooth e dispositivos": liga e desliga o Bluetooth, procura
//! dispositivos, pareia, conecta e remove (fones, caixas de som, mouse,
//! teclado, controles). Fala com o bluetoothd pelo `bluetoothctl`, sem
//! precisar de administrador.

use std::{
    process::Command,
    sync::mpsc::{channel, Receiver},
    time::{Duration, Instant},
};

use penas::{componentes as c, desenho, tema, Ui};

#[derive(Clone, Debug, Default)]
struct Disp {
    mac: String,
    nome: String,
    icone: String,
    conectado: bool,
    pareado: bool,
}

#[derive(Clone, Debug, Default)]
struct Estado {
    /// Há um adaptador Bluetooth (o bluetoothd respondeu e achou um).
    adaptador: bool,
    ligado: bool,
    pareados: Vec<Disp>,
}

enum Resposta {
    Estado(Estado),
    /// Fim da procura: os dispositivos achados (ainda não pareados).
    Achados(Vec<Disp>),
    Feito(Result<String, String>),
}

#[derive(Default)]
pub struct PaginaBluetooth {
    estado: Option<Estado>,
    achados: Vec<Disp>,
    procurando: bool,
    ocupado: Option<String>,
    rx: Option<Receiver<Resposta>>,
    lido_em: Option<Instant>,
    pub aviso: Option<String>,
}

fn bt(args: &[&str]) -> Result<String, String> {
    let o = Command::new("bluetoothctl").args(args).output().map_err(|e| format!("bluetoothctl: {e}"))?;
    let saida = String::from_utf8_lossy(&o.stdout).to_string();
    let erro = String::from_utf8_lossy(&o.stderr).to_string();
    // O bluetoothctl nem sempre devolve código de erro: olha o texto.
    let falhou = !o.status.success()
        || saida.contains("Failed to")
        || saida.contains("org.bluez.Error")
        || saida.contains("not available");
    if falhou {
        let linha = saida
            .lines()
            .chain(erro.lines())
            .find(|l| l.contains("Failed") || l.contains("Error") || l.contains("not available"))
            .unwrap_or("o dispositivo não respondeu")
            .trim()
            .to_string();
        Err(traduzir(&linha))
    } else {
        Ok(saida)
    }
}

fn traduzir(e: &str) -> String {
    if e.contains("Blocked") || e.contains("rfkill") {
        "o Bluetooth está bloqueado (tecla de modo avião do notebook?)".into()
    } else if e.contains("AuthenticationFailed") || e.contains("AuthenticationRejected") {
        "o pareamento foi recusado (o dispositivo está em modo de pareamento?)".into()
    } else if e.contains("AuthenticationTimeout") || e.contains("ConnectionAttemptFailed") || e.contains("page-timeout") {
        "o dispositivo não respondeu (está ligado e perto?)".into()
    } else if e.contains("AlreadyExists") {
        "já está pareado".into()
    } else if e.contains("No default controller") || e.contains("not available") {
        "nenhum adaptador Bluetooth encontrado".into()
    } else if e.contains("br-connection-profile-unavailable") {
        "o som do sistema ainda não está pronto para este fone; tente de novo em alguns segundos".into()
    } else {
        e.to_string()
    }
}

/// "Device AA:BB:CC:DD:EE:FF Nome" → (mac, nome).
fn ler_lista(t: &str) -> Vec<(String, String)> {
    t.lines()
        .filter_map(|l| {
            let l = l.trim();
            let resto = l.strip_prefix("Device ")?;
            let (mac, nome) = resto.split_once(' ').unwrap_or((resto, resto));
            (mac.len() == 17).then(|| (mac.to_string(), nome.trim().to_string()))
        })
        .collect()
}

fn info(mac: &str, nome: &str) -> Disp {
    let t = bt(&["info", mac]).unwrap_or_default();
    let campo = |k: &str| t.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()));
    Disp {
        mac: mac.to_string(),
        nome: campo("Alias:").or_else(|| campo("Name:")).unwrap_or_else(|| nome.to_string()),
        icone: campo("Icon:").unwrap_or_default(),
        conectado: campo("Connected:").as_deref() == Some("yes"),
        pareado: campo("Paired:").as_deref() == Some("yes"),
    }
}

fn ler_estado() -> Estado {
    let Ok(show) = bt(&["show"]) else { return Estado::default() };
    let ligado = show.lines().any(|l| l.trim() == "Powered: yes");
    let lista = bt(&["devices", "Paired"]).map(|t| ler_lista(&t)).unwrap_or_default();
    let pareados = lista.iter().map(|(m, n)| info(m, n)).collect();
    Estado { adaptador: true, ligado, pareados }
}

/// Nome amigável do tipo, pelo ícone que o BlueZ informa.
fn tipo(icone: &str) -> &'static str {
    match icone {
        i if i.starts_with("audio-headset") || i.starts_with("audio-headphones") => "Fone de ouvido",
        i if i.starts_with("audio") => "Áudio",
        "input-mouse" => "Mouse",
        "input-keyboard" => "Teclado",
        "input-gaming" => "Controle",
        "input-tablet" => "Mesa digitalizadora",
        "phone" => "Celular",
        "computer" => "Computador",
        _ => "Dispositivo",
    }
}

/// Ícone simples desenhado: fone, mouse, teclado, controle ou genérico.
fn desenhar_icone(ui: &mut Ui, icone: &str, x: f32, y: f32) {
    let c = tema::DESTAQUE();
    ui.tela.retangulo(x, y, 36.0, 36.0, 18.0, tema::DESTAQUE_SUAVE());
    let (cx, cy) = (x + 18.0, y + 18.0);
    match tipo(icone) {
        "Fone de ouvido" | "Áudio" => {
            ui.tela.contorno(cx - 9.0, cy - 10.0, 18.0, 18.0, 2.0, c);
            ui.tela.retangulo(cx - 11.0, cy, 6.0, 9.0, 2.0, c);
            ui.tela.retangulo(cx + 5.0, cy, 6.0, 9.0, 2.0, c);
            ui.tela.retangulo(cx - 8.0, cy - 1.0, 16.0, 10.0, 0.0, tema::DESTAQUE_SUAVE());
            ui.tela.retangulo(cx - 11.0, cy, 6.0, 9.0, 2.0, c);
            ui.tela.retangulo(cx + 5.0, cy, 6.0, 9.0, 2.0, c);
        }
        "Mouse" => {
            ui.tela.retangulo(cx - 6.0, cy - 9.0, 12.0, 18.0, 6.0, c);
            ui.tela.retangulo(cx - 0.6, cy - 9.0, 1.2, 7.0, 0.0, tema::DESTAQUE_SUAVE());
        }
        "Teclado" => {
            ui.tela.retangulo(cx - 11.0, cy - 6.0, 22.0, 12.0, 2.0, c);
            for k in 0..4 {
                ui.tela.retangulo(cx - 8.0 + k as f32 * 4.5, cy - 3.0, 2.5, 2.5, 0.5, tema::DESTAQUE_SUAVE());
            }
            ui.tela.retangulo(cx - 6.0, cy + 2.0, 12.0, 2.0, 0.5, tema::DESTAQUE_SUAVE());
        }
        "Controle" => {
            ui.tela.retangulo(cx - 11.0, cy - 5.0, 22.0, 11.0, 5.5, c);
            ui.tela.retangulo(cx - 7.0, cy - 1.0, 5.0, 1.5, 0.0, tema::DESTAQUE_SUAVE());
            ui.tela.retangulo(cx - 5.2, cy - 2.8, 1.5, 5.0, 0.0, tema::DESTAQUE_SUAVE());
            ui.tela.retangulo(cx + 4.0, cy - 2.0, 2.5, 2.5, 1.2, tema::DESTAQUE_SUAVE());
        }
        _ => {
            // Símbolo do Bluetooth.
            ui.tela.linha(cx - 1.0, cy - 10.0, cx - 1.0, cy + 10.0, 1.8, c);
            ui.tela.linha(cx - 1.0, cy - 10.0, cx + 6.0, cy - 4.0, 1.8, c);
            ui.tela.linha(cx + 6.0, cy - 4.0, cx - 7.0, cy + 5.0, 1.8, c);
            ui.tela.linha(cx - 1.0, cy + 10.0, cx + 6.0, cy + 4.0, 1.8, c);
            ui.tela.linha(cx + 6.0, cy + 4.0, cx - 7.0, cy - 5.0, 1.8, c);
        }
    }
}

impl PaginaBluetooth {
    fn rodar(&mut self, rotulo: &str, f: impl FnOnce() -> Resposta + Send + 'static) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
            let _ = tx.send(Resposta::Estado(ler_estado()));
        });
        self.ocupado = Some(rotulo.to_string());
        self.rx = Some(rx);
    }

    fn reler(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Resposta::Estado(ler_estado()));
        });
        self.rx = Some(rx);
        self.lido_em = Some(Instant::now());
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Respostas das tarefas em segundo plano.
        let mut fim = false;
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(Resposta::Estado(e)) => {
                        self.estado = Some(e);
                        if self.ocupado.is_none() {
                            fim = true;
                        }
                    }
                    Ok(Resposta::Achados(v)) => {
                        self.procurando = false;
                        self.achados = v;
                        self.ocupado = None;
                    }
                    Ok(Resposta::Feito(r)) => {
                        self.ocupado = None;
                        match r {
                            Ok(m) => {
                                if !m.is_empty() {
                                    self.aviso = Some(m);
                                }
                            }
                            Err(e) => self.aviso = Some(format!("Não deu: {e}.")),
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ui.acordar_em = Some(Duration::from_millis(150));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        fim = true;
                        break;
                    }
                }
            }
        }
        if fim {
            self.rx = None;
            self.ocupado = None;
            self.procurando = false;
        }
        // Relê a cada 5 s (um fone que liga e conecta sozinho, por exemplo).
        if self.rx.is_none() && self.lido_em.map(|t| t.elapsed() >= Duration::from_secs(5)).unwrap_or(true) {
            self.reler();
        }
        ui.acordar_em = Some(ui.acordar_em.unwrap_or(Duration::from_secs(5)).min(Duration::from_secs(5)));

        let x = x0 + 32.0;
        let w = (l - 64.0).min(640.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Bluetooth e dispositivos");
        let Some(estado) = self.estado.clone() else {
            ui.tela.texto(x, 100.0, 13.5, tema::TEXTO_SECUNDARIO(), "Procurando o adaptador Bluetooth…");
            return;
        };
        if !estado.adaptador {
            let y = c::paragrafo(
                ui,
                x,
                96.0,
                w,
                "Este computador não tem um adaptador Bluetooth (ou ele está desligado no BIOS). Um adaptador USB de Bluetooth funciona: é só conectar.",
            );
            let _ = y;
            return;
        }
        let livre = self.ocupado.is_none();

        // Liga/desliga.
        let mut y = 92.0;
        ui.tela.retangulo(x, y, w, 64.0, 8.0, tema::SUPERFICIE());
        desenhar_icone(ui, "", x + 14.0, y + 14.0);
        ui.tela.texto(x + 64.0, y + 28.0, 14.5, tema::TEXTO(), "Bluetooth");
        let sub = if estado.ligado { "Ligado" } else { "Desligado" };
        ui.tela.texto(x + 64.0, y + 48.0, 12.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.0, sub, w - 150.0));
        let mut ligado = estado.ligado;
        if c::chave(ui, x + w - 66.0, y + 20.0, &mut ligado) && livre {
            let liga = ligado;
            self.rodar(if liga { "Ligando…" } else { "Desligando…" }, move || {
                Resposta::Feito(bt(&["power", if liga { "on" } else { "off" }]).map(|_| String::new()))
            });
        }
        y += 84.0;
        if !estado.ligado {
            return;
        }

        // Adicionar dispositivo: procura por 12 s.
        let rot = if self.procurando { "Procurando…" } else { "Adicionar dispositivo" };
        if c::botao(ui, x, y, rot, c::Estilo::Principal, livre) {
            self.procurando = true;
            self.achados.clear();
            self.rodar("Procurando…", || {
                let _ = bt(&["--timeout", "12", "scan", "on"]);
                let lista = bt(&["devices"]).map(|t| ler_lista(&t)).unwrap_or_default();
                let v: Vec<Disp> = lista
                    .iter()
                    .map(|(m, n)| info(m, n))
                    .filter(|d| !d.pareado && !d.nome.replace('-', ":").eq_ignore_ascii_case(&d.mac))
                    .collect();
                Resposta::Achados(v)
            });
        }
        if let Some(o) = &self.ocupado {
            ui.tela.texto(x + c::largura_botao(rot) + 16.0, y + 23.0, 12.5, tema::TEXTO_SECUNDARIO(), o);
        }
        y += 56.0;

        if self.procurando || !self.achados.is_empty() {
            ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Encontrados");
            ui.tela.texto(x, y + 20.0, 12.5, tema::TEXTO_SECUNDARIO(), "Ponha o dispositivo em modo de pareamento (o botão do Bluetooth apertado até piscar).");
            y += 34.0;
            if self.achados.is_empty() {
                ui.tela.texto(x + 4.0, y + 18.0, 13.0, tema::TEXTO_SECUNDARIO(), "Procurando dispositivos por perto…");
                y += 40.0;
            }
            for d in self.achados.clone() {
                ui.tela.retangulo(x, y, w, 56.0, 8.0, tema::SUPERFICIE());
                desenhar_icone(ui, &d.icone, x + 12.0, y + 10.0);
                ui.tela.texto(x + 60.0, y + 25.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &d.nome, w - 200.0));
                ui.tela.texto(x + 60.0, y + 44.0, 12.0, tema::TEXTO_SECUNDARIO(), tipo(&d.icone));
                if c::botao_direita(ui, x + w - 12.0, y + 10.0, "Parear", c::Estilo::Principal, livre) {
                    let (mac, nome) = (d.mac.clone(), d.nome.clone());
                    self.achados.retain(|a| a.mac != mac);
                    self.rodar(&format!("Pareando com {nome}…"), move || {
                        let r = bt(&["--agent", "NoInputNoOutput", "--timeout", "25", "pair", &mac])
                            .and_then(|_| bt(&["trust", &mac]))
                            .and_then(|_| bt(&["--timeout", "15", "connect", &mac]))
                            .map(|_| format!("{nome} pareado e conectado."));
                        Resposta::Feito(r)
                    });
                }
                y += 64.0;
            }
            y += 10.0;
        }

        ui.tela.texto(x, y, 15.0, tema::TEXTO(), "Seus dispositivos");
        y += 14.0;
        if estado.pareados.is_empty() {
            ui.tela.texto(x, y + 22.0, 13.0, tema::TEXTO_SECUNDARIO(), "Nenhum dispositivo pareado ainda.");
        }
        for d in &estado.pareados {
            ui.tela.retangulo(x, y, w, 56.0, 8.0, tema::SUPERFICIE());
            desenhar_icone(ui, &d.icone, x + 12.0, y + 10.0);
            ui.tela.texto(x + 60.0, y + 25.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, &d.nome, w - 300.0));
            let st = format!("{} · {}", tipo(&d.icone), if d.conectado { "Conectado" } else { "Não conectado" });
            ui.tela.texto(x + 60.0, y + 44.0, 12.0, if d.conectado { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() }, &st);
            let mac = d.mac.clone();
            let nome = d.nome.clone();
            let remover_x = x + w - 12.0;
            if c::botao_direita(ui, remover_x, y + 10.0, "Remover", c::Estilo::Comum, livre) {
                self.rodar(&format!("Removendo {nome}…"), move || {
                    Resposta::Feito(bt(&["remove", &mac]).map(|_| format!("{nome} removido.")))
                });
            } else {
                let rot = if d.conectado { "Desconectar" } else { "Conectar" };
                let bx = remover_x - c::largura_botao("Remover") - 8.0;
                if c::botao_direita(ui, bx, y + 10.0, rot, if d.conectado { c::Estilo::Comum } else { c::Estilo::Principal }, livre) {
                    let conectar = !d.conectado;
                    self.rodar(if conectar { "Conectando…" } else { "Desconectando…" }, move || {
                        let r = if conectar {
                            bt(&["--timeout", "15", "connect", &mac]).map(|_| format!("{nome} conectado."))
                        } else {
                            bt(&["disconnect", &mac]).map(|_| String::new())
                        };
                        Resposta::Feito(r)
                    });
                }
            }
            y += 64.0;
        }
    }
}
