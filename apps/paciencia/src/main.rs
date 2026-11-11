// SPDX-License-Identifier: GPL-3.0-or-later
//! Paciência dos Pássaros: a Paciência (Klondike) de sempre, com pássaros
//! do Brasil nas figuras. Valetes são sabiás, damas são beija-flores, reis
//! são ararinhas-azuis e o ás de espadas é o Mingau.
//!
//! Arraste as cartas com o mouse; clique duplo manda para as fundações;
//! clique no monte para virar uma carta. Ctrl+Z desfaz, F2 começa outro jogo.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use penas::{desenho, tema, App, Cor, Keysym, Opcoes, Tecla, Tela, Ui};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Carta {
    /// 0 copas, 1 ouros (vermelhos), 2 espadas, 3 paus (pretos).
    naipe: u8,
    /// 1 (ás) a 13 (rei).
    valor: u8,
}

impl Carta {
    fn vermelha(self) -> bool {
        self.naipe < 2
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pilha {
    Monte,
    Descarte,
    Fundacao(usize),
    Coluna(usize),
}

#[derive(Clone)]
struct Mesa {
    monte: Vec<Carta>,
    descarte: Vec<Carta>,
    fundacoes: [Vec<Carta>; 4],
    /// Cartas e se estão viradas para cima.
    colunas: [Vec<(Carta, bool)>; 7],
    jogadas: u32,
}

struct Arraste {
    origem: Pilha,
    /// Índice da primeira carta arrastada na pilha de origem.
    indice: usize,
    cartas: Vec<Carta>,
    /// Onde o mouse pegou a carta, relativo ao canto dela.
    pega: (f32, f32),
    inicio: (f32, f32),
    andou: bool,
}

struct Jogo {
    mesa: Mesa,
    historico: Vec<Mesa>,
    arraste: Option<Arraste>,
    semente: u64,
    inicio: Instant,
    venceu: Option<Duration>,
    /// Animação de vitória: cartas pulando.
    festa: f32,
}

fn aleatorio(s: &mut u64) -> u64 {
    // xorshift64*
    *s ^= *s >> 12;
    *s ^= *s << 25;
    *s ^= *s >> 27;
    s.wrapping_mul(0x2545F4914F6CDD1D)
}

fn embaralhar(semente: &mut u64) -> Mesa {
    let mut baralho: Vec<Carta> = (0..4).flat_map(|n| (1..=13).map(move |v| Carta { naipe: n, valor: v })).collect();
    for i in (1..baralho.len()).rev() {
        let j = (aleatorio(semente) % (i as u64 + 1)) as usize;
        baralho.swap(i, j);
    }
    let mut colunas: [Vec<(Carta, bool)>; 7] = Default::default();
    for (c, coluna) in colunas.iter_mut().enumerate() {
        for k in 0..=c {
            let carta = baralho.pop().unwrap();
            coluna.push((carta, k == c));
        }
    }
    Mesa { monte: baralho, descarte: Vec::new(), fundacoes: Default::default(), colunas, jogadas: 0 }
}

// ------------------------------------------------------------- desenho

const MESA_COR: Cor = Cor::hex(0x1F6E5A);
const MESA_ESCURA: Cor = Cor::hex(0x175546);
const VERMELHO: Cor = Cor::hex(0xC8352E);
const PRETO: Cor = Cor::hex(0x1A2433);

fn rotulo(v: u8) -> &'static str {
    ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"][(v - 1) as usize]
}

/// Símbolo do naipe centrado em (cx, cy), com "raio" r.
fn naipe(t: &mut Tela, n: u8, cx: f32, cy: f32, r: f32, cor: Cor) {
    match n {
        0 => {
            // Copas: dois círculos e um triângulo.
            t.retangulo(cx - r, cy - r * 0.75, r * 1.05, r * 1.05, r * 0.52, cor);
            t.retangulo(cx - r * 0.05, cy - r * 0.75, r * 1.05, r * 1.05, r * 0.52, cor);
            t.poligono(&[(cx - r * 0.98, cy - 0.05 * r), (cx + r * 0.98, cy - 0.05 * r), (cx, cy + r)], cor);
        }
        1 => {
            t.poligono(&[(cx, cy - r), (cx + r * 0.72, cy), (cx, cy + r), (cx - r * 0.72, cy)], cor);
        }
        2 => {
            // Espadas: copas de cabeça para baixo e um pé.
            t.retangulo(cx - r, cy - r * 0.25, r * 1.05, r * 1.05, r * 0.52, cor);
            t.retangulo(cx - r * 0.05, cy - r * 0.25, r * 1.05, r * 1.05, r * 0.52, cor);
            t.poligono(&[(cx - r * 0.98, cy + 0.3 * r), (cx + r * 0.98, cy + 0.3 * r), (cx, cy - r)], cor);
            t.poligono(&[(cx, cy + 0.3 * r), (cx + r * 0.35, cy + r), (cx - r * 0.35, cy + r)], cor);
        }
        _ => {
            // Paus: três bolinhas e um pé.
            let b = r * 0.95;
            t.retangulo(cx - b / 2.0, cy - r, b, b, b / 2.0, cor);
            t.retangulo(cx - r, cy - r * 0.25, b, b, b / 2.0, cor);
            t.retangulo(cx + r - b, cy - r * 0.25, b, b, b / 2.0, cor);
            t.poligono(&[(cx, cy), (cx + r * 0.35, cy + r), (cx - r * 0.35, cy + r)], cor);
        }
    }
}

/// Pássaro estilizado para as figuras (J, Q, K) e o ás de espadas.
fn passaro(t: &mut Tela, cx: f32, cy: f32, s: f32, corpo: Cor, peito: Cor, bico: Cor, coroa: bool) {
    // Rabo
    t.poligono(&[(cx - s * 0.1, cy + s * 0.25), (cx - s * 0.55, cy + s * 0.75), (cx - s * 0.3, cy + s * 0.8)], corpo);
    // Corpo e peito
    t.retangulo(cx - s * 0.38, cy - s * 0.2, s * 0.76, s * 0.7, s * 0.35, corpo);
    t.retangulo(cx - s * 0.12, cy - s * 0.05, s * 0.42, s * 0.5, s * 0.21, peito);
    // Asa
    t.poligono(&[(cx - s * 0.32, cy), (cx + s * 0.05, cy + s * 0.12), (cx - s * 0.2, cy + s * 0.45)], corpo.com_alfa(255));
    t.linha(cx - s * 0.3, cy + s * 0.05, cx - s * 0.12, cy + s * 0.35, s * 0.03, Cor(0, 0, 0, 60));
    // Cabeça
    t.retangulo(cx - s * 0.05, cy - s * 0.52, s * 0.44, s * 0.42, s * 0.21, corpo);
    // Olho
    t.retangulo(cx + s * 0.18, cy - s * 0.42, s * 0.14, s * 0.14, s * 0.07, Cor::hex(0xFFFFFF));
    t.retangulo(cx + s * 0.22, cy - s * 0.38, s * 0.07, s * 0.07, s * 0.035, PRETO);
    // Bico
    t.poligono(&[(cx + s * 0.36, cy - s * 0.36), (cx + s * 0.58, cy - s * 0.26), (cx + s * 0.34, cy - s * 0.2)], bico);
    // Patinhas num galho
    t.linha(cx - s * 0.45, cy + s * 0.62, cx + s * 0.45, cy + s * 0.58, s * 0.06, Cor::hex(0x6B4A2E));
    t.linha(cx - s * 0.05, cy + s * 0.48, cx - s * 0.08, cy + s * 0.6, s * 0.04, bico);
    t.linha(cx + s * 0.1, cy + s * 0.48, cx + s * 0.1, cy + s * 0.6, s * 0.04, bico);
    if coroa {
        let (bx, by) = (cx + s * 0.17, cy - s * 0.55);
        t.poligono(
            &[(bx - s * 0.16, by), (bx - s * 0.16, by - s * 0.14), (bx - s * 0.08, by - s * 0.06), (bx, by - s * 0.16),
              (bx + s * 0.08, by - s * 0.06), (bx + s * 0.16, by - s * 0.14), (bx + s * 0.16, by)],
            Cor::hex(0xE8B23A),
        );
    }
}

fn desenhar_carta(t: &mut Tela, c: Carta, x: f32, y: f32, l: f32, a: f32) {
    t.retangulo(x + 1.0, y + 2.0, l, a, l * 0.08, Cor(0, 0, 0, 50));
    t.retangulo(x, y, l, a, l * 0.08, Cor::hex(0xB8C4CF));
    t.retangulo(x + 1.0, y + 1.0, l - 2.0, a - 2.0, l * 0.08 - 1.0, Cor::hex(0xFFFFFF));
    let cor = if c.vermelha() { VERMELHO } else { PRETO };
    let tam = (l * 0.2).max(11.0);
    t.texto(x + l * 0.08, y + tam + l * 0.04, tam, cor, rotulo(c.valor));
    naipe(t, c.naipe, x + l * 0.15, y + tam * 1.75 + l * 0.04, l * 0.065, cor);
    // Canto de baixo (espelhado só na posição).
    let lr = desenho::medir(tam * 0.85, rotulo(c.valor));
    t.texto(x + l - l * 0.08 - lr, y + a - l * 0.08, tam * 0.85, cor, rotulo(c.valor));
    let (cx, cy) = (x + l / 2.0 + l * 0.05, y + a / 2.0 + a * 0.05);
    match c.valor {
        11..=13 => {
            // Moldura e o pássaro da figura.
            t.retangulo(x + l * 0.24, y + a * 0.2, l * 0.64, a * 0.66, l * 0.05, cor.com_alfa(40));
            let (corpo, peito, bico) = match c.valor {
                11 => (Cor::hex(0x8A6A4A), Cor::hex(0xE39B4A), Cor::hex(0xD9A441)), // sabiá-laranjeira
                12 => (Cor::hex(0x2E9E6A), Cor::hex(0xB7E4C7), Cor::hex(0x2A2A2A)), // beija-flor
                _ => (Cor::hex(0x3B7FC4), Cor::hex(0x8FC1EC), Cor::hex(0x2A2A2A)),  // ararinha-azul
            };
            passaro(t, cx, cy - a * 0.02, l * 0.5, corpo, peito, bico, c.valor == 13);
            naipe(t, c.naipe, x + l * 0.78, y + a * 0.28, l * 0.07, cor);
        }
        1 if c.naipe == 2 => {
            // Ás de espadas: o Mingau.
            passaro(t, cx - l * 0.03, cy, l * 0.5, Cor::hex(0x4A8FD6), Cor::hex(0xD6E8F7), Cor::hex(0x2A2A2A), false);
        }
        1 => naipe(t, c.naipe, cx - l * 0.05, cy, l * 0.2, cor),
        v => {
            // Pontos do naipe em duas colunas.
            let n = v as usize;
            let colunas = if n <= 3 { 1 } else { 2 };
            let por_coluna = n.div_ceil(colunas);
            for k in 0..n {
                let col = if colunas == 1 { 0 } else { k % 2 };
                let lin = if colunas == 1 { k } else { k / 2 };
                let px = if colunas == 1 { cx - l * 0.05 } else { x + l * (0.38 + 0.3 * col as f32) };
                let passo = a * 0.6 / por_coluna.max(1) as f32;
                let py = y + a * 0.25 + passo * (lin as f32 + 0.5);
                naipe(t, c.naipe, px, py, l * 0.075, cor);
            }
        }
    }
}

fn verso(t: &mut Tela, x: f32, y: f32, l: f32, a: f32) {
    t.retangulo(x + 1.0, y + 2.0, l, a, l * 0.08, Cor(0, 0, 0, 50));
    t.retangulo(x, y, l, a, l * 0.08, Cor::hex(0xFFFFFF));
    t.retangulo(x + 4.0, y + 4.0, l - 8.0, a - 8.0, l * 0.06, Cor::hex(0x1E5FA3));
    // Penas em diagonal
    let mut k = -a;
    while k < l + a {
        t.linha(x + 6.0 + k, y + 6.0, x + 6.0 + k - a * 0.5, y + a - 6.0, 1.2, Cor(255, 255, 255, 40));
        k += l * 0.16;
    }
    t.retangulo(x + l / 2.0 - l * 0.16, y + a / 2.0 - l * 0.16, l * 0.32, l * 0.32, l * 0.16, Cor::hex(0x8FC1EC));
    t.retangulo(x + l / 2.0 - l * 0.08, y + a / 2.0 - l * 0.08, l * 0.16, l * 0.16, l * 0.08, Cor::hex(0x1E5FA3));
}

fn lugar_vazio(t: &mut Tela, x: f32, y: f32, l: f32, a: f32) {
    t.retangulo(x, y, l, a, l * 0.08, Cor(255, 255, 255, 60));
    t.retangulo(x + 2.0, y + 2.0, l - 4.0, a - 4.0, l * 0.07, MESA_COR);
}

// --------------------------------------------------------------- regras

impl Mesa {
    fn pode_na_fundacao(&self, f: usize, c: Carta) -> bool {
        match self.fundacoes[f].last() {
            None => c.valor == 1,
            Some(t) => t.naipe == c.naipe && c.valor == t.valor + 1,
        }
    }

