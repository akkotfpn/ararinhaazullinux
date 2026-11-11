// SPDX-License-Identifier: GPL-3.0-or-later
//! Tipos de arquivo (nome em português e ícone), Propriedades e Lixeira.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

use penas::{tema, Cor, Tela};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Categoria {
    Imagem,
    Audio,
    Video,
    Texto,
    Documento,
    Planilha,
    Apresentacao,
    Pdf,
    Compactado,
    Codigo,
    Programa,
    Outro,
}

/// Categoria e nome do tipo, como na coluna "Tipo" do Windows.
pub fn tipo_de(nome: &str) -> (Categoria, String) {
    let ext = match nome.rsplit_once('.') {
        Some((r, e)) if !r.is_empty() => e.to_lowercase(),
        _ => String::new(),
    };
    use Categoria::*;
    let (cat, desc): (Categoria, &str) = match ext.as_str() {
        "png" => (Imagem, "Imagem PNG"),
        "jpg" | "jpeg" => (Imagem, "Imagem JPEG"),
        "gif" => (Imagem, "Imagem GIF"),
        "bmp" => (Imagem, "Imagem BMP"),
        "webp" => (Imagem, "Imagem WebP"),
        "svg" => (Imagem, "Imagem SVG"),
        "ico" => (Imagem, "Ícone"),
        "mp3" => (Audio, "Áudio MP3"),
        "ogg" | "oga" | "opus" => (Audio, "Áudio Ogg"),
        "flac" => (Audio, "Áudio FLAC"),
        "wav" => (Audio, "Áudio WAV"),
        "m4a" | "aac" => (Audio, "Áudio AAC"),
        "mp4" | "m4v" => (Video, "Vídeo MP4"),
        "mkv" => (Video, "Vídeo MKV"),
        "webm" => (Video, "Vídeo WebM"),
        "avi" => (Video, "Vídeo AVI"),
        "mov" => (Video, "Vídeo QuickTime"),
        "txt" => (Texto, "Documento de texto"),
        "md" => (Texto, "Texto Markdown"),
        "log" => (Texto, "Registro (log)"),
        "csv" => (Planilha, "Valores separados por vírgula"),
        "conf" | "cfg" | "ini" | "toml" => (Texto, "Arquivo de configuração"),
        "doc" | "docx" => (Documento, "Documento do Word"),
        "odt" => (Documento, "Documento de texto (ODF)"),
        "rtf" => (Documento, "Documento RTF"),
        "xls" | "xlsx" => (Planilha, "Planilha do Excel"),
        "ods" => (Planilha, "Planilha (ODF)"),
        "ppt" | "pptx" => (Apresentacao, "Apresentação do PowerPoint"),
        "odp" => (Apresentacao, "Apresentação (ODF)"),
        "pdf" => (Pdf, "Documento PDF"),
        "zip" => (Compactado, "Pasta compactada (ZIP)"),
        "tar" | "gz" | "tgz" | "xz" | "zst" | "bz2" | "7z" | "rar" => (Compactado, "Arquivo compactado"),
        "rs" => (Codigo, "Código Rust"),
        "py" => (Codigo, "Código Python"),
        "c" | "h" => (Codigo, "Código C"),
        "js" | "ts" => (Codigo, "Código JavaScript"),
        "html" | "htm" => (Codigo, "Página HTML"),
        "css" => (Codigo, "Folha de estilo CSS"),
        "json" => (Codigo, "Arquivo JSON"),
        "xml" => (Codigo, "Arquivo XML"),
        "sh" => (Programa, "Script do terminal"),
        "desktop" => (Programa, "Atalho de aplicativo"),
        "flatpak" | "flatpakref" => (Programa, "Pacote Flatpak"),
        "appimage" => (Programa, "Aplicativo (AppImage)"),
        "exe" | "msi" => (Programa, "Programa do Windows"),
        "iso" | "img" => (Compactado, "Imagem de disco"),
        "ttf" | "otf" => (Documento, "Fonte"),
        "" => (Outro, "Arquivo"),
        _ => (Outro, ""),
    };
    let desc = if desc.is_empty() { format!("Arquivo {}", ext.to_uppercase()) } else { desc.to_string() };
    (cat, desc)
}

