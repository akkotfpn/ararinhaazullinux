// SPDX-License-Identifier: GPL-3.0-or-later
//! Modelo de receita e validação.

use crate::toml::{self, Tabela, Valor};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ARQUITETURAS: &[&str] = &["x86_64", "aarch64"];

#[derive(Debug, Clone, PartialEq)]
pub enum Tipo {
    /// Gera um pacote `.pena` e o instala na raiz do sistema em construção.
    Pacote,
    /// Instala direto na toolchain temporária ou na raiz (sem `.pena`).
    Toolchain,
}

#[derive(Debug, Clone)]
pub struct Fonte {
    pub url: String,
    pub sha256: Option<String>,
    /// Subpasta, dentro do código principal, para onde esta fonte é movida
    /// (ex.: gmp/mpfr/mpc dentro do gcc).
    pub destino: Option<String>,
    /// `false` para arquivos que não são pacotes compactados (ex.: patches):
    /// são apenas copiados ao lado do código, como no LFS (`../x.patch`).
    pub extrair: bool,
}

impl Fonte {
    pub fn nome_arquivo(&self) -> String {
        self.url.rsplit('/').next().unwrap_or("fonte").to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Receita {
    pub nome: String,
    pub versao: String,
    pub revisao: i64,
    pub resumo: String,
    pub licenca: String,
    pub categoria: String,
    pub tipo: Tipo,
    pub fontes: Vec<Fonte>,
    pub dep_construcao: Vec<String>,
    pub dep_execucao: Vec<String>,
    pub passos: Vec<String>,
    pub testar: Vec<String>,
    pub instalar: Vec<String>,
    pub diretorio: Option<String>,
    /// Subpasta criada dentro do código onde todos os passos rodam
    /// (ex.: "build", para `../configure`).
    pub construir_em: Option<String>,
    /// `true` = os passos rodam DENTRO da raiz em construção (o chroot do
    /// LFS, cap. 7 em diante), via namespaces de usuário, sem root real.
    pub isolado: bool,
    /// Arquivos ou pastas do repositório copiados para o lado do código
    /// (ex.: "kernel/config"), visíveis como ../config nos passos.
    pub copiar: Vec<String>,
    /// Cargo.lock do repositório (ex.: "plumagem/Cargo.lock"): cada crate
    /// listada vira uma fonte, travada pelo próprio checksum do Cargo.lock.
    pub crates: Option<String>,
    pub arquiteturas: Vec<String>,
    pub caminho: PathBuf,
    /// Conteúdo bruto, usado para calcular a "impressão digital" da receita.
    pub texto: String,
}

impl Receita {
    pub fn id(&self) -> String {
        format!("{}-{}-{}", self.nome, self.versao, self.revisao)
    }
    pub fn suporta(&self, arq: &str) -> bool {
        self.arquiteturas.iter().any(|a| a == arq)
    }
}

fn texto(t: &Tabela, chave: &str) -> Option<String> {
    t.get(chave).and_then(|v| v.como_texto()).map(str::to_string)
}

fn lista_textos(t: Option<&Tabela>, chave: &str) -> Result<Vec<String>, String> {
    let Some(t) = t else { return Ok(Vec::new()) };
    match t.get(chave) {
        None => Ok(Vec::new()),
        Some(Valor::Lista(l)) => l
            .iter()
            .map(|v| {
                v.como_texto()
                    .map(str::to_string)
                    .ok_or_else(|| format!("'{chave}' deve ser uma lista de textos"))
            })
            .collect(),
        Some(_) => Err(format!("'{chave}' deve ser uma lista")),
    }
}

fn ler_fonte(t: &Tabela) -> Result<Fonte, String> {
    let url = texto(t, "url").ok_or("fonte sem 'url'")?;
    let sha256 = texto(t, "sha256").filter(|s| !s.is_empty());
    let extrair = match t.get("extrair") {
        None => true,
        Some(Valor::Logico(b)) => *b,
        Some(_) => return Err("'extrair' deve ser true ou false".into()),
    };
    Ok(Fonte { url, sha256, destino: texto(t, "destino"), extrair })
}

pub fn de_texto(conteudo: &str, caminho: &Path) -> Result<Receita, String> {
    let t = toml::ler(conteudo).map_err(|e| e.to_string())?;
    let pacote = t
        .get("pacote")
        .and_then(|v| v.como_tabela())
        .ok_or("falta a seção [pacote]")?;
    let obrigatorio = |chave: &str| -> Result<String, String> {
        texto(pacote, chave)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("[pacote] sem '{chave}'"))
    };

    let tipo = match texto(pacote, "tipo").as_deref() {
        None | Some("pacote") => Tipo::Pacote,
        Some("toolchain") => Tipo::Toolchain,
        Some(o) => return Err(format!("tipo desconhecido '{o}' (use pacote ou toolchain)")),
    };

    let fontes = match t.get("fonte") {
        None => Vec::new(),
        Some(Valor::Tabela(f)) => vec![ler_fonte(f)?],
        Some(Valor::Lista(l)) => l
            .iter()
            .map(|v| v.como_tabela().ok_or("fonte inválida".to_string()).and_then(ler_fonte))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("'fonte' deve ser [fonte] ou [[fonte]]".into()),
    };

    let dep = t.get("dependencias").and_then(|v| v.como_tabela());
    let cons = t.get("construcao").and_then(|v| v.como_tabela());
    let arqs = t.get("arquiteturas").and_then(|v| v.como_tabela());

    Ok(Receita {
        nome: obrigatorio("nome")?,
        versao: obrigatorio("versao")?,
        revisao: pacote.get("revisao").and_then(|v| v.como_inteiro()).unwrap_or(1),
        resumo: obrigatorio("resumo")?,
        licenca: obrigatorio("licenca")?,
        categoria: obrigatorio("categoria")?,
        tipo,
        fontes,
        dep_construcao: lista_textos(dep, "construcao")?,
        dep_execucao: lista_textos(dep, "execucao")?,
        passos: lista_textos(cons, "passos")?,
        testar: lista_textos(cons, "testar")?,
        instalar: lista_textos(cons, "instalar")?,
        diretorio: cons.and_then(|c| texto(c, "diretorio")),
        construir_em: cons.and_then(|c| texto(c, "construir_em")),
        copiar: lista_textos(cons, "copiar")?,
        crates: cons.and_then(|c| texto(c, "crates")),
        isolado: match cons.and_then(|c| texto(c, "ambiente")).as_deref() {
            None | Some("hospedeiro") => false,
            Some("isolado") => true,
            Some(o) => return Err(format!("ambiente desconhecido '{o}' (use hospedeiro ou isolado)")),
        },
        arquiteturas: {
            let a = lista_textos(arqs, "suportadas")?;
            if a.is_empty() {
                ARQUITETURAS.iter().map(|s| s.to_string()).collect()
            } else {
                a
            }
        },
        caminho: caminho.to_path_buf(),
        texto: conteudo.to_string(),
    })
}

pub fn carregar(caminho: &Path) -> Result<Receita, String> {
    let conteudo = fs::read_to_string(caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
    de_texto(&conteudo, caminho).map_err(|e| format!("{}: {e}", caminho.display()))
}

/// Lê um Cargo.lock e devolve uma fonte por crate do crates.io, com o
/// SHA-256 que o próprio Cargo.lock registra (é o hash do arquivo `.crate`).
/// Pacotes locais (sem `source`) ficam de fora: vêm do repositório.
pub fn fontes_de_cargo_lock(texto: &str) -> Result<Vec<Fonte>, String> {
    const REGISTRO: &str = "registry+https://github.com/rust-lang/crates.io-index";
    let mut fontes = Vec::new();
    for bloco in texto.split("[[package]]").skip(1) {
        let campo = |nome: &str| {
            bloco.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == nome).then(|| v.trim().trim_matches('"').to_string())
            })
        };
        let (Some(nome), Some(versao)) = (campo("name"), campo("version")) else {
            return Err("Cargo.lock: pacote sem name ou version".into());
        };
        match campo("source") {
            None => continue,
            Some(s) if s == REGISTRO => {}
            Some(s) => return Err(format!("Cargo.lock: {nome} vem de fonte não suportada ({s})")),
        }
        let sha = campo("checksum").ok_or_else(|| format!("Cargo.lock: {nome} {versao} sem checksum"))?;
        if !hex64(&sha) {
            return Err(format!("Cargo.lock: checksum inválido em {nome} {versao}"));
        }
        fontes.push(Fonte {
            url: format!("https://static.crates.io/crates/{nome}/{nome}-{versao}.crate"),
            sha256: Some(sha),
            destino: None,
            extrair: false,
        });
    }
    Ok(fontes)
}