    fn pode_na_coluna(&self, col: usize, c: Carta) -> bool {
        match self.colunas[col].last() {
            None => c.valor == 13,
            Some((t, aberta)) => *aberta && t.vermelha() != c.vermelha() && c.valor + 1 == t.valor,
        }
    }

    fn venceu(&self) -> bool {
        self.fundacoes.iter().all(|f| f.len() == 13)
    }

    /// Vira a última carta de cada coluna que ficou fechada.
    fn virar_abertas(&mut self) {
        for col in self.colunas.iter_mut() {
            if let Some(ultima) = col.last_mut() {
                ultima.1 = true;
            }
        }
    }
}

impl Jogo {
    fn novo() -> Jogo {
        let mut semente = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(42) | 1;
        let mesa = embaralhar(&mut semente);
        Jogo { mesa, historico: Vec::new(), arraste: None, semente, inicio: Instant::now(), venceu: None, festa: 0.0 }
    }

    fn reiniciar(&mut self) {
        self.mesa = embaralhar(&mut self.semente);
        self.historico.clear();
        self.arraste = None;
        self.inicio = Instant::now();
        self.venceu = None;
        self.festa = 0.0;
    }

    fn guardar(&mut self) {
        self.historico.push(self.mesa.clone());
        if self.historico.len() > 200 {
            self.historico.remove(0);
        }
    }

