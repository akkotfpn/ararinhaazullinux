// SPDX-License-Identifier: GPL-3.0-or-later
//! Contas e relógio do sistema, pedidos pelas Configurações.
//!
//! O ararinha-sistema roda confinado (não escreve no /etc nem no /home).
//! Para criar contas e mudar o fuso, ele confere o pedido e entrega o
//! trabalho ao systemd numa unidade passageira (`systemd-run`), que roda
//! sem o confinamento e aparece no journal.

use std::{
    io::Write,
    process::{Command, Stdio},
};

/// Roda um comando como root, fora do confinamento, esperando o fim.
/// `entrada` vai pela entrada padrão (senhas nunca na linha de comando).
pub(crate) fn rodar_fora(args: &[&str], entrada: Option<&str>) -> Result<(), String> {
    let mut c = Command::new("/usr/bin/systemd-run");
    c.args(["--quiet", "--wait", "--collect", "--pipe", "--property=PrivateNetwork=yes", "--"]);
    c.args(args);
    c.stdin(if entrada.is_some() { Stdio::piped() } else { Stdio::null() });
    c.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut f = c.spawn().map_err(|e| format!("systemd-run: {e}"))?;
    if let (Some(texto), Some(mut e)) = (entrada, f.stdin.take()) {
        let _ = e.write_all(texto.as_bytes());
    }
    let s = f.wait_with_output().map_err(|e| e.to_string())?;
    if s.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&s.stderr).trim().lines().last().unwrap_or("falhou").to_string())
    }
}

/// Como `rodar_fora`, mas devolve o que o comando escreveu (para ler
/// dispositivos que o serviço confinado não enxerga).
pub(crate) fn rodar_fora_saida(args: &[&str]) -> String {
    Command::new("/usr/bin/systemd-run")
        .args(["--quiet", "--wait", "--collect", "--pipe", "--property=PrivateNetwork=yes", "--"])
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn de_hex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let b: Option<Vec<u8>> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect();
    String::from_utf8(b?).ok()
}

fn login_valido(l: &str) -> bool {
    let mut c = l.chars();
    matches!(c.next(), Some('a'..='z' | '_'))
        && l.len() <= 32
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_' || x == '-')
        && !["root", "bin", "daemon", "nobody", "messagebus", "ninho"].contains(&l)
}

fn existe(login: &str) -> bool {
    std::fs::read_to_string("/etc/passwd").unwrap_or_default().lines().any(|l| l.split(':').next() == Some(login))
}

fn grupos_existentes(quer: &[&str]) -> Vec<String> {
    let g = std::fs::read_to_string("/etc/group").unwrap_or_default();
    quer.iter().filter(|n| g.lines().any(|l| l.split(':').next() == Some(**n))).map(|s| s.to_string()).collect()
}

