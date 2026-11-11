// SPDX-License-Identifier: GPL-3.0-or-later
//! Memória instalada pela tabela SMBIOS (/sys/firmware/dmi/tables/DMI), que
//! só o root lê. Devolve slots, capacidade máxima e cada pente: posição,
//! tamanho, tipo, velocidade, fabricante e modelo. Números de série ficam
//! de fora (privacidade).
//!
//! Formato: "slots=4;maximo_mb=65536;modulo=DIMM A1|8192|DDR4|3200|Kingston|KF432C16;..."
//! Um slot vazio vem como "modulo=DIMM B1|0|||".

const DMI: &str = "/sys/firmware/dmi/tables/DMI";

fn texto(strings: &[&[u8]], indice: u8) -> String {
    if indice == 0 {
        return String::new();
    }
    strings
        .get(indice as usize - 1)
        .map(|s| String::from_utf8_lossy(s).trim().to_string())
        .unwrap_or_default()
        .chars()
        .filter(|c| !matches!(c, ';' | '|' | '=') && !c.is_control())
        .collect()
}

fn u16_em(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_em(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_em(d: &[u8], o: usize) -> Option<u64> {
    d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
}

fn tipo_memoria(t: u8) -> &'static str {
    match t {
        0x0F => "SDRAM",
        0x12 => "DDR",
        0x13 => "DDR2",
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        0x07 => "RAM",
        _ => "",
    }
}

pub fn memoria() -> Result<String, String> {
    let dados = std::fs::read(DMI).map_err(|e| format!("tabela SMBIOS: {e}"))?;
    let mut slots: u32 = 0;
    let mut maximo_mb: u64 = 0;
    let mut modulos = Vec::new();
    let mut i = 0usize;
    while i + 4 <= dados.len() {
        let tipo = dados[i];
        let tam = dados[i + 1] as usize;
        if tam < 4 || i + tam > dados.len() {
            break;
        }
        let estrutura = &dados[i..i + tam];
        // Strings: depois da parte formatada, terminadas por dois zeros.
        let mut j = i + tam;
        let inicio = j;
        while j + 1 < dados.len() && !(dados[j] == 0 && dados[j + 1] == 0) {
            j += 1;
        }
        let area = &dados[inicio..j.min(dados.len())];
        let strings: Vec<&[u8]> = area.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
        match tipo {
            16 => {
                // Uso 0x03 = memória do sistema.
                if estrutura.get(5) == Some(&3) || estrutura.get(5).is_none() {
                    slots += u16_em(estrutura, 0x0D).unwrap_or(0) as u32;
                    let max_kb = u32_em(estrutura, 0x07).unwrap_or(0);
                    maximo_mb += if max_kb == 0x8000_0000 {
                        u64_em(estrutura, 0x0F).unwrap_or(0) / (1024 * 1024)
                    } else {
                        max_kb as u64 / 1024
                    };
                }
            }
            17 => {
                let bruto = u16_em(estrutura, 0x0C).unwrap_or(0);
                let mb: u64 = match bruto {
                    0 | 0xFFFF => 0,
                    0x7FFF => u32_em(estrutura, 0x1C).unwrap_or(0) as u64 & 0x7FFF_FFFF,
                    t if t & 0x8000 != 0 => (t & 0x7FFF) as u64 / 1024,
                    t => t as u64,
                };
                let local = texto(&strings, *estrutura.get(0x10).unwrap_or(&0));
                let banco = texto(&strings, *estrutura.get(0x11).unwrap_or(&0));
                let nome = match (local.is_empty(), banco.is_empty()) {
                    (false, true) => local,
                    (true, false) => banco,
                    (false, false) if !local.contains(&banco) => format!("{local} ({banco})"),
                    _ => local,
                };
                let tipo_m = if mb > 0 { tipo_memoria(*estrutura.get(0x12).unwrap_or(&0)) } else { "" };
                let veloc = if mb > 0 {
                    u16_em(estrutura, 0x20).filter(|v| *v > 0 && *v != 0xFFFF).or(u16_em(estrutura, 0x15)).unwrap_or(0)
                } else {
                    0
                };
                let fabricante = if mb > 0 { texto(&strings, *estrutura.get(0x17).unwrap_or(&0)) } else { String::new() };
                let modelo = if mb > 0 { texto(&strings, *estrutura.get(0x1A).unwrap_or(&0)) } else { String::new() };
                modulos.push(format!(
                    "modulo={nome}|{mb}|{tipo_m}|{}|{fabricante}|{modelo}",
                    if veloc > 0 && veloc != 0xFFFF { veloc.to_string() } else { String::new() }
                ));
            }
            127 => break,
            _ => {}
        }
        i = j + 2;
    }
    if slots == 0 {
        slots = modulos.len() as u32;
    }
    let mut r = format!("slots={slots};maximo_mb={maximo_mb}");
    for m in modulos.iter().take(32) {
        r.push(';');
        r.push_str(m);
    }
    Ok(r)
}