    fn desfazer(&mut self) {
        if let Some(m) = self.historico.pop() {
            self.mesa = m;
            self.venceu = None;
        }
    }

    fn comprar(&mut self) {
        self.guardar();
        if let Some(c) = self.mesa.monte.pop() {
            self.mesa.descarte.push(c);
        } else {
            // Monte vazio: o descarte volta para o monte.
            let mut d = std::mem::take(&mut self.mesa.descarte);
            d.reverse();
            self.mesa.monte = d;
        }
        self.mesa.jogadas += 1;
    }

    /// Tira as cartas da origem (a partir de `indice`).
    fn tirar(&mut self, origem: Pilha, indice: usize) -> Vec<Carta> {
        match origem {
            Pilha::Descarte => self.mesa.descarte.split_off(indice),
            Pilha::Fundacao(f) => self.mesa.fundacoes[f].split_off(indice),
            Pilha::Coluna(c) => self.mesa.colunas[c].split_off(indice).into_iter().map(|(c, _)| c).collect(),
            Pilha::Monte => Vec::new(),
        }
    }

    /// Tenta pôr `cartas` (vindas de `origem`) em `destino`.
    fn mover(&mut self, origem: Pilha, indice: usize, destino: Pilha) -> bool {
        let cartas: Vec<Carta> = match origem {
            Pilha::Descarte => self.mesa.descarte[indice..].to_vec(),
            Pilha::Fundacao(f) => self.mesa.fundacoes[f][indice..].to_vec(),
            Pilha::Coluna(c) => self.mesa.colunas[c][indice..].iter().map(|(c, _)| *c).collect(),
            Pilha::Monte => return false,
        };
        let Some(&primeira) = cartas.first() else { return false };
        let ok = match destino {
            Pilha::Fundacao(f) => cartas.len() == 1 && self.mesa.pode_na_fundacao(f, primeira),
            Pilha::Coluna(c) => destino != origem && self.mesa.pode_na_coluna(c, primeira),
            _ => false,
        };
        if !ok {
            return false;
        }
        self.guardar();
        let cartas = self.tirar(origem, indice);
        match destino {
            Pilha::Fundacao(f) => self.mesa.fundacoes[f].extend(cartas),
            Pilha::Coluna(c) => self.mesa.colunas[c].extend(cartas.into_iter().map(|c| (c, true))),
            _ => {}
        }
        self.mesa.virar_abertas();
        self.mesa.jogadas += 1;
        if self.mesa.venceu() && self.venceu.is_none() {
            self.venceu = Some(self.inicio.elapsed());
        }
        true
    }