/// Lista: "login|nome|admin(sim/nao)" separados por ';'.
pub fn listar() -> String {
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let grupos = std::fs::read_to_string("/etc/group").unwrap_or_default();
    let admins: Vec<&str> = grupos
        .lines()
        .find(|l| l.split(':').next() == Some("wheel"))
        .and_then(|l| l.split(':').nth(3))
        .map(|m| m.split(',').collect())
        .unwrap_or_default();
    passwd
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split(':').collect();
            let uid: u32 = c.get(2)?.parse().ok()?;
            if !(1000..60000).contains(&uid) {
                return None;
            }
            let nome = c.get(4).map(|g| g.split(',').next().unwrap_or("")).unwrap_or("");
            let nome: String = nome.chars().filter(|x| !matches!(x, '|' | ';')).collect();
            Some(format!("{}|{}|{}", c[0], nome, if admins.contains(&c[0]) { "sim" } else { "nao" }))
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub fn criar(login: &str, tipo: &str, nome_hex: &str, senha_hex: &str) -> Result<String, String> {
    if !login_valido(login) {
        return Err("nome de usuário inválido (letras minúsculas, números, - e _)".into());
    }
    if existe(login) {
        return Err(format!("o usuário \"{login}\" já existe"));
    }
    let nome = de_hex(nome_hex).ok_or("nome inválido")?;
    let senha = de_hex(senha_hex).ok_or("senha inválida")?;
    if senha.chars().count() < 4 || senha.contains('\n') || senha.contains(':') {
        return Err("a senha precisa de pelo menos 4 caracteres (sem \":\")".into());
    }
    let nome: String = nome.chars().filter(|c| !matches!(c, ':' | ',' | '\n')).take(64).collect();
    let mut grupos = grupos_existentes(&["seat", "video", "audio", "input", "render", "netdev"]);
    if tipo == "admin" {
        grupos.extend(grupos_existentes(&["wheel"]));
    }
    let g = grupos.join(",");
    let mut args = vec!["/usr/sbin/useradd", "-m", "-s", "/bin/bash", "-c", nome.as_str()];
    if !g.is_empty() {
        args.extend(["-G", g.as_str()]);
    }
    args.push(login);
    rodar_fora(&args, None)?;
    rodar_fora(&["/usr/sbin/chpasswd"], Some(&format!("{login}:{senha}\n")))?;
    Ok(listar())
}

pub fn remover(login: &str, quem: &str) -> Result<String, String> {
    if !login_valido(login) || !existe(login) {
        return Err("conta não encontrada".into());
    }
    if login == quem {
        return Err("não dá para apagar a conta em uso".into());
    }
    rodar_fora(&["/usr/sbin/userdel", "-r", login], None)?;
    Ok(listar())
}

pub fn tipo(login: &str, admin: bool, quem: &str) -> Result<String, String> {
    if !login_valido(login) || !existe(login) {
        return Err("conta não encontrada".into());
    }
    if login == quem && !admin {
        return Err("não dá para tirar o administrador da própria conta".into());
    }
    let args = if admin { ["/usr/bin/gpasswd", "-a", login, "wheel"] } else { ["/usr/bin/gpasswd", "-d", login, "wheel"] };
    rodar_fora(&args, None)?;
    Ok(listar())
}

/// "fuso=America/Sao_Paulo ntp=sim hora=2026-11-11 13:00"
pub fn hora_estado() -> String {
    let fuso = std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_str().and_then(|s| s.split("zoneinfo/").nth(1)).map(str::to_string))
        .unwrap_or_else(|| "UTC".into());
    let ntp = Command::new("/usr/bin/timedatectl")
        .args(["show", "-p", "NTP", "--value"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
        .unwrap_or(false);
    format!("fuso={fuso} ntp={}", if ntp { "sim" } else { "nao" })
}

pub fn fuso(z: &str) -> Result<String, String> {
    let valido = !z.contains("..") && z.chars().all(|c| c.is_ascii_alphanumeric() || "/_-+".contains(c));
    if !valido || !std::path::Path::new(&format!("/usr/share/zoneinfo/{z}")).is_file() {
        return Err("fuso horário desconhecido".into());
    }
    rodar_fora(&["/usr/bin/timedatectl", "set-timezone", z], None)?;
    Ok(hora_estado())
}

pub fn ntp(sim: bool) -> Result<String, String> {
    rodar_fora(&["/usr/bin/timedatectl", "set-ntp", if sim { "true" } else { "false" }], None)?;
    Ok(hora_estado())
}

/// LANG do /etc/locale.conf.
pub fn idioma() -> String {
    std::fs::read_to_string("/etc/locale.conf")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("LANG=").map(|v| v.trim_matches('"').to_string()))
        .unwrap_or_else(|| "pt_BR.UTF-8".into())
}

pub fn definir_idioma(id: &str) -> Result<String, String> {
    if !["pt_BR.UTF-8", "en_US.UTF-8"].contains(&id) {
        return Err("idioma não disponível".into());
    }
    rodar_fora(&["/usr/bin/localectl", "set-locale", &format!("LANG={id}")], None)?;
    Ok(idioma())
}
