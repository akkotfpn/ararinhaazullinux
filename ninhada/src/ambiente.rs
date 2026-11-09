// SPDX-License-Identifier: GPL-3.0-or-later
//! Pastas de trabalho e variáveis que as receitas recebem.

use std::path::{Path, PathBuf};

/// Onde a Ninhada trabalha para uma arquitetura.
#[derive(Debug, Clone)]
pub struct Ambiente {
    pub arq: String,
    /// Cache de fontes baixadas (compartilhado entre arquiteturas).
    pub fontes: PathBuf,
    /// Base de saída desta arquitetura: `saida/<arq>`.
    pub base: PathBuf,
}

impl Ambiente {
    pub fn novo(saida: &Path, arq: &str) -> Ambiente {
        Ambiente {
            arq: arq.to_string(),
            fontes: saida.join("fontes"),
            base: saida.join(arq),
        }
    }
    /// Sistema em construção (o "$LFS" do Linux From Scratch).
    pub fn raiz(&self) -> PathBuf {
        self.base.join("raiz")
    }
    /// Toolchain cruzada temporária (o "$LFS/tools").
    pub fn ferramentas(&self) -> PathBuf {
        self.raiz().join("ferramentas")
    }
    pub fn obra(&self, nome: &str) -> PathBuf {
        self.base.join("obra").join(nome)
    }
    pub fn destino(&self, nome: &str) -> PathBuf {
        self.base.join("destino").join(nome)
    }
    pub fn pacotes(&self) -> PathBuf {
        self.base.join("pacotes")
    }
    pub fn estado(&self) -> PathBuf {
        self.base.join("estado")
    }
    pub fn registros(&self) -> PathBuf {
        self.base.join("registros")
    }

    /// Triplet do alvo. "ararinha" no campo de fornecedor garante que a
    /// toolchain cruzada nunca se confunda com a do sistema hospedeiro.
    pub fn alvo(&self) -> String {
        format!("{}-ararinha-linux-gnu", self.arq)
    }
    /// Nome da arquitetura para o kernel (`make ARCH=...`).
    pub fn arq_kernel(&self) -> &'static str {
        match self.arq.as_str() {
            "aarch64" => "arm64",
            _ => "x86",
        }
    }
    /// Nome do carregador dinâmico da glibc.
    pub fn carregador(&self) -> &'static str {
        match self.arq.as_str() {
            "aarch64" => "ld-linux-aarch64.so.1",
            _ => "ld-linux-x86-64.so.2",
        }
    }

    pub fn variaveis(&self, destino: &Path, nucleos: usize) -> Vec<(String, String)> {
        let caminho_hospedeiro = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into());
        let v = |s: &str, p: &Path| (s.to_string(), p.display().to_string());
        vec![
            ("ARQ".into(), self.arq.clone()),
            ("ARQ_KERNEL".into(), self.arq_kernel().into()),
            ("ALVO".into(), self.alvo()),
            ("CARREGADOR".into(), self.carregador().into()),
            ("PREFIXO".into(), "/usr".into()),
            v("RAIZ", &self.raiz()),
            v("FERRAMENTAS", &self.ferramentas()),
            v("DESTINO", destino),
            v("FONTES", &self.fontes),
            ("NUCLEOS".into(), nucleos.to_string()),
            ("MAKEFLAGS".into(), format!("-j{nucleos}")),
            // Mesmo efeito do ambiente limpo do LFS: idioma neutro, toolchain
            // própria antes do hospedeiro no PATH.
            ("LC_ALL".into(), "POSIX".into()),
            ("PATH".into(), format!("{}/bin:{}", self.ferramentas().display(), caminho_hospedeiro)),
            ("CONFIG_SITE".into(), format!("{}/usr/share/config.site", self.raiz().display())),
        ]
    }
}
