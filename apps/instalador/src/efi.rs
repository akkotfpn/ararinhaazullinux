// SPDX-License-Identifier: GPL-3.0-or-later
//! Entrada de boot UEFI para o disco instalado ("Ararinha Azul Linux"),
//! posta em primeiro lugar no BootOrder: o computador liga pelo disco, não
//! pelo pendrive. Escrita direto no efivarfs (formato EFI_LOAD_OPTION).

use std::fs;

const GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
const VARS: &str = "/sys/firmware/efi/efivars";

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect()
}

/// GUID em texto para os 16 bytes do UEFI (3 primeiros campos little-endian).
fn guid(s: &str) -> Option<[u8; 16]> {
    let h: String = s.chars().filter(|c| *c != '-').collect();
    if h.len() != 32 {
        return None;
    }
    let b: Vec<u8> = (0..16).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()).collect::<Option<_>>()?;
    let mut g = [0u8; 16];
    g[0..4].copy_from_slice(&[b[3], b[2], b[1], b[0]]);
    g[4..6].copy_from_slice(&[b[5], b[4]]);
    g[6..8].copy_from_slice(&[b[7], b[6]]);
    g[8..16].copy_from_slice(&b[8..16]);
    Some(g)
}

fn ler_var(nome: &str) -> Option<Vec<u8>> {
    fs::read(format!("{VARS}/{nome}-{GLOBAL}")).ok().filter(|v| v.len() >= 4).map(|v| v[4..].to_vec())
}

fn gravar_var(nome: &str, dados: &[u8]) -> std::io::Result<()> {
    // 0x07 = guardada na NVRAM, visível no boot e com o sistema rodando.
    let mut v = vec![0x07, 0, 0, 0];
    v.extend_from_slice(dados);
    fs::write(format!("{VARS}/{nome}-{GLOBAL}"), v)
}

/// `particao`: ex. "sda1" (a ESP do disco instalado), número 1.
pub fn registrar(particao: &str, numero: u32, arquivo: &str) -> Result<(), String> {
    if !std::path::Path::new(VARS).is_dir() {
        return Err("o computador não ligou em modo UEFI".into());
    }
    let sys = format!("/sys/class/block/{particao}");
    let inicio: u64 = fs::read_to_string(format!("{sys}/start")).map_err(|e| e.to_string())?.trim().parse().map_err(|_| "início da partição")?;
    let tamanho: u64 = fs::read_to_string(format!("{sys}/size")).map_err(|e| e.to_string())?.trim().parse().map_err(|_| "tamanho da partição")?;
    let partuuid = std::process::Command::new("blkid")
        .args(["-o", "value", "-s", "PARTUUID", &format!("/dev/{particao}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let assinatura = guid(&partuuid).ok_or("PARTUUID da ESP não encontrado")?;

    // Caminho do dispositivo: HD(...)/File(\EFI\BOOT\BOOTX64.EFI)/Fim.
    let mut dp = vec![0x04, 0x01, 42, 0];
    dp.extend_from_slice(&numero.to_le_bytes());
    dp.extend_from_slice(&inicio.to_le_bytes());
    dp.extend_from_slice(&tamanho.to_le_bytes());
    dp.extend_from_slice(&assinatura);
    dp.extend_from_slice(&[0x02, 0x02]);
    let caminho = utf16z(arquivo);
    let l = (4 + caminho.len()) as u16;
    dp.extend_from_slice(&[0x04, 0x04]);
    dp.extend_from_slice(&l.to_le_bytes());
    dp.extend_from_slice(&caminho);
    dp.extend_from_slice(&[0x7f, 0xff, 0x04, 0x00]);

    let mut opcao = Vec::new();
    opcao.extend_from_slice(&1u32.to_le_bytes()); // ativa
    opcao.extend_from_slice(&(dp.len() as u16).to_le_bytes());
    opcao.extend_from_slice(&utf16z("Ararinha Azul Linux"));
    opcao.extend_from_slice(&dp);

    // Reaproveita a entrada com o mesmo nome; senão, o primeiro número livre.
    let mut ordem: Vec<u16> = ler_var("BootOrder").map(|b| b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()).unwrap_or_default();
    let nome_de = |n: u16| format!("Boot{n:04X}");
    let existente = (0u16..0x100).find(|n| {
        ler_var(&nome_de(*n)).is_some_and(|v| v.len() > 6 && v[6..].starts_with(&utf16z("Ararinha Azul Linux")[..38]))
    });
    let n = existente.or_else(|| (0x0A00u16..0x0AFF).find(|n| ler_var(&nome_de(*n)).is_none())).ok_or("sem número de entrada livre")?;
    gravar_var(&nome_de(n), &opcao).map_err(|e| format!("gravar a entrada de boot: {e}"))?;
    ordem.retain(|x| *x != n);
    ordem.insert(0, n);
    let bytes: Vec<u8> = ordem.iter().flat_map(|x| x.to_le_bytes()).collect();
    gravar_var("BootOrder", &bytes).map_err(|e| format!("gravar a ordem de boot: {e}"))
}
