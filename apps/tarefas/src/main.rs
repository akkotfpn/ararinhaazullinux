// SPDX-License-Identifier: GPL-3.0-or-later
//! Gerenciador de Tarefas do Ararinha: processos (com Finalizar tarefa),
//! desempenho (gráficos de processador e memória) e hardware. Lê tudo do /proc a cada
//! segundo e só redesenha quando há dado novo ou o usuário mexe.

use std::{
    collections::HashMap,
    fs,
    time::{Duration, Instant},
};

use penas::{desenho, tema, App, Cor, Opcoes, Tecla, Ui};

mod hardware;

const HISTORICO: usize = 60;
const LINHA_A: f32 = 30.0;

#[derive(Clone, Copy, PartialEq)]
enum Aba {
    Processos,
    Desempenho,
    Hardware,
}

#[derive(Clone, Copy, PartialEq)]
enum Ordem {
    Nome,
    Pid,
    Cpu,
    Memoria,
}

struct Processo {
    pid: u32,
    nome: String,
    usuario: u32,
    cpu: f32,
    memoria_kb: u64,
}

struct Tarefas {
    aba: Aba,
    ordem: Ordem,
    processos: Vec<Processo>,
    selecionado: Option<u32>,
    rolagem: f32,
    // Amostras anteriores, para calcular o uso entre leituras.
    cpu_total_ant: u64,
    cpu_ocioso_ant: u64,
    tempos_ant: HashMap<u32, u64>,
    total_ant_proc: u64,
    nucleos: usize,
    hist_cpu: Vec<f32>,
    hist_mem: Vec<f32>,
    mem_total_kb: u64,
    mem_usada_kb: u64,
    proxima: Instant,
    aviso: Option<String>,
    meu_uid: u32,
    cpu_modelo: String,
    memoria: Option<Result<hardware::Memoria, String>>,
    rx_memoria: Option<std::sync::mpsc::Receiver<Result<hardware::Memoria, String>>>,
    secoes: Option<Vec<(String, Vec<(String, String)>)>>,
    rolagem_hw: f32,
}

fn ler_cpu() -> (u64, u64) {
    let t = fs::read_to_string("/proc/stat").unwrap_or_default();
    let l = t.lines().next().unwrap_or("");
    let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    let total: u64 = v.iter().take(8).sum();
    let ocioso = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    (total, ocioso)
}

fn ler_mem() -> (u64, u64) {
    let t = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let campo = |nome: &str| {
        t.lines()
            .find(|l| l.starts_with(nome))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let total = campo("MemTotal:");
    let disp = campo("MemAvailable:");
    (total, total.saturating_sub(disp))
}

fn meu_uid() -> u32 {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// (nome, tempo de CPU em ticks) de /proc/<pid>/stat.
fn ler_stat(pid: u32) -> Option<(String, u64)> {
    let t = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let ini = t.find('(')?;
    let fim = t.rfind(')')?;
    let nome = t[ini + 1..fim].to_string();
    let resto: Vec<&str> = t[fim + 2..].split_whitespace().collect();
    // Depois do ")": estado é o campo 0; utime=11, stime=12.
    let ut: u64 = resto.get(11)?.parse().ok()?;
    let st: u64 = resto.get(12)?.parse().ok()?;
    Some((nome, ut + st))
}

fn ler_status(pid: u32) -> (u64, u32) {
    let t = fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let mut rss = 0;
    let mut uid = 0;
    for l in t.lines() {
        if let Some(v) = l.strip_prefix("VmRSS:") {
            rss = v.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0);
        } else if let Some(v) = l.strip_prefix("Uid:") {
            uid = v.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0);
        }
    }
    (rss, uid)
}

/// Nome amigável: o nome do executável completo quando o "comm" foi cortado.
fn nome_bonito(pid: u32, comm: String) -> String {
    if comm.len() >= 15 {
        if let Ok(c) = fs::read(format!("/proc/{pid}/cmdline")) {
            if let Some(prim) = c.split(|b| *b == 0).next() {
                let s = String::from_utf8_lossy(prim);
                if let Some(base) = s.rsplit('/').next() {
                    if base.starts_with(&comm[..comm.len().min(15)]) {
                        return base.to_string();
                    }
                }
            }
        }
    }
    comm
}

