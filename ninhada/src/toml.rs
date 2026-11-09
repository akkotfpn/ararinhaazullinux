// SPDX-License-Identifier: GPL-3.0-or-later
//! Leitor de um subconjunto de TOML, suficiente para as receitas.
//!
//! Suporta: comentários, `[tabela]`, `[a.b]`, `[[lista.de.tabelas]]`,
//! chaves simples, com aspas ou pontilhadas (`nome.pt-BR`), strings básicas
//! (`"..."`), literais (`'...'`), multilinha (`"""..."""`), inteiros,
//! booleanos e arrays (inclusive em várias linhas).
//!
//! Não suporta datas nem tabelas em linha (`{ a = 1 }`), que não usamos.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Valor {
    Texto(String),
    Inteiro(i64),
    Logico(bool),
    Lista(Vec<Valor>),
    Tabela(Tabela),
}

pub type Tabela = BTreeMap<String, Valor>;

#[derive(Debug, PartialEq)]
pub struct ErroToml {
    pub linha: usize,
    pub mensagem: String,
}

impl fmt::Display for ErroToml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "linha {}: {}", self.linha, self.mensagem)
    }
}

impl Valor {
    pub fn como_texto(&self) -> Option<&str> {
        match self {
            Valor::Texto(s) => Some(s),
            _ => None,
        }
    }
    pub fn como_inteiro(&self) -> Option<i64> {
        match self {
            Valor::Inteiro(i) => Some(*i),
            _ => None,
        }
    }
    pub fn como_tabela(&self) -> Option<&Tabela> {
        match self {
            Valor::Tabela(t) => Some(t),
            _ => None,
        }
    }
    #[allow(dead_code)]
    pub fn como_lista(&self) -> Option<&Vec<Valor>> {
        match self {
            Valor::Lista(l) => Some(l),
            _ => None,
        }
    }
}

struct Leitor<'a> {
    c: Vec<char>,
    i: usize,
    linha: usize,
    _orig: &'a str,
}

impl<'a> Leitor<'a> {
    fn erro<T>(&self, msg: impl Into<String>) -> Result<T, ErroToml> {
        Err(ErroToml { linha: self.linha, mensagem: msg.into() })
    }
    fn espiar(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn espiar_em(&self, n: usize) -> Option<char> {
        self.c.get(self.i + n).copied()
    }
    fn avancar(&mut self) -> Option<char> {
        let ch = self.c.get(self.i).copied();
        if let Some(c) = ch {
            self.i += 1;
            if c == '\n' {
                self.linha += 1;
            }
        }
        ch
    }
    fn pular_espacos(&mut self) {
        while matches!(self.espiar(), Some(' ') | Some('\t') | Some('\r')) {
            self.avancar();
        }
    }
    /// Pula espaços, quebras de linha e comentários.
    fn pular_tudo(&mut self) {
        loop {
            match self.espiar() {
                Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                    self.avancar();
                }
                Some('#') => self.pular_comentario(),
                _ => break,
            }
        }
    }
    fn pular_comentario(&mut self) {
        while let Some(c) = self.espiar() {
            if c == '\n' {
                break;
            }
            self.avancar();
        }
    }
    fn fim_de_linha(&mut self) -> Result<(), ErroToml> {
        self.pular_espacos();
        if self.espiar() == Some('#') {
            self.pular_comentario();
        }
        match self.espiar() {
            None | Some('\n') => Ok(()),
            Some(c) => self.erro(format!("caractere inesperado '{c}' no fim da linha")),
        }
    }

