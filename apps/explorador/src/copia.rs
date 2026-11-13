// SPDX-License-Identifier: GPL-3.0-or-later
//! Copiar e mover arquivos fora da janela (numa thread), com progresso.
//! Copiar de um servidor de rede pode levar minutos: a janela continua
//! respondendo e mostra "Copiando 3 de 10: foto.jpg (42%)".

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

/// Mensagem de progresso: começa com \u{1} (a janela só atualiza o aviso);
/// a última, sem o \u{1}, encerra a tarefa.
fn progresso(tx: &Sender<String>, msg: String) {
    let _ = tx.send(format!("\u{1}{msg}"));
}

/// Nome que ainda não existe na pasta ("foto - Cópia.jpg", "(3)"...).
pub fn nome_livre(dir: &Path, base: &str, sufixo: &str) -> PathBuf {
    let (raiz, ext) = match base.rsplit_once('.') {
        Some((r, e)) if !r.is_empty() && !base.ends_with('.') => (r.to_string(), format!(".{e}")),
        _ => (base.to_string(), String::new()),
    };
    let mut p = dir.join(base);
    let mut n = 1;
    while p.symlink_metadata().is_ok() {
        n += 1;
        let extra = if n == 2 && !sufixo.is_empty() { sufixo.to_string() } else { format!("{sufixo} ({n})") };
        p = dir.join(format!("{raiz}{extra}{ext}"));
    }
    p
}

fn tamanho_total(p: &Path) -> u64 {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::read_dir(p)
            .map(|d| d.flatten().map(|e| tamanho_total(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

struct Estado<'a> {
    tx: &'a Sender<String>,
    total: u64,
    feito: u64,
    ultimo: Instant,
    rotulo: String,
}

impl Estado<'_> {
    fn avancar(&mut self, n: u64) {
        self.feito += n;
        if self.ultimo.elapsed() >= Duration::from_millis(300) {
            self.ultimo = Instant::now();
            let pct = if self.total > 0 { (self.feito * 100 / self.total).min(100) } else { 0 };
            progresso(self.tx, format!("{} ({pct}%)", self.rotulo));
        }
    }
}

fn copiar_arquivo(de: &Path, para: &Path, est: &mut Estado) -> std::io::Result<()> {
    let mut a = fs::File::open(de)?;
    let mut b = fs::File::create(para)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = a.read(&mut buf)?;
        if n == 0 {
            break;
        }
        b.write_all(&buf[..n])?;
        est.avancar(n as u64);
    }
    if let Ok(m) = fs::metadata(de) {
        let _ = b.set_permissions(m.permissions());
        if let Ok(t) = m.modified() {
            let _ = b.set_modified(t);
        }
    }
    Ok(())
}

fn copiar_tudo(de: &Path, para: &Path, est: &mut Estado) -> std::io::Result<()> {
    let m = fs::symlink_metadata(de)?;
    if m.file_type().is_symlink() {
        let alvo = fs::read_link(de)?;
        std::os::unix::fs::symlink(alvo, para)
    } else if m.is_dir() {
        fs::create_dir(para)?;
        for e in fs::read_dir(de)?.flatten() {
            copiar_tudo(&e.path(), &para.join(e.file_name()), est)?;
        }
        Ok(())
    } else {
        copiar_arquivo(de, para, est)
    }
}

/// Copia (ou move) `origens` para a pasta `destino`, numa thread. A última
/// mensagem no canal é o resumo.
pub fn transferir(origens: Vec<PathBuf>, destino: PathBuf, mover: bool, tx: Sender<String>) {
    std::thread::spawn(move || {
        let verbo = if mover { "Movendo" } else { "Copiando" };
        progresso(&tx, format!("{verbo} {} item(ns)…", origens.len()));
        let mut feitos = 0;
        let mut erros: Vec<String> = Vec::new();
        let n = origens.len();
        for (i, o) in origens.iter().enumerate() {
            let nome = o.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if nome.is_empty() {
                continue;
            }
            // Não deixa pôr uma pasta dentro dela mesma.
            if destino.starts_with(o) {
                erros.push(format!("\"{nome}\" não pode ir para dentro dela mesma"));
                continue;
            }
            if mover && o.parent() == Some(destino.as_path()) {
                continue;
            }
            let alvo = nome_livre(&destino, &nome, if mover { "" } else { " - Cópia" });
            // Mover no mesmo disco é só renomear (instantâneo).
            if mover && fs::rename(o, &alvo).is_ok() {
                feitos += 1;
                continue;
            }
            let mut est = Estado {
                tx: &tx,
                total: tamanho_total(o),
                feito: 0,
                ultimo: Instant::now() - Duration::from_secs(1),
                rotulo: if n > 1 { format!("{verbo} {} de {n}: {nome}", i + 1) } else { format!("{verbo} \"{nome}\"") },
            };
            est.avancar(0);
            match copiar_tudo(o, &alvo, &mut est) {
                Ok(()) => {
                    if mover {
                        let r = if o.is_dir() { fs::remove_dir_all(o) } else { fs::remove_file(o) };
                        if let Err(e) = r {
                            erros.push(format!("\"{nome}\" foi copiado, mas não saiu da origem: {e}"));
                        }
                    }
                    feitos += 1;
                }
                Err(e) => {
                    // Não deixa um pedaço pela metade.
                    let _ = if alvo.is_dir() { fs::remove_dir_all(&alvo) } else { fs::remove_file(&alvo) };
                    erros.push(format!("\"{nome}\": {e}"));
                }
            }
        }
        let pasta = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
        let msg = if erros.is_empty() {
            format!("{feitos} item(ns) {} para \"{pasta}\".", if mover { "movidos" } else { "copiados" })
        } else if erros.len() == 1 {
            format!("Não deu: {}", erros[0])
        } else {
            format!("{} item(ns) não deram certo (o primeiro: {}).", erros.len(), erros[0])
        };
        let _ = tx.send(msg);
    });
}
