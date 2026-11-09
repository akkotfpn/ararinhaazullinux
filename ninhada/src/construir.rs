// SPDX-License-Identifier: GPL-3.0-or-later
//! Construção de uma receita: extrair, compilar, instalar e empacotar.

use crate::ambiente::Ambiente;
use crate::fontes::{self, Trava};
use crate::receita::{Receita, Tipo};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Opcoes {
    pub forcar: bool,
    pub testar: bool,
    pub isolar: bool,
    pub nucleos: usize,
}

pub enum Resultado {
    Construido(Option<PathBuf>),
    JaEstavaPronto,
}

/// Verifica se o kernel deixa criar namespaces sem privilégio. Usamos um
/// namespace de usuário (mesmo UID) com rede própria vazia: a construção não
/// consegue baixar nada escondido, o que reforça a reprodutibilidade.
pub fn isolamento_disponivel() -> bool {
    Command::new("unshare")
        .args(["--user", "--map-current-user", "--net", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn executar(cmd: &mut Command, oque: &str) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{oque}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{oque} falhou ({status})"))
    }
}

fn limpar_e_criar(p: &Path) -> Result<(), String> {
    if p.exists() {
        fs::remove_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Extrai `arquivo` em `dentro` e devolve a pasta de topo, se houver uma só.
fn extrair(arquivo: &Path, dentro: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dentro).map_err(|e| e.to_string())?;
    executar(
        Command::new("tar").arg("-xf").arg(arquivo).arg("-C").arg(dentro).arg("--no-same-owner"),
        &format!("extrair {}", arquivo.display()),
    )?;
    let entradas: Vec<_> = fs::read_dir(dentro)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    if entradas.len() == 1 && entradas[0].is_dir() {
        Ok(entradas[0].clone())
    } else {
        Ok(dentro.to_path_buf())
    }
}

/// Impressão digital: muda se a receita, qualquer fonte ou qualquer
/// dependência mudar (as impressões das dependências entram em `deps`).
/// Assim, corrigir o GCC reconstrói a glibc e tudo que veio depois.
pub fn impressao(r: &Receita, trava: &Trava, arq: &str, deps: &str) -> Result<String, String> {
    let mut s = format!("arq={arq}\ndeps={deps}\n{}\n", r.texto);
    for f in &r.fontes {
        s.push_str(&format!("{} {}\n", f.url, trava.hash_esperado(f).unwrap_or_default()));
    }
    let tmp = std::env::temp_dir().join(format!("ninhada-impressao-{}", std::process::id()));
    fs::write(&tmp, s).map_err(|e| e.to_string())?;
    let h = fontes::sha256(&tmp);
    let _ = fs::remove_file(&tmp);
    h
}

/// Onde os passos rodam.
enum Modo<'a> {
    /// No hospedeiro, com a toolchain cruzada; `isolar` = sem rede.
    Hospedeiro { isolar: bool },
    /// Dentro da raiz em construção (o "chroot" do LFS).
    Isolado { raiz: &'a Path },
}

/// Script que entra no ambiente isolado. Roda como "root" de um namespace de
/// usuário (sem privilégio nenhum na máquina real), monta /dev, /proc, /sys e
/// /run como no LFS 7.3 e faz o chroot. Tudo é desmontado sozinho quando o
/// namespace termina.
const ENTRAR_ISOLADO: &str = r#"
set -e
R="$1"; D="$2"; P="$3"; shift 3
mkdir -p "$R/dev" "$R/proc" "$R/sys" "$R/run" "$R/tmp"
mount --rbind /dev "$R/dev"
mount -t proc proc "$R/proc"
mount -t sysfs sysfs "$R/sys" 2>/dev/null || mount --rbind /sys "$R/sys"
mount -t tmpfs tmpfs "$R/run"
exec chroot "$R" /usr/bin/env -i "$@" /bin/bash -e -u -o pipefail -c 'cd "$1"; eval "$2"' _ "$D" "$P"
"#;

/// Caminho visto de dentro da raiz: /x/raiz/usr/src -> /usr/src
fn dentro(raiz: &Path, p: &Path) -> Result<String, String> {
    let rel = p
        .strip_prefix(raiz)
        .map_err(|_| format!("{} não está dentro da raiz", p.display()))?;
    Ok(format!("/{}", rel.display()))
}

fn rodar_passos(
    passos: &[String],
    etapa: &str,
    dir: &Path,
    vars: &[(String, String)],
    modo: &Modo,
    log: &Path,
) -> Result<(), String> {
    for (i, passo) in passos.iter().enumerate() {
        let mut registro = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        writeln!(registro, "\n### {etapa} {}/{}: {passo}", i + 1, passos.len()).ok();
        let saida = registro.try_clone().map_err(|e| e.to_string())?;

        let mut cmd = match modo {
            Modo::Hospedeiro { isolar: true } => {
                let mut c = Command::new("unshare");
                c.args(["--user", "--map-current-user", "--net", "--", "bash"]);
                c.args(["-e", "-u", "-o", "pipefail", "-c", passo]);
                c.current_dir(dir).env_clear().env("HOME", dir).env("TERM", "dumb");
                c.envs(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                c
            }
            Modo::Hospedeiro { isolar: false } => {
                let mut c = Command::new("bash");
                c.args(["-e", "-u", "-o", "pipefail", "-c", passo]);
                c.current_dir(dir).env_clear().env("HOME", dir).env("TERM", "dumb");
                c.envs(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                c
            }
            Modo::Isolado { raiz } => {
                let mut c = Command::new("unshare");
                c.args([
                    "--user", "--map-root-user", "--mount", "--pid", "--net", "--fork",
                    "--kill-child", "--", "bash", "-c", ENTRAR_ISOLADO, "_",
                ]);
                c.arg(raiz).arg(dentro(raiz, dir)?).arg(passo);
                c.args(vars.iter().map(|(k, v)| format!("{k}={v}")));
                // O próprio unshare/bash rodam no hospedeiro com PATH mínimo.
                c.env_clear().env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin");
                c
            }
        };
        cmd.stdin(Stdio::null()).stdout(saida).stderr(registro);
        let status = cmd.status().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("{etapa} {}/{} falhou: {passo}", i + 1, passos.len()));
        }
    }
    Ok(())
}

/// Para entrar numa raiz de outra arquitetura (ex.: aarch64 numa máquina
/// x86_64), o kernel precisa de um emulador registrado com a flag F.
pub fn pode_entrar(arq: &str) -> Result<(), String> {
    let nativa = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        _ => "x86_64",
    };
    if arq == nativa {
        return Ok(());
    }
    let reg = format!("/proc/sys/fs/binfmt_misc/qemu-{arq}");
    match fs::read_to_string(&reg) {
        Ok(t) if t.contains("enabled") && t.lines().any(|l| l.starts_with("flags:") && l.contains('F')) => Ok(()),
        Ok(_) => Err(format!("{reg} existe mas não está ativo com a flag F")),
        Err(_) => Err(format!(
            "para construir {arq} no ambiente isolado, esta máquina precisa do emulador: \
             sudo apt-get install -y qemu-user-binfmt"
        )),
    }
}

fn ultimas_linhas(log: &Path, n: usize) -> String {
    let texto = fs::read_to_string(log).unwrap_or_default();
    let linhas: Vec<&str> = texto.lines().collect();
    linhas[linhas.len().saturating_sub(n)..].join("\n")
}

/// Só `usr/` pode existir num pacote (ADR 0005/0006).
fn conferir_conteudo(destino: &Path) -> Result<(), String> {
    let mut proibidos = Vec::new();
    for e in fs::read_dir(destino).map_err(|e| e.to_string())?.flatten() {
        let nome = e.file_name().to_string_lossy().to_string();
        if nome != "usr" {
            proibidos.push(nome);
        }
    }
    if proibidos.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "o pacote instalou fora de /usr: {}. Pacotes só podem escrever em /usr \
             (configurações padrão vão em /usr/share/factory/etc).",
            proibidos.join(", ")
        ))
    }
}