    fn chave_simples(&mut self) -> Result<String, ErroToml> {
        self.pular_espacos();
        match self.espiar() {
            Some('"') => {
                self.avancar();
                self.string_basica()
            }
            Some('\'') => {
                self.avancar();
                self.string_literal()
            }
            _ => {
                let mut s = String::new();
                while let Some(c) = self.espiar() {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        s.push(c);
                        self.avancar();
                    } else {
                        break;
                    }
                }
                if s.is_empty() {
                    return self.erro("chave esperada");
                }
                Ok(s)
            }
        }
    }

    fn chave(&mut self) -> Result<Vec<String>, ErroToml> {
        let mut partes = vec![self.chave_simples()?];
        loop {
            self.pular_espacos();
            if self.espiar() == Some('.') {
                self.avancar();
                partes.push(self.chave_simples()?);
            } else {
                break;
            }
        }
        Ok(partes)
    }

    fn string_basica(&mut self) -> Result<String, ErroToml> {
        let mut s = String::new();
        loop {
            match self.avancar() {
                None | Some('\n') => return self.erro("string sem aspas de fechamento"),
                Some('"') => return Ok(s),
                Some('\\') => s.push(self.escape()?),
                Some(c) => s.push(c),
            }
        }
    }

    fn escape(&mut self) -> Result<char, ErroToml> {
        match self.avancar() {
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('r') => Ok('\r'),
            Some('"') => Ok('"'),
            Some('\\') => Ok('\\'),
            Some(c) => self.erro(format!("escape desconhecido '\\{c}'")),
            None => self.erro("escape incompleto"),
        }
    }

    fn string_literal(&mut self) -> Result<String, ErroToml> {
        let mut s = String::new();
        loop {
            match self.avancar() {
                None | Some('\n') => return self.erro("string literal sem fechamento"),
                Some('\'') => return Ok(s),
                Some(c) => s.push(c),
            }
        }
    }

    fn resto_da_linha_vazio(&self) -> bool {
        let mut j = self.i;
        while let Some(c) = self.c.get(j) {
            match c {
                ' ' | '\t' | '\r' => j += 1,
                '\n' => return true,
                _ => return false,
            }
        }
        false
    }

    /// Texto literal multilinha (três aspas simples): sem escapes, útil para
    /// comandos com barras invertidas (sed '\(...\)\1').
    fn literal_multilinha(&mut self) -> Result<String, ErroToml> {
        if self.espiar() == Some('\n') {
            self.avancar();
        }
        let mut s = String::new();
        loop {
            if self.espiar() == Some('\'')
                && self.espiar_em(1) == Some('\'')
                && self.espiar_em(2) == Some('\'')
            {
                self.avancar();
                self.avancar();
                self.avancar();
                return Ok(s);
            }
            match self.avancar() {
                None => return self.erro("texto literal multilinha sem fechamento"),
                Some(c) => s.push(c),
            }
        }
    }

    fn string_multilinha(&mut self) -> Result<String, ErroToml> {
        // Já consumiu as três aspas de abertura. Quebra de linha logo após
        // a abertura é ignorada, como no TOML.
        if self.espiar() == Some('\n') {
            self.avancar();
        }
        let mut s = String::new();
        loop {
            if self.espiar() == Some('"')
                && self.espiar_em(1) == Some('"')
                && self.espiar_em(2) == Some('"')
            {
                self.avancar();
                self.avancar();
                self.avancar();
                return Ok(s);
            }
            match self.avancar() {
                None => return self.erro("string multilinha sem fechamento"),
                // Barra no fim da linha (como no TOML): junta com a próxima
                // linha, descartando quebras e espaços iniciais.
                Some('\\') if self.resto_da_linha_vazio() => {
                    while matches!(self.espiar(), Some(' ' | '\t' | '\r' | '\n')) {
                        self.avancar();
                    }
                }
                Some('\\') => s.push(self.escape()?),
                Some(c) => s.push(c),
            }
        }
    }

    fn valor(&mut self) -> Result<Valor, ErroToml> {
        self.pular_espacos();
        match self.espiar() {
            Some('"') => {
                if self.espiar_em(1) == Some('"') && self.espiar_em(2) == Some('"') {
                    self.avancar();
                    self.avancar();
                    self.avancar();
                    Ok(Valor::Texto(self.string_multilinha()?))
                } else {
                    self.avancar();
                    Ok(Valor::Texto(self.string_basica()?))
                }
            }
            Some('\'') => {
                if self.espiar_em(1) == Some('\'') && self.espiar_em(2) == Some('\'') {
                    self.avancar();
                    self.avancar();
                    self.avancar();
                    return Ok(Valor::Texto(self.literal_multilinha()?));
                }
                self.avancar();
                Ok(Valor::Texto(self.string_literal()?))
            }
            Some('[') => {
                self.avancar();
                let mut itens = Vec::new();
                loop {
                    self.pular_tudo();
                    if self.espiar() == Some(']') {
                        self.avancar();
                        break;
                    }
                    itens.push(self.valor()?);
                    self.pular_tudo();
                    match self.avancar() {
                        Some(',') => continue,
                        Some(']') => break,
                        _ => return self.erro("esperado ',' ou ']' na lista"),
                    }
                }
                Ok(Valor::Lista(itens))
            }
            Some('t') | Some('f') => {
                let mut p = String::new();
                while let Some(c) = self.espiar() {
                    if c.is_alphabetic() {
                        p.push(c);
                        self.avancar();
                    } else {
                        break;
                    }
                }
                match p.as_str() {
                    "true" => Ok(Valor::Logico(true)),
                    "false" => Ok(Valor::Logico(false)),
                    _ => self.erro(format!("valor desconhecido '{p}'")),
                }
            }
            Some(c) if c.is_ascii_digit() || c == '-' || c == '+' => {
                let mut p = String::new();
                while let Some(c) = self.espiar() {
                    if c.is_ascii_digit() || c == '-' || c == '+' || c == '_' {
                        if c != '_' {
                            p.push(c);
                        }
                        self.avancar();
                    } else {
                        break;
                    }
                }
                match p.parse::<i64>() {
                    Ok(n) => Ok(Valor::Inteiro(n)),
                    Err(_) => self.erro(format!("número inválido '{p}'")),
                }
            }
            Some(c) => self.erro(format!("valor inesperado começando com '{c}'")),
            None => self.erro("valor esperado"),
        }
    }
}

