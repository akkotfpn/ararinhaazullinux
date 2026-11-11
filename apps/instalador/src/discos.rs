// SPDX-License-Identifier: GPL-3.0-or-later
//! Descobre os discos do computador e qual deles tem o sistema em uso.

use std::{fs, path::Path};

#[derive(Debug, Clone)]
pub struct Disco {
    /// Nome no /dev (sda, nvme0n1...).
    pub nome: String,
    pub bytes: u64,
    pub modelo: String,
    pub removivel: bool,
    /// USB, SATA, NVMe...
    pub conexao: String,
}

/// Disco (inteiro) onde está a raiz em uso, pelo /proc/self/mountinfo.
pub fn disco_do_sistema() -> Option<String> {
    let info = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let linha = info.lines().find(|l| l.split_whitespace().nth(4) == Some("/"))?;
    let majmin = linha.split_whitespace().nth(2)?;
    // /sys/dev/block/M:m -> .../block/sda/sda2
    let alvo = fs::read_link(format!("/sys/dev/block/{majmin}")).ok()?;
    let partes: Vec<String> = alvo.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let i = partes.iter().position(|p| p == "block")?;
    partes.get(i + 1).cloned()
}

fn ler(p: &str) -> String {
    fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Discos inteiros onde dá para instalar (sem o do sistema, CD, loop, zram).
pub fn listar() -> Vec<Disco> {
    let sistema = disco_do_sistema();
    let mut v = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/block") else { return v };
    for e in rd.flatten() {
        let nome = e.file_name().to_string_lossy().to_string();
        if ["loop", "ram", "zram", "sr", "fd", "dm-", "md"].iter().any(|p| nome.starts_with(p)) {
            continue;
        }
        if Some(&nome) == sistema.as_ref() {
            continue;
        }
        let setores: u64 = ler(&format!("/sys/block/{nome}/size")).parse().unwrap_or(0);
        if setores == 0 {
            continue;
        }
        let modelo = ler(&format!("/sys/block/{nome}/device/model"));
        let fabricante = ler(&format!("/sys/block/{nome}/device/vendor"));
        let modelo = format!("{fabricante} {modelo}").trim().to_string();
        let removivel = ler(&format!("/sys/block/{nome}/removable")) == "1";
        let caminho = fs::canonicalize(format!("/sys/block/{nome}")).unwrap_or_default();
        let c = caminho.to_string_lossy();
        let conexao = if nome.starts_with("nvme") {
            "NVMe"
        } else if c.contains("/usb") {
            "USB"
        } else if nome.starts_with("mmcblk") {
            "Cartão SD"
        } else if nome.starts_with("vd") || c.contains("virtio") {
            "Virtual"
        } else {
            "SATA"
        };
        v.push(Disco {
            nome,
            bytes: setores * 512,
            modelo: if modelo.is_empty() { "Disco".into() } else { modelo },
            removivel,
            conexao: conexao.into(),
        });
    }
    v.sort_by(|a, b| a.nome.cmp(&b.nome));
    v
}

/// Nome da partição n de um disco (sda1, nvme0n1p1, mmcblk0p1).
pub fn particao(disco: &str, n: u32) -> String {
    if disco.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("{disco}p{n}")
    } else {
        format!("{disco}{n}")
    }
}

pub fn existe(p: &str) -> bool {
    Path::new(p).exists()
}

pub fn tamanho_legivel(b: u64) -> String {
    let gb = b as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0).replace('.', ",")
    } else {
        format!("{:.0} GB", gb)
    }
}
