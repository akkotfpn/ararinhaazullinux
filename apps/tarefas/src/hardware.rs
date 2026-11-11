// SPDX-License-Identifier: GPL-3.0-or-later
//! Aba "Hardware": o que tem dentro do computador. Lê o /proc e o /sys
//! (qualquer usuário pode) e pede ao serviço do sistema os pentes de
//! memória (a tabela SMBIOS só o root lê).

use std::fs;

#[derive(Clone, Default)]
pub struct Modulo {
    pub slot: String,
    pub mb: u64,
    pub tipo: String,
    pub mts: String,
    pub fabricante: String,
    pub modelo: String,
}

#[derive(Clone, Default)]
pub struct Memoria {
    pub slots: u32,
    pub maximo_mb: u64,
    pub modulos: Vec<Modulo>,
}

impl Memoria {
    pub fn usados(&self) -> usize {
        self.modulos.iter().filter(|m| m.mb > 0).count()
    }
    pub fn instalada_mb(&self) -> u64 {
        self.modulos.iter().map(|m| m.mb).sum()
    }
}

/// Pede ao ararinha-sistema (bloqueia; chamar numa thread).
pub fn memoria() -> Result<Memoria, String> {
    let r = penas::sistema::pedir("hardware memoria")?;
    let mut m = Memoria::default();
    for parte in r.split(';') {
        match parte.split_once('=') {
            Some(("slots", v)) => m.slots = v.parse().unwrap_or(0),
            Some(("maximo_mb", v)) => m.maximo_mb = v.parse().unwrap_or(0),
            Some(("modulo", v)) => {
                let c: Vec<&str> = v.split('|').collect();
                let g = |i: usize| c.get(i).map(|s| s.to_string()).unwrap_or_default();
                m.modulos.push(Modulo {
                    slot: g(0),
                    mb: g(1).parse().unwrap_or(0),
                    tipo: g(2),
                    mts: g(3),
                    fabricante: g(4),
                    modelo: g(5),
                });
            }
            _ => {}
        }
    }
    Ok(m)
}

