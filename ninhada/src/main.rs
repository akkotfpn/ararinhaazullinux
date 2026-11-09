// SPDX-License-Identifier: GPL-3.0-or-later
//! Ninhada — o sistema de build do Ararinha Azul Linux.
//!
//! Constrói o sistema inteiro a partir do código-fonte original de cada
//! projeto, no método Linux From Scratch, de forma automática e reproduzível.

mod ambiente;
mod construir;
mod fontes;
mod grafo;
mod hospedeiro;
mod receita;
mod toml;

use ambiente::Ambiente;
use receita::{Receita, Tipo};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const AJUDA: &str = "\
Ninhada — constrói o Ararinha Azul Linux a partir do código-fonte

USO
  ninhada <comando> [opções] [pacotes ou categorias]

COMANDOS
  listar               lista as receitas
  verificar            valida todas as receitas e as dependências
  info <pacote>        mostra os detalhes de uma receita
  plano [alvos]        mostra a ordem de construção
  fontes [alvos]       baixa e confere as fontes
  construir [alvos]    constrói os alvos e tudo de que dependem
  hospedeiro           confere se esta máquina tem o necessário para construir
  ajuda                mostra esta ajuda

OPÇÕES
  --arq <x86_64|aarch64>  arquitetura alvo (padrão: a da máquina)
  --repo <pasta>          raiz do repositório (padrão: procura a partir daqui)
  --saida <pasta>         onde gravar tudo (padrão: <repo>/saida)
  --nucleos <n>           tarefas paralelas (padrão: núcleos, limitado a 1 por GB de RAM)
  --travar                (fontes) grava o hash das fontes sem hash na trava
  --forcar                (construir) reconstrói mesmo se nada mudou
  --testar                (construir) roda os testes de cada pacote
  --sem-isolamento        (construir) não usa namespaces (não recomendado)

Sem alvos, plano/fontes/construir consideram todas as receitas.
Um alvo pode ser um pacote (glibc) ou uma categoria (toolchain).

Também aceita os comandos em inglês: list, check, plan, sources, build, host, help.
";

struct Args {
    comando: String,
    alvos: Vec<String>,
    arq: String,
    repo: Option<PathBuf>,
    saida: Option<PathBuf>,
    nucleos: usize,
    travar: bool,
    forcar: bool,
    testar: bool,
    isolar: bool,
}

fn ler_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        comando: String::new(),
        alvos: Vec::new(),
        arq: match std::env::consts::ARCH {
            "aarch64" => "aarch64".into(),
            _ => "x86_64".into(),
        },
        repo: None,
        saida: None,
        nucleos: hospedeiro::nucleos_seguros(),
        travar: false,
        forcar: false,
        testar: false,
        isolar: true,
    };
    while let Some(x) = it.next() {
        let mut valor = |nome: &str| it.next().ok_or_else(|| format!("{nome} precisa de um valor"));
        match x.as_str() {
            "--arq" | "--arch" => a.arq = valor("--arq")?,
            "--repo" => a.repo = Some(PathBuf::from(valor("--repo")?)),
            "--saida" | "--output" => a.saida = Some(PathBuf::from(valor("--saida")?)),
            "--nucleos" | "--jobs" => {
                a.nucleos = valor("--nucleos")?.parse().map_err(|_| "--nucleos precisa de um número")?
            }
            "--travar" | "--lock" => a.travar = true,
            "--forcar" | "--force" => a.forcar = true,
            "--testar" | "--test" => a.testar = true,
            "--sem-isolamento" | "--no-sandbox" => a.isolar = false,
            "-h" | "--help" | "--ajuda" => a.comando = "ajuda".into(),
            o if o.starts_with("--") => return Err(format!("opção desconhecida: {o}")),
            o if a.comando.is_empty() => a.comando = o.to_string(),
            o => a.alvos.push(o.to_string()),
        }
    }
    if !receita::ARQUITETURAS.contains(&a.arq.as_str()) {
        return Err(format!("arquitetura '{}' não suportada (use x86_64 ou aarch64)", a.arq));
    }
    a.comando = match a.comando.as_str() {
        "" | "help" => "ajuda".into(),
        "list" => "listar".into(),
        "check" => "verificar".into(),
        "plan" => "plano".into(),
        "sources" => "fontes".into(),
        "build" => "construir".into(),
        "host" => "hospedeiro".into(),
        o => o.into(),
    };
    Ok(a)
}

fn achar_repo(dado: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(r) = dado {
        return Ok(r);
    }
    let mut atual = std::env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if atual.join("receitas").is_dir() {
            return Ok(atual);
        }
        if !atual.pop() {
            return Err("não achei a pasta 'receitas/'. Use --repo <pasta>.".into());
        }
    }
}

fn carregar(repo: &Path) -> Result<BTreeMap<String, Receita>, String> {
    receita::carregar_todas(&repo.join("receitas")).map_err(|erros| erros.join("\n"))
}

