// SPDX-License-Identifier: GPL-3.0-or-later
//! "Abrir com…" e apps padrão.
//!
//! Os apps instalados (os nossos, os do Flathub) vêm dos arquivos
//! `.desktop`; o app padrão de cada tipo fica no `~/.config/mimeapps.list`,
//! o mesmo arquivo que o Chromium e os apps Flatpak leem.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug)]
pub struct AppDesktop {
    /// Nome do arquivo (org.videolan.VLC.desktop) ou do nosso app ("imagens").
    pub id: String,
    pub nome: String,
    exec: String,
    pub tipos: Vec<String>,
}

/// Tipo MIME pela extensão (o suficiente para casar com os apps).
pub fn tipo_mime(nome: &str) -> String {
    let ext = Path::new(nome).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let t = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/vnd.microsoft.icon",
        "heic" => "image/heic",
        "tif" | "tiff" => "image/tiff",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg",
        "wav" => "audio/x-wav",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "mpg" | "mpeg" => "video/mpeg",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "rtf" => "application/rtf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "xz" => "application/x-xz",
        "iso" => "application/x-cd-image",
        "torrent" => "application/x-bittorrent",
        "sh" => "application/x-shellscript",
        "py" => "text/x-python",
        "rs" => "text/rust",
        "c" | "h" => "text/x-c",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    };
    t.to_string()
}

/// Nossos apps (sem .desktop com MimeType ainda).
fn nossos() -> Vec<AppDesktop> {
    let a = |id: &str, nome: &str, tipos: &[&str]| AppDesktop { id: id.into(), nome: nome.into(), exec: format!("{id} %f"), tipos: tipos.iter().map(|s| s.to_string()).collect() };
    vec![
        a("imagens", "Imagens", &["image/png", "image/jpeg", "image/gif", "image/webp", "image/bmp", "image/vnd.microsoft.icon"]),
        a("musicas", "Músicas", &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav", "audio/mp4", "audio/aac", "video/mp4"]),
        a("notas", "Bloco de Notas", &["text/plain", "text/markdown", "text/csv", "application/json", "application/xml", "text/x-python", "text/rust", "text/x-c", "application/x-shellscript", "text/css", "text/html"]),
    ]
}

fn pastas_desktop() -> Vec<PathBuf> {
    let casa = std::env::var("HOME").unwrap_or_default();
    vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from(format!("{casa}/.local/share/flatpak/exports/share/applications")),
        PathBuf::from(format!("{casa}/.local/share/applications")),
    ]
}

fn ler_desktop(p: &Path) -> Option<AppDesktop> {
    let t = std::fs::read_to_string(p).ok()?;
    let (mut nome, mut nome_pt, mut exec, mut tipos, mut oculto, mut na_secao) = (None, None, None, Vec::new(), false, false);
    for l in t.lines() {
        if l.starts_with('[') {
            na_secao = l == "[Desktop Entry]";
            continue;
        }
        if !na_secao {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            match k.trim() {
                "Name" => nome = Some(v.to_string()),
                "Name[pt_BR]" => nome_pt = Some(v.to_string()),
                "Exec" => exec = Some(v.to_string()),
                "MimeType" => tipos = v.split(';').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                "NoDisplay" | "Hidden" => oculto |= v.trim() == "true",
                "Type" if v.trim() != "Application" => return None,
                _ => {}
            }
        }
    }
    if oculto {
        return None;
    }
    Some(AppDesktop { id: p.file_name()?.to_string_lossy().to_string(), nome: nome_pt.or(nome)?, exec: exec?, tipos })
}

/// Todos os apps (os nossos primeiro), sem repetir id.
pub fn todos() -> Vec<AppDesktop> {
    let mut v = nossos();
    for d in pastas_desktop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut arqs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
        arqs.sort();
        for p in arqs {
            if let Some(a) = ler_desktop(&p) {
                if !v.iter().any(|x| x.id == a.id) {
                    v.push(a);
                }
            }
        }
    }
    v
}

/// Apps que dizem abrir este tipo (os que casam a família, image/*, depois).
pub fn para_tipo(tipo: &str) -> (Vec<AppDesktop>, Vec<AppDesktop>) {
    let familia = tipo.split('/').next().unwrap_or("");
    let (mut sim, mut outros) = (Vec::new(), Vec::new());
    for a in todos() {
        if a.tipos.iter().any(|t| t == tipo || t == &format!("{familia}/*")) {
            sim.push(a);
        } else {
            outros.push(a);
        }
    }
    (sim, outros)
}

fn mimeapps() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("mimeapps.list")
}

/// App padrão do tipo (id), pelo mimeapps.list.
pub fn padrao(tipo: &str) -> Option<String> {
    let t = std::fs::read_to_string(mimeapps()).ok()?;
    let mut na = false;
    for l in t.lines() {
        if l.starts_with('[') {
            na = l.trim() == "[Default Applications]";
            continue;
        }
        if na {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == tipo {
                    return v.split(';').next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
        }
    }
    None
}

/// Grava o app padrão do tipo (mantém o resto do arquivo).
pub fn definir_padrao(tipo: &str, id: &str) -> std::io::Result<()> {
    let p = mimeapps();
    let t = std::fs::read_to_string(&p).unwrap_or_default();
    let mut linhas: Vec<String> = t.lines().map(str::to_string).collect();
    let nova = format!("{tipo}={id};");
    let ini = linhas.iter().position(|l| l.trim() == "[Default Applications]");
    match ini {
        None => {
            if !linhas.is_empty() {
                linhas.push(String::new());
            }
            linhas.push("[Default Applications]".into());
            linhas.push(nova);
        }
        Some(i) => {
            let fim = linhas.iter().skip(i + 1).position(|l| l.starts_with('[')).map(|k| k + i + 1).unwrap_or(linhas.len());
            match (i + 1..fim).find(|&k| linhas[k].split_once('=').map(|(a, _)| a.trim() == tipo).unwrap_or(false)) {
                Some(k) => linhas[k] = nova,
                None => linhas.insert(fim, nova),
            }
        }
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&p, linhas.join("\n") + "\n")
}

pub fn achar(id: &str) -> Option<AppDesktop> {
    todos().into_iter().find(|a| a.id == id)
}

/// Divide o Exec (com aspas) e troca os códigos de campo pelo arquivo.
fn argumentos(exec: &str, arquivo: &Path) -> Vec<String> {
    let mut partes = Vec::new();
    let mut atual = String::new();
    let mut aspas = false;
    for c in exec.chars() {
        match c {
            '"' => aspas = !aspas,
            ' ' if !aspas => {
                if !atual.is_empty() {
                    partes.push(std::mem::take(&mut atual));
                }
            }
            _ => atual.push(c),
        }
    }
    if !atual.is_empty() {
        partes.push(atual);
    }
    let arq = arquivo.to_string_lossy().to_string();
    let mut usou = false;
    let mut v = Vec::new();
    for p in partes {
        match p.as_str() {
            "%f" | "%F" | "%u" | "%U" => {
                v.push(arq.clone());
                usou = true;
            }
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            _ => v.push(p.replace("%%", "%")),
        }
    }
    if !usou {
        v.push(arq);
    }
    v
}

pub fn abrir(app: &AppDesktop, arquivo: &Path) -> Result<(), String> {
    let a = argumentos(&app.exec, arquivo);
    let (prog, resto) = a.split_first().ok_or("app sem comando")?;
    Command::new(prog).args(resto).spawn().map(|_| ()).map_err(|e| e.to_string())
}