    /// Clique duplo: manda a carta para a fundação que a aceitar.
    fn para_fundacao(&mut self, origem: Pilha, indice: usize) -> bool {
        (0..4).any(|f| self.mover(origem, indice, Pilha::Fundacao(f)))
    }

    /// Manda tudo o que der para as fundações (botão "Recolher").
    fn recolher(&mut self) {
        loop {
            let mut moveu = false;
            if !self.mesa.descarte.is_empty() {
                let i = self.mesa.descarte.len() - 1;
                moveu |= self.para_fundacao(Pilha::Descarte, i);
            }
            for c in 0..7 {
                if !self.mesa.colunas[c].is_empty() {
                    let i = self.mesa.colunas[c].len() - 1;
                    moveu |= self.para_fundacao(Pilha::Coluna(c), i);
                }
            }
            if !moveu {
                break;
            }
        }
    }
}

// ------------------------------------------------------------- tela

struct Geometria {
    cl: f32,
    ca: f32,
    margem: f32,
    passo: f32,
    topo: f32,
    linha2: f32,
    /// Distância entre cartas abertas/fechadas empilhadas na coluna.
    aberta: f32,
    fechada: f32,
}

impl Geometria {
    fn de(l: f32, a: f32) -> Geometria {
        let margem = 16.0;
        let cl = ((l - margem * 2.0 - 6.0 * 14.0) / 7.0).clamp(56.0, 120.0);
        let passo = (l - margem * 2.0 - cl) / 6.0;
        let ca = cl * 1.4;
        let topo = 56.0;
        let linha2 = topo + ca + 20.0;
        let aberta = (ca * 0.26).min(((a - linha2 - ca - 10.0) / 12.0).max(14.0));
        Geometria { cl, ca, margem, passo, topo, linha2, aberta, fechada: (aberta * 0.45).max(8.0) }
    }
    fn x(&self, i: usize) -> f32 {
        self.margem + i as f32 * self.passo
    }
    fn posicao(&self, p: Pilha) -> (f32, f32) {
        match p {
            Pilha::Monte => (self.x(0), self.topo),
            Pilha::Descarte => (self.x(1), self.topo),
            Pilha::Fundacao(f) => (self.x(3 + f), self.topo),
            Pilha::Coluna(c) => (self.x(c), self.linha2),
        }
    }
}

impl Jogo {
    fn y_na_coluna(&self, g: &Geometria, c: usize, i: usize) -> f32 {
        let mut y = g.linha2;
        for (k, (_, aberta)) in self.mesa.colunas[c].iter().enumerate() {
            if k == i {
                break;
            }
            y += if *aberta { g.aberta } else { g.fechada };
        }
        y
    }