fn rodar(a: Args) -> Result<(), String> {
    if a.comando == "ajuda" {
        print!("{AJUDA}");
        return Ok(());
    }
    if a.comando == "hospedeiro" {
        return hospedeiro::verificar();
    }
    let repo = achar_repo(a.repo.clone())?;
    let saida = a.saida.clone().unwrap_or_else(|| repo.join("saida"));
    let todas = carregar(&repo)?;
    let caminho_trava = repo.join("receitas").join("fontes.trava");

    match a.comando.as_str() {
        "listar" => {
            println!("{:<22} {:<12} {:<10} {:<9} resumo", "pacote", "versão", "categoria", "tipo");
            for r in todas.values() {
                let tipo = if r.tipo == Tipo::Toolchain { "toolchain" } else { "pacote" };
                println!("{:<22} {:<12} {:<10} {:<9} {}", r.nome, r.versao, r.categoria, tipo, r.resumo);
            }
            println!("\n{} receitas", todas.len());
        }
        "verificar" => {
            let mut erros = receita::validar(&todas);
            for arq in receita::ARQUITETURAS {
                if let Err(e) = grafo::plano(&[], &todas, arq) {
                    erros.push(format!("[{arq}] {e}"));
                }
            }
            if let Err(e) = fontes::Trava::abrir(&caminho_trava) {
                erros.push(e);
            }
            if erros.is_empty() {
                println!("✓ {} receitas válidas, sem ciclos, nas duas arquiteturas.", todas.len());
            } else {
                return Err(erros.join("\n"));
            }
        }
        "info" => {
            let nome = a.alvos.first().ok_or("diga qual pacote: ninhada info <pacote>")?;
            let r = todas.get(nome).ok_or_else(|| format!("pacote '{nome}' não existe"))?;
            let trava = fontes::Trava::abrir(&caminho_trava)?;
            println!("{} {} (revisão {})", r.nome, r.versao, r.revisao);
            println!("  {}", r.resumo);
            println!("  licença:      {}", r.licenca);
            println!("  categoria:    {}", r.categoria);
            println!("  arquiteturas: {}", r.arquiteturas.join(", "));
            println!("  constrói com: {}", r.dep_construcao.join(", "));
            println!("  roda com:     {}", r.dep_execucao.join(", "));
            for f in &r.fontes {
                let h = trava.hash_esperado(f).unwrap_or_else(|| "(sem hash — rode fontes --travar)".into());
                println!("  fonte:        {}\n                {h}", f.url);
            }
            println!("  receita:      {}", r.caminho.display());
        }
        "plano" => {
            let p = grafo::plano(&a.alvos, &todas, &a.arq)?;
            println!("Ordem de construção para {} ({} pacotes):", a.arq, p.len());
            for (i, n) in p.iter().enumerate() {
                println!("  {:>3}. {}", i + 1, todas[n].id());
            }
        }
        "fontes" => {
            let p = grafo::plano(&a.alvos, &todas, &a.arq)?;
            let mut trava = fontes::Trava::abrir(&caminho_trava)?;
            let amb = Ambiente::novo(&saida, &a.arq);
            let mut erros = Vec::new();
            for n in &p {
                println!("• {}", todas[n].id());
                if let Err(e) = fontes::obter_todas(&todas[n], &amb.fontes, &mut trava, a.travar) {
                    erros.push(e);
                }
            }
            if a.travar {
                trava.salvar()?;
                println!("Trava atualizada: {}", caminho_trava.display());
                println!("Revise os hashes novos contra os checksums oficiais antes do commit.");
            }
            if !erros.is_empty() {
                return Err(erros.join("\n"));
            }
            println!("✓ fontes conferidas");
        }
        "construir" => {
            let p = grafo::plano(&a.alvos, &todas, &a.arq)?;
            let mut trava = fontes::Trava::abrir(&caminho_trava)?;
            let amb = Ambiente::novo(&saida, &a.arq);
            let isolar = a.isolar && construir::isolamento_disponivel();
            if a.isolar && !isolar {
                eprintln!("aviso: namespaces indisponíveis; construindo sem isolamento de rede.");
            }
            let op = construir::Opcoes { forcar: a.forcar, testar: a.testar, isolar, nucleos: a.nucleos };
            println!("Construindo {} pacotes para {} em {}", p.len(), a.arq, amb.base.display());
            let mut impressoes: BTreeMap<String, String> = BTreeMap::new();
            for (i, n) in p.iter().enumerate() {
                let r = &todas[n];
                // O plano já vem em ordem: as dependências têm impressão calculada.
                let mut deps: Vec<String> = Vec::new();
                for d in r.dep_construcao.iter().chain(&r.dep_execucao) {
                    for x in receita::resolver_dep(d, &todas) {
                        if let Some(imp) = impressoes.get(&x) {
                            deps.push(format!("{x}={imp}"));
                        }
                    }
                }
                deps.sort();
                deps.dedup();
                let imp = construir::impressao(r, &trava, &a.arq, &deps.join(","))?;
                impressoes.insert(n.clone(), imp.clone());
                print!("[{}/{}] {} ... ", i + 1, p.len(), r.id());
                use std::io::Write;
                std::io::stdout().flush().ok();
                match construir::construir(r, &amb, &mut trava, &op, &imp)? {
                    construir::Resultado::JaEstavaPronto => println!("já pronto"),
                    construir::Resultado::Construido(Some(pena)) => {
                        println!("ok → {}", pena.file_name().unwrap_or_default().to_string_lossy())
                    }
                    construir::Resultado::Construido(None) => println!("ok"),
                }
            }
            println!("✓ concluído");
        }
        o => return Err(format!("comando desconhecido: {o}\n\n{AJUDA}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match ler_args().and_then(rodar) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}