/// Acrescenta às receitas com `crates = "..."` as fontes do Cargo.lock.
pub fn expandir_crates(todas: &mut BTreeMap<String, Receita>, repo: &Path) -> Result<(), String> {
    for r in todas.values_mut() {
        let Some(lock) = &r.crates else { continue };
        let caminho = repo.join(lock);
        let texto = fs::read_to_string(&caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
        let fontes = fontes_de_cargo_lock(&texto).map_err(|e| format!("{}: {e}", r.nome))?;
        r.fontes.extend(fontes);
    }
    Ok(())
}

/// Procura todos os `receita.toml` dentro de `raiz`.
pub fn carregar_todas(raiz: &Path) -> Result<BTreeMap<String, Receita>, Vec<String>> {
    let mut arquivos = Vec::new();
    procurar(raiz, &mut arquivos);
    arquivos.sort();
    let mut receitas = BTreeMap::new();
    let mut erros = Vec::new();
    for arq in arquivos {
        match carregar(&arq) {
            Ok(r) => {
                if let Some(anterior) = receitas.get(&r.nome) {
                    let a: &Receita = anterior;
                    erros.push(format!(
                        "pacote '{}' definido duas vezes: {} e {}",
                        r.nome,
                        a.caminho.display(),
                        r.caminho.display()
                    ));
                } else {
                    receitas.insert(r.nome.clone(), r);
                }
            }
            Err(e) => erros.push(e),
        }
    }
    if erros.is_empty() {
        Ok(receitas)
    } else {
        Err(erros)
    }
}

fn procurar(dir: &Path, saida: &mut Vec<PathBuf>) {
    let Ok(entradas) = fs::read_dir(dir) else { return };
    for e in entradas.flatten() {
        let p = e.path();
        if p.is_dir() {
            procurar(&p, saida);
        } else if p.file_name().is_some_and(|n| n == "receita.toml") {
            saida.push(p);
        }
    }
}

/// Resolve uma dependência: nome de pacote ou nome de categoria
/// (ex.: "toolchain" = todos os pacotes dessa categoria).
pub fn resolver_dep(nome: &str, todas: &BTreeMap<String, Receita>) -> Vec<String> {
    if todas.contains_key(nome) {
        return vec![nome.to_string()];
    }
    todas
        .values()
        .filter(|r| r.categoria == nome)
        .map(|r| r.nome.clone())
        .collect()
}

fn hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Valida cada receita e as referências entre elas.
pub fn validar(todas: &BTreeMap<String, Receita>) -> Vec<String> {
    let mut erros = Vec::new();
    for r in todas.values() {
        let onde = r.caminho.display();
        let pasta = r.caminho.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
        if pasta != Some(r.nome.as_str()) {
            erros.push(format!("{onde}: a pasta deve se chamar '{}'", r.nome));
        }
        if !r.nome.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '+' || c == '.') {
            erros.push(format!("{onde}: nome '{}' deve ter só minúsculas, números, '-', '+' ou '.'", r.nome));
        }
        if r.revisao < 1 {
            erros.push(format!("{onde}: revisão deve ser 1 ou maior"));
        }
        for a in &r.arquiteturas {
            if !ARQUITETURAS.contains(&a.as_str()) {
                erros.push(format!("{onde}: arquitetura desconhecida '{a}'"));
            }
        }
        for f in &r.fontes {
            if !(f.url.starts_with("https://") || f.url.starts_with("file://")) {
                erros.push(format!("{onde}: fonte deve usar https:// ({})", f.url));
            }
            if let Some(h) = &f.sha256 {
                if !hex64(h) {
                    erros.push(format!("{onde}: sha256 inválido para {}", f.url));
                }
            }
        }
        let destinos: Vec<_> = r.fontes.iter().filter(|f| f.destino.is_none() && f.extrair).collect();
        // Receitas só com fontes avulsas (crates de um Cargo.lock cujo código
        // vem do repositório, arquivos .zip abertos nos passos) também valem.
        let so_avulsas = r.fontes.iter().all(|f| !f.extrair);
        if !r.fontes.is_empty() && destinos.len() != 1 && !so_avulsas {
            erros.push(format!("{onde}: deve haver exatamente uma fonte principal (sem 'destino')"));
        }
        if r.passos.is_empty() && r.instalar.is_empty() {
            erros.push(format!("{onde}: receita sem passos de construção nem de instalação"));
        }
        for d in r.dep_construcao.iter().chain(&r.dep_execucao) {
            let resolvidas = resolver_dep(d, todas);
            if resolvidas.is_empty() {
                erros.push(format!("{onde}: dependência '{d}' não existe (nem como pacote, nem como categoria)"));
            }
            if resolvidas.iter().any(|x| x == &r.nome) {
                erros.push(format!("{onde}: '{}' depende de si mesmo", r.nome));
            }
        }
    }
    erros
}

