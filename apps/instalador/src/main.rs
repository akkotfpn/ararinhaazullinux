// SPDX-License-Identifier: GPL-3.0-or-later
//! Instalar o Ararinha: copia o sistema do pendrive para o disco do
//! computador, em quatro passos (boas-vindas, disco, confirmação, cópia).
//! O trabalho pesado é do ajudante `ararinha-instalar`.

mod discos;

use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use discos::Disco;
use penas::{desenho, tema, App, Cor, Opcoes, Ui};

static MINGAU_PNG: &[u8] = include_bytes!("../recursos/mingau.png");

#[derive(PartialEq)]
enum Passo {
    BoasVindas,
    Disco,
    Confirmar,
    Copiando,
    Pronto,
    Falhou(String),
}

struct Instalador {
    passo: Passo,
    discos: Vec<Disco>,
    escolhido: Option<usize>,
    entendi: bool,
    etapa: String,
    progresso: u32,
    rx: Option<Receiver<String>>,
    mingau: Option<penas::desenho::Imagem>,
}

impl Instalador {
    fn novo() -> Instalador {
        Instalador {
            passo: Passo::BoasVindas,
            discos: discos::listar(),
            escolhido: None,
            entendi: false,
            etapa: String::new(),
            progresso: 0,
            rx: None,
            mingau: desenho::icone(MINGAU_PNG, 120, 141),
        }
    }

    fn comecar(&mut self) {
        let Some(d) = self.escolhido.and_then(|i| self.discos.get(i)).cloned() else { return };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let filho = Command::new("ararinha-instalar")
                .args([d.nome.as_str(), "APAGAR-TUDO"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            match filho {
                Ok(mut f) => {
                    if let Some(saida) = f.stdout.take() {
                        for linha in BufReader::new(saida).lines().map_while(Result::ok) {
                            if tx.send(linha).is_err() {
                                return;
                            }
                        }
                    }
                    let ok = f.wait().map(|s| s.success()).unwrap_or(false);
                    let _ = tx.send(if ok { "FIM".into() } else { "FALHOU".into() });
                }
                Err(e) => {
                    let _ = tx.send(format!("ERRO Não deu para iniciar o ajudante: {e}"));
                    let _ = tx.send("FALHOU".into());
                }
            }
        });
        self.rx = Some(rx);
        self.passo = Passo::Copiando;
        self.etapa = "Começando…".into();
        self.progresso = 0;
    }

    fn botao(ui: &mut Ui, x: f32, y: f32, rot: &str, principal: bool, ativo: bool) -> bool {
        let w = desenho::medir(13.5, rot) + 36.0;
        let x = x - w;
        let s = ativo && ui.sobre(x, y, w, 38.0);
        let (fundo, texto) = if !ativo {
            (tema::SUPERFICIE_ALTA(), tema::TEXTO_SECUNDARIO())
        } else if principal {
            (if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() }, tema::BRANCO())
        } else {
            (if s { tema::SUPERFICIE_ALTA() } else { tema::FUNDO() }, tema::TEXTO())
        };
        if !principal {
            ui.tela.retangulo(x, y, w, 38.0, 8.0, tema::BORDA());
            ui.tela.retangulo(x + 1.0, y + 1.0, w - 2.0, 36.0, 7.0, fundo);
        } else {
            ui.tela.retangulo(x, y, w, 38.0, 8.0, fundo);
        }
        ui.tela.texto(x + 18.0, y + 24.0, 13.5, texto, rot);
        ativo && ui.clicou(x, y, w, 38.0)
    }
}

fn paragrafo(ui: &mut Ui, x: f32, mut y: f32, largura: f32, texto: &str) -> f32 {
    // Quebra simples por palavras.
    let mut linha = String::new();
    for palavra in texto.split_whitespace() {
        let tentativa = if linha.is_empty() { palavra.to_string() } else { format!("{linha} {palavra}") };
        if desenho::medir(13.5, &tentativa) > largura && !linha.is_empty() {
            ui.tela.texto(x, y, 13.5, tema::TEXTO_SECUNDARIO(), &linha);
            y += 22.0;
            linha = palavra.to_string();
        } else {
            linha = tentativa;
        }
    }
    if !linha.is_empty() {
        ui.tela.texto(x, y, 13.5, tema::TEXTO_SECUNDARIO(), &linha);
        y += 22.0;
    }
    y
}