    /// Carta (aberta) sob o mouse: pilha e índice.
    fn carta_em(&self, g: &Geometria, mx: f32, my: f32) -> Option<(Pilha, usize)> {
        let dentro = |x: f32, y: f32, a: f32| mx >= x && mx < x + g.cl && my >= y && my < y + a;
        let (x, y) = g.posicao(Pilha::Monte);
        if dentro(x, y, g.ca) {
            return Some((Pilha::Monte, 0));
        }
        let (x, y) = g.posicao(Pilha::Descarte);
        if !self.mesa.descarte.is_empty() && dentro(x, y, g.ca) {
            return Some((Pilha::Descarte, self.mesa.descarte.len() - 1));
        }
        for f in 0..4 {
            let (x, y) = g.posicao(Pilha::Fundacao(f));
            if !self.mesa.fundacoes[f].is_empty() && dentro(x, y, g.ca) {
                return Some((Pilha::Fundacao(f), self.mesa.fundacoes[f].len() - 1));
            }
        }
        for c in 0..7 {
            let n = self.mesa.colunas[c].len();
            for i in (0..n).rev() {
                let y = self.y_na_coluna(g, c, i);
                let a = if i == n - 1 { g.ca } else if self.mesa.colunas[c][i].1 { g.aberta } else { g.fechada };
                if dentro(g.x(c), y, a) {
                    return Some((Pilha::Coluna(c), i));
                }
            }
        }
        None
    }

