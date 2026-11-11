// SPDX-License-Identifier: GPL-3.0-or-later
//! Firewall: perfil (rede pública ou de casa) e portas liberadas.
//!
//! O estado fica em /etc/ararinha/firewall.conf; as regras extras são
//! geradas em /etc/ararinha/firewall.nft, que o /etc/nftables.conf inclui
//! no fim. Sem o arquivo, vale o perfil "rede pública" (o mais fechado).

use std::{fs, process::Command};

const CONF: &str = "/etc/ararinha/firewall.conf";
const REGRAS: &str = "/etc/ararinha/firewall.nft";
/// Existe quando a pessoa desligou o firewall (o boot respeita).
const DESLIGADO: &str = "/etc/ararinha/firewall-desligado";

#[derive(Default)]
struct Estado {
    casa: bool,
    portas: Vec<(u16, &'static str)>,
}

fn protocolo(p: &str) -> Option<&'static str> {
    match p {
        "tcp" => Some("tcp"),
        "udp" => Some("udp"),
        _ => None,
    }
}

/// "8080/tcp" → (8080, "tcp"), só portas 1..65535 e tcp/udp.
fn ler_porta(texto: &str) -> Option<(u16, &'static str)> {
    let (n, p) = texto.split_once('/')?;
    let n: u16 = n.parse().ok().filter(|n| *n > 0)?;
    Some((n, protocolo(p)?))
}

fn ler() -> Estado {
    let mut e = Estado::default();
    for l in fs::read_to_string(CONF).unwrap_or_default().lines() {
        let Some((k, v)) = l.split_once('=') else { continue };
        match k.trim() {
            "perfil" => e.casa = v.trim() == "casa",
            "porta" => {
                if let Some(p) = ler_porta(v.trim()) {
                    if !e.portas.contains(&p) {
                        e.portas.push(p);
                    }
                }
            }
            _ => {}
        }
    }
    e
}

fn gravar_e_aplicar(e: &Estado) -> Result<(), String> {
    let _ = fs::create_dir_all("/etc/ararinha");
    let mut conf = String::from("# Firewall do Ararinha (Configurações > Firewall)\n");
    conf += &format!("perfil = {}\n", if e.casa { "casa" } else { "publica" });
    for (n, p) in &e.portas {
        conf += &format!("porta = {n}/{p}\n");
    }
    let mut nft = String::from("# Gerado pelo ararinha-sistema; não edite (use Configurações > Firewall).\n");
    if e.casa {
        // Rede de casa: responde ao ping dos outros aparelhos.
        nft += "add rule inet ararinha entrada icmp type echo-request accept\n";
        nft += "add rule inet ararinha entrada icmpv6 type echo-request accept\n";
    }
    for (n, p) in &e.portas {
        nft += &format!("add rule inet ararinha entrada {p} dport {n} accept\n");
    }
    fs::write(CONF, conf).map_err(|e| format!("{CONF}: {e}"))?;
    fs::write(REGRAS, nft).map_err(|e| format!("{REGRAS}: {e}"))?;
    if std::path::Path::new(DESLIGADO).exists() {
        // Desligado: guarda as escolhas, mas não carrega regras.
        return Ok(());
    }
    let s = Command::new("/usr/sbin/nft")
        .args(["-f", "/etc/nftables.conf"])
        .output()
        .map_err(|e| format!("nft: {e}"))?;
    if !s.status.success() {
        return Err(format!("nft: {}", String::from_utf8_lossy(&s.stderr).trim()));
    }
    Ok(())
}

/// "ativo=sim perfil=casa portas=22/tcp,8080/udp"
pub fn estado() -> String {
    let e = ler();
    let ativo = Command::new("/usr/sbin/nft")
        .args(["list", "table", "inet", "ararinha"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let portas: Vec<String> = e.portas.iter().map(|(n, p)| format!("{n}/{p}")).collect();
    format!(
        "ativo={} perfil={} portas={}",
        if ativo { "sim" } else { "nao" },
        if e.casa { "casa" } else { "publica" },
        portas.join(",")
    )
}

/// Liga ou desliga o firewall (e lembra para o próximo boot).
pub fn ligar(sim: bool) -> Result<String, String> {
    if sim {
        let _ = fs::remove_file(DESLIGADO);
        gravar_e_aplicar(&ler())?;
    } else {
        fs::write(DESLIGADO, "").map_err(|e| format!("{DESLIGADO}: {e}"))?;
        let s = Command::new("/usr/sbin/nft").args(["flush", "ruleset"]).output().map_err(|e| format!("nft: {e}"))?;
        if !s.status.success() {
            return Err(format!("nft: {}", String::from_utf8_lossy(&s.stderr).trim()));
        }
    }
    Ok(estado())
}

pub fn definir_perfil(p: &str) -> Result<String, String> {
    let mut e = ler();
    e.casa = match p {
        "casa" => true,
        "publica" => false,
        _ => return Err("perfil deve ser casa ou publica".into()),
    };
    gravar_e_aplicar(&e)?;
    Ok(estado())
}

pub fn abrir(porta: &str) -> Result<String, String> {
    let p = ler_porta(porta).ok_or("porta inválida (use número/tcp ou número/udp)")?;
    let mut e = ler();
    if !e.portas.contains(&p) {
        if e.portas.len() >= 64 {
            return Err("portas demais liberadas".into());
        }
        e.portas.push(p);
    }
    gravar_e_aplicar(&e)?;
    Ok(estado())
}

pub fn fechar(porta: &str) -> Result<String, String> {
    let p = ler_porta(porta).ok_or("porta inválida")?;
    let mut e = ler();
    e.portas.retain(|x| *x != p);
    gravar_e_aplicar(&e)?;
    Ok(estado())
}