#[cfg(test)]
mod testes {
    use super::*;

    const ZLIB: &str = r#"
[pacote]
nome = "zlib"
versao = "1.3.1"
resumo = "Compressão"
licenca = "Zlib"
categoria = "base"
[fonte]
url = "https://zlib.net/zlib-1.3.1.tar.gz"
[construcao]
passos = ["make"]
"#;

    #[test]
    fn le_receita_minima() {
        let r = de_texto(ZLIB, Path::new("receitas/base/zlib/receita.toml")).unwrap();
        assert_eq!(r.id(), "zlib-1.3.1-1");
        assert_eq!(r.tipo, Tipo::Pacote);
        assert_eq!(r.fontes[0].nome_arquivo(), "zlib-1.3.1.tar.gz");
        assert!(r.suporta("aarch64"));
    }

    #[test]
    fn valida_pasta_e_dependencias() {
        let mut r = de_texto(ZLIB, Path::new("receitas/base/errado/receita.toml")).unwrap();
        r.dep_construcao.push("inexistente".into());
        let mut todas = BTreeMap::new();
        todas.insert(r.nome.clone(), r);
        let erros = validar(&todas);
        assert!(erros.iter().any(|e| e.contains("pasta")));
        assert!(erros.iter().any(|e| e.contains("inexistente")));
    }

    #[test]
    fn le_cargo_lock() {
        let lock = r#"version = 4

[[package]]
name = "plumagem"
version = "0.1.0"

[[package]]
name = "bitflags"
version = "2.9.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "1b8e56985ec62d17e9c1001dc89c88ecd7dc08e47eba5ec7c29c7b5eeecde967"
"#;
        let f = fontes_de_cargo_lock(lock).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].nome_arquivo(), "bitflags-2.9.1.crate");
        assert!(!f[0].extrair);
    }

    #[test]
    fn falta_campo_obrigatorio() {
        let e = de_texto("[pacote]\nnome = \"x\"\n", Path::new("x/receita.toml")).unwrap_err();
        assert!(e.contains("versao"));
    }
}