    /// Pilha onde soltar, pelo centro da carta arrastada.
    fn destino_em(&self, g: &Geometria, cx: f32, cy: f32) -> Option<Pilha> {
        let mut melhor: Option<(f32, Pilha)> = None;
        let mut considerar = |p: Pilha, x: f32, y: f32, a: f32| {
            let (mx, my) = (x + g.cl / 2.0, y + a / 2.0);
            let d = (mx - cx).abs() + (my - cy).abs() * 0.5;
            if (cx - mx).abs() < g.cl && cy > y - g.ca * 0.5 && cy < y + a + g.ca * 0.5 && melhor.map(|(m, _)| d < m).unwrap_or(true) {
                melhor = Some((d, p));
            }
        };
        for f in 0..4 {
            let (x, y) = g.posicao(Pilha::Fundacao(f));
            considerar(Pilha::Fundacao(f), x, y, g.ca);
        }
        for c in 0..7 {
            let n = self.mesa.colunas[c].len();
            let y = if n == 0 { g.linha2 } else { self.y_na_coluna(g, c, n - 1) };
            considerar(Pilha::Coluna(c), g.x(c), g.linha2, y - g.linha2 + g.ca);
        }
        melhor.map(|(_, p)| p)
    }

    fn botao(ui: &mut Ui, x: f32, rot: &str) -> (bool, f32) {
        let w = desenho::medir(13.0, rot) + 28.0;
        let s = ui.sobre(x, 10.0, w, 32.0);
        ui.tela.retangulo(x, 10.0, w, 32.0, 8.0, if s { Cor(255, 255, 255, 60) } else { Cor(255, 255, 255, 30) });
        ui.tela.texto(x + 14.0, 31.0, 13.0, Cor::hex(0xFFFFFF), rot);
        (ui.clicou(x, 10.0, w, 32.0), w)
    }
}

impl App for Jogo {
    fn desenhar(&mut self, ui: &mut Ui) {
        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Outra(k) if ui.ctrl && (k == Keysym::z || k == Keysym::Z) => self.desfazer(),
                Tecla::Outra(Keysym::F2) => self.reiniciar(),
                Tecla::Texto(s) if s == " " => self.comprar(),
                _ => {}
            }
        }
        let (l, a) = (ui.largura(), ui.altura());
        let g = Geometria::de(l, a);
        ui.tela.limpar(MESA_COR);
        // Vinheta suave nas bordas
        ui.tela.retangulo(0.0, 0.0, l, 52.0, 0.0, MESA_ESCURA);

        // Barra de cima
        let (novo, w1) = Self::botao(ui, 12.0, "Novo jogo");
        let (desf, w2) = Self::botao(ui, 20.0 + w1, "Desfazer");
        let (rec, _) = Self::botao(ui, 28.0 + w1 + w2, "Recolher");
        let tempo = self.venceu.unwrap_or_else(|| self.inicio.elapsed()).as_secs();
        let info = format!("Jogadas: {}   Tempo: {}:{:02}", self.mesa.jogadas, tempo / 60, tempo % 60);
        let li = desenho::medir(13.0, &info);
        ui.tela.texto(l - 16.0 - li, 31.0, 13.0, Cor::hex(0xEAF2F9), &info);
        if novo {
            self.reiniciar();
        }
        if desf {
            self.desfazer();
        }
        if rec {
            self.recolher();
        }