fn formatar_mem(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{:.1} GB", kb as f64 / 1024.0 / 1024.0).replace('.', ",")
    } else {
        format!("{:.1} MB", kb as f64 / 1024.0).replace('.', ",")
    }
}

impl Tarefas {
    fn novo() -> Tarefas {
        let nucleos = fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count()
            .max(1);
        let mut t = Tarefas {
            aba: Aba::Processos,
            ordem: Ordem::Cpu,
            processos: Vec::new(),
            selecionado: None,
            rolagem: 0.0,
            cpu_total_ant: 0,
            cpu_ocioso_ant: 0,
            tempos_ant: HashMap::new(),
            total_ant_proc: 0,
            nucleos,
            hist_cpu: Vec::new(),
            hist_mem: Vec::new(),
            mem_total_kb: 0,
            mem_usada_kb: 0,
            proxima: Instant::now(),
            aviso: None,
            meu_uid: meu_uid(),
            cpu_modelo: hardware::modelo_cpu(),
            memoria: None,
            rx_memoria: None,
            secoes: None,
            rolagem_hw: 0.0,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(hardware::memoria());
        });
        t.rx_memoria = Some(rx);
        t.amostrar();
        t
    }

    fn amostrar(&mut self) {
        let (total, ocioso) = ler_cpu();
        let dt = total.saturating_sub(self.cpu_total_ant);
        if self.cpu_total_ant != 0 && dt > 0 {
            let uso = 1.0 - ocioso.saturating_sub(self.cpu_ocioso_ant) as f32 / dt as f32;
            self.hist_cpu.push((uso * 100.0).clamp(0.0, 100.0));
        }
        let (mt, mu) = ler_mem();
        self.mem_total_kb = mt;
        self.mem_usada_kb = mu;
        if mt > 0 {
            self.hist_mem.push(mu as f32 / mt as f32 * 100.0);
        }
        for h in [&mut self.hist_cpu, &mut self.hist_mem] {
            if h.len() > HISTORICO {
                let sobra = h.len() - HISTORICO;
                h.drain(..sobra);
            }
        }

        let mut novos = HashMap::new();
        let mut lista = Vec::new();
        if let Ok(rd) = fs::read_dir("/proc") {
            for e in rd.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
                let Some((comm, tempo)) = ler_stat(pid) else { continue };
                let (rss, uid) = ler_status(pid);
                // Linhas do kernel (sem memória própria) não interessam.
                if rss == 0 {
                    continue;
                }
                let cpu = match self.tempos_ant.get(&pid) {
                    // A fração do tempo total da máquina, como no Windows.
                    Some(ant) if dt > 0 && self.total_ant_proc != 0 => {
                        tempo.saturating_sub(*ant) as f32 / dt as f32 * 100.0
                    }
                    _ => 0.0,
                };
                novos.insert(pid, tempo);
                lista.push(Processo { pid, nome: nome_bonito(pid, comm), usuario: uid, cpu, memoria_kb: rss });
            }
        }
        self.tempos_ant = novos;
        self.total_ant_proc = total;
        self.cpu_total_ant = total;
        self.cpu_ocioso_ant = ocioso;
        self.processos = lista;
        self.ordenar();
    }

    fn ordenar(&mut self) {
        match self.ordem {
            Ordem::Nome => self.processos.sort_by_key(|p| p.nome.to_lowercase()),
            Ordem::Pid => self.processos.sort_by_key(|p| p.pid),
            Ordem::Cpu => self
                .processos
                .sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memoria_kb.cmp(&a.memoria_kb))),
            Ordem::Memoria => self.processos.sort_by(|a, b| b.memoria_kb.cmp(&a.memoria_kb)),
        }
    }

    fn finalizar(&mut self) {
        let Some(pid) = self.selecionado else { return };
        let Some(p) = self.processos.iter().find(|p| p.pid == pid) else { return };
        if p.usuario != self.meu_uid && self.meu_uid != 0 {
            self.aviso = Some(format!("\"{}\" é do sistema; não dá para finalizar daqui.", p.nome));
            return;
        }
        let nome = p.nome.clone();
        match std::process::Command::new("kill").arg("-TERM").arg(pid.to_string()).status() {
            Ok(s) if s.success() => {
                self.aviso = Some(format!("\"{nome}\" foi finalizado."));
                self.selecionado = None;
                self.proxima = Instant::now() + Duration::from_millis(300);
            }
            _ => self.aviso = Some(format!("Não deu para finalizar \"{nome}\".")),
        }
    }

    fn processos(&mut self, ui: &mut Ui, y0: f32) {
        let (l, a) = (ui.largura(), ui.altura());
        let rodape = 52.0;
        let cols = [
            ("Nome", Ordem::Nome, 16.0),
            ("PID", Ordem::Pid, l - 330.0),
            ("CPU", Ordem::Cpu, l - 240.0),
            ("Memória", Ordem::Memoria, l - 140.0),
        ];
        // Cabeçalho com totais, como no Windows.
        let cpu_agora = self.hist_cpu.last().copied().unwrap_or(0.0);
        let mem_pct = if self.mem_total_kb > 0 { self.mem_usada_kb as f32 / self.mem_total_kb as f32 * 100.0 } else { 0.0 };
        ui.tela.retangulo(0.0, y0, l, 46.0, 0.0, tema::SUPERFICIE());
        for (titulo, ordem, x) in cols {
            let destaque = self.ordem == ordem;
            let texto = match ordem {
                Ordem::Cpu => format!("{:.0}%", cpu_agora),
                Ordem::Memoria => format!("{:.0}%", mem_pct),
                _ => String::new(),
            };
            if !texto.is_empty() {
                ui.tela.texto(x, y0 + 18.0, 14.0, tema::TEXTO(), &texto);
            }
            ui.tela.texto(x, y0 + 37.0, 12.5, if destaque { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() }, titulo);
            let largura_col = if ordem == Ordem::Nome { l - 360.0 } else { 90.0 };
            if ui.clicou(x - 6.0, y0, largura_col, 46.0) {
                self.ordem = ordem;
                self.ordenar();
            }
        }
        ui.tela.retangulo(0.0, y0 + 46.0, l, 1.0, 0.0, tema::BORDA());

        let lista_y = y0 + 47.0;
        let lista_a = a - lista_y - rodape;
        let conteudo = self.processos.len() as f32 * LINHA_A;
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (conteudo - lista_a).max(0.0));
        for t in &ui.teclas {
            let idx = self.selecionado.and_then(|s| self.processos.iter().position(|p| p.pid == s));
            let novo = match (t, idx) {
                (Tecla::Baixo, Some(i)) => Some((i + 1).min(self.processos.len().saturating_sub(1))),
                (Tecla::Cima, Some(i)) => Some(i.saturating_sub(1)),
                (Tecla::Baixo | Tecla::Cima, None) => Some(0),
                _ => None,
            };
            if let Some(i) = novo {
                self.selecionado = self.processos.get(i).map(|p| p.pid);
                let yi = i as f32 * LINHA_A;
                if yi < self.rolagem {
                    self.rolagem = yi;
                } else if yi + LINHA_A > self.rolagem + lista_a {
                    self.rolagem = yi + LINHA_A - lista_a;
                }
            }
        }
        let primeiro = (self.rolagem / LINHA_A) as usize;
        let mut clicado = None;
        for (i, p) in self.processos.iter().enumerate().skip(primeiro) {
            let y = lista_y + i as f32 * LINHA_A - self.rolagem;
            if y > lista_y + lista_a {
                break;
            }
            let sel = self.selecionado == Some(p.pid);
            let sobre = ui.sobre(0.0, y, l, LINHA_A) && ui.mouse.is_some_and(|(_, my)| my < lista_y + lista_a);
            if sel {
                ui.tela.retangulo(0.0, y, l, LINHA_A, 0.0, tema::DESTAQUE_SUAVE());
            } else if sobre {
                ui.tela.retangulo(0.0, y, l, LINHA_A, 0.0, tema::SUPERFICIE_ALTA());
            }
            // Fundo amarelado proporcional ao uso, como o Windows faz.
            let calor = |v: f32| Cor(255, 196, 0, (v.clamp(0.0, 100.0) * 1.4) as u8 + 18);
            ui.tela.retangulo(cols[2].2 - 8.0, y, 96.0, LINHA_A, 0.0, calor(p.cpu * 2.0));
            let mem_rel = p.memoria_kb as f32 / self.mem_total_kb.max(1) as f32 * 100.0;
            ui.tela.retangulo(cols[3].2 - 8.0, y, 124.0, LINHA_A, 0.0, calor(mem_rel * 4.0));
            let ty = y + 20.0;
            let nome = desenho::encurtar(13.0, &p.nome, l - 370.0);
            ui.tela.texto(16.0, ty, 13.0, tema::TEXTO(), &nome);
            ui.tela.texto(cols[1].2, ty, 13.0, tema::TEXTO_SECUNDARIO(), &p.pid.to_string());
            ui.tela.texto(cols[2].2, ty, 13.0, tema::TEXTO(), &format!("{:.1}%", p.cpu).replace('.', ","));
            ui.tela.texto(cols[3].2, ty, 13.0, tema::TEXTO(), &formatar_mem(p.memoria_kb));
            if (ui.clique || ui.clique_direito) && sobre {
                clicado = Some(p.pid);
            }
        }
        if let Some(pid) = clicado {
            self.selecionado = Some(pid);
        }
        // Barra de rolagem
        if conteudo > lista_a {
            let ba = (lista_a / conteudo * lista_a).max(24.0);
            let by = lista_y + self.rolagem / (conteudo - lista_a) * (lista_a - ba);
            ui.tela.retangulo(l - 7.0, by, 4.0, ba, 2.0, tema::BORDA());
        }

        // Rodapé
        let ry = a - rodape;
        ui.tela.retangulo(0.0, ry, l, rodape, 0.0, tema::FUNDO());
        ui.tela.retangulo(0.0, ry, l, 1.0, 0.0, tema::BORDA());
        let ativo = self.selecionado.is_some();
        let (bx, bl) = (l - 170.0, 154.0);
        let sobre = ativo && ui.sobre(bx, ry + 10.0, bl, 32.0);
        let fundo = if !ativo { tema::SUPERFICIE_ALTA() } else if sobre { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() };
        ui.tela.retangulo(bx, ry + 10.0, bl, 32.0, 6.0, fundo);
        let rot = "Finalizar tarefa";
        let lr = desenho::medir(13.0, rot);
        ui.tela.texto(bx + bl / 2.0 - lr / 2.0, ry + 31.0, 13.0, if ativo { tema::BRANCO() } else { tema::TEXTO_SECUNDARIO() }, rot);
        let apertou_del = ui.teclas.iter().any(|t| matches!(t, Tecla::Outra(k) if *k == penas::Keysym::Delete));
        if ativo && (ui.clicou(bx, ry + 10.0, bl, 32.0) || apertou_del) {
            self.finalizar();
        }
        let info = self
            .aviso
            .clone()
            .unwrap_or_else(|| format!("{} processos", self.processos.len()));
        ui.tela.texto(16.0, ry + 31.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, &info, l - 210.0));
    }

    fn grafico(ui: &mut Ui, x: f32, y: f32, l: f32, a: f32, hist: &[f32], cor: Cor, fundo: Cor) {
        ui.tela.retangulo(x, y, l, a, 0.0, cor);
        ui.tela.retangulo(x + 1.0, y + 1.0, l - 2.0, a - 2.0, 0.0, tema::JANELA());
        for i in 1..10 {
            let gx = x + l * i as f32 / 10.0;
            ui.tela.retangulo(gx, y + 1.0, 1.0, a - 2.0, 0.0, tema::SUPERFICIE_ALTA());
        }
        for i in 1..4 {
            let gy = y + a * i as f32 / 4.0;
            ui.tela.retangulo(x + 1.0, gy, l - 2.0, 1.0, 0.0, tema::SUPERFICIE_ALTA());
        }
        if hist.is_empty() {
            return;
        }
        let passo = (l - 2.0) / (HISTORICO - 1) as f32;
        let x_ini = x + l - 1.0 - (hist.len() - 1) as f32 * passo;
        let ponto = |i: usize| (x_ini + i as f32 * passo, y + a - 1.0 - hist[i] / 100.0 * (a - 2.0));
        // Área preenchida e linha por cima.
        let mut poli = vec![(x_ini, y + a - 1.0)];
        poli.extend((0..hist.len()).map(ponto));
        poli.push((x + l - 1.0, y + a - 1.0));
        if hist.len() > 1 {
            ui.tela.poligono(&poli, fundo);
        }
        for i in 1..hist.len() {
            let (x0, y0) = ponto(i - 1);
            let (x1, y1) = ponto(i);
            ui.tela.linha(x0, y0, x1, y1, 1.6, cor);
        }
    }

    fn desempenho(&mut self, ui: &mut Ui, y0: f32) {
        let (l, a) = (ui.largura(), ui.altura());
        let gl = l - 48.0;
        let ga = ((a - y0 - 214.0) / 2.0).max(70.0);
        let cpu = self.hist_cpu.last().copied().unwrap_or(0.0);
        let mut y = y0 + 34.0;
        ui.tela.texto(24.0, y, 18.0, tema::TEXTO(), "Processador");
        let t = format!("{:.0}%  ·  {} núcleos", cpu, self.nucleos);
        let lt = desenho::medir(13.0, &t);
        ui.tela.texto(l - 24.0 - lt, y, 13.0, tema::TEXTO_SECUNDARIO(), &t);
        ui.tela.texto(24.0, y + 20.0, 13.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(13.0, &self.cpu_modelo, gl));
        y += 32.0;
        Self::grafico(ui, 24.0, y, gl, ga, &self.hist_cpu, Cor::hex(0x2B7BC4), Cor(43, 123, 196, 40));
        ui.tela.texto(24.0, y + ga + 16.0, 11.5, tema::TEXTO_SECUNDARIO(), "60 segundos");
        y += ga + 50.0;
        ui.tela.texto(24.0, y, 18.0, tema::TEXTO(), "Memória");
        let t = format!("{} de {} em uso", formatar_mem(self.mem_usada_kb), formatar_mem(self.mem_total_kb));
        let lt = desenho::medir(13.0, &t);
        ui.tela.texto(l - 24.0 - lt, y, 13.0, tema::TEXTO_SECUNDARIO(), &t);
        let resumo = match &self.memoria {
            Some(Ok(m)) if !m.modulos.is_empty() => {
                let instalada = formatar_mem(m.instalada_mb() * 1024);
                let livres = (m.slots as usize).saturating_sub(m.usados());
                let tipo = m.modulos.iter().find(|x| x.mb > 0).map(|x| format!(" {}", x.tipo)).unwrap_or_default();
                format!(
                    "{instalada}{tipo} instalados  ·  {} de {} slots em uso{}",
                    m.usados(),
                    m.slots,
                    if livres > 0 { format!(" ({livres} livre{})", if livres > 1 { "s" } else { "" }) } else { String::new() }
                )
            }
            Some(Err(_)) => "Slots de memória: indisponível".into(),
            _ => "Lendo os slots de memória…".into(),
        };
        ui.tela.texto(24.0, y + 20.0, 13.0, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(13.0, &resumo, gl));
        y += 32.0;
        Self::grafico(ui, 24.0, y, gl, ga, &self.hist_mem, Cor::hex(0x8B3FB8), Cor(139, 63, 184, 40));
    }

    fn hardware(&mut self, ui: &mut Ui, y0: f32) {
        let l = ui.largura();
        let secoes = self.secoes.get_or_insert_with(hardware::secoes).clone();
        // Memória com os slots, logo depois do processador.
        let mut todas = secoes;
        let mem: Vec<(String, String)> = match &self.memoria {
            Some(Ok(m)) => {
                let mut v = vec![(
                    "Instalada".to_string(),
                    format!(
                        "{} em {} de {} slots{}",
                        formatar_mem(m.instalada_mb() * 1024),
                        m.usados(),
                        m.slots,
                        if m.maximo_mb > 0 { format!(" · aceita até {}", formatar_mem(m.maximo_mb * 1024)) } else { String::new() }
                    ),
                )];
                for md in &m.modulos {
                    let slot = if md.slot.is_empty() { "Slot".to_string() } else { md.slot.clone() };
                    let desc = if md.mb == 0 {
                        "vazio".to_string()
                    } else {
                        let mut d = formatar_mem(md.mb * 1024);
                        if !md.tipo.is_empty() {
                            d += &format!(" {}", md.tipo);
                        }
                        if !md.mts.is_empty() {
                            d += &format!(" · {} MT/s", md.mts);
                        }
                        let marca = format!("{} {}", md.fabricante, md.modelo).trim().to_string();
                        if !marca.is_empty() && !marca.eq_ignore_ascii_case("not specified") && !marca.contains("Unknown") {
                            d += &format!(" · {marca}");
                        }
                        d
                    };
                    v.push((slot, desc));
                }
                v
            }
            Some(Err(e)) => vec![("Memória".into(), format!("{} ({e})", formatar_mem(self.mem_total_kb)))],
            None => vec![("Memória".into(), "Lendo…".into())],
        };
        let pos = todas.iter().position(|(t, _)| t == "Processador").map(|p| p + 1).unwrap_or(todas.len());
        todas.insert(pos, ("Memória".into(), mem));

        let altura_total: f32 = todas.iter().map(|(_, v)| 44.0 + v.len() as f32 * 26.0 + 14.0).sum::<f32>() + 24.0;
        self.rolagem_hw = (self.rolagem_hw + ui.rolagem).clamp(0.0, (altura_total - (ui.altura() - y0)).max(0.0));
        let mut y = y0 + 16.0 - self.rolagem_hw;
        let col = 190.0;
        for (titulo, linhas) in &todas {
            ui.tela.texto(24.0, y + 22.0, 16.0, tema::TEXTO(), titulo);
            y += 34.0;
            ui.tela.retangulo(24.0, y, l - 48.0, linhas.len() as f32 * 26.0 + 8.0, 8.0, tema::FUNDO());
            y += 4.0;
            for (k, v) in linhas {
                ui.tela.texto(40.0, y + 18.0, 13.0, tema::TEXTO_SECUNDARIO(), k);
                ui.tela.texto(40.0 + col, y + 18.0, 13.0, tema::TEXTO(), &desenho::encurtar(13.0, v, l - 88.0 - col));
                y += 26.0;
            }
            y += 18.0;
        }
        // Cabeçalho das abas por cima do que rolou.
        ui.tela.retangulo(0.0, 0.0, l, y0, 0.0, tema::FUNDO());
    }
}

