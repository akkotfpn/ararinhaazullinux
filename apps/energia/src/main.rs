// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-energia: aplica o plano de energia escolhido nas Configurações.
//!
//! Instalado com setuid root, só aceita os três planos e só escreve em
//! arquivos fixos do /sys e em /var/lib/ararinha/energia:
//!
//!   ararinha-energia padrao|economico|desempenho   (escolhe e aplica)
//!   ararinha-energia aplicar                       (reaplica o salvo; boot)
//!   ararinha-energia                               (mostra o plano atual)
//!
//! O que cada plano faz:
//!   Padrão       governador schedutil (ou o equilíbrio do driver), EPP
//!                "balance_performance", perfil de plataforma "balanced".
//!   Econômico    governador powersave, EPP "power", perfil "low-power",
//!                sem turbo. Bom para notebook na bateria.
//!   Desempenho   governador performance, EPP "performance", perfil
//!                "performance", turbo liberado.

use std::{fs, path::Path, process::exit};

const SALVO: &str = "/var/lib/ararinha/energia";
const PLANOS: [&str; 3] = ["padrao", "economico", "desempenho"];

fn ler(p: &str) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn escrever(p: &str, v: &str) -> bool {
    Path::new(p).exists() && fs::write(p, v).is_ok()
}

/// Escreve no mesmo arquivo de cada núcleo; devolve quantos aceitaram.
fn em_cada_cpu(arquivo: &str, valor: &str) -> usize {
    let Ok(rd) = fs::read_dir("/sys/devices/system/cpu") else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let nome = e.file_name();
        let nome = nome.to_string_lossy();
        if !(nome.starts_with("cpu") && nome[3..].chars().all(|c| c.is_ascii_digit()) && nome.len() > 3) {
            continue;
        }
        let p = format!("/sys/devices/system/cpu/{nome}/cpufreq/{arquivo}");
        if escrever(&p, valor) {
            n += 1;
        }
    }
    n
}

fn aplicar(plano: &str) -> Vec<String> {
    let mut feito = Vec::new();
    let disponiveis = ler("/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors").unwrap_or_default();
    let tem = |g: &str| disponiveis.split_whitespace().any(|x| x == g);
    let governador = match plano {
        "economico" => "powersave",
        "desempenho" => "performance",
        // intel_pstate/amd-pstate ativos só têm performance/powersave;
        // o equilíbrio fica a cargo do EPP.
        _ if tem("schedutil") => "schedutil",
        _ if tem("ondemand") => "ondemand",
        _ => "powersave",
    };
    if tem(governador) && em_cada_cpu("scaling_governor", governador) > 0 {
        feito.push(format!("processador: {governador}"));
    }
    let epp = match plano {
        "economico" => "power",
        "desempenho" => "performance",
        _ => "balance_performance",
    };
    if em_cada_cpu("energy_performance_preference", epp) > 0 {
        feito.push(format!("preferência: {epp}"));
    }
    let perfil = match plano {
        "economico" => "low-power",
        "desempenho" => "performance",
        _ => "balanced",
    };
    let escolhas = ler("/sys/firmware/acpi/platform_profile_choices").unwrap_or_default();
    if escolhas.split_whitespace().any(|x| x == perfil) && escrever("/sys/firmware/acpi/platform_profile", perfil) {
        feito.push(format!("perfil: {perfil}"));
    }
    // Turbo: desligado só no econômico.
    let sem_turbo = plano == "economico";
    if escrever("/sys/devices/system/cpu/intel_pstate/no_turbo", if sem_turbo { "1" } else { "0" })
        || escrever("/sys/devices/system/cpu/cpufreq/boost", if sem_turbo { "0" } else { "1" })
    {
        feito.push(format!("turbo: {}", if sem_turbo { "desligado" } else { "ligado" }));
    }
    feito
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let atual = ler(SALVO).filter(|p| PLANOS.contains(&p.as_str())).unwrap_or_else(|| "padrao".into());
    match args.get(1).map(String::as_str) {
        None => println!("{atual}"),
        Some("aplicar") => {
            let f = aplicar(&atual);
            println!("{atual}: {}", if f.is_empty() { "nada a ajustar neste computador".into() } else { f.join(", ") });
        }
        Some(p) if PLANOS.contains(&p) => {
            let _ = fs::create_dir_all("/var/lib/ararinha");
            if fs::write(SALVO, format!("{p}\n")).is_err() {
                eprintln!("Não deu para salvar o plano de energia.");
                exit(1);
            }
            let f = aplicar(p);
            if f.is_empty() {
                println!("Plano salvo. Este computador não deixa ajustar o processador (máquina virtual?).");
            } else {
                println!("Plano aplicado ({}).", f.join(", "));
            }
        }
        Some(_) => {
            eprintln!("Uso: ararinha-energia [padrao|economico|desempenho|aplicar]");
            exit(2);
        }
    }
}