fn ler(p: &str) -> String {
    fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Nome do processador, sem os excessos de marketing.
pub fn modelo_cpu() -> String {
    let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let nome = info
        .lines()
        .find(|l| l.starts_with("model name") || l.starts_with("Model") || l.starts_with("Hardware"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| "Processador".into());
    nome.replace("(R)", "").replace("(TM)", "").replace("(tm)", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct Cpu {
    pub modelo: String,
    pub nucleos: usize,
    pub threads: usize,
    pub ghz_max: Option<f64>,
    pub cache: String,
    pub virtualizacao: bool,
}

pub fn cpu() -> Cpu {
    let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let threads = info.lines().filter(|l| l.starts_with("processor")).count().max(1);
    let mut nucleos_fisicos = std::collections::HashSet::new();
    let mut fisico = String::new();
    for l in info.lines() {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim() {
                "physical id" => fisico = v.trim().to_string(),
                "core id" => {
                    nucleos_fisicos.insert(format!("{fisico}-{}", v.trim()));
                }
                _ => {}
            }
        }
    }
    let nucleos = if nucleos_fisicos.is_empty() { threads } else { nucleos_fisicos.len() };
    let khz: f64 = ler("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq").parse().unwrap_or(0.0);
    let ghz_max = (khz > 0.0).then_some(khz / 1_000_000.0).or_else(|| {
        info.lines()
            .find(|l| l.starts_with("cpu MHz"))
            .and_then(|l| l.split_once(':'))
            .and_then(|(_, v)| v.trim().parse::<f64>().ok())
            .map(|m| m / 1000.0)
    });
    let cache = info
        .lines()
        .find(|l| l.starts_with("cache size"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().replace("KB", "KB").to_string())
        .unwrap_or_default();
    let virtualizacao = info.contains(" vmx") || info.contains(" svm");
    Cpu { modelo: modelo_cpu(), nucleos, threads, ghz_max, cache, virtualizacao }
}

/// Seções da aba Hardware: (título, [(rótulo, valor)]).
pub fn secoes() -> Vec<(String, Vec<(String, String)>)> {
    let mut s = Vec::new();

    // Computador e placa-mãe.
    let fab = ler("/sys/class/dmi/id/sys_vendor");
    let produto = ler("/sys/class/dmi/id/product_name");
    let placa = format!("{} {}", ler("/sys/class/dmi/id/board_vendor"), ler("/sys/class/dmi/id/board_name")).trim().to_string();
    let bios = format!("{} {} ({})", ler("/sys/class/dmi/id/bios_vendor"), ler("/sys/class/dmi/id/bios_version"), ler("/sys/class/dmi/id/bios_date"));
    let mut v = vec![("Fabricante".into(), fab), ("Modelo".into(), produto)];
    if !placa.is_empty() {
        v.push(("Placa-mãe".into(), placa));
    }
    v.push(("BIOS/UEFI".into(), bios.trim().to_string()));
    let uefi = std::path::Path::new("/sys/firmware/efi").exists();
    v.push(("Inicialização".into(), if uefi { "UEFI".into() } else { "BIOS (legado)".into() }));
    s.push(("Computador".to_string(), v.into_iter().filter(|(_, b): &(String, String)| !b.trim().is_empty() && b.trim() != "()").collect()));

    // Processador.
    let c = cpu();
    let mut v = vec![("Modelo".to_string(), c.modelo.clone())];
    v.push(("Núcleos".into(), format!("{} núcleos, {} threads", c.nucleos, c.threads)));
    if let Some(g) = c.ghz_max {
        v.push(("Velocidade máxima".into(), format!("{g:.2} GHz").replace('.', ",")));
    }
    if !c.cache.is_empty() {
        v.push(("Cache".into(), c.cache));
    }
    v.push(("Virtualização".into(), if c.virtualizacao { "Disponível".into() } else { "Não disponível".into() }));
    s.push(("Processador".to_string(), v));

    // Placa de vídeo.
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir("/sys/class/drm") {
        let mut nomes: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("card") && !n.contains('-')).collect();
        nomes.sort();
        for n in nomes {
            let base = format!("/sys/class/drm/{n}/device");
            let vendor = ler(&format!("{base}/vendor"));
            let device = ler(&format!("{base}/device"));
            let driver = fs::read_link(format!("{base}/driver"))
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_default();
            let marca = match vendor.as_str() {
                "0x8086" => "Intel",
                "0x10de" => "NVIDIA",
                "0x1002" => "AMD",
                "0x1af4" => "Virtio (máquina virtual)",
                "0x1234" => "Bochs/QEMU (máquina virtual)",
                "0x15ad" => "VMware (máquina virtual)",
                "0x80ee" => "VirtualBox (máquina virtual)",
                _ => "",
            };
            let nome = if marca.is_empty() { format!("{vendor}:{device}") } else { marca.to_string() };
            let vram = ler(&format!("{base}/mem_info_vram_total")).parse::<u64>().ok().map(|b| format!(" · {} MB de vídeo", b / 1024 / 1024)).unwrap_or_default();
            v.push((format!("Placa {}", v.len() + 1), format!("{nome} (driver {driver}{vram})")));
        }
    }
    if v.is_empty() {
        v.push(("Placa".into(), "Tela do firmware (sem driver de vídeo)".into()));
    }
    s.push(("Vídeo".to_string(), v));

    // Discos.
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir("/sys/block") {
        let mut nomes: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        nomes.sort();
        for n in nomes {
            if n.starts_with("loop") || n.starts_with("ram") || n.starts_with("zram") || n.starts_with("dm-") || n.starts_with("sr") {
                continue;
            }
            let setores: u64 = ler(&format!("/sys/block/{n}/size")).parse().unwrap_or(0);
            if setores == 0 {
                continue;
            }
            let gb = setores as f64 * 512.0 / 1e9;
            let modelo = ler(&format!("/sys/block/{n}/device/model"));
            let tipo = if n.starts_with("nvme") {
                "SSD NVMe"
            } else if ler(&format!("/sys/block/{n}/queue/rotational")) == "1" {
                "HD"
            } else {
                "SSD"
            };
            let usb = fs::read_link(format!("/sys/block/{n}")).map(|p| p.to_string_lossy().contains("/usb")).unwrap_or(false);
            let tam = if gb >= 1000.0 { format!("{:.1} TB", gb / 1000.0) } else { format!("{gb:.0} GB") };
            let modelo = if modelo.is_empty() { n.clone() } else { modelo };
            v.push((n.clone(), format!("{modelo} · {tam} · {}{}", tipo, if usb { " (USB)" } else { "" }).replace('.', ",")));
        }
    }
    s.push(("Armazenamento".to_string(), v));

    // Rede.
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir("/sys/class/net") {
        let mut nomes: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n != "lo").collect();
        nomes.sort();
        for n in nomes {
            if !std::path::Path::new(&format!("/sys/class/net/{n}/device")).exists() {
                continue;
            }
            let sem_fio = std::path::Path::new(&format!("/sys/class/net/{n}/wireless")).exists();
            let driver = fs::read_link(format!("/sys/class/net/{n}/device/driver"))
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_default();
            let vel = ler(&format!("/sys/class/net/{n}/speed")).parse::<i64>().ok().filter(|v| *v > 0).map(|v| format!(" · {v} Mb/s")).unwrap_or_default();
            v.push((n.clone(), format!("{} (driver {driver}){vel}", if sem_fio { "Wi-Fi" } else { "Cabo (Ethernet)" })));
        }
    }
    s.push(("Rede".to_string(), v));

    // Som.
    let cartoes = fs::read_to_string("/proc/asound/cards").unwrap_or_default();
    let v: Vec<(String, String)> = cartoes
        .lines()
        .filter(|l| l.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()))
        .filter_map(|l| l.split_once("]:").map(|(_, d)| ("Placa".to_string(), d.trim().split(" - ").last().unwrap_or("").trim().to_string())))
        .collect();
    if !v.is_empty() {
        s.push(("Som".to_string(), v));
    }

    // Bateria.
    if let Ok(rd) = fs::read_dir("/sys/class/power_supply") {
        let mut v = Vec::new();
        for e in rd.flatten() {
            let b = e.path();
            if ler(&b.join("type").to_string_lossy()) != "Battery" {
                continue;
            }
            let cap = ler(&b.join("capacity").to_string_lossy());
            let cheio: f64 = ler(&b.join("energy_full").to_string_lossy()).parse().unwrap_or(0.0);
            let projeto: f64 = ler(&b.join("energy_full_design").to_string_lossy()).parse().unwrap_or(0.0);
            let saude = if projeto > 0.0 { format!(" · saúde {:.0}%", cheio / projeto * 100.0) } else { String::new() };
            v.push(("Bateria".into(), format!("{cap}% carregada{saude}")));
        }
        if !v.is_empty() {
            s.push(("Energia".to_string(), v));
        }
    }
    s
}
