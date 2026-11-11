// SPDX-License-Identifier: GPL-3.0-or-later
//! Calculadora do Ararinha: a padrão de bolso, com vírgula decimal e
//! teclado (números, + - * /, Enter, Backspace, Esc, %).

use penas::{desenho, tema, App, Cor, Opcoes, Tecla, Ui};

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Somar,
    Subtrair,
    Multiplicar,
    Dividir,
}

impl Op {
    fn simbolo(self) -> &'static str {
        match self {
            Op::Somar => "+",
            Op::Subtrair => "−",
            Op::Multiplicar => "×",
            Op::Dividir => "÷",
        }
    }
    fn aplicar(self, a: f64, b: f64) -> Option<f64> {
        match self {
            Op::Somar => Some(a + b),
            Op::Subtrair => Some(a - b),
            Op::Multiplicar => Some(a * b),
            Op::Dividir => (b != 0.0).then(|| a / b),
        }
    }
}

#[derive(Clone, Copy)]
enum Botao {
    Digito(u8),
    Virgula,
    Op(Op),
    Igual,
    Porcento,
    LimparEntrada,
    Limpar,
    Apagar,
    Inverso,
    Quadrado,
    Raiz,
    Sinal,
}

const GRADE: [[(&str, Botao); 4]; 6] = [
    [("%", Botao::Porcento), ("CE", Botao::LimparEntrada), ("C", Botao::Limpar), ("⌫", Botao::Apagar)],
    [("¹/x", Botao::Inverso), ("x²", Botao::Quadrado), ("√x", Botao::Raiz), ("÷", Botao::Op(Op::Dividir))],
    [("7", Botao::Digito(7)), ("8", Botao::Digito(8)), ("9", Botao::Digito(9)), ("×", Botao::Op(Op::Multiplicar))],
    [("4", Botao::Digito(4)), ("5", Botao::Digito(5)), ("6", Botao::Digito(6)), ("−", Botao::Op(Op::Subtrair))],
    [("1", Botao::Digito(1)), ("2", Botao::Digito(2)), ("3", Botao::Digito(3)), ("+", Botao::Op(Op::Somar))],
    [("±", Botao::Sinal), ("0", Botao::Digito(0)), (",", Botao::Virgula), ("=", Botao::Igual)],
];

struct Calculadora {
    /// O que está sendo digitado (com '.' interno).
    entrada: String,
    acumulado: Option<f64>,
    pendente: Option<Op>,
    /// Linha de cima: a conta em andamento.
    historico: String,
    /// A próxima tecla numérica começa um número novo.
    novo: bool,
    erro: Option<String>,
    ultima: Option<(Op, f64)>,
}

/// Número no jeito brasileiro: 1.234.567,89.
fn formatar(v: f64) -> String {
    if !v.is_finite() {
        return "Não dá".into();
    }
    if v.abs() >= 1e16 || (v != 0.0 && v.abs() < 1e-10) {
        return format!("{v:e}").replace('.', ",");
    }
    let mut s = format!("{:.12}", v);
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    agrupar(&s)
}

/// Põe pontos de milhar e vírgula numa string com '.' decimal.
fn agrupar(s: &str) -> String {
    let (sinal, resto) = if let Some(r) = s.strip_prefix('-') { ("-", r) } else { ("", s) };
    let (int, dec) = match resto.split_once('.') {
        Some((i, d)) => (i, Some(d)),
        None => (resto, None),
    };
    let mut g = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    let mut r = format!("{sinal}{g}");
    if let Some(d) = dec {
        r.push(',');
        r.push_str(d);
    }
    r
}

impl Calculadora {
    fn novo() -> Calculadora {
        Calculadora {
            entrada: "0".into(),
            acumulado: None,
            pendente: None,
            historico: String::new(),
            novo: true,
            erro: None,
            ultima: None,
        }
    }

    fn valor(&self) -> f64 {
        self.entrada.parse().unwrap_or(0.0)
    }

