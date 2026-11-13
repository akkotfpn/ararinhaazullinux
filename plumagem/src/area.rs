// SPDX-License-Identifier: GPL-3.0-or-later
//! Histórico da área de transferência (Win+V).
//!
//! Quando um app copia texto, a Plumagem pede uma cópia do conteúdo (por um
//! pipe, como qualquer app que cola) e guarda na lista. Escolher um item da
//! lista faz da Plumagem a dona da área de transferência com aquele texto;
//! os apps que colarem recebem o texto por `enviar`.
//!
//! Só texto, até 25 itens e 256 KB cada; nada vai para o disco (sai ao
//! encerrar a sessão, como no Windows com a sincronização desligada).

use std::{
    io::{Read, Write},
    os::fd::{FromRawFd, OwnedFd},
    sync::Mutex,
};

pub const MAX_ITENS: usize = 25;
const MAX_BYTES: usize = 256 * 1024;

/// Mais recente primeiro.
pub static HISTORICO: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Texto que a própria Plumagem está oferecendo (item escolhido no Win+V).
pub static PROPRIA: Mutex<Option<String>> = Mutex::new(None);
/// Muda a cada item novo (para redesenhar o painel).
pub static VERSAO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub const TIPOS_TEXTO: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];

extern "C" {
    fn pipe2(fds: *mut i32, flags: i32) -> i32;
}
const O_CLOEXEC: i32 = 0o2000000;