/// Ícone de arquivo: folha com a dobra, colorida pelo tipo e com um
/// símbolo simples no meio.
pub fn icone_tipo(t: &mut Tela, cat: Categoria, x: f32, y: f32, s: f32) {
    use Categoria::*;
    let cor = match cat {
        Imagem => Cor::hex(0x2E9E6A),
        Audio => Cor::hex(0x8B3FB8),
        Video => Cor::hex(0xD24A3C),
        Texto => Cor::hex(0x7C8B99),
        Documento => Cor::hex(0x2B66C4),
        Planilha => Cor::hex(0x1F8A4C),
        Apresentacao => Cor::hex(0xD9732B),
        Pdf => Cor::hex(0xC62F2F),
        Compactado => Cor::hex(0xA8792A),
        Codigo => Cor::hex(0x3B5BA8),
        Programa => Cor::hex(0x4A5A6A),
        Outro => Cor::hex(0x9AA8B6),
    };
    // Folha
    let (fx, fy, fl, fa) = (x + 0.16 * s, y + 0.06 * s, 0.68 * s, 0.88 * s);
    t.retangulo(fx, fy, fl, fa, 0.07 * s, Cor::hex(0x9AA8B6));
    t.retangulo(fx + 1.0, fy + 1.0, fl - 2.0, fa - 2.0, 0.06 * s, tema::BRANCO());
    // Faixa colorida embaixo e dobra no canto.
    t.retangulo(fx + 1.0, fy + fa * 0.72, fl - 2.0, fa * 0.28 - 1.0, 0.05 * s, cor);
    t.poligono(
        &[(fx + fl - 0.22 * s, fy), (fx + fl, fy + 0.22 * s), (fx + fl - 0.22 * s, fy + 0.22 * s)],
        Cor::hex(0xD3DDE6),
    );
    let (cx, cy) = (fx + fl / 2.0, fy + fa * 0.4);
    let e = (0.05 * s).max(1.2);
    match cat {
        Imagem => {
            t.poligono(&[(cx - 0.2 * s, cy + 0.12 * s), (cx - 0.05 * s, cy - 0.08 * s), (cx + 0.06 * s, cy + 0.12 * s)], cor);
            t.poligono(&[(cx - 0.02 * s, cy + 0.12 * s), (cx + 0.1 * s, cy - 0.02 * s), (cx + 0.2 * s, cy + 0.12 * s)], cor.com_alfa(170));
            t.retangulo(cx + 0.08 * s, cy - 0.16 * s, 0.08 * s, 0.08 * s, 0.04 * s, Cor::hex(0xE8B23A));
        }
        Audio => {
            t.retangulo(cx - 0.12 * s, cy + 0.04 * s, 0.1 * s, 0.08 * s, 0.04 * s, cor);
            t.linha(cx - 0.03 * s, cy + 0.08 * s, cx - 0.03 * s, cy - 0.16 * s, e, cor);
            t.linha(cx - 0.03 * s, cy - 0.16 * s, cx + 0.12 * s, cy - 0.1 * s, e, cor);
        }
        Video => {
            t.poligono(&[(cx - 0.08 * s, cy - 0.13 * s), (cx + 0.13 * s, cy), (cx - 0.08 * s, cy + 0.13 * s)], cor);
        }
        Pdf | Documento | Texto | Planilha | Apresentacao => {
            for k in 0..3 {
                let ly = cy - 0.12 * s + k as f32 * 0.1 * s;
                t.linha(cx - 0.18 * s, ly, cx + 0.18 * s, ly, e, cor.com_alfa(190));
            }
        }
        Compactado => {
            for k in 0..4 {
                let ly = fy + 0.06 * s + k as f32 * 0.09 * s;
                t.retangulo(cx - 0.04 * s + if k % 2 == 0 { 0.0 } else { 0.04 * s }, ly, 0.04 * s, 0.05 * s, 0.0, cor);
            }
        }
        Codigo => {
            t.linha(cx - 0.08 * s, cy - 0.1 * s, cx - 0.17 * s, cy, e, cor);
            t.linha(cx - 0.17 * s, cy, cx - 0.08 * s, cy + 0.1 * s, e, cor);
            t.linha(cx + 0.08 * s, cy - 0.1 * s, cx + 0.17 * s, cy, e, cor);
            t.linha(cx + 0.17 * s, cy, cx + 0.08 * s, cy + 0.1 * s, e, cor);
        }
        Programa => {
            t.retangulo(cx - 0.14 * s, cy - 0.12 * s, 0.28 * s, 0.24 * s, 0.04 * s, cor);
            t.retangulo(cx - 0.12 * s, cy - 0.06 * s, 0.24 * s, 0.16 * s, 0.02 * s, tema::BRANCO());
        }
        Outro => {}
    }
}

