// SPDX-License-Identifier: GPL-3.0-or-later
//! Secure Boot com chave própria de cada computador.
//!
//! Quando o firmware está em modo de configuração (Setup Mode), o ajudante:
//! 1. gera três pares de chaves RSA-3072 (PK, KEK e db) com o openssl, em
//!    /etc/ararinha/secureboot do disco novo (pasta 0700 do root);
//! 2. assina o kernel da ESP com a chave db (systemd-sbsign);
//! 3. cadastra as chaves no firmware: db e KEK acrescentados aos que já
//!    existirem (para não quebrar placas com ROM assinada pela Microsoft) e
//!    PK por último, o que tira o firmware do modo de configuração.
//!
//! As variáveis são gravadas no efivarfs no formato autenticado da UEFI
//! (EFI_VARIABLE_AUTHENTICATION_2), cada uma assinada em PKCS#7 pela chave
//! acima dela (PK assina PK e KEK; KEK assina db).

use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const EFIVARS: &str = "/sys/firmware/efi/efivars";
const GUID_GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
const GUID_DB: &str = "d719b2cb-3d3a-4596-a3bc-dad00e67656f";
const GUID_X509: &str = "a5c059a1-94e4-4aa7-87b5-ab155c2bf072";
const GUID_PKCS7: &str = "4aafd29d-68df-49ee-8aa9-347d375665a7";

// Atributos: não volátil, boot, runtime, escrita autenticada por tempo.
const ATTR: u32 = 0x1 | 0x2 | 0x4 | 0x20;
const ACRESCENTAR: u32 = 0x40;

/// O firmware está em modo de configuração (aceita cadastrar chaves)?
pub fn modo_configuracao() -> bool {
    let v = fs::read(format!("{EFIVARS}/SetupMode-{GUID_GLOBAL}")).unwrap_or_default();
    v.len() >= 5 && v[4] == 1
}

/// O Secure Boot está ligado agora?
pub fn ligado() -> bool {
    let v = fs::read(format!("{EFIVARS}/SecureBoot-{GUID_GLOBAL}")).unwrap_or_default();
    v.len() >= 5 && v[4] == 1
}

/// Ferramentas necessárias presentes no sistema em uso?
pub fn disponivel() -> bool {
    Path::new(EFIVARS).is_dir() && achar("openssl").is_some() && sbsign().is_some()
}

fn achar(nome: &str) -> Option<PathBuf> {
    ["/usr/bin", "/usr/sbin", "/bin"].iter().map(|d| Path::new(d).join(nome)).find(|p| p.exists())
}

fn sbsign() -> Option<PathBuf> {
    ["/usr/bin/systemd-sbsign", "/usr/lib/systemd/systemd-sbsign"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

fn rodar(cmd: &mut Command, oque: &str) -> Result<(), String> {
    let s = cmd.output().map_err(|e| format!("{oque}: {e}"))?;
    if s.status.success() {
        Ok(())
    } else {
        Err(format!("{oque}: {}", String::from_utf8_lossy(&s.stderr).trim()))
    }
}

/// GUID textual para os 16 bytes da UEFI (3 primeiros campos little-endian).
fn guid(texto: &str) -> [u8; 16] {
    let h: String = texto.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let b: Vec<u8> = (0..16).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap_or(0)).collect();
    let mut g = [0u8; 16];
    g[0..4].copy_from_slice(&[b[3], b[2], b[1], b[0]]);
    g[4..6].copy_from_slice(&[b[5], b[4]]);
    g[6..8].copy_from_slice(&[b[7], b[6]]);
    g[8..16].copy_from_slice(&b[8..16]);
    g
}

/// GUID aleatório (dono das assinaturas), do /dev/urandom.
fn guid_novo() -> [u8; 16] {
    let mut g = [0u8; 16];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut g);
    }
    g[7] = (g[7] & 0x0f) | 0x40;
    g[8] = (g[8] & 0x3f) | 0x80;
    g
}