impl App for Instalador {
    fn desenhar(&mut self, ui: &mut Ui) {
        // Mensagens do ajudante.
        if let Some(rx) = &self.rx {
            let linhas: Vec<String> = rx.try_iter().collect();
            for l in linhas {
                if let Some(e) = l.strip_prefix("ETAPA ") {
                    self.etapa = e.to_string();
                } else if let Some(p) = l.strip_prefix("PROGRESSO ") {
                    self.progresso = p.trim().parse().unwrap_or(self.progresso);
                } else if let Some(e) = l.strip_prefix("ERRO ") {
                    self.passo = Passo::Falhou(e.to_string());
                } else if l == "FIM" {
                    self.passo = Passo::Pronto;
                    self.rx = None;
                    break;
                } else if l == "FALHOU" {
                    if !matches!(self.passo, Passo::Falhou(_)) {
                        self.passo = Passo::Falhou("A instalação não terminou.".into());
                    }
                    self.rx = None;
                    break;
                }
            }
            ui.acordar_em = Some(Duration::from_millis(300));
        }
        ui.fechar = false;

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());
        // Lateral com os passos.
        let lat = 220.0;
        ui.tela.retangulo(0.0, 0.0, lat, a, 0.0, tema::FUNDO());
        ui.tela.retangulo(lat - 1.0, 0.0, 1.0, a, 0.0, tema::BORDA());
        if let Some(m) = &self.mingau {
            ui.tela.imagem(50, 24, m);
        }
        let passos = ["Boas-vindas", "Escolher o disco", "Confirmar", "Instalar"];
        let atual = match self.passo {
            Passo::BoasVindas => 0,
            Passo::Disco => 1,
            Passo::Confirmar => 2,
            _ => 3,
        };
        for (i, p) in passos.iter().enumerate() {
            let y = 200.0 + i as f32 * 40.0;
            let cor = if i == atual { tema::DESTAQUE() } else if i < atual { tema::TEXTO() } else { tema::TEXTO_SECUNDARIO() };
            ui.tela.retangulo(24.0, y - 14.0, 24.0, 24.0, 12.0, if i <= atual { tema::DESTAQUE() } else { tema::BORDA() });
            let n = format!("{}", i + 1);
            let ln = desenho::medir(12.0, &n);
            ui.tela.texto(36.0 - ln / 2.0, y + 3.0, 12.0, tema::BRANCO(), &n);
            ui.tela.texto(58.0, y + 4.0, 13.5, cor, p);
        }