fn empacotar(r: &Receita, amb: &Ambiente, destino: &Path, trava: &Trava) -> Result<PathBuf, String> {
    conferir_conteudo(destino)?;
    let meta = destino.join(".PENA");
    fs::create_dir_all(&meta).map_err(|e| e.to_string())?;

    // Lista de arquivos com hash e de links simbólicos com o alvo.
    let lista = Command::new("bash")
        .args([
            "-c",
            "set -euo pipefail; LC_ALL=C; \
             find . -path ./.PENA -prune -o -type f -print0 | sort -z | xargs -0 -r sha256sum | sed 's#  \\./#  /#'; \
             find . -path ./.PENA -prune -o -type l -printf 'link  /%P -> %l\\n' | sort",
        ])
        .current_dir(destino)
        .output()
        .map_err(|e| e.to_string())?;
    if !lista.status.success() {
        return Err("não consegui listar os arquivos do pacote".into());
    }
    fs::write(meta.join("arquivos.lista"), &lista.stdout).map_err(|e| e.to_string())?;

    let tamanho = Command::new("du")
        .args(["-sb", "--exclude=.PENA", "."])
        .current_dir(destino)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "0".into());

    let lista_toml = |v: &[String]| v.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(", ");
    let mut manifesto = format!(
        "# Gerado pela Ninhada. Não edite.\n[pacote]\nnome = \"{}\"\nversao = \"{}\"\nrevisao = {}\n\
         arquitetura = \"{}\"\nresumo = \"{}\"\nlicenca = \"{}\"\ntamanho_instalado = {}\n\n\
         [dependencias]\nexecucao = [{}]\n",
        r.nome,
        r.versao,
        r.revisao,
        amb.arq,
        r.resumo.replace('"', "\\\""),
        r.licenca,
        tamanho,
        lista_toml(&r.dep_execucao),
    );
    for f in &r.fontes {
        manifesto.push_str(&format!(
            "\n[[fonte]]\nurl = \"{}\"\nsha256 = \"{}\"\n",
            f.url,
            trava.hash_esperado(f).unwrap_or_default()
        ));
    }
    fs::write(meta.join("manifesto.toml"), manifesto).map_err(|e| e.to_string())?;

    fs::create_dir_all(amb.pacotes()).map_err(|e| e.to_string())?;
    let arquivo = amb.pacotes().join(format!("{}-{}.pena", r.id(), amb.arq));
    // Grava num temporário e só renomeia no fim: nunca sobra .pena pela metade.
    let tmp = arquivo.with_extension("pena.tmp");
    // Tar reproduzível: ordem fixa, datas e donos zerados.
    let r_tar = executar(
        Command::new("tar")
            .args([
                "--sort=name",
                "--mtime=@0",
                "--owner=0",
                "--group=0",
                "--numeric-owner",
                "--zstd",
                "-cf",
            ])
            .arg(&tmp)
            .arg("-C")
            .arg(destino)
            .arg("."),
        "empacotar",
    );
    if let Err(e) = r_tar {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &arquivo).map_err(|e| e.to_string())?;
    Ok(arquivo)
}

