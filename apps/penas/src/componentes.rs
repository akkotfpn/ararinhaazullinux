// SPDX-License-Identifier: GPL-3.0-or-later
//! Componentes prontos das Penas: o mesmo botão, campo, caixa de marcar e
//! chave em todos os apps, nos dois temas.
//!
//! Medidas do sistema de design do Ararinha:
//! espaçamento 4/8/12/16/24, raios 6/8/12, texto 11,5/13/14/16/22,
//! botões e campos com 36 px de altura.

use crate::{desenho, tema, Tecla, Ui};

pub mod medida {
    pub const ESP_1: f32 = 4.0;
    pub const ESP_2: f32 = 8.0;
    pub const ESP_3: f32 = 12.0;
    pub const ESP_4: f32 = 16.0;
    pub const ESP_5: f32 = 24.0;
    pub const RAIO_P: f32 = 6.0;
    pub const RAIO: f32 = 8.0;
    pub const RAIO_G: f32 = 12.0;
    pub const TEXTO_P: f32 = 11.5;
    pub const TEXTO: f32 = 13.0;
    pub const TEXTO_M: f32 = 14.0;
    pub const SUBTITULO: f32 = 16.0;
    pub const TITULO: f32 = 22.0;
    pub const ALTURA: f32 = 36.0;
}
use medida::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Estilo {
    /// Ação principal (azul da ararinha).
    Principal,
    /// Ação comum (contorno).
    Comum,
    /// Ação que apaga ou não tem volta.
    Perigo,
}

/// Largura que um botão com esse rótulo ocupa.
pub fn largura_botao(rotulo: &str) -> f32 {
    desenho::medir(TEXTO, rotulo) + 2.0 * ESP_5
}

/// Botão. Devolve true no clique. `ativo = false` desenha apagado e ignora.
pub fn botao(ui: &mut Ui, x: f32, y: f32, rotulo: &str, estilo: Estilo, ativo: bool) -> bool {
    let w = largura_botao(rotulo);
    let s = ativo && ui.sobre(x, y, w, ALTURA);
    let apertado = s && ui.pressionado;
    let (fundo, texto, borda) = match (ativo, estilo) {
        (false, _) => (tema::SUPERFICIE_ALTA(), tema::TEXTO_SECUNDARIO(), None),
        (true, Estilo::Principal) => (
            if apertado || s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() },
            tema::BRANCO(),
            None,
        ),
        (true, Estilo::Perigo) => (tema::ERRO(), tema::BRANCO(), None),
        (true, Estilo::Comum) => (
            if apertado { tema::BORDA() } else if s { tema::SUPERFICIE_ALTA() } else { tema::JANELA() },
            tema::TEXTO(),
            Some(if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() }),
        ),
    };
    if let Some(b) = borda {
        ui.tela.retangulo(x, y, w, ALTURA, RAIO, b);
        ui.tela.retangulo(x + 1.0, y + 1.0, w - 2.0, ALTURA - 2.0, RAIO - 1.0, fundo);
    } else {
        ui.tela.retangulo(x, y, w, ALTURA, RAIO, fundo);
    }
    ui.tela.texto(x + ESP_5, y + 23.0, TEXTO, texto, rotulo);
    ativo && ui.clicou(x, y, w, ALTURA)
}

/// Botão alinhado pela direita (`x` é a borda direita). Útil em rodapés.
pub fn botao_direita(ui: &mut Ui, x: f32, y: f32, rotulo: &str, estilo: Estilo, ativo: bool) -> bool {
    botao(ui, x - largura_botao(rotulo), y, rotulo, estilo, ativo)
}

/// Caixa de marcar com rótulo. Inverte `marcado` no clique e devolve true.
pub fn caixa_marcar(ui: &mut Ui, x: f32, y: f32, rotulo: &str, marcado: &mut bool) -> bool {
    let w = 30.0 + desenho::medir(TEXTO, rotulo);
    let s = ui.sobre(x - 4.0, y - 4.0, w + 8.0, 28.0);
    let lado = 20.0;
    if *marcado {
        ui.tela.retangulo(x, y, lado, lado, RAIO_P - 2.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        ui.tela.linha(x + 5.0, y + 10.0, x + 9.0, y + 14.0, 2.0, tema::BRANCO());
        ui.tela.linha(x + 9.0, y + 14.0, x + 15.0, y + 6.0, 2.0, tema::BRANCO());
    } else {
        ui.tela.retangulo(x, y, lado, lado, RAIO_P - 2.0, if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() });
        ui.tela.retangulo(x + 1.5, y + 1.5, lado - 3.0, lado - 3.0, RAIO_P - 3.0, tema::JANELA());
    }
    ui.tela.texto(x + 30.0, y + 15.0, TEXTO, tema::TEXTO(), rotulo);
    if ui.clicou(x - 4.0, y - 4.0, w + 8.0, 28.0) {
        *marcado = !*marcado;
        return true;
    }
    false
}