        let x0 = lat + 36.0;
        let largura = l - x0 - 36.0;
        let by = a - 62.0;
        let bx = l - 32.0;
        match &self.passo {
            Passo::BoasVindas => {
                ui.tela.texto(x0, 64.0, 24.0, tema::TEXTO(), "Instalar o Ararinha Azul Linux");
                let mut y = 104.0;
                y = paragrafo(ui, x0, y, largura, "Este assistente copia o Ararinha que está rodando agora (do pendrive) para o disco do computador. Depois é só tirar o pendrive e ligar de novo.");
                y += 10.0;
                y = paragrafo(ui, x0, y, largura, "Tudo o que você já fez aqui vai junto: sua conta, seus arquivos e as configurações.");
                y += 10.0;
                ui.tela.retangulo(x0, y, largura, 64.0, 8.0, tema::DESTAQUE_SUAVE());
                paragrafo(ui, x0 + 16.0, y + 26.0, largura - 32.0, "Atenção: o disco escolhido será apagado por inteiro. Faça cópia do que for importante antes.");
                if Self::botao(ui, bx, by, "Avançar", true, true) {
                    self.discos = discos::listar();
                    self.passo = Passo::Disco;
                }
            }
            Passo::Disco => {
                ui.tela.texto(x0, 64.0, 22.0, tema::TEXTO(), "Onde instalar?");
                ui.tela.texto(x0, 92.0, 13.0, tema::TEXTO_SECUNDARIO(), "Escolha o disco. O pendrive em uso não aparece aqui.");
                if self.discos.is_empty() {
                    ui.tela.texto(x0, 140.0, 14.0, tema::ERRO(), "Nenhum outro disco encontrado neste computador.");
                }
                let mut escolha = None;
                for (i, d) in self.discos.iter().enumerate() {
                    let y = 116.0 + i as f32 * 74.0;
                    let sel = self.escolhido == Some(i);
                    let s = ui.sobre(x0, y, largura, 64.0);
                    ui.tela.retangulo(x0, y, largura, 64.0, 8.0, if sel { tema::DESTAQUE() } else if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
                    ui.tela.retangulo(x0 + 1.0, y + 1.0, largura - 2.0, 62.0, 7.0, if sel { tema::DESTAQUE_SUAVE() } else { tema::JANELA() });
                    // Ícone de disco
                    ui.tela.retangulo(x0 + 16.0, y + 18.0, 40.0, 28.0, 5.0, Cor::hex(0x6B7C8F));
                    ui.tela.retangulo(x0 + 19.0, y + 21.0, 34.0, 18.0, 3.0, Cor::hex(0xC9D3DD));
                    ui.tela.retangulo(x0 + 44.0, y + 40.0, 6.0, 3.0, 1.0, Cor::hex(0x3FBF6B));
                    let titulo = format!("{} — {}", d.modelo, discos::tamanho_legivel(d.bytes));
                    ui.tela.texto(x0 + 72.0, y + 28.0, 14.0, tema::TEXTO(), &desenho::encurtar(14.0, &titulo, largura - 90.0));
                    let det = format!("/dev/{} · {}{}", d.nome, d.conexao, if d.removivel { " · removível" } else { "" });
                    ui.tela.texto(x0 + 72.0, y + 48.0, 12.0, tema::TEXTO_SECUNDARIO(), &det);
                    if ui.clicou(x0, y, largura, 64.0) {
                        escolha = Some(i);
                    }
                }
                if let Some(i) = escolha {
                    self.escolhido = Some(i);
                }
                let pequeno = self.escolhido.and_then(|i| self.discos.get(i)).map(|d| d.bytes < 12_000_000_000).unwrap_or(false);
                if pequeno {
                    ui.tela.texto(x0, by + 24.0, 12.5, tema::ERRO(), "Esse disco é pequeno demais (mínimo 12 GB).");
                }
                if Self::botao(ui, bx, by, "Avançar", true, self.escolhido.is_some() && !pequeno) {
                    self.entendi = false;
                    self.passo = Passo::Confirmar;
                }
                if Self::botao(ui, bx - 130.0, by, "Voltar", false, true) {
                    self.passo = Passo::BoasVindas;
                }
            }
            Passo::Confirmar => {
                let d = self.escolhido.and_then(|i| self.discos.get(i)).cloned();
                ui.tela.texto(x0, 64.0, 22.0, tema::TEXTO(), "Tudo pronto para instalar");
                if let Some(d) = d {
                    let mut y = 110.0;
                    y = paragrafo(ui, x0, y, largura, &format!(
                        "O Ararinha será instalado em \"{}\" ({}, /dev/{}).",
                        d.modelo,
                        discos::tamanho_legivel(d.bytes),
                        d.nome
                    ));
                    y += 6.0;
                    ui.tela.retangulo(x0, y, largura, 70.0, 8.0, Cor(194, 65, 59, 36));
                    paragrafo(ui, x0 + 16.0, y + 28.0, largura - 32.0, "Tudo o que existe nesse disco (outros sistemas, como o Windows, e arquivos) será apagado para sempre.");
                    y += 96.0;
                    // Caixinha "Entendi"
                    let s = ui.sobre(x0, y - 4.0, largura, 30.0);
                    ui.tela.retangulo(x0, y, 20.0, 20.0, 4.0, if self.entendi { tema::DESTAQUE() } else if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
                    if !self.entendi {
                        ui.tela.retangulo(x0 + 1.5, y + 1.5, 17.0, 17.0, 3.0, tema::JANELA());
                    } else {
                        ui.tela.linha(x0 + 5.0, y + 10.0, x0 + 9.0, y + 14.0, 2.0, tema::BRANCO());
                        ui.tela.linha(x0 + 9.0, y + 14.0, x0 + 15.0, y + 6.0, 2.0, tema::BRANCO());
                    }
                    ui.tela.texto(x0 + 30.0, y + 15.0, 13.5, tema::TEXTO(), "Entendi: pode apagar esse disco e instalar o Ararinha.");
                    if ui.clicou(x0, y - 4.0, largura, 30.0) {
                        self.entendi = !self.entendi;
                    }
                }
                if Self::botao(ui, bx, by, "Instalar agora", true, self.entendi) {
                    self.comecar();
                }
                if Self::botao(ui, bx - 170.0, by, "Voltar", false, true) {
                    self.passo = Passo::Disco;
                }
            }
            Passo::Copiando => {
                ui.tela.texto(x0, 64.0, 22.0, tema::TEXTO(), "Instalando…");
                ui.tela.texto(x0, 110.0, 14.0, tema::TEXTO(), &self.etapa);
                let (px, py, pl) = (x0, 136.0, largura);
                ui.tela.retangulo(px, py, pl, 10.0, 5.0, tema::SUPERFICIE_ALTA());
                ui.tela.retangulo(px, py, pl * self.progresso as f32 / 100.0, 10.0, 5.0, tema::DESTAQUE());
                ui.tela.texto(px, py + 34.0, 12.5, tema::TEXTO_SECUNDARIO(), &format!("{}%  ·  Não desligue o computador nem tire o pendrive.", self.progresso));
            }
            Passo::Pronto => {
                ui.tela.texto(x0, 64.0, 22.0, tema::TEXTO(), "Instalação concluída!");
                let y = paragrafo(ui, x0, 110.0, largura, "O Ararinha já está no disco. Desligue, tire o pendrive e ligue o computador de novo. Se ele voltar para o pendrive, escolha o disco no menu de inicialização da placa-mãe (geralmente F12, F11 ou Esc ao ligar).");
                let _ = y;
                if Self::botao(ui, bx, by, "Desligar agora", true, true) {
                    let _ = Command::new("systemctl").arg("poweroff").spawn();
                }
            }
            Passo::Falhou(msg) => {
                let msg = msg.clone();
                ui.tela.texto(x0, 64.0, 22.0, tema::ERRO(), "A instalação não deu certo");
                let y = paragrafo(ui, x0, 110.0, largura, &msg);
                paragrafo(ui, x0, y + 10.0, largura, "Nada foi mudado no pendrive. Você pode tentar de novo, escolhendo outro disco se for o caso.");
                if Self::botao(ui, bx, by, "Tentar de novo", true, true) {
                    self.discos = discos::listar();
                    self.passo = Passo::Disco;
                }
            }
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Instalar o Ararinha".into(),
            app_id: "instalador".into(),
            tamanho: (860, 560),
            minimo: (760, 500),
        },
        Box::new(Instalador::novo()),
    );
}
