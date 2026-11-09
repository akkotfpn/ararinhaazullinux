// SPDX-License-Identifier: GPL-3.0-or-later
//! Confere a máquina hospedeira, como o `version-check.sh` do LFS (cap. 2).
//! A máquina só dá a partida: depois da toolchain cruzada, nada do
//! hospedeiro entra no sistema.

use std::path::Path;
use std::process::Command;

const PROGRAMAS: &[(&str, &str)] = &[
    ("bash", "executar as receitas"),
    ("make", "compilar"),
    ("gcc", "compilar a toolchain cruzada"),
    ("g++", "compilar o GCC"),
    ("bison", "glibc e binutils"),
    ("gawk", "glibc"),
    ("m4", "GMP e outros"),
    ("makeinfo", "documentação do binutils/gcc (pacote texinfo)"),
    ("perl", "vários scripts de build"),
    ("python3", "glibc"),
    ("patch", "aplicar patches"),
    ("tar", "extrair e empacotar"),
    ("xz", "fontes .tar.xz"),
    ("gzip", "fontes .tar.gz"),
    ("zstd", "pacotes .pena"),
    ("curl", "baixar as fontes"),
    ("sha256sum", "conferir as fontes"),
    ("unshare", "isolar as construções (util-linux)"),
    ("find", "listar arquivos"),
    ("diff", "testes de vários pacotes"),
];

fn existe(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
        .unwrap_or(false)
}

pub fn verificar() -> Result<(), String> {
    let mut faltando = Vec::new();
    for (prog, para) in PROGRAMAS {
        if existe(prog) {
            println!("  ✓ {prog}");
        } else {
            println!("  ✗ {prog:<10} necessário para {para}");
            faltando.push(*prog);
        }
    }
    // O LFS exige /bin/sh apontando para o bash.
    let sh = std::fs::read_link("/bin/sh").ok();
    if !sh.as_deref().is_some_and(|p| p.ends_with("bash") || p == Path::new("bash")) {
        println!("  ! /bin/sh não aponta para o bash (alguns pacotes falham assim)");
    }
    let isolamento = crate::construir::isolamento_disponivel();
    println!(
        "  {} namespaces sem privilégio (isolamento de rede das construções)",
        if isolamento { "✓" } else { "!" }
    );
    if let Ok(o) = Command::new("nproc").output() {
        println!("  núcleos: {}", String::from_utf8_lossy(&o.stdout).trim());
    }
    if faltando.is_empty() {
        println!("✓ hospedeiro pronto");
        Ok(())
    } else {
        Err(format!("faltam programas: {}", faltando.join(", ")))
    }
}