/// Chave liga/desliga (estilo Windows 11). Devolve true quando muda.
pub fn chave(ui: &mut Ui, x: f32, y: f32, ligada: &mut bool) -> bool {
    let (w, a) = (40.0, 20.0);
    let s = ui.sobre(x, y, w, a);
    if *ligada {
        ui.tela.retangulo(x, y, w, a, a / 2.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        ui.tela.retangulo(x + w - 16.0, y + 4.0, 12.0, 12.0, 6.0, tema::BRANCO());
    } else {
        ui.tela.retangulo(x, y, w, a, a / 2.0, tema::TEXTO_SECUNDARIO());
        ui.tela.retangulo(x + 1.0, y + 1.0, w - 2.0, a - 2.0, a / 2.0 - 1.0, tema::JANELA());
        ui.tela.retangulo(x + 5.0, y + 5.0, 10.0, 10.0, 5.0, tema::TEXTO_SECUNDARIO());
    }
    if ui.clicou(x, y, w, a) {
        *ligada = !*ligada;
        return true;
    }
    false
}

/// Campo de texto de uma linha. `senha` mostra pontos. Desenha o foco e o
/// cursor; devolve true se foi clicado (o app passa o foco para ele).
/// O texto em si é editado por [`editar`], com as teclas do quadro.
pub fn campo(ui: &mut Ui, x: f32, y: f32, l: f32, texto: &str, dica: &str, foco: bool, senha: bool) -> bool {
    let s = ui.sobre(x, y, l, ALTURA);
    let borda = if foco { tema::DESTAQUE() } else if s { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() };
    ui.tela.retangulo(x, y, l, ALTURA, RAIO_P, borda);
    ui.tela.retangulo(x + 1.0, y + 1.0, l - 2.0, ALTURA - 2.0, RAIO_P - 1.0, tema::JANELA());
    if foco {
        // Linha de foco embaixo, como no Windows 11.
        ui.tela.retangulo(x + 1.0, y + ALTURA - 3.0, l - 2.0, 2.0, 1.0, tema::DESTAQUE());
    }
    // O que aparece (pontinhos na senha) e a janela visível ao redor do cursor.
    let chars: Vec<char> = if senha { vec!['•'; texto.chars().count()] } else { texto.chars().collect() };
    let n = chars.len();
    let pos = if foco { cursor_de(texto).min(n) } else { n };
    let livre = l - 2.0 * ESP_3 - 4.0;
    let largura = |a: usize, b: usize| desenho::medir(TEXTO_M, &chars[a..b].iter().collect::<String>());
    let mut ini = 0;
    while ini < pos && largura(ini, pos) > livre {
        ini += 1;
    }
    let mut fim = pos;
    while fim < n && largura(ini, fim + 1) <= livre {
        fim += 1;
    }
    let visivel: String = chars[ini..fim].iter().collect();
    if foco && selecionado(texto) {
        let w = desenho::medir(TEXTO_M, &visivel);
        ui.tela.retangulo(x + ESP_3 - 1.0, y + 8.0, w + 2.0, 20.0, 3.0, tema::DESTAQUE_SUAVE());
    }
    if texto.is_empty() && !dica.is_empty() {
        ui.tela.texto(x + ESP_3, y + 23.0, TEXTO, tema::TEXTO_SECUNDARIO(), dica);
    } else {
        ui.tela.texto(x + ESP_3, y + 23.0, TEXTO_M, tema::TEXTO(), &visivel);
    }
    if foco {
        let cx = if pos > ini { largura(ini, pos) } else { 0.0 };
        ui.tela.retangulo(x + ESP_3 + cx, y + 9.0, 1.5, 18.0, 0.0, tema::DESTAQUE());
    }
    let clicou = ui.clicou(x, y, l, ALTURA);
    // Clique no texto: o cursor vai para onde clicou.
    if clicou {
        if let Some((mx, _)) = ui.mouse {
            let alvo = mx - x - ESP_3;
            let mut p = ini;
            while p < fim && largura(ini, p + 1) - desenho::medir(TEXTO_M, &chars[p].to_string()) / 2.0 < alvo {
                p += 1;
            }
            definir_cursor(texto, p);
            SELECIONADO.with(|s| s.set(0));
        }
    }
    clicou
}

thread_local! {
    /// Campo com "tudo selecionado" (Ctrl+A): o endereço do texto dele.
    static SELECIONADO: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Cursor do campo em foco: (endereço do texto, posição em caracteres).
    /// Outro campo (outro endereço) começa com o cursor no fim.
    static CURSOR: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, usize::MAX)) };
}