/// Um pipe: (leitura, escrita).
pub fn cano() -> Option<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` tem espaço para os dois descritores.
    if unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC) } != 0 {
        return None;
    }
    // SAFETY: os descritores acabaram de ser criados e são nossos.
    Some(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// Melhor tipo de texto entre os oferecidos.
pub fn tipo_texto(tipos: &[String]) -> Option<String> {
    TIPOS_TEXTO.iter().find(|t| tipos.iter().any(|m| m == *t)).map(|t| t.to_string())
}

/// Lê o texto do pipe numa thread e guarda no histórico.
pub fn ler_em_segundo_plano(leitura: OwnedFd) {
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(leitura);
        let mut b = Vec::new();
        let _ = (&mut f).take(MAX_BYTES as u64 + 1).read_to_end(&mut b);
        if b.is_empty() || b.len() > MAX_BYTES {
            return;
        }
        let Ok(t) = String::from_utf8(b) else { return };
        if t.trim().is_empty() {
            return;
        }
        guardar(t);
    });
}

pub fn guardar(t: String) {
    // Configurações › Privacidade: histórico desligado.
    if crate::painel::ler_chave("historico_area").as_deref() == Some("nao") {
        return;
    }
    if let Ok(mut h) = HISTORICO.lock() {
        h.retain(|x| x != &t);
        h.insert(0, t);
        h.truncate(MAX_ITENS);
    }
    VERSAO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn itens() -> Vec<String> {
    HISTORICO.lock().map(|h| h.clone()).unwrap_or_default()
}

pub fn limpar() {
    if let Ok(mut h) = HISTORICO.lock() {
        h.clear();
    }
    VERSAO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Se a Plumagem é a dona da área de transferência, escreve o texto no fd
/// de quem pediu (numa thread: o app pode demorar para ler). Devolve false
/// se não for (o pedido segue para o Xwayland).
pub fn enviar(fd: OwnedFd) -> bool {
    let Some(t) = PROPRIA.lock().ok().and_then(|p| p.clone()) else { return false };
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(fd);
        let _ = f.write_all(t.as_bytes());
    });
    true
}

// ------------------------------------------------------------ arquivos

/// Tipos que trazem arquivos copiados (Explorador, Nautilus, Dolphin...).
pub const TIPOS_ARQUIVOS: [&str; 2] = ["x-special/gnome-copied-files", "text/uri-list"];

/// Arquivos na área de transferência agora (de qualquer app): caminhos e
/// se foram recortados. A área de trabalho usa para "Colar".
pub static ARQUIVOS: Mutex<Option<(Vec<std::path::PathBuf>, bool)>> = Mutex::new(None);
/// A própria Plumagem oferece os arquivos (copiados na área de trabalho).
pub static PROPRIA_ARQUIVOS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn tipo_arquivos(tipos: &[String]) -> Option<String> {
    TIPOS_ARQUIVOS.iter().find(|t| tipos.iter().any(|m| m == *t)).map(|t| t.to_string())
}

pub fn uri_de(p: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut s = String::from("file://");
    for &b in p.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn caminho_de_uri(uri: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let resto = uri.trim().strip_prefix("file://")?;
    let resto = if resto.starts_with('/') { resto } else { &resto[resto.find('/')?..] };
    let b = resto.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&resto[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    Some(std::path::PathBuf::from(std::ffi::OsString::from_vec(out)))
}

/// Lê a lista de arquivos do pipe numa thread e guarda em `ARQUIVOS`.
pub fn ler_arquivos_em_segundo_plano(leitura: OwnedFd) {
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(leitura);
        let mut b = Vec::new();
        let _ = (&mut f).take(MAX_BYTES as u64).read_to_end(&mut b);
        let t = String::from_utf8_lossy(&b);
        let mut corte = false;
        let mut v = Vec::new();
        for (i, linha) in t.lines().enumerate() {
            let l = linha.trim();
            if i == 0 && (l == "cut" || l == "copy") {
                corte = l == "cut";
                continue;
            }
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some(c) = caminho_de_uri(l) {
                v.push(c);
            }
        }
        if let Ok(mut a) = ARQUIVOS.lock() {
            *a = (!v.is_empty()).then_some((v, corte));
        }
    });
}

/// Entrega os arquivos que a Plumagem copiou, no formato pedido.
pub fn enviar_arquivos(mime: &str, fd: OwnedFd) -> bool {
    if !PROPRIA_ARQUIVOS.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let Some((v, corte)) = ARQUIVOS.lock().ok().and_then(|a| a.clone()) else { return false };
    let uris: Vec<String> = v.iter().map(|p| uri_de(p)).collect();
    let t = if mime == "x-special/gnome-copied-files" {
        format!("{}\n{}", if corte { "cut" } else { "copy" }, uris.join("\n"))
    } else {
        uris.join("\r\n") + "\r\n"
    };
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(fd);
        let _ = f.write_all(t.as_bytes());
    });
    true
}

/// Nome livre na pasta ("foto - Cópia.png", "Nova pasta (2)").
pub fn nome_livre(dir: &std::path::Path, base: &str, sufixo: &str) -> std::path::PathBuf {
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

/// Cola os arquivos da área de transferência na pasta (numa thread; avisa
/// por notificação se algo falhar).
pub fn colar_em(destino: std::path::PathBuf) {
    let Some((v, corte)) = ARQUIVOS.lock().ok().and_then(|a| a.clone()) else { return };
    if corte {
        if let Ok(mut a) = ARQUIVOS.lock() {
            *a = None;
        }
    }
    std::thread::spawn(move || {
        let mut falhas = 0;
        for o in &v {
            let Some(nome) = o.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            if destino.starts_with(o) || (corte && o.parent() == Some(destino.as_path())) {
                continue;
            }
            let alvo = nome_livre(&destino, &nome, if corte { "" } else { " - Cópia" });
            let ok = if corte {
                std::fs::rename(o, &alvo).is_ok()
                    || std::process::Command::new("mv").arg("--").arg(o).arg(&alvo).status().map(|s| s.success()).unwrap_or(false)
            } else {
                std::process::Command::new("cp").arg("-a").arg("--").arg(o).arg(&alvo).status().map(|s| s.success()).unwrap_or(false)
            };
            if !ok {
                falhas += 1;
            }
        }
        if falhas > 0 {
            let _ = std::process::Command::new("ararinha-notificar")
                .args(["Não deu para colar", &format!("{falhas} item(ns) não puderam ser colados na área de trabalho.")])
                .spawn();
        }
    });
}

/// Arquivos soltos na área de trabalho (arrastados do Explorador ou de
/// outro app): lê a lista do pipe e move para a Área de Trabalho (copia se
/// vierem de outro disco ou com Ctrl, como no Windows).
pub fn receber_soltos(leitura: OwnedFd, ctrl: bool) {
    std::thread::spawn(move || {
        use std::os::unix::fs::MetadataExt;
        let mut f = std::fs::File::from(leitura);
        let mut b = Vec::new();
        let _ = (&mut f).take(MAX_BYTES as u64).read_to_end(&mut b);
        let t = String::from_utf8_lossy(&b);
        let Some(destino) = crate::painel::pasta_area() else { return };
        let _ = std::fs::create_dir_all(&destino);
        let disp_destino = std::fs::metadata(&destino).map(|m| m.dev()).unwrap_or(0);
        for (i, linha) in t.lines().enumerate() {
            let l = linha.trim();
            if (i == 0 && (l == "cut" || l == "copy")) || l.is_empty() || l.starts_with('#') {
                continue;
            }
            let Some(o) = caminho_de_uri(l) else { continue };
            if o.parent() == Some(destino.as_path()) {
                continue;
            }
            let mesmo_disco = std::fs::metadata(&o).map(|m| m.dev() == disp_destino).unwrap_or(false);
            crate::painel::mover_para(&o, &destino, ctrl || !mesmo_disco);
        }
    });
}

/// Imagem (PNG) que a Plumagem oferece na área de transferência (captura).
pub static PROPRIA_IMAGEM: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Entrega a imagem copiada, se for ela que a Plumagem está oferecendo.
pub fn enviar_imagem(fd: OwnedFd) -> bool {
    let Some(b) = PROPRIA_IMAGEM.lock().ok().and_then(|p| p.clone()) else { return false };
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(fd);
        let _ = f.write_all(&b);
    });
    true
}