        // Monte
        let (x, y) = g.posicao(Pilha::Monte);
        if self.mesa.monte.is_empty() {
            lugar_vazio(ui.tela, x, y, g.cl, g.ca);
            // Setinha de "virar de novo".
            let (cx, cy) = (x + g.cl / 2.0, y + g.ca / 2.0);
            ui.tela.retangulo(cx - 14.0, cy - 14.0, 28.0, 28.0, 14.0, Cor(255, 255, 255, 80));
            ui.tela.retangulo(cx - 10.0, cy - 10.0, 20.0, 20.0, 10.0, MESA_COR);
        } else {
            verso(ui.tela, x, y, g.cl, g.ca);
        }
        // Descarte (mostra até três, em leque)
        let (x, y) = g.posicao(Pilha::Descarte);
        let arrastando_de = self.arraste.as_ref().filter(|a| a.andou).map(|a| (a.origem, a.indice));
        let mut descarte_vis: Vec<Carta> = self.mesa.descarte.clone();
        if let Some((Pilha::Descarte, i)) = arrastando_de {
            descarte_vis.truncate(i);
        }
        if descarte_vis.is_empty() {
            lugar_vazio(ui.tela, x, y, g.cl, g.ca);
        } else {
            let n = descarte_vis.len();
            for (k, c) in descarte_vis.iter().enumerate().skip(n.saturating_sub(3)) {
                let off = (k + 3 - n.min(3).max(k + 1 - n.saturating_sub(3)).min(3)) as f32;
                let _ = off;
                let desloc = (k as f32 - n.saturating_sub(3) as f32) * g.cl * 0.18;
                desenhar_carta(ui.tela, *c, x + desloc.min(g.passo - g.cl - 4.0).max(0.0), y, g.cl, g.ca);
            }
        }
        // Fundações
        for f in 0..4 {
            let (x, y) = g.posicao(Pilha::Fundacao(f));
            let mut pilha = self.mesa.fundacoes[f].clone();
            if let Some((Pilha::Fundacao(ff), i)) = arrastando_de {
                if ff == f {
                    pilha.truncate(i);
                }
            }
            match pilha.last() {
                Some(c) => desenhar_carta(ui.tela, *c, x, y, g.cl, g.ca),
                None => {
                    lugar_vazio(ui.tela, x, y, g.cl, g.ca);
                    let ordem = [0u8, 1, 2, 3];
                    naipe(ui.tela, ordem[f], x + g.cl / 2.0, y + g.ca / 2.0, g.cl * 0.16, Cor(255, 255, 255, 70));
                }
            }
        }
        // Colunas
        for c in 0..7 {
            let x = g.x(c);
            let mut fim = self.mesa.colunas[c].len();
            if let Some((Pilha::Coluna(cc), i)) = arrastando_de {
                if cc == c {
                    fim = i;
                }
            }
            if fim == 0 {
                lugar_vazio(ui.tela, x, g.linha2, g.cl, g.ca);
            }
            for i in 0..fim {
                let (carta, aberta) = self.mesa.colunas[c][i];
                let y = self.y_na_coluna(&g, c, i);
                if aberta {
                    desenhar_carta(ui.tela, carta, x, y, g.cl, g.ca);
                } else {
                    verso(ui.tela, x, y, g.cl, g.ca);
                }
            }
        }