impl App for Tarefas {
    fn desenhar(&mut self, ui: &mut Ui) {
        let agora = Instant::now();
        if agora >= self.proxima {
            self.amostrar();
            self.proxima = agora + Duration::from_secs(1);
        }
        ui.acordar_em = Some(self.proxima.saturating_duration_since(Instant::now()));
        if let Some(rx) = &self.rx_memoria {
            if let Ok(m) = rx.try_recv() {
                self.memoria = Some(m);
                self.rx_memoria = None;
            }
        }

        let l = ui.largura();
        ui.tela.limpar(tema::JANELA());
        // Abas
        ui.tela.retangulo(0.0, 0.0, l, 44.0, 0.0, tema::FUNDO());
        let mut x = 12.0;
        // A aba Hardware desenha primeiro (rola por baixo do cabeçalho).
        if self.aba == Aba::Hardware {
            self.hardware(ui, 45.0);
        }
        for (aba, nome) in [(Aba::Processos, "Processos"), (Aba::Desempenho, "Desempenho"), (Aba::Hardware, "Hardware")] {
            let w = desenho::medir(14.0, nome) + 28.0;
            let sel = self.aba == aba;
            if !sel && ui.sobre(x, 6.0, w, 34.0) {
                ui.tela.retangulo(x, 6.0, w, 34.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            ui.tela.texto(x + 14.0, 28.0, 14.0, if sel { tema::TEXTO() } else { tema::TEXTO_SECUNDARIO() }, nome);
            if sel {
                ui.tela.retangulo(x + 10.0, 38.0, w - 20.0, 3.0, 1.5, tema::DESTAQUE());
            }
            if ui.clicou(x, 6.0, w, 34.0) {
                self.aba = aba;
                self.aviso = None;
            }
            x += w + 4.0;
        }
        ui.tela.retangulo(0.0, 44.0, l, 1.0, 0.0, tema::BORDA());
        match self.aba {
            Aba::Processos => self.processos(ui, 45.0),
            Aba::Desempenho => self.desempenho(ui, 45.0),
            Aba::Hardware => {}
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Gerenciador de Tarefas".into(),
            app_id: "tarefas".into(),
            tamanho: (760, 620),
            minimo: (560, 420),
        },
        Box::new(Tarefas::novo()),
    );
}