fn selecionado(texto: &str) -> bool {
    !texto.is_empty() && SELECIONADO.with(|s| s.get()) == texto.as_ptr() as usize
}

/// Posição do cursor (em caracteres) de um campo desenhado à mão.
pub fn cursor(texto: &str) -> usize {
    cursor_de(texto)
}

/// Largura do texto até o cursor (para desenhar a barrinha).
pub fn x_do_cursor(texto: &str, tamanho: f32, senha: bool) -> f32 {
    let p = cursor_de(texto);
    let antes: String = if senha { "•".repeat(p) } else { texto.chars().take(p).collect() };
    desenho::medir(tamanho, &antes)
}

fn cursor_de(texto: &str) -> usize {
    let n = texto.chars().count();
    let (p, c) = CURSOR.with(|c| c.get());
    if p == texto.as_ptr() as usize { c.min(n) } else { n }
}

fn definir_cursor(texto: &str, pos: usize) {
    CURSOR.with(|c| c.set((texto.as_ptr() as usize, pos)));
}

/// Índice em bytes do caractere `c`.
fn byte_de(texto: &str, c: usize) -> usize {
    texto.char_indices().nth(c).map(|(b, _)| b).unwrap_or(texto.len())
}

/// Aplica as teclas de texto deste quadro ao campo em foco, como no
/// Windows: setas, Home/End e clique movem o cursor; Ctrl+setas pulam
/// palavras; Ctrl+A seleciona tudo; Ctrl+C/X/V copiam, recortam e colam;
/// Ctrl+Backspace apaga a palavra. Devolve as teclas que não são de edição
/// (Enter, Tab, Cima/Baixo...).
pub fn editar(texto: &mut String, teclas: Vec<Tecla>) -> Vec<Tecla> {
    use crate::Keysym;
    let mut resto = Vec::new();
    let mut sel = selecionado(texto);
    let mut pos = cursor_de(texto);
    let palavra_antes = |t: &str, p: usize| -> usize {
        let cs: Vec<char> = t.chars().collect();
        let mut q = p.min(cs.len());
        while q > 0 && cs[q - 1].is_whitespace() {
            q -= 1;
        }
        while q > 0 && !cs[q - 1].is_whitespace() {
            q -= 1;
        }
        q
    };
    let palavra_depois = |t: &str, p: usize| -> usize {
        let cs: Vec<char> = t.chars().collect();
        let mut q = p.min(cs.len());
        while q < cs.len() && !cs[q].is_whitespace() {
            q += 1;
        }
        while q < cs.len() && cs[q].is_whitespace() {
            q += 1;
        }
        q
    };
    for t in teclas {
        let n = texto.chars().count();
        match t {
            Tecla::Texto(s) | Tecla::Colado(s) => {
                if sel {
                    texto.clear();
                    pos = 0;
                    sel = false;
                }
                // Campo de uma linha: quebras viram espaço.
                let s = s.replace(['\n', '\r'], " ").replace('\t', " ");
                let b = byte_de(texto, pos);
                texto.insert_str(b, &s);
                pos += s.chars().count();
            }
            Tecla::Apagar => {
                if sel {
                    texto.clear();
                    pos = 0;
                    sel = false;
                } else if pos > 0 {
                    let b = byte_de(texto, pos - 1);
                    texto.remove(b);
                    pos -= 1;
                }
            }
            Tecla::Outra(Keysym::Delete) => {
                if sel {
                    texto.clear();
                    pos = 0;
                    sel = false;
                } else if pos < n {
                    let b = byte_de(texto, pos);
                    texto.remove(b);
                }
            }
            Tecla::Esquerda | Tecla::Voltar => {
                pos = if sel { 0 } else if crate::mods_ctrl() { palavra_antes(texto, pos) } else { pos.saturating_sub(1) };
                sel = false;
            }
            Tecla::Direita | Tecla::Avancar => {
                pos = if sel { n } else if crate::mods_ctrl() { palavra_depois(texto, pos) } else { (pos + 1).min(n) };
                sel = false;
            }
            Tecla::Outra(Keysym::Home) => {
                pos = 0;
                sel = false;
            }
            Tecla::Outra(Keysym::End) => {
                pos = n;
                sel = false;
            }
            Tecla::Outra(k) if k == Keysym::a || k == Keysym::A => sel = !texto.is_empty(),
            Tecla::Outra(k) if k == Keysym::c || k == Keysym::C => {
                if sel {
                    crate::copiar(texto);
                }
            }
            Tecla::Outra(k) if k == Keysym::x || k == Keysym::X => {
                if sel {
                    crate::copiar(texto);
                    texto.clear();
                    pos = 0;
                    sel = false;
                }
            }
            Tecla::Outra(k) if k == Keysym::v || k == Keysym::V => crate::pedir_colar(),
            // Ctrl+Backspace: apaga a palavra antes do cursor.
            Tecla::Outra(Keysym::BackSpace) => {
                if sel {
                    texto.clear();
                    pos = 0;
                    sel = false;
                } else {
                    let q = palavra_antes(texto, pos);
                    let (b0, b1) = (byte_de(texto, q), byte_de(texto, pos));
                    texto.replace_range(b0..b1, "");
                    pos = q;
                }
            }
            outra => {
                sel = false;
                resto.push(outra);
            }
        }
    }
    SELECIONADO.with(|s| s.set(if sel { texto.as_ptr() as usize } else { 0 }));
    definir_cursor(texto, pos);
    resto
}

