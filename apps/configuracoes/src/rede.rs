// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Rede e Internet": o cabo (systemd-networkd) e o Wi-Fi (iwd, pelo
//! iwctl). Os comandos do iwctl demoram (procurar redes, conectar), então
//! rodam numa linha separada e a página só mostra o resultado.

use std::{
    fs,
    process::Command,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use penas::{desenho, tema, Cor, Tecla, Ui};

#[derive(Debug, Clone)]
pub struct RedeWifi {
    pub nome: String,
    /// "open", "psk", "8021x"...
    pub seguranca: String,
    /// Intensidade em dBm (de -100 a -30, mais perto de zero é melhor).
    pub sinal: i32,
    pub conectada: bool,
}

enum Pedido {
    Procurar,
    Conectar(String, Option<String>),
    Desconectar,
    Esquecer(String),
}

enum Resposta {
    Redes(Vec<RedeWifi>),
    Feito(String),
    Erro(String),
}

pub struct PaginaRede {
    placa: Option<String>,
    redes: Vec<RedeWifi>,
    tx: Option<Sender<Pedido>>,
    rx: Option<Receiver<Resposta>>,
    ocupado: bool,
    proxima_busca: Instant,
    /// Rede protegida esperando a senha.
    senha_para: Option<String>,
    senha: String,
    mostrar_senha: bool,
    pub aviso: Option<String>,
    /// Rede pedida pela linha de comando (bandeja): já abre a senha dela.
    pub pedida: Option<String>,
    rolagem: f32,
}

/// Placa Wi-Fi (a primeira com /sys/class/net/*/wireless).
fn placa_wifi() -> Option<String> {
    let mut v: Vec<String> = fs::read_dir("/sys/class/net")
        .ok()?
        .flatten()
        .filter(|e| e.path().join("wireless").exists())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v.into_iter().next()
}

/// Tira as cores (sequências ESC [ ... m) da saída do iwctl.
fn sem_cores(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut c = s.chars().peekable();
    while let Some(ch) = c.next() {
        if ch == '\u{1b}' {
            if c.peek() == Some(&'[') {
                c.next();
                for x in c.by_ref() {
                    if x.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn iwctl(args: &[&str]) -> Result<String, String> {
    let saida = Command::new("iwctl")
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|e| format!("iwctl: {e}"))?;
    let texto = sem_cores(&String::from_utf8_lossy(&saida.stdout));
    if saida.status.success() {
        Ok(texto)
    } else {
        let erro = sem_cores(&String::from_utf8_lossy(&saida.stderr));
        let msg = if erro.trim().is_empty() { texto } else { erro };
        Err(msg.trim().to_string())
    }
}

/// Lê a tabela do `iwctl station X get-networks rssi-dbms`.
fn ler_redes(placa: &str) -> Result<Vec<RedeWifi>, String> {
    let texto = iwctl(&["station", placa, "get-networks", "rssi-dbms"])?;
    let linhas: Vec<&str> = texto.lines().collect();
    let Some(cab) = linhas.iter().position(|l| l.contains("Network name") && l.contains("Security")) else {
        return Ok(Vec::new());
    };
    let cabecalho = linhas[cab];
    let col_seg = cabecalho.find("Security").unwrap_or(40);
    let col_sinal = cabecalho.find("Signal").unwrap_or(col_seg + 20);
    let mut redes = Vec::new();
    for l in &linhas[cab + 1..] {
        if l.trim().is_empty() || l.trim_start().starts_with('-') {
            continue;
        }
        let chars: Vec<char> = l.chars().collect();
        let pedaco = |a: usize, b: usize| -> String {
            chars.get(a.min(chars.len())..b.min(chars.len())).map(|s| s.iter().collect::<String>()).unwrap_or_default()
        };
        let inicio = pedaco(0, 6);
        let conectada = inicio.contains('>');
        let nome = pedaco(6.min(col_seg), col_seg).trim().to_string();
        let seguranca = pedaco(col_seg, col_sinal).trim().to_string();
        let sinal_txt = pedaco(col_sinal, chars.len()).trim().to_string();
        let mut sinal: i32 = sinal_txt.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(-90);
        if sinal.abs() > 200 {
            sinal /= 100;
        }
        if !nome.is_empty() {
            redes.push(RedeWifi { nome, seguranca, sinal, conectada });
        }
    }
    redes.sort_by(|a, b| b.conectada.cmp(&a.conectada).then(b.sinal.cmp(&a.sinal)));
    Ok(redes)
}

fn trabalhador(placa: String, rx: Receiver<Pedido>, tx: Sender<Resposta>) {
    for pedido in rx {
        let r = match pedido {
            Pedido::Procurar => {
                let _ = iwctl(&["station", &placa, "scan"]);
                thread::sleep(Duration::from_millis(1800));
                match ler_redes(&placa) {
                    Ok(v) => Resposta::Redes(v),
                    Err(e) => Resposta::Erro(format!("Não deu para procurar redes: {e}")),
                }
            }
            Pedido::Conectar(nome, senha) => {
                let mut args: Vec<String> = Vec::new();
                if let Some(s) = &senha {
                    args.push("--passphrase".into());
                    args.push(s.clone());
                } else {
                    args.push("--dont-ask".into());
                }
                args.extend(["station".into(), placa.clone(), "connect".into(), nome.clone()]);
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                match iwctl(&refs) {
                    Ok(_) => Resposta::Feito(format!("Conectado a \"{nome}\".")),
                    Err(e) if e.contains("Operation failed") || e.contains("Invalid") => {
                        Resposta::Erro(format!("Não conectou a \"{nome}\": confira a senha."))
                    }
                    Err(e) => Resposta::Erro(format!("Não conectou a \"{nome}\": {e}")),
                }
            }
            Pedido::Desconectar => match iwctl(&["station", &placa, "disconnect"]) {
                Ok(_) => Resposta::Feito("Wi-Fi desconectado.".into()),
                Err(e) => Resposta::Erro(format!("Não deu para desconectar: {e}")),
            },
            Pedido::Esquecer(nome) => match iwctl(&["known-networks", &nome, "forget"]) {
                Ok(_) => Resposta::Feito(format!("\"{nome}\" esquecida: a senha foi apagada.")),
                Err(e) => Resposta::Erro(format!("Não deu para esquecer: {e}")),
            },
        };
        if tx.send(r).is_err() {
            return;
        }
        // Depois de mudar algo, a lista é lida de novo.
        if let Ok(v) = ler_redes(&placa) {
            if tx.send(Resposta::Redes(v)).is_err() {
                return;
            }
        }
    }
}

/// IP e nome da interface com a rota padrão (cabo ou Wi-Fi).
fn conexao_atual() -> Option<(String, String)> {
    let rotas = fs::read_to_string("/proc/net/route").ok()?;
    let iface = rotas.lines().skip(1).find_map(|l| {
        let c: Vec<&str> = l.split_whitespace().collect();
        (c.len() > 2 && c[1] == "00000000").then(|| c[0].to_string())
    })?;
    let ip = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", &iface])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().skip_while(|w| *w != "inet").nth(1).map(|v| v.split('/').next().unwrap_or("").to_string()))
        .unwrap_or_default();
    Some((iface, ip))
}

fn barras_sinal(t: &mut penas::Tela, x: f32, y: f32, dbm: i32, cor: Cor) {
    let n = match dbm {
        d if d >= -55 => 4,
        d if d >= -67 => 3,
        d if d >= -78 => 2,
        _ => 1,
    };
    for i in 0..4 {
        let h = 5.0 + i as f32 * 4.0;
        let c = if i < n { cor } else { tema::BORDA() };
        t.retangulo(x + i as f32 * 6.0, y + 17.0 - h, 4.0, h, 1.0, c);
    }
}

impl PaginaRede {
    pub fn nova() -> PaginaRede {
        let placa = placa_wifi();
        let (tx, rx) = match &placa {
            Some(p) => {
                let (tx_p, rx_p) = channel();
                let (tx_r, rx_r) = channel();
                let p = p.clone();
                thread::spawn(move || trabalhador(p, rx_p, tx_r));
                (Some(tx_p), Some(rx_r))
            }
            None => (None, None),
        };
        PaginaRede {
            placa,
            redes: Vec::new(),
            tx,
            rx,
            ocupado: false,
            proxima_busca: Instant::now(),
            senha_para: None,
            senha: String::new(),
            mostrar_senha: false,
            aviso: None,
            pedida: None,
            rolagem: 0.0,
        }
    }

    fn pedir(&mut self, p: Pedido) {
        if let Some(tx) = &self.tx {
            if tx.send(p).is_ok() {
                self.ocupado = true;
            }
        }
    }

    fn conectar(&mut self, rede: &RedeWifi) {
        if rede.seguranca == "open" {
            self.aviso = Some(format!("Conectando a \"{}\"...", rede.nome));
            self.pedir(Pedido::Conectar(rede.nome.clone(), None));
        } else {
            self.senha_para = Some(rede.nome.clone());
            self.senha.clear();
        }
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Respostas do iwctl.
        if let Some(rx) = &self.rx {
            while let Ok(r) = rx.try_recv() {
                match r {
                    Resposta::Redes(v) => {
                        self.redes = v;
                        self.ocupado = false;
                        if let Some(nome) = self.pedida.take() {
                            if let Some(r) = self.redes.iter().find(|r| r.nome == nome).cloned() {
                                if !r.conectada {
                                    self.conectar(&r);
                                }
                            }
                        }
                    }
                    Resposta::Feito(m) => {
                        self.aviso = Some(m);
                        self.ocupado = false;
                    }
                    Resposta::Erro(m) => {
                        self.aviso = Some(m);
                        self.ocupado = false;
                    }
                }
            }
        }
        // Procura redes a cada 20 s enquanto a página está aberta.
        if self.placa.is_some() && Instant::now() >= self.proxima_busca && !self.ocupado {
            self.pedir(Pedido::Procurar);
            self.proxima_busca = Instant::now() + Duration::from_secs(20);
        }
        ui.acordar_em = Some(Duration::from_millis(if self.ocupado { 200 } else { 1000 }));

        // Teclado: a senha.
        if self.senha_para.is_some() {
            for t in std::mem::take(&mut ui.teclas) {
                match t {
                    Tecla::Texto(s) => self.senha.push_str(&s),
                    Tecla::Apagar => {
                        self.senha.pop();
                    }
                    Tecla::Esc => self.senha_para = None,
                    Tecla::Enter => self.enviar_senha(),
                    _ => {}
                }
            }
        }

        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO(), "Rede e Internet");
        let estado = match conexao_atual() {
            Some((iface, ip)) => {
                let tipo = if fs::metadata(format!("/sys/class/net/{iface}/wireless")).is_ok() { "Wi-Fi" } else { "Cabo" };
                format!("Conectado ({tipo}: {iface}, IP {ip})")
            }
            None => "Sem conexão com a internet".into(),
        };
        ui.tela.texto(x0 + 32.0, 82.0, 13.0, tema::TEXTO_SECUNDARIO(), &estado);

        let Some(placa) = self.placa.clone() else {
            ui.tela.texto(x0 + 32.0, 130.0, 15.0, tema::TEXTO(), "Wi-Fi");
            ui.tela.texto(
                x0 + 32.0,
                154.0,
                13.0,
                tema::TEXTO_SECUNDARIO(),
                "Nenhuma placa Wi-Fi encontrada. Se o computador tem uma, ela pode precisar de firmware.",
            );
            return;
        };

        // Cabeçalho do Wi-Fi com "Procurar".
        ui.tela.texto(x0 + 32.0, 130.0, 15.0, tema::TEXTO(), &format!("Redes Wi-Fi ({placa})"));
        let (bx, by, bl) = (x0 + l - 32.0 - 120.0, 110.0, 120.0);
        let sobre = ui.sobre(bx, by, bl, 30.0);
        ui.tela.retangulo(bx, by, bl, 30.0, 6.0, if sobre { tema::SUPERFICIE_ALTA() } else { tema::FUNDO() });
        let rot = if self.ocupado { "Procurando…" } else { "Procurar" };
        let lt = desenho::medir(13.0, rot);
        ui.tela.texto(bx + bl / 2.0 - lt / 2.0, by + 20.0, 13.0, tema::TEXTO(), rot);
        if ui.clicou(bx, by, bl, 30.0) && !self.ocupado {
            self.pedir(Pedido::Procurar);
            self.proxima_busca = Instant::now() + Duration::from_secs(20);
        }

        // Lista
        let (lx, ly0, ll) = (x0 + 32.0, 150.0, (l - 64.0).min(640.0));
        let item_a = 56.0;
        let fim = ui.altura() - 50.0;
        let conteudo = self.redes.len() as f32 * (item_a + 6.0);
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (conteudo - (fim - ly0)).max(0.0));
        if self.redes.is_empty() {
            let msg = if self.ocupado { "Procurando redes…" } else { "Nenhuma rede encontrada." };
            ui.tela.texto(lx, ly0 + 30.0, 13.0, tema::TEXTO_SECUNDARIO(), msg);
        }
        let mut acao: Option<(usize, u8)> = None;
        for (i, r) in self.redes.iter().enumerate() {
            let y = ly0 + i as f32 * (item_a + 6.0) - self.rolagem;
            if y + item_a < ly0 || y > fim {
                continue;
            }
            let pedindo = self.senha_para.as_deref() == Some(r.nome.as_str());
            let h = if pedindo { item_a + 50.0 } else { item_a };
            let sobre = ui.sobre(lx, y, ll, h);
            ui.tela.retangulo(lx, y, ll, h, 8.0, if r.conectada { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(lx + 1.0, y + 1.0, ll - 2.0, h - 2.0, 7.0, if r.conectada { tema::DESTAQUE_SUAVE() } else if sobre { tema::FUNDO() } else { tema::JANELA() });
            barras_sinal(ui.tela, lx + 16.0, y + 16.0, r.sinal, if r.conectada { tema::DESTAQUE() } else { tema::TEXTO() });
            ui.tela.texto(lx + 58.0, y + 25.0, 14.0, tema::TEXTO(), &desenho::encurtar(14.0, &r.nome, ll - 260.0));
            let seg = match r.seguranca.as_str() {
                "open" => "Aberta",
                "psk" => "Protegida (WPA)",
                "8021x" => "Corporativa (802.1X)",
                "wep" => "WEP (insegura)",
                o => o,
            };
            let sub = if r.conectada { format!("Conectada · {seg}") } else { seg.to_string() };
            ui.tela.texto(lx + 58.0, y + 44.0, 12.0, tema::TEXTO_SECUNDARIO(), &sub);
            // Botões à direita
            let botoes: Vec<(&str, u8)> = if r.conectada {
                vec![("Desconectar", 1), ("Esquecer", 2)]
            } else if !pedindo {
                vec![("Conectar", 0)]
            } else {
                vec![]
            };
            let mut bx = lx + ll - 12.0;
            for (rot, cod) in botoes.iter().rev() {
                let bw = desenho::medir(13.0, rot) + 28.0;
                bx -= bw;
                let s = ui.sobre(bx, y + 12.0, bw, 32.0);
                let principal = *cod == 0;
                let fundo = if principal {
                    if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() }
                } else if s {
                    tema::SUPERFICIE_ALTA()
                } else {
                    tema::FUNDO()
                };
                ui.tela.retangulo(bx, y + 12.0, bw, 32.0, 6.0, fundo);
                ui.tela.texto(bx + 14.0, y + 33.0, 13.0, if principal { tema::BRANCO() } else { tema::TEXTO() }, rot);
                if ui.clicou(bx, y + 12.0, bw, 32.0) && !self.ocupado {
                    acao = Some((i, *cod));
                }
                bx -= 8.0;
            }
            if pedindo {
                // Campo de senha e Conectar.
                let (cx, cy, cl) = (lx + 58.0, y + item_a + 2.0, ll - 58.0 - 12.0 - 120.0);
                ui.tela.retangulo(cx, cy, cl, 34.0, 6.0, tema::DESTAQUE());
                ui.tela.retangulo(cx + 1.0, cy + 1.0, cl - 2.0, 32.0, 5.0, tema::JANELA());
                let vis = if self.senha.is_empty() {
                    String::new()
                } else if self.mostrar_senha {
                    self.senha.clone()
                } else {
                    "•".repeat(self.senha.chars().count())
                };
                let fimx = if vis.is_empty() {
                    ui.tela.texto(cx + 10.0, cy + 22.0, 13.0, tema::TEXTO_SECUNDARIO(), "Senha da rede");
                    0.0
                } else {
                    ui.tela.texto(cx + 10.0, cy + 22.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, &vis, cl - 60.0))
                };
                ui.tela.retangulo(cx + 11.0 + fimx, cy + 8.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
                // Olho: mostrar senha
                let ox = cx + cl - 34.0;
                let s_olho = ui.sobre(ox, cy + 2.0, 30.0, 30.0);
                ui.tela.texto(ox + 2.0, cy + 22.0, 11.0, if s_olho || self.mostrar_senha { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() }, if self.mostrar_senha { "ocultar" } else { "ver" });
                if ui.clicou(ox, cy + 2.0, 30.0, 30.0) {
                    self.mostrar_senha = !self.mostrar_senha;
                }
                let bx = cx + cl + 10.0;
                let s = ui.sobre(bx, cy, 110.0, 34.0);
                ui.tela.retangulo(bx, cy, 110.0, 34.0, 6.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
                ui.tela.texto(bx + 22.0, cy + 22.0, 13.0, tema::BRANCO(), "Conectar");
                if ui.clicou(bx, cy, 110.0, 34.0) {
                    acao = Some((i, 3));
                }
            }
        }
        match acao {
            Some((i, 0)) => {
                if let Some(r) = self.redes.get(i).cloned() {
                    self.conectar(&r);
                }
            }
            Some((_, 1)) => {
                self.aviso = Some("Desconectando…".into());
                self.pedir(Pedido::Desconectar);
            }
            Some((i, 2)) => {
                if let Some(r) = self.redes.get(i) {
                    let nome = r.nome.clone();
                    self.pedir(Pedido::Esquecer(nome));
                }
            }
            Some((_, 3)) => self.enviar_senha(),
            _ => {}
        }
    }

    fn enviar_senha(&mut self) {
        let Some(nome) = self.senha_para.clone() else { return };
        if self.senha.chars().count() < 8 {
            self.aviso = Some("A senha de Wi-Fi tem pelo menos 8 caracteres.".into());
            return;
        }
        let senha = std::mem::take(&mut self.senha);
        self.senha_para = None;
        self.aviso = Some(format!("Conectando a \"{nome}\"..."));
        self.pedir(Pedido::Conectar(nome, Some(senha)));
    }
}