/// Lixeira do usuário (padrão freedesktop).
pub fn lixeira(casa: &Path) -> PathBuf {
    casa.join(".local/share/Trash")
}

/// Desfaz o %XX do Path= do .trashinfo.
fn decodificar(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Onde o item estava antes de ir para a Lixeira.
pub fn origem_na_lixeira(casa: &Path, nome: &str) -> Option<PathBuf> {
    let info = fs::read_to_string(lixeira(casa).join("info").join(format!("{nome}.trashinfo"))).ok()?;
    let p = info.lines().find_map(|l| l.strip_prefix("Path="))?;
    Some(PathBuf::from(decodificar(p.trim())))
}

/// Esvazia a Lixeira; devolve quantos itens foram apagados.
pub fn esvaziar_lixeira(casa: &Path) -> Result<usize, String> {
    let l = lixeira(casa);
    let mut n = 0;
    for sub in ["files", "info"] {
        let Ok(rd) = fs::read_dir(l.join(sub)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let r = if p.is_dir() && !p.is_symlink() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            r.map_err(|e| e.to_string())?;
            if sub == "files" {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Tamanho e quantidade (arquivos, pastas) de uma árvore, com limite para
/// não travar a janela em pastas enormes.
pub fn medir_arvore(p: &Path) -> (u64, u64, u64, bool) {
    let mut pilha = vec![p.to_path_buf()];
    let (mut bytes, mut arquivos, mut pastas) = (0u64, 0u64, 0u64);
    let mut visitados = 0;
    while let Some(d) = pilha.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            visitados += 1;
            if visitados > 200_000 {
                return (bytes, arquivos, pastas, true);
            }
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                pastas += 1;
                pilha.push(e.path());
            } else {
                arquivos += 1;
                bytes += m.len();
            }
        }
    }
    (bytes, arquivos, pastas, false)
}

/// Nome do dono de um uid (pelo /etc/passwd).
pub fn nome_usuario(uid: u32) -> String {
    fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split(':').collect();
            (c.len() > 2 && c[2].parse::<u32>().ok() == Some(uid)).then(|| c[0].to_string())
        })
        .unwrap_or_else(|| uid.to_string())
}

pub struct Propriedades {
    pub titulo: String,
    pub categoria: Option<Categoria>,
    pub pasta: bool,
    pub linhas: Vec<(String, String)>,
}