/// Faixa de aviso (informação ou erro) com texto quebrado em linhas.
/// Devolve a altura ocupada.
pub fn aviso(ui: &mut Ui, x: f32, y: f32, l: f32, texto: &str, erro: bool) -> f32 {
    let linhas = quebrar(texto, TEXTO, l - 2.0 * ESP_4);
    let a = linhas.len() as f32 * 20.0 + 2.0 * ESP_3 + 4.0;
    let fundo = if erro { crate::Cor(194, 65, 59, 40) } else { tema::DESTAQUE_SUAVE() };
    ui.tela.retangulo(x, y, l, a, RAIO, fundo);
    ui.tela.retangulo(x, y, 3.0, a, 1.5, if erro { tema::ERRO() } else { tema::DESTAQUE() });
    for (i, linha) in linhas.iter().enumerate() {
        ui.tela.texto(x + ESP_4, y + ESP_3 + 16.0 + i as f32 * 20.0, TEXTO, tema::TEXTO(), linha);
    }
    a
}

/// Parágrafo com quebra por palavras. Devolve o y depois da última linha.
pub fn paragrafo(ui: &mut Ui, x: f32, y: f32, l: f32, texto: &str) -> f32 {
    let mut y = y;
    for linha in quebrar(texto, TEXTO_M - 0.5, l) {
        ui.tela.texto(x, y, TEXTO_M - 0.5, tema::TEXTO_SECUNDARIO(), &linha);
        y += 22.0;
    }
    y
}

/// Quebra um texto em linhas que cabem em `l`.
pub fn quebrar(texto: &str, tamanho: f32, l: f32) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut linha = String::new();
    for palavra in texto.split_whitespace() {
        let tentativa = if linha.is_empty() { palavra.to_string() } else { format!("{linha} {palavra}") };
        if desenho::medir(tamanho, &tentativa) > l && !linha.is_empty() {
            linhas.push(std::mem::replace(&mut linha, palavra.to_string()));
        } else {
            linha = tentativa;
        }
    }
    if !linha.is_empty() {
        linhas.push(linha);
    }
    linhas
}

/// Força da senha de 0 (vazia) a 4 (forte), só pelo tamanho e variedade.
pub fn forca_senha(s: &str) -> u8 {
    let n = s.chars().count();
    if n == 0 {
        return 0;
    }
    let classes = [
        s.chars().any(|c| c.is_lowercase()),
        s.chars().any(|c| c.is_uppercase()),
        s.chars().any(|c| c.is_ascii_digit()),
        s.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    match (n, classes) {
        (0..=7, _) => 1,
        (8..=11, 0..=2) => 2,
        (8..=11, _) | (12..=15, 0..=1) => 3,
        _ => 4,
    }
}

/// Barrinha de força da senha com legenda.
pub fn medidor_senha(ui: &mut Ui, x: f32, y: f32, l: f32, senha: &str) {
    let f = forca_senha(senha);
    let (cor, rot) = match f {
        0 => (tema::BORDA(), ""),
        1 => (tema::ERRO(), "Fraca: use pelo menos 8 caracteres"),
        2 => (crate::Cor::hex(0xD98E04), "Razoável"),
        3 => (tema::DESTAQUE(), "Boa"),
        _ => (crate::Cor::hex(0x2E9E5B), "Forte"),
    };
    let seg = (l - 3.0 * ESP_1) / 4.0;
    for i in 0..4 {
        let c = if (i as u8) < f { cor } else { tema::SUPERFICIE_ALTA() };
        ui.tela.retangulo(x + i as f32 * (seg + ESP_1), y, seg, 4.0, 2.0, c);
    }
    if !rot.is_empty() {
        ui.tela.texto(x, y + 20.0, TEXTO_P, tema::TEXTO_SECUNDARIO(), rot);
    }
}