        // Mouse: apertar, arrastar, soltar.
        if let Some((mx, my)) = ui.mouse {
            if ui.clique && self.venceu.is_none() {
                match self.carta_em(&g, mx, my) {
                    Some((Pilha::Monte, _)) => self.comprar(),
                    Some((origem, indice)) => {
                        let aberta = match origem {
                            Pilha::Coluna(c) => self.mesa.colunas[c][indice].1,
                            _ => true,
                        };
                        if !aberta {
                            // Carta fechada no fim da coluna: vira.
                            if let Pilha::Coluna(c) = origem {
                                if indice + 1 == self.mesa.colunas[c].len() {
                                    self.guardar();
                                    self.mesa.colunas[c][indice].1 = true;
                                }
                            }
                        } else if ui.duplo {
                            let ultima = match origem {
                                Pilha::Coluna(c) => indice + 1 == self.mesa.colunas[c].len(),
                                _ => true,
                            };
                            if ultima {
                                self.para_fundacao(origem, indice);
                            }
                            self.arraste = None;
                        } else {
                            let (px, py) = match origem {
                                Pilha::Coluna(c) => (g.x(c), self.y_na_coluna(&g, c, indice)),
                                p => g.posicao(p),
                            };
                            let px = if origem == Pilha::Descarte {
                                let n = self.mesa.descarte.len();
                                px + ((n - 1) as f32 - n.saturating_sub(3) as f32) * g.cl * 0.18
                            } else {
                                px
                            };
                            let cartas = match origem {
                                Pilha::Descarte => self.mesa.descarte[indice..].to_vec(),
                                Pilha::Fundacao(f) => self.mesa.fundacoes[f][indice..].to_vec(),
                                Pilha::Coluna(c) => self.mesa.colunas[c][indice..].iter().map(|(c, _)| *c).collect(),
                                Pilha::Monte => Vec::new(),
                            };
                            self.arraste = Some(Arraste {
                                origem,
                                indice,
                                cartas,
                                pega: (mx - px, my - py),
                                inicio: (mx, my),
                                andou: false,
                            });
                        }
                    }
                    None => {}
                }
            }
            if let Some(arr) = self.arraste.as_mut() {
                if !arr.andou && ((mx - arr.inicio.0).abs() > 4.0 || (my - arr.inicio.1).abs() > 4.0) {
                    arr.andou = true;
                }
            }
            // Desenha o que está sendo arrastado por cima de tudo.
            if let Some(arr) = self.arraste.as_ref().filter(|a| a.andou) {
                let (x0, y0) = (mx - arr.pega.0, my - arr.pega.1);
                for (k, c) in arr.cartas.iter().enumerate() {
                    desenhar_carta(ui.tela, *c, x0, y0 + k as f32 * g.aberta, g.cl, g.ca);
                }
            }
            if ui.soltou || (!ui.pressionado && self.arraste.is_some()) {
                if let Some(arr) = self.arraste.take() {
                    if arr.andou {
                        let (cx, cy) = (mx - arr.pega.0 + g.cl / 2.0, my - arr.pega.1 + g.ca / 2.0);
                        if let Some(dest) = self.destino_em(&g, cx, cy) {
                            self.mover(arr.origem, arr.indice, dest);
                        }
                    }
                }
                ui.redesenhar = true;
            }
        }

        // Vitória
        if let Some(t) = self.venceu {
            self.festa += 1.0;
            ui.acordar_em = Some(Duration::from_millis(40));
            for k in 0..12 {
                let fase = self.festa * 0.05 + k as f32 * 0.9;
                let px = (l / 2.0 + (fase * 1.3).sin() * l * 0.38).max(0.0);
                let py = (a * 0.55 - (fase * 2.1).sin().abs() * a * 0.4).max(60.0);
                let c = Carta { naipe: (k % 4) as u8, valor: 13 - (k % 3) as u8 };
                desenhar_carta(ui.tela, c, px - g.cl / 2.0, py, g.cl * 0.8, g.ca * 0.8);
            }
            let (bl, ba) = (380.0, 150.0);
            let (bx, by) = ((l - bl) / 2.0, (a - ba) / 2.0);
            ui.tela.retangulo(bx, by, bl, ba, 14.0, tema::BORDA());
            ui.tela.retangulo(bx + 1.0, by + 1.0, bl - 2.0, ba - 2.0, 13.0, tema::JANELA());
            let t1 = "Você venceu!";
            let lt = desenho::medir(22.0, t1);
            ui.tela.texto(bx + bl / 2.0 - lt / 2.0, by + 46.0, 22.0, tema::TEXTO(), t1);
            let t2 = format!("{} jogadas em {}:{:02}", self.mesa.jogadas, t.as_secs() / 60, t.as_secs() % 60);
            let lt = desenho::medir(13.0, &t2);
            ui.tela.texto(bx + bl / 2.0 - lt / 2.0, by + 74.0, 13.0, tema::TEXTO_SECUNDARIO(), &t2);
            let (nx, ny, nl) = (bx + bl / 2.0 - 70.0, by + 96.0, 140.0);
            let s = ui.sobre(nx, ny, nl, 36.0);
            ui.tela.retangulo(nx, ny, nl, 36.0, 8.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
            let lt = desenho::medir(13.5, "Jogar de novo");
            ui.tela.texto(nx + nl / 2.0 - lt / 2.0, ny + 23.0, 13.5, tema::BRANCO(), "Jogar de novo");
            if ui.clicou(nx, ny, nl, 36.0) {
                self.reiniciar();
            }
        } else {
            // O relógio da barra anda.
            ui.acordar_em = Some(Duration::from_millis(1000));
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Paciência dos Pássaros".into(),
            app_id: "paciencia".into(),
            tamanho: (980, 720),
            minimo: (640, 520),
        },
        Box::new(Jogo::novo()),
    );
}