    fn mostrar(&mut self, v: f64) {
        let mut s = format!("{:.12}", v);
        if s.contains('.') {
            s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        if v.abs() >= 1e16 {
            s = format!("{v}");
        }
        self.entrada = s;
    }

    fn falhar(&mut self, msg: &str) {
        let erro = Some(msg.to_string());
        *self = Calculadora::novo();
        self.erro = erro;
    }

    fn apertar(&mut self, b: Botao) {
        if self.erro.is_some() {
            let digito = matches!(b, Botao::Digito(_) | Botao::Virgula);
            *self = Calculadora::novo();
            if !digito {
                return;
            }
        }
        match b {
            Botao::Digito(d) => {
                if self.novo {
                    self.entrada = "0".into();
                    self.novo = false;
                }
                if self.entrada.chars().filter(|c| c.is_ascii_digit()).count() >= 16 {
                    return;
                }
                if self.entrada == "0" {
                    self.entrada = d.to_string();
                } else if self.entrada == "-0" {
                    self.entrada = format!("-{d}");
                } else {
                    self.entrada.push((b'0' + d) as char);
                }
            }
            Botao::Virgula => {
                if self.novo {
                    self.entrada = "0".into();
                    self.novo = false;
                }
                if !self.entrada.contains('.') {
                    self.entrada.push('.');
                }
            }
            Botao::Op(op) => {
                if !self.novo || self.acumulado.is_none() {
                    self.calcular_pendente();
                    if self.erro.is_some() {
                        return;
                    }
                }
                let a = self.acumulado.unwrap_or_else(|| self.valor());
                self.acumulado = Some(a);
                self.pendente = Some(op);
                self.historico = format!("{} {}", formatar(a), op.simbolo());
                self.novo = true;
                self.ultima = None;
            }
            Botao::Igual => {
                let b = self.valor();
                if let (Some(a), Some(op)) = (self.acumulado, self.pendente) {
                    self.historico = format!("{} {} {} =", formatar(a), op.simbolo(), formatar(b));
                    match op.aplicar(a, b) {
                        Some(r) => {
                            self.mostrar(r);
                            self.ultima = Some((op, b));
                        }
                        None => return self.falhar("Não dá para dividir por zero"),
                    }
                } else if let Some((op, b2)) = self.ultima {
                    // "=" de novo repete a última conta, como no Windows.
                    self.historico = format!("{} {} {} =", formatar(b), op.simbolo(), formatar(b2));
                    match op.aplicar(b, b2) {
                        Some(r) => self.mostrar(r),
                        None => return self.falhar("Não dá para dividir por zero"),
                    }
                }
                self.acumulado = None;
                self.pendente = None;
                self.novo = true;
            }
            Botao::Porcento => {
                let v = self.valor();
                let r = match (self.acumulado, self.pendente) {
                    (Some(a), Some(Op::Somar | Op::Subtrair)) => a * v / 100.0,
                    _ => v / 100.0,
                };
                self.mostrar(r);
                self.novo = true;
            }
            Botao::LimparEntrada => {
                self.entrada = "0".into();
                self.novo = true;
            }
            Botao::Limpar => *self = Calculadora::novo(),
            Botao::Apagar => {
                if self.novo {
                    self.historico.clear();
                    return;
                }
                self.entrada.pop();
                if self.entrada.is_empty() || self.entrada == "-" {
                    self.entrada = "0".into();
                }
            }
            Botao::Inverso => {
                let v = self.valor();
                if v == 0.0 {
                    return self.falhar("Não dá para dividir por zero");
                }
                self.historico = format!("1/({})", formatar(v));
                self.mostrar(1.0 / v);
                self.novo = true;
            }
            Botao::Quadrado => {
                let v = self.valor();
                self.historico = format!("sqr({})", formatar(v));
                self.mostrar(v * v);
                self.novo = true;
            }
            Botao::Raiz => {
                let v = self.valor();
                if v < 0.0 {
                    return self.falhar("Entrada inválida");
                }
                self.historico = format!("√({})", formatar(v));
                self.mostrar(v.sqrt());
                self.novo = true;
            }
            Botao::Sinal => {
                if let Some(r) = self.entrada.strip_prefix('-') {
                    self.entrada = r.to_string();
                } else if self.entrada != "0" {
                    self.entrada = format!("-{}", self.entrada);
                }
            }
        }
    }

    fn calcular_pendente(&mut self) {
        if let (Some(a), Some(op)) = (self.acumulado, self.pendente) {
            match op.aplicar(a, self.valor()) {
                Some(r) => {
                    self.acumulado = Some(r);
                    self.mostrar(r);
                }
                None => self.falhar("Não dá para dividir por zero"),
            }
        }
    }

    fn texto_visor(&self) -> String {
        if let Some(e) = &self.erro {
            return e.clone();
        }
        if self.novo {
            return formatar(self.valor());
        }
        agrupar(&self.entrada)
    }
}

impl App for Calculadora {
    fn desenhar(&mut self, ui: &mut Ui) {
        for t in std::mem::take(&mut ui.teclas) {
            let b = match t {
                Tecla::Texto(s) => match s.as_str() {
                    d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => Some(Botao::Digito(d.as_bytes()[0] - b'0')),
                    "," | "." => Some(Botao::Virgula),
                    "+" => Some(Botao::Op(Op::Somar)),
                    "-" => Some(Botao::Op(Op::Subtrair)),
                    "*" | "x" => Some(Botao::Op(Op::Multiplicar)),
                    "/" => Some(Botao::Op(Op::Dividir)),
                    "=" => Some(Botao::Igual),
                    "%" => Some(Botao::Porcento),
                    _ => None,
                },
                Tecla::Enter => Some(Botao::Igual),
                Tecla::Apagar => Some(Botao::Apagar),
                Tecla::Esc => Some(Botao::Limpar),
                _ => None,
            };
            if let Some(b) = b {
                self.apertar(b);
            }
        }

        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::FUNDO);
        let m = 8.0;
        // Visor
        let visor_a = (a * 0.28).clamp(110.0, 200.0);
        ui.tela.texto(16.0, 30.0, 14.0, tema::TEXTO, "Padrão");
        let h = desenho::encurtar(14.0, &self.historico, l - 32.0);
        let lh = desenho::medir(14.0, &h);
        ui.tela.texto(l - 16.0 - lh, 58.0, 14.0, tema::TEXTO_SECUNDARIO, &h);
        let visor = self.texto_visor();
        let mut tam = if self.erro.is_some() { 26.0 } else { 46.0 };
        while tam > 16.0 && desenho::medir(tam, &visor) > l - 32.0 {
            tam -= 2.0;
        }
        let lv = desenho::medir(tam, &visor);
        ui.tela.texto(l - 16.0 - lv, visor_a - 14.0, tam, tema::TEXTO, &visor);

