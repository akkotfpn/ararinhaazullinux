// SPDX-License-Identifier: GPL-3.0-or-later
//! Pendrives e HDs externos: montar para quem está usando o computador
//! (em /run/media/<usuário>/<nome>) e ejetar (desmontar e desligar o
//! dispositivo, para poder tirar com segurança).
//!
//! Só aceita partições de discos removíveis ou ligados por USB; o disco do
//! sistema nunca entra na lista.

use std::path::{Path, PathBuf};


use crate::contas::{rodar_fora, rodar_fora_saida};

pub struct Parte {
    pub dev: String,
    pub disco: String,
    pub rotulo: String,
    pub bytes: u64,
    pub ponto: String,
}

fn ler(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p).unwrap_or_default().trim().to_string()
}

/// Rótulo pelo /dev/disk/by-label (os nomes vêm com \x20 no lugar de espaço).
fn rotulo(dev: &str) -> Option<String> {
    for e in std::fs::read_dir("/dev/disk/by-label").ok()?.flatten() {
        if std::fs::read_link(e.path()).ok()?.file_name()?.to_str()? == dev {
            return Some(e.file_name().to_string_lossy().replace("\\x20", " "));
        }
    }
    None
}

fn montagens() -> Vec<(String, String)> {
    ler("/proc/mounts")
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            Some((c.first()?.trim_start_matches("/dev/").to_string(), c.get(1)?.replace("\\040", " ")))
        })
        .collect()
}

pub fn listar() -> Vec<Parte> {
    let mont = montagens();
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/block") else { return v };
    for e in rd.flatten() {
        let disco = e.file_name().to_string_lossy().to_string();
        if !(disco.starts_with("sd") || disco.starts_with("mmcblk")) {
            continue;
        }
        let real = std::fs::canonicalize(e.path()).unwrap_or_default();
        let usb = real.to_string_lossy().contains("/usb");
        if !usb && ler(e.path().join("removable")) != "1" {
            continue;
        }
        // Partições (sdb1, mmcblk0p1); sem partições, o disco inteiro.
        let mut partes: Vec<String> = std::fs::read_dir(e.path())
            .map(|r| r.flatten().map(|p| p.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with(&disco) && n != &disco).collect())
            .unwrap_or_default();
        if partes.is_empty() {
            if ler(e.path().join("size")).parse::<u64>().unwrap_or(0) == 0 {
                continue; // leitor de cartão vazio
            }
            partes.push(disco.clone());
        }
        partes.sort();
        for dev in partes {
            let base = if dev == disco { e.path() } else { e.path().join(&dev) };
            let bytes = ler(base.join("size")).parse::<u64>().unwrap_or(0) * 512;
            // O sistema (/, /boot) nunca é tratado como removível.
            let ponto = mont.iter().find(|(d, _)| *d == dev).map(|(_, p)| p.clone()).unwrap_or_default();
            if ponto == "/" || ponto.starts_with("/boot") || ponto.starts_with("/efi") {
                continue;
            }
            v.push(Parte { rotulo: rotulo(&dev).unwrap_or_default(), dev, disco: disco.clone(), bytes, ponto });
        }
    }
    v
}

/// "dev|rótulo|bytes|ponto" separados por ';'.
pub fn listar_texto() -> String {
    listar()
        .iter()
        .map(|p| format!("{}|{}|{}|{}", p.dev, p.rotulo.replace(['|', ';'], " "), p.bytes, p.ponto))
        .collect::<Vec<_>>()
        .join(";")
}

fn achar(dev: &str) -> Result<Parte, String> {
    listar().into_iter().find(|p| p.dev == dev).ok_or_else(|| "dispositivo removível não encontrado".to_string())
}

fn blkid(dev: &str, campo: &str) -> String {
    rodar_fora_saida(&["/usr/sbin/blkid", "-o", "value", "-s", campo, &format!("/dev/{dev}")])
}

pub fn montar(dev: &str, usuario: &str, uid: u32, gid: u32) -> Result<String, String> {
    let p = achar(dev)?;
    if !p.ponto.is_empty() {
        return Ok(p.ponto);
    }
    let rot = if p.rotulo.is_empty() { blkid(dev, "LABEL") } else { p.rotulo.clone() };
    let nome: String = if rot.is_empty() { dev.to_string() } else { rot };
    let nome: String = nome.chars().filter(|c| !matches!(c, '/' | '\0') && !c.is_control()).collect();
    let nome = if nome.is_empty() || nome.starts_with('.') { dev.to_string() } else { nome };
    let base = PathBuf::from(format!("/run/media/{usuario}"));
    let ponto = base.join(&nome);
    let ponto_s = ponto.to_string_lossy().to_string();
    let fs = blkid(dev, "TYPE");
    let mut opcoes = "nosuid,nodev".to_string();
    let tipo = match fs.as_str() {
        "vfat" | "exfat" => {
            opcoes += &format!(",uid={uid},gid={gid},umask=077,utf8");
            fs.clone()
        }
        "ntfs" => {
            opcoes += &format!(",uid={uid},gid={gid},umask=077");
            "ntfs3".into()
        }
        "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "f2fs" | "iso9660" | "udf" => fs.clone(),
        "" => return Err("o dispositivo não tem um sistema de arquivos conhecido".into()),
        o => return Err(format!("sistema de arquivos \"{o}\" não suportado")),
    };
    rodar_fora(&["/usr/bin/mkdir", "-p", &ponto_s], None)?;
    rodar_fora(&["/usr/bin/chown", &format!("{uid}:{gid}"), &base.to_string_lossy()], None)?;
    if let Err(e) = rodar_fora(&["/usr/bin/mount", "-t", &tipo, "-o", &opcoes, &format!("/dev/{dev}"), &ponto_s], None) {
        let _ = rodar_fora(&["/usr/bin/rmdir", &ponto_s], None);
        return Err(format!("não deu para abrir o dispositivo: {e}"));
    }
    Ok(ponto_s)
}

/// Desmonta todas as partições do mesmo disco e desliga o dispositivo.
pub fn ejetar(dev: &str) -> Result<String, String> {
    let p = achar(dev)?;
    for outra in listar().into_iter().filter(|o| o.disco == p.disco && !o.ponto.is_empty()) {
        rodar_fora(&["/usr/bin/umount", &outra.ponto], None).map_err(|e| {
            if e.contains("busy") || e.contains("ocupad") {
                "o dispositivo está em uso: feche os arquivos e programas abertos nele".to_string()
            } else {
                format!("não deu para ejetar: {e}")
            }
        })?;
        if outra.ponto.starts_with("/run/media/") {
            let _ = rodar_fora(&["/usr/bin/rmdir", &outra.ponto], None);
        }
    }
    rodar_fora(&["/usr/bin/sync"], None)?;
    // Desliga (pendrive para de piscar); cartões de memória não têm isso.
    let apagar = format!("/sys/block/{}/device/delete", p.disco);
    if Path::new(&apagar).exists() {
        let _ = rodar_fora(&["/usr/bin/tee", &apagar], Some("1\n"));
    }
    Ok(String::new())
}