pub fn construir(
    r: &Receita,
    amb: &Ambiente,
    trava: &mut Trava,
    op: &Opcoes,
    impressao_atual: &str,
) -> Result<Resultado, String> {
    let carimbo = amb.estado().join(format!("{}.ok", r.nome));
    if !op.forcar && fs::read_to_string(&carimbo).ok().as_deref() == Some(impressao_atual) {
        return Ok(Resultado::JaEstavaPronto);
    }

    let fontes = fontes::obter_todas(r, &amb.fontes, trava, false)?;

    if r.isolado {
        pode_entrar(&amb.arq)?;
    }
    // No modo isolado o código precisa estar DENTRO da raiz para ser visto.
    let obra = if r.isolado {
        amb.raiz().join("ninhada-obra").join(&r.nome)
    } else {
        amb.obra(&r.nome)
    };
    limpar_e_criar(&obra)?;
    let mut codigo = obra.clone();
    let mut extras = Vec::new();
    let mut avulsos = Vec::new();
    for (i, (f, arquivo)) in fontes.iter().enumerate() {
        if !f.extrair {
            avulsos.push(arquivo.clone());
            continue;
        }
        match &f.destino {
            None => codigo = extrair(arquivo, &obra.join("codigo"))?,
            Some(d) => extras.push((d.clone(), extrair(arquivo, &obra.join(format!("extra-{i}")))?)),
        }
    }
    // Patches e afins ficam ao lado do código: `../nome.patch`.
    let ao_lado = codigo.parent().map(Path::to_path_buf).unwrap_or_else(|| obra.clone());
    for a in avulsos {
        let nome = a.file_name().ok_or("fonte sem nome de arquivo")?;
        fs::copy(&a, ao_lado.join(nome)).map_err(|e| format!("copiar {}: {e}", a.display()))?;
    }
    for (d, origem) in extras {
        let alvo = codigo.join(&d);
        if alvo.exists() {
            fs::remove_dir_all(&alvo).map_err(|e| e.to_string())?;
        }
        fs::rename(&origem, &alvo).map_err(|e| format!("mover {} para {d}: {e}", origem.display()))?;
    }
    let mut dir = match &r.diretorio {
        Some(d) => codigo.join(d),
        None => codigo,
    };
    if let Some(sub) = &r.construir_em {
        dir = dir.join(sub);
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    // Toolchain instala direto na raiz; pacote instala numa pasta própria.
    let destino = match r.tipo {
        Tipo::Toolchain => amb.raiz(),
        Tipo::Pacote => {
            let d = if r.isolado {
                amb.raiz().join("ninhada-destino").join(&r.nome)
            } else {
                amb.destino(&r.nome)
            };
            limpar_e_criar(&d)?;
            d
        }
    };
    fs::create_dir_all(amb.ferramentas()).map_err(|e| e.to_string())?;
    fs::create_dir_all(amb.registros()).map_err(|e| e.to_string())?;
    let log = amb.registros().join(format!("{}.log", r.nome));
    let _ = fs::remove_file(&log);
    let raiz = amb.raiz();
    let (vars, modo) = if r.isolado {
        (amb.variaveis_isoladas(&dentro(&raiz, &destino)?, op.nucleos), Modo::Isolado { raiz: &raiz })
    } else {
        (amb.variaveis(&destino, op.nucleos), Modo::Hospedeiro { isolar: op.isolar })
    };

    let mut etapas: Vec<(&str, &[String])> = vec![("construção", &r.passos)];
    if op.testar {
        etapas.push(("teste", &r.testar));
    }
    etapas.push(("instalação", &r.instalar));
    for (nome, passos) in etapas {
        if let Err(e) = rodar_passos(passos, nome, &dir, &vars, &modo, &log) {
            return Err(format!(
                "{e}\n--- últimas linhas de {} ---\n{}",
                log.display(),
                ultimas_linhas(&log, 30)
            ));
        }
    }

    let pacote = match r.tipo {
        Tipo::Toolchain => None,
        Tipo::Pacote => {
            let p = empacotar(r, amb, &destino, trava)?;
            // Instala na raiz para os próximos pacotes poderem usá-lo.
            fs::create_dir_all(amb.raiz()).map_err(|e| e.to_string())?;
            executar(
                Command::new("tar")
                    .arg("-xf")
                    .arg(&p)
                    .arg("-C")
                    .arg(amb.raiz())
                    .args(["--exclude=./.PENA", "--no-same-owner"]),
                "instalar na raiz",
            )?;
            Some(p)
        }
    };

    // Libera espaço: o código compilado não é mais necessário.
    let _ = fs::remove_dir_all(&obra);
    if r.isolado && r.tipo == Tipo::Pacote {
        let _ = fs::remove_dir_all(&destino);
    }
    fs::create_dir_all(amb.estado()).map_err(|e| e.to_string())?;
    fs::write(&carimbo, impressao_atual).map_err(|e| e.to_string())?;
    Ok(Resultado::Construido(pacote))
}