        // Teclas
        let gy = visor_a;
        let bl = (l - m * 5.0) / 4.0;
        let ba = (a - gy - m * 7.0) / 6.0;
        let mut apertado = None;
        for (li, linha) in GRADE.iter().enumerate() {
            for (ci, (rotulo, botao)) in linha.iter().enumerate() {
                let x = m + ci as f32 * (bl + m);
                let y = gy + m + li as f32 * (ba + m);
                let sobre = ui.sobre(x, y, bl, ba);
                let (fundo, texto) = match botao {
                    Botao::Igual => (if sobre { tema::DESTAQUE_FORTE } else { tema::DESTAQUE }, tema::BRANCO),
                    Botao::Digito(_) | Botao::Virgula | Botao::Sinal => {
                        (if sobre { tema::SUPERFICIE_ALTA } else { tema::SUPERFICIE }, tema::TEXTO)
                    }
                    _ => (if sobre { tema::BORDA } else { Cor::hex(0xEEF2F6) }, tema::TEXTO),
                };
                ui.tela.retangulo(x, y, bl, ba, 6.0, tema::BORDA);
                ui.tela.retangulo(x + 1.0, y + 1.0, bl - 2.0, ba - 2.0, 5.0, fundo);
                let ts = if matches!(botao, Botao::Digito(_)) { 20.0 } else { 17.0 };
                let lt = desenho::medir(ts, rotulo);
                ui.tela.texto(x + bl / 2.0 - lt / 2.0, y + ba / 2.0 + ts * 0.36, ts, texto, rotulo);
                if ui.clicou(x, y, bl, ba) {
                    apertado = Some(*botao);
                }
            }
        }
        if let Some(b) = apertado {
            self.apertar(b);
            ui.redesenhar = true;
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Calculadora".into(),
            app_id: "calculadora".into(),
            tamanho: (340, 520),
            minimo: (300, 440),
        },
        Box::new(Calculadora::novo()),
    );
}