/// Monta as Propriedades de um ou mais itens.
pub fn propriedades(
    caminhos: &[PathBuf],
    tamanho_legivel: impl Fn(u64) -> String,
    data_legivel: impl Fn(SystemTime) -> String,
) -> Propriedades {
    let mut linhas = Vec::new();
    if caminhos.len() == 1 {
        let p = &caminhos[0];
        let nome = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
        let meta = fs::symlink_metadata(p).ok();
        let pasta = p.is_dir();
        let (cat, tipo) = if pasta { (None, "Pasta de arquivos".to_string()) } else {
            let (c, t) = tipo_de(&nome);
            (Some(c), t)
        };
        linhas.push(("Tipo".into(), tipo));
        linhas.push(("Local".into(), p.parent().map(|x| x.display().to_string()).unwrap_or_default()));
        if pasta {
            let (b, a, d, cortado) = medir_arvore(p);
            let mais = if cortado { " (ou mais)" } else { "" };
            linhas.push(("Tamanho".into(), format!("{}{mais} ({} bytes)", tamanho_legivel(b), b)));
            linhas.push(("Contém".into(), format!("{a} arquivos, {d} pastas{mais}")));
        } else if let Some(m) = &meta {
            linhas.push(("Tamanho".into(), format!("{} ({} bytes)", tamanho_legivel(m.len()), m.len())));
        }
        if let Some(m) = &meta {
            if let Ok(t) = m.created() {
                linhas.push(("Criado em".into(), data_legivel(t)));
            }
            if let Ok(t) = m.modified() {
                linhas.push(("Modificado em".into(), data_legivel(t)));
            }
            if let Ok(t) = m.accessed() {
                linhas.push(("Acessado em".into(), data_legivel(t)));
            }
            linhas.push(("Dono".into(), nome_usuario(m.uid())));
            let modo = m.mode();
            let pode_gravar = fs::metadata(p).map(|x| !x.permissions().readonly()).unwrap_or(false);
            linhas.push((
                "Atributos".into(),
                format!(
                    "{}{}{}",
                    if pode_gravar { "Leitura e gravação" } else { "Somente leitura" },
                    if nome.starts_with('.') { ", oculto" } else { "" },
                    if !pasta && modo & 0o111 != 0 { ", executável" } else { "" }
                ),
            ));
            if m.file_type().is_symlink() {
                if let Ok(alvo) = fs::read_link(p) {
                    linhas.push(("Atalho para".into(), alvo.display().to_string()));
                }
            }
        }
        Propriedades { titulo: nome, categoria: cat, pasta, linhas }
    } else {
        let (mut bytes, mut arquivos, mut pastas) = (0u64, 0u64, 0u64);
        for p in caminhos {
            if p.is_dir() {
                pastas += 1;
                let (b, a, d, _) = medir_arvore(p);
                bytes += b;
                arquivos += a;
                pastas += d;
            } else if let Ok(m) = fs::metadata(p) {
                arquivos += 1;
                bytes += m.len();
            }
        }
        linhas.push(("Tipo".into(), "Vários tipos".into()));
        if let Some(pai) = caminhos[0].parent() {
            linhas.push(("Local".into(), pai.display().to_string()));
        }
        linhas.push(("Tamanho".into(), format!("{} ({} bytes)", tamanho_legivel(bytes), bytes)));
        linhas.push(("Contém".into(), format!("{arquivos} arquivos, {pastas} pastas")));
        Propriedades { titulo: format!("{} itens", caminhos.len()), categoria: None, pasta: false, linhas }
    }
}

/// Busca por nome a partir de uma pasta (sem diferenciar maiúsculas),
/// até `limite` resultados.
pub fn buscar(raiz: &Path, termo: &str, ocultos: bool, limite: usize) -> Vec<PathBuf> {
    let termo = termo.to_lowercase();
    let mut achados = Vec::new();
    let mut pilha = vec![(raiz.to_path_buf(), 0)];
    let mut visitados = 0;
    while let Some((d, nivel)) = pilha.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            visitados += 1;
            if visitados > 300_000 || achados.len() >= limite {
                return achados;
            }
            let nome = e.file_name().to_string_lossy().to_string();
            if !ocultos && nome.starts_with('.') {
                continue;
            }
            if nome.to_lowercase().contains(&termo) {
                achados.push(e.path());
            }
            if nivel < 12 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                pilha.push((e.path(), nivel + 1));
            }
        }
    }
    achados
}