/// EFI_SIGNATURE_LIST com um certificado X.509 (DER).
fn lista_assinaturas(cert_der: &[u8], dono: &[u8; 16]) -> Vec<u8> {
    let tam_assinatura = 16 + cert_der.len() as u32;
    let mut v = Vec::new();
    v.extend_from_slice(&guid(GUID_X509));
    v.extend_from_slice(&(28 + tam_assinatura).to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&tam_assinatura.to_le_bytes());
    v.extend_from_slice(dono);
    v.extend_from_slice(cert_der);
    v
}

/// EFI_TIME de agora (UTC), com os campos de enchimento zerados.
fn agora_efi() -> [u8; 16] {
    let seg = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let dias = seg.div_euclid(86_400);
    let resto = seg.rem_euclid(86_400);
    // Algoritmo de dias para data civil (Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + if mes <= 2 { 1 } else { 0 };
    let mut t = [0u8; 16];
    t[0..2].copy_from_slice(&(ano as u16).to_le_bytes());
    t[2] = mes as u8;
    t[3] = dia as u8;
    t[4] = (resto / 3600) as u8;
    t[5] = (resto % 3600 / 60) as u8;
    t[6] = (resto % 60) as u8;
    t
}

/// Monta a variável autenticada (EFI_VARIABLE_AUTHENTICATION_2 + dados),
/// assinada com a chave/certificado PEM indicados.
fn autenticada(dir: &Path, nome: &str, guid_var: &str, attr: u32, dados: &[u8], chave: &Path, cert: &Path) -> Result<Vec<u8>, String> {
    let tempo = agora_efi();
    let mut a_assinar = Vec::new();
    for u in nome.encode_utf16() {
        a_assinar.extend_from_slice(&u.to_le_bytes());
    }
    a_assinar.extend_from_slice(&guid(guid_var));
    a_assinar.extend_from_slice(&attr.to_le_bytes());
    a_assinar.extend_from_slice(&tempo);
    a_assinar.extend_from_slice(dados);
    let entrada = dir.join(format!("{nome}.tbs"));
    let saida = dir.join(format!("{nome}.p7"));
    fs::write(&entrada, &a_assinar).map_err(|e| format!("{nome}: {e}"))?;
    rodar(
        Command::new("openssl").args(["smime", "-sign", "-binary", "-noattr", "-md", "sha256", "-outform", "DER", "-in"])
            .arg(&entrada)
            .arg("-signer")
            .arg(cert)
            .arg("-inkey")
            .arg(chave)
            .arg("-out")
            .arg(&saida),
        &format!("assinatura de {nome}"),
    )?;
    let p7 = fs::read(&saida).map_err(|e| format!("{nome}: {e}"))?;
    let _ = fs::remove_file(&entrada);
    let _ = fs::remove_file(&saida);
    let mut v = Vec::new();
    v.extend_from_slice(&tempo);
    v.extend_from_slice(&(8 + 16 + p7.len() as u32).to_le_bytes());
    v.extend_from_slice(&0x0200u16.to_le_bytes());
    v.extend_from_slice(&0x0EF1u16.to_le_bytes());
    v.extend_from_slice(&guid(GUID_PKCS7));
    v.extend_from_slice(&p7);
    v.extend_from_slice(dados);
    Ok(v)
}

/// Grava a variável no efivarfs (atributos + carga autenticada).
fn gravar_var(nome: &str, guid_var: &str, attr: u32, carga: &[u8]) -> Result<(), String> {
    let caminho = format!("{EFIVARS}/{nome}-{guid_var}");
    // Variáveis que já existem vêm imutáveis no efivarfs.
    if Path::new(&caminho).exists() {
        let _ = Command::new("chattr").args(["-i", &caminho]).status();
    }
    let mut conteudo = attr.to_le_bytes().to_vec();
    conteudo.extend_from_slice(carga);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&caminho)
        .map_err(|e| format!("{nome}: {e}"))?;
    // O efivarfs exige a variável inteira numa única escrita.
    f.write_all(&conteudo).map_err(|e| format!("o firmware recusou {nome}: {e}"))
}