/// Navega até a tabela indicada pelo caminho, criando o que faltar.
/// Se encontrar uma lista de tabelas, entra no último elemento.
fn tabela_em<'t>(
    raiz: &'t mut Tabela,
    caminho: &[String],
    linha: usize,
) -> Result<&'t mut Tabela, ErroToml> {
    let mut atual = raiz;
    for parte in caminho {
        let entrada = atual
            .entry(parte.clone())
            .or_insert_with(|| Valor::Tabela(Tabela::new()));
        atual = match entrada {
            Valor::Tabela(t) => t,
            Valor::Lista(l) => match l.last_mut() {
                Some(Valor::Tabela(t)) => t,
                _ => {
                    return Err(ErroToml {
                        linha,
                        mensagem: format!("'{parte}' não é uma tabela"),
                    })
                }
            },
            _ => {
                return Err(ErroToml {
                    linha,
                    mensagem: format!("'{parte}' já tem um valor e não é uma tabela"),
                })
            }
        };
    }
    Ok(atual)
}

pub fn ler(texto: &str) -> Result<Tabela, ErroToml> {
    let mut l = Leitor { c: texto.chars().collect(), i: 0, linha: 1, _orig: texto };
    let mut raiz = Tabela::new();
    let mut atual: Vec<String> = Vec::new();

    loop {
        l.pular_tudo();
        match l.espiar() {
            None => break,
            Some('[') => {
                l.avancar();
                let lista = l.espiar() == Some('[');
                if lista {
                    l.avancar();
                }
                let caminho = l.chave()?;
                l.pular_espacos();
                if l.avancar() != Some(']') || (lista && l.avancar() != Some(']')) {
                    return l.erro("cabeçalho de tabela sem ']'");
                }
                l.fim_de_linha()?;
                if lista {
                    let (ultimo, pai) = caminho.split_last().ok_or(ErroToml {
                        linha: l.linha,
                        mensagem: "cabeçalho vazio".into(),
                    })?;
                    let t = tabela_em(&mut raiz, pai, l.linha)?;
                    let e = t.entry(ultimo.clone()).or_insert_with(|| Valor::Lista(Vec::new()));
                    match e {
                        Valor::Lista(v) => v.push(Valor::Tabela(Tabela::new())),
                        _ => return l.erro(format!("'{ultimo}' já existe e não é lista de tabelas")),
                    }
                } else {
                    tabela_em(&mut raiz, &caminho, l.linha)?;
                }
                atual = caminho;
            }
            Some(_) => {
                let chave = l.chave()?;
                l.pular_espacos();
                if l.avancar() != Some('=') {
                    return l.erro("esperado '=' depois da chave");
                }
                let valor = l.valor()?;
                l.fim_de_linha()?;
                let linha = l.linha;
                let t = tabela_em(&mut raiz, &atual, linha)?;
                let (ultimo, meio) = chave.split_last().ok_or(ErroToml {
                    linha,
                    mensagem: "chave vazia".into(),
                })?;
                let t = tabela_em(t, meio, linha)?;
                if t.contains_key(ultimo) {
                    return Err(ErroToml { linha, mensagem: format!("chave '{ultimo}' repetida") });
                }
                t.insert(ultimo.clone(), valor);
            }
        }
    }
    Ok(raiz)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_receita_completa() {
        let t = ler(r#"
# comentário
[pacote]
nome = "zlib"   # fim de linha
revisao = 2
ativo = true
titulo.pt-BR = "Zlib"

[[fonte]]
url = "https://a/b.tar.gz"
[[fonte]]
url = 'https://c/d.tar.xz'
destino = "mpfr"

[construcao]
passos = [
  "./configure --prefix=$PREFIXO",  # comentário
  "make",
]
script = """
echo "oi"
"""
"#)
        .unwrap();
        let p = t["pacote"].como_tabela().unwrap();
        assert_eq!(p["nome"], Valor::Texto("zlib".into()));
        assert_eq!(p["revisao"], Valor::Inteiro(2));
        assert_eq!(p["ativo"], Valor::Logico(true));
        let fontes = t["fonte"].como_lista().unwrap();
        assert_eq!(fontes.len(), 2);
        let f2 = fontes[1].como_tabela().unwrap();
        assert_eq!(f2["destino"].como_texto(), Some("mpfr"));
        let c = t["construcao"].como_tabela().unwrap();
        assert_eq!(c["passos"].como_lista().unwrap().len(), 2);
        assert_eq!(c["script"].como_texto(), Some("echo \"oi\"\n"));
    }

    #[test]
    fn literal_multilinha_mantem_barras() {
        let t = ler("a = '''\nsed 's@\\(x\\)@\\1@' f\n'''\n").unwrap();
        assert_eq!(t["a"].como_texto(), Some("sed 's@\\(x\\)@\\1@' f\n"));
    }

    #[test]
    fn barra_no_fim_da_linha_junta_linhas() {
        let t = ler("a = \"\"\"\n  ./configure --x \\\n      --y\n\"\"\"\n").unwrap();
        assert_eq!(t["a"].como_texto(), Some("  ./configure --x --y\n"));
    }

    #[test]
    fn nome_pontilhado_em_tabela_com_texto_da_erro() {
        // `nome` já é texto; `nome.pt-BR` não pode virar tabela dentro dele.
        let r = ler("[a]\nnome = \"x\"\nnome.pt-BR = \"y\"\n");
        assert!(r.is_err());
    }

    #[test]
    fn chave_repetida_da_erro() {
        let e = ler("[a]\nx = 1\nx = 2\n").unwrap_err();
        assert!(e.mensagem.contains("repetida"));
        assert_eq!(e.linha, 3);
    }

    #[test]
    fn string_sem_fechar_da_erro() {
        assert!(ler("a = \"abc\n").is_err());
    }
}
