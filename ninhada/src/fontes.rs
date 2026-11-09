// SPDX-License-Identifier: GPL-3.0-or-later
//! Download e verificação de código-fonte.
//!
//! Regra: nenhuma fonte é usada sem SHA-256 conferido. O hash vem da própria
//! receita (`sha256 = "..."`) ou do arquivo de trava `receitas/fontes.trava`.
//! `ninhada fontes --travar` grava na trava o hash das fontes ainda sem hash;
//! essas linhas devem ser revisadas contra o checksum ou a assinatura
//! publicada pelo projeto original antes do commit.

use crate::receita::{Fonte, Receita};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Trava {
    caminho: PathBuf,
    /// url → sha256
    pub hashes: BTreeMap<String, String>,
}

impl Trava {
    pub fn abrir(caminho: &Path) -> Result<Trava, String> {
        let mut hashes = BTreeMap::new();
        if let Ok(texto) = fs::read_to_string(caminho) {
            for (n, linha) in texto.lines().enumerate() {
                let linha = linha.trim();
                if linha.is_empty() || linha.starts_with('#') {
                    continue;
                }
                let mut partes = linha.split_whitespace();
                match (partes.next(), partes.next(), partes.next()) {
                    (Some(h), Some(u), None) if h.len() == 64 => {
                        hashes.insert(u.to_string(), h.to_string());
                    }
                    _ => {
                        return Err(format!("{}:{}: linha inválida", caminho.display(), n + 1));
                    }
                }
            }
        }
        Ok(Trava { caminho: caminho.to_path_buf(), hashes })
    }

    pub fn salvar(&self) -> Result<(), String> {
        let mut s = String::from(
            "# Hashes SHA-256 das fontes, gerados por `ninhada fontes --travar`.\n\
             # Revise cada linha nova contra o checksum ou a assinatura oficial\n\
             # do projeto original antes de fazer commit.\n",
        );
        for (url, h) in &self.hashes {
            s.push_str(&format!("{h}  {url}\n"));
        }
        // Escrita atômica: temporário + renomear (OSTEP, crash consistency).
        let tmp = self.caminho.with_extension("trava.tmp");
        fs::write(&tmp, s).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.caminho).map_err(|e| e.to_string())
    }

    pub fn hash_esperado(&self, f: &Fonte) -> Option<String> {
        f.sha256.clone().or_else(|| self.hashes.get(&f.url).cloned())
    }
}

pub fn sha256(arquivo: &Path) -> Result<String, String> {
    let saida = Command::new("sha256sum")
        .arg(arquivo)
        .output()
        .map_err(|e| format!("não consegui executar sha256sum: {e}"))?;
    if !saida.status.success() {
        return Err(format!("sha256sum falhou em {}", arquivo.display()));
    }
    let texto = String::from_utf8_lossy(&saida.stdout);
    texto
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| "saída inesperada do sha256sum".into())
}

fn baixar(url: &str, destino: &Path) -> Result<(), String> {
    let tmp = destino.with_extension("baixando");
    let status = Command::new("curl")
        .args(["-fL", "--retry", "3", "--proto", "=https,file", "-sS", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .map_err(|e| format!("não consegui executar curl: {e}"))?;
    if !status.success() {
        let _ = fs::remove_file(&tmp);
        return Err(format!("falha ao baixar {url}"));
    }
    fs::rename(&tmp, destino).map_err(|e| e.to_string())
}

/// Garante que a fonte está no cache e com o hash certo. Devolve o caminho.
pub fn obter(f: &Fonte, cache: &Path, trava: &mut Trava, travar: bool) -> Result<PathBuf, String> {
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let arquivo = cache.join(f.nome_arquivo());
    let esperado = trava.hash_esperado(f);
    if esperado.is_none() && !travar {
        return Err(format!(
            "{} não tem hash. Rode `ninhada fontes --travar` e revise o resultado.",
            f.url
        ));
    }
    if !arquivo.exists() {
        eprintln!("  baixando {}", f.url);
        baixar(&f.url, &arquivo)?;
    }
    let obtido = sha256(&arquivo)?;
    match esperado {
        Some(h) if h == obtido => Ok(arquivo),
        Some(h) => {
            // Não apaga: deixa o arquivo para investigação.
            Err(format!(
                "HASH NÃO CONFERE para {}\n  esperado: {h}\n  obtido:   {obtido}\n  arquivo:  {}",
                f.url,
                arquivo.display()
            ))
        }
        None => {
            eprintln!("  travado {obtido}  {}", f.url);
            trava.hashes.insert(f.url.clone(), obtido);
            Ok(arquivo)
        }
    }
}

pub fn obter_todas(
    r: &Receita,
    cache: &Path,
    trava: &mut Trava,
    travar: bool,
) -> Result<Vec<(Fonte, PathBuf)>, String> {
    r.fontes
        .iter()
        .map(|f| obter(f, cache, trava, travar).map(|p| (f.clone(), p)))
        .collect()
}