fn gerar_par(dir: &Path, nome: &str, cn: &str) -> Result<(), String> {
    rodar(
        Command::new("openssl")
            .args(["req", "-new", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "36500", "-nodes", "-subj"])
            .arg(format!("/CN={cn}/"))
            .arg("-keyout")
            .arg(dir.join(format!("{nome}.key")))
            .arg("-out")
            .arg(dir.join(format!("{nome}.crt"))),
        &format!("chave {nome}"),
    )?;
    rodar(
        Command::new("openssl")
            .args(["x509", "-outform", "DER", "-in"])
            .arg(dir.join(format!("{nome}.crt")))
            .arg("-out")
            .arg(dir.join(format!("{nome}.der"))),
        &format!("certificado {nome}"),
    )?;
    let _ = fs::set_permissions(dir.join(format!("{nome}.key")), fs::Permissions::from_mode(0o600));
    Ok(())
}

/// Gera as chaves em `dir` (no disco novo), assina `efi` e cadastra no
/// firmware. Só chame com o firmware em modo de configuração.
pub fn configurar(dir: &Path, efis: &[PathBuf]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let nome_pc = fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_default();
    let base = if nome_pc.is_empty() { "Ararinha Azul Linux".to_string() } else { format!("Ararinha Azul Linux ({nome_pc})") };
    for (n, cn) in [("PK", "Plataforma"), ("KEK", "KEK"), ("db", "Assinatura")] {
        gerar_par(dir, n, &format!("{base} {cn}"))?;
    }

    // Kernel assinado com a chave db.
    let sbsign = sbsign().ok_or("falta o systemd-sbsign")?;
    for efi in efis {
        let assinado = efi.with_extension("assinado");
        rodar(
            Command::new(&sbsign)
                .arg("sign")
                .arg(format!("--private-key={}", dir.join("db.key").display()))
                .arg(format!("--certificate={}", dir.join("db.crt").display()))
                .arg(format!("--output={}", assinado.display()))
                .arg(efi),
            "assinar o kernel",
        )?;
        fs::rename(&assinado, efi).map_err(|e| format!("kernel assinado: {e}"))?;
    }

    let dono = guid_novo();
    fs::write(dir.join("dono.guid"), dono).map_err(|e| e.to_string())?;
    let ler = |n: &str| fs::read(dir.join(format!("{n}.der"))).map_err(|e| format!("{n}: {e}"));
    let (pk, kek, db) = (ler("PK")?, ler("KEK")?, ler("db")?);
    let k = |n: &str| dir.join(format!("{n}.key"));
    let c = |n: &str| dir.join(format!("{n}.crt"));

    // db e KEK acrescentados; PK por último (sai do modo de configuração).
    let lista_db = lista_assinaturas(&db, &dono);
    let carga = autenticada(dir, "db", GUID_DB, ATTR | ACRESCENTAR, &lista_db, &k("KEK"), &c("KEK"))?;
    gravar_var("db", GUID_DB, ATTR | ACRESCENTAR, &carga)?;
    let lista_kek = lista_assinaturas(&kek, &dono);
    let carga = autenticada(dir, "KEK", GUID_GLOBAL, ATTR | ACRESCENTAR, &lista_kek, &k("PK"), &c("PK"))?;
    gravar_var("KEK", GUID_GLOBAL, ATTR | ACRESCENTAR, &carga)?;
    let lista_pk = lista_assinaturas(&pk, &dono);
    let carga = autenticada(dir, "PK", GUID_GLOBAL, ATTR, &lista_pk, &k("PK"), &c("PK"))?;
    gravar_var("PK", GUID_GLOBAL, ATTR, &carga)?;
    Ok(())
}
