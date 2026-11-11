// SPDX-License-Identifier: GPL-3.0-or-later
//! Ninho: a tela de entrada (login) do Ararinha, desenhada pela própria
//! Plumagem (`plumagem --ninho`, rodando como root).
//!
//! Mostra o relógio, um cartão de vidro com o Mingau e a senha do usuário.
//! Na primeira vez, sem nenhuma conta comum, o cartão vira "Criar sua conta".
//! Ao entrar, grava o usuário escolhido em `$XDG_RUNTIME_DIR/escolha` e
//! encerra; o script `ninho` abre então a sessão daquele usuário.

use std::{
    ffi::{CStr, CString},
    io::Write,
    os::raw::c_char,
    process::{Command, Stdio},
};

use smithay::{
    backend::renderer::{
        element::{
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            Kind,
        },
        ImportMem, Renderer,
    },
    desktop::Space,
    input::keyboard::{Keysym, ModifiersState},
    output::Output,
    utils::{Logical, Point, Size},
};
use tracing::{info, warn};

use crate::{
    desenho::{self, tema, Cor, Tela},
    shell::WindowElement,
};

static MINGAU_PNG: &[u8] = include_bytes!("../resources/mingau-128.png");

const CARTAO_L: i32 = 400;
const CARTAO_A_ENTRAR: i32 = 400;
const CARTAO_A_CRIAR: i32 = 500;
const CAMPO_X: f32 = 40.0;
const CAMPO_L: f32 = 320.0;
const CAMPO_A: f32 = 40.0;
const RELOGIO_A: i32 = 110;
const ENERGIA_L: i32 = 236;
const ENERGIA_A: i32 = 48;

#[derive(Debug, Clone)]
pub struct Usuario {
    pub login: String,
    pub nome: String,
    /// Foto da conta (~/.face), já redonda.
    pub foto: Option<image::RgbaImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modo {
    Entrar,
    Criar,
    /// Primeiro Voo: idioma e teclado.
    Idioma,
    /// Primeiro Voo: fuso horário.
    Fuso,
}

/// Opções do Primeiro Voo.
const IDIOMAS: [(&str, &str); 2] = [("Português (Brasil)", "pt_BR.UTF-8"), ("English (US)", "en_US.UTF-8")];
/// Nome, layout XKB, variante XKB e mapa do console.
const TECLADOS: [(&str, &str, &str, &str); 3] = [
    ("ABNT2 (Brasil)", "br", "", "br-abnt2"),
    ("US Internacional (com acentos)", "us", "intl", "us-acentos"),
    ("US (inglês)", "us", "", "us"),
];
const FUSOS: [(&str, &str); 7] = [
    ("Brasília (SP, RJ, MG, Sul, Nordeste…)", "America/Sao_Paulo"),
    ("Manaus (Amazonas, Roraima, Rondônia)", "America/Manaus"),
    ("Cuiabá (Mato Grosso, MS)", "America/Cuiaba"),
    ("Rio Branco (Acre)", "America/Rio_Branco"),
    ("Fernando de Noronha", "America/Noronha"),
    ("Lisboa (Portugal)", "Europe/Lisbon"),
    ("UTC (horário universal)", "UTC"),
];
/// Marca de que o Primeiro Voo já foi feito.
const MARCA_VOO: &str = "/var/lib/ararinha/primeiro-voo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pedido {
    Entrar,
    Criar,
    Continuar,
    Reiniciar,
    Desligar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alvo {
    /// Opção do Primeiro Voo: grupo (0 idioma, 1 teclado, 2 fuso) e índice.
    Opcao(u8, usize),
    Campo(usize),
    Botao,
    Anterior,
    Proximo,
    Reiniciar,
    Desligar,
}

pub struct Ninho {
    /// Escolhas do Primeiro Voo: idioma, teclado e fuso.
    voo: (usize, usize, usize),
    usuarios: Vec<Usuario>,
    sel: usize,
    modo: Modo,
    /// Entrar: [senha]. Criar: [nome, senha, confirmação].
    campos: Vec<String>,
    foco: usize,
    erro: Option<String>,
    pub pedido: Option<Pedido>,
    relogio: (u64, String, String),
    mingau: Option<image::RgbaImage>,
    cache: Vec<(String, MemoryRenderBuffer)>,
}

impl std::fmt::Debug for Ninho {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ninho").field("modo", &self.modo).finish()
    }
}

/// Contas comuns (UID 1000–59999 com shell de verdade).
pub fn ler_usuarios() -> Vec<Usuario> {
    let texto = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    texto
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split(':').collect();
            if c.len() < 7 {
                return None;
            }
            let uid: u32 = c[2].parse().ok()?;
            if !(1000..60000).contains(&uid) || c[6].ends_with("nologin") || c[6].ends_with("false") {
                return None;
            }
            let nome = c[4].split(',').next().unwrap_or("").trim();
            Some(Usuario {
                login: c[0].to_string(),
                nome: if nome.is_empty() { c[0].to_string() } else { nome.to_string() },
                foto: desenho::foto_da_conta(c[5], 104),
            })
        })
        .collect()
}

/// Nome de usuário derivado do nome da pessoa: "Davi Souza" → "davi".
pub fn login_de(nome: &str) -> String {
    let primeiro = nome.split_whitespace().next().unwrap_or("").to_lowercase();
    let mut s: String = primeiro
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .take(31)
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 'u');
    }
    s
}

#[link(name = "crypt")]
extern "C" {
    fn crypt(frase: *const c_char, config: *const c_char) -> *mut c_char;
}

/// Confere a senha com o /etc/shadow.
fn senha_confere(login: &str, senha: &str) -> bool {
    let Ok(shadow) = std::fs::read_to_string("/etc/shadow") else {
        warn!("não deu para ler /etc/shadow");
        return false;
    };
    let Some(hash) = shadow
        .lines()
        .find_map(|l| l.strip_prefix(login).and_then(|r| r.strip_prefix(':')))
        .and_then(|r| r.split(':').next())
    else {
        return false;
    };
    if hash.is_empty() {
        return senha.is_empty();
    }
    if hash.starts_with('!') || hash.starts_with('*') {
        return false;
    }
    let (Ok(k), Ok(h)) = (CString::new(senha), CString::new(hash)) else { return false };
    // SAFETY: argumentos são strings C válidas; o resultado (estático da
    // libcrypt) é copiado antes de qualquer outra chamada.
    let r = unsafe { crypt(k.as_ptr(), h.as_ptr()) };
    if r.is_null() {
        return false;
    }
    let r = unsafe { CStr::from_ptr(r) };
    r.to_bytes() == hash.as_bytes()
}

impl Ninho {
    pub fn novo() -> Ninho {
        let usuarios = ler_usuarios();
        let modo = if !usuarios.is_empty() {
            Modo::Entrar
        } else if std::path::Path::new(MARCA_VOO).exists() {
            Modo::Criar
        } else {
            Modo::Idioma
        };
        let mut n = Ninho {
            voo: (0, 0, 0),
            usuarios,
            sel: 0,
            modo,
            campos: Vec::new(),
            foco: 0,
            erro: None,
            pedido: None,
            relogio: (0, String::new(), String::new()),
            mingau: desenho::icone(MINGAU_PNG, 88, 102),
            cache: Vec::new(),
        };
        n.limpar_campos();
        n
    }

    fn limpar_campos(&mut self) {
        self.campos = match self.modo {
            Modo::Entrar => vec![String::new()],
            Modo::Criar => vec![String::new(), String::new(), String::new()],
            Modo::Idioma | Modo::Fuso => vec![String::new()],
        };
        self.foco = 0;
    }

    pub fn usuario(&self) -> Option<&Usuario> {
        self.usuarios.get(self.sel)
    }

    fn cartao_a(&self) -> i32 {
        match self.modo {
            Modo::Entrar => CARTAO_A_ENTRAR,
            Modo::Criar | Modo::Idioma | Modo::Fuso => CARTAO_A_CRIAR,
        }
    }

    fn cartao_origem(&self, tela: Size<i32, Logical>) -> Point<i32, Logical> {
        let y = ((tela.h - self.cartao_a()) / 2 + RELOGIO_A / 2).max(RELOGIO_A + 8);
        ((tela.w - CARTAO_L) / 2, y).into()
    }

    /// Posição vertical dos campos e do botão, por modo.
    fn campos_y(&self) -> (Vec<f32>, f32) {
        match self.modo {
            Modo::Entrar => (vec![236.0], 290.0),
            Modo::Criar => (vec![196.0, 290.0, 344.0], 404.0),
            Modo::Idioma | Modo::Fuso => (vec![], 430.0),
        }
    }

    /// Opções clicáveis do Primeiro Voo: (grupo, índice, y, texto).
    fn opcoes(&self) -> Vec<(u8, usize, f32, &'static str)> {
        match self.modo {
            Modo::Idioma => {
                let mut v: Vec<(u8, usize, f32, &'static str)> =
                    IDIOMAS.iter().enumerate().map(|(i, o)| (0, i, 196.0 + i as f32 * 36.0, o.0)).collect();
                v.extend(TECLADOS.iter().enumerate().map(|(i, o)| (1, i, 306.0 + i as f32 * 36.0, o.0)));
                v
            }
            Modo::Fuso => FUSOS.iter().enumerate().map(|(i, o)| (2, i, 168.0 + i as f32 * 36.0, o.0)).collect(),
            _ => Vec::new(),
        }
    }

    fn alvo(&self, p: Point<f64, Logical>, tela: Size<i32, Logical>) -> Option<Alvo> {
        let (x, y) = (p.x as f32, p.y as f32);
        // Botões de energia, no canto de baixo à direita.
        let ex = (tela.w - ENERGIA_L - 16) as f32;
        let ey = (tela.h - ENERGIA_A - 16) as f32;
        if y >= ey && y < ey + ENERGIA_A as f32 && x >= ex && x < ex + ENERGIA_L as f32 {
            return Some(if x < ex + ENERGIA_L as f32 / 2.0 { Alvo::Reiniciar } else { Alvo::Desligar });
        }
        let o = self.cartao_origem(tela);
        let (cx, cy) = (x - o.x as f32, y - o.y as f32);
        let (campos, botao) = self.campos_y();
        let dentro_x = cx >= CAMPO_X && cx < CAMPO_X + CAMPO_L;
        for (i, fy) in campos.iter().enumerate() {
            if dentro_x && cy >= *fy && cy < fy + CAMPO_A {
                return Some(Alvo::Campo(i));
            }
        }
        if dentro_x && cy >= botao && cy < botao + CAMPO_A {
            return Some(Alvo::Botao);
        }
        for (g, i, oy, _) in self.opcoes() {
            if dentro_x && cy >= oy && cy < oy + 32.0 {
                return Some(Alvo::Opcao(g, i));
            }
        }
        if self.modo == Modo::Entrar && self.usuarios.len() > 1 && (176.0..212.0).contains(&cy) {
            if (CAMPO_X..CAMPO_X + 36.0).contains(&cx) {
                return Some(Alvo::Anterior);
            }
            if (CAMPO_X + CAMPO_L - 36.0..CAMPO_X + CAMPO_L).contains(&cx) {
                return Some(Alvo::Proximo);
            }
        }
        None
    }

    /// Clique com o botão esquerdo em `p` (coordenadas da tela).
    pub fn clique(&mut self, p: Point<f64, Logical>, tela: Size<i32, Logical>) {
        match self.alvo(p, tela) {
            Some(Alvo::Opcao(g, i)) => match g {
                0 => self.voo.0 = i,
                1 => self.voo.1 = i,
                _ => self.voo.2 = i,
            },
            Some(Alvo::Campo(i)) => self.foco = i,
            Some(Alvo::Botao) => self.enviar(),
            Some(Alvo::Anterior) => self.trocar_usuario(-1),
            Some(Alvo::Proximo) => self.trocar_usuario(1),
            Some(Alvo::Reiniciar) => self.pedido = Some(Pedido::Reiniciar),
            Some(Alvo::Desligar) => self.pedido = Some(Pedido::Desligar),
            None => {}
        }
    }

    fn trocar_usuario(&mut self, d: i32) {
        let n = self.usuarios.len() as i32;
        if n > 0 {
            self.sel = ((self.sel as i32 + d).rem_euclid(n)) as usize;
            self.limpar_campos();
            self.erro = None;
        }
    }

    fn enviar(&mut self) {
        self.pedido = Some(match self.modo {
            Modo::Entrar => Pedido::Entrar,
            Modo::Criar => Pedido::Criar,
            Modo::Idioma | Modo::Fuso => Pedido::Continuar,
        });
    }

    pub fn tecla(&mut self, sym: Keysym, mods: ModifiersState) {
        // Primeiro Voo: setas escolhem, Enter continua.
        if matches!(self.modo, Modo::Idioma | Modo::Fuso) {
            match sym {
                Keysym::Return | Keysym::KP_Enter => self.enviar(),
                Keysym::Down if self.modo == Modo::Fuso => self.voo.2 = (self.voo.2 + 1).min(FUSOS.len() - 1),
                Keysym::Up if self.modo == Modo::Fuso => self.voo.2 = self.voo.2.saturating_sub(1),
                Keysym::Down => self.voo.1 = (self.voo.1 + 1).min(TECLADOS.len() - 1),
                Keysym::Up => self.voo.1 = self.voo.1.saturating_sub(1),
                Keysym::Left | Keysym::Right => self.voo.0 = 1 - self.voo.0,
                _ => {}
            }
            return;
        }
        match sym {
            Keysym::Return | Keysym::KP_Enter => {
                if self.foco + 1 < self.campos.len() {
                    self.foco += 1;
                } else {
                    self.enviar();
                }
            }
            Keysym::Tab | Keysym::ISO_Left_Tab => {
                let n = self.campos.len();
                self.foco = if mods.shift { (self.foco + n - 1) % n } else { (self.foco + 1) % n };
            }
            Keysym::Up => self.foco = self.foco.saturating_sub(1),
            Keysym::Down => self.foco = (self.foco + 1).min(self.campos.len() - 1),
            Keysym::Left if self.modo == Modo::Entrar && self.campos[0].is_empty() => self.trocar_usuario(-1),
            Keysym::Right if self.modo == Modo::Entrar && self.campos[0].is_empty() => self.trocar_usuario(1),
            Keysym::BackSpace => {
                if mods.ctrl {
                    self.campos[self.foco].clear();
                } else {
                    self.campos[self.foco].pop();
                }
            }
            Keysym::Escape => self.campos[self.foco].clear(),
            _ => {
                if mods.ctrl || mods.alt || mods.logo {
                    return;
                }
                if let Some(c) = sym.key_char() {
                    if !c.is_control() && self.campos[self.foco].chars().count() < 64 {
                        self.campos[self.foco].push(c);
                        self.erro = None;
                    }
                }
            }
        }
    }

    /// Executa o pedido pendente. Devolve o login escolhido, se a entrada
    /// foi aceita (o chamador grava e encerra a Plumagem).
    pub fn processar(&mut self) -> Option<String> {
        match self.pedido.take()? {
            Pedido::Reiniciar => {
                let _ = Command::new("systemctl").arg("reboot").spawn();
                None
            }
            Pedido::Desligar => {
                let _ = Command::new("systemctl").arg("poweroff").spawn();
                None
            }
            Pedido::Entrar => {
                let u = self.usuario()?.clone();
                if senha_confere(&u.login, &self.campos[0]) {
                    info!(login = u.login, "entrada aceita");
                    Some(u.login)
                } else {
                    self.erro = Some("Senha incorreta. Tente de novo.".into());
                    self.campos[0].clear();
                    None
                }
            }
            Pedido::Continuar => {
                if self.modo == Modo::Idioma {
                    self.modo = Modo::Fuso;
                } else {
                    self.aplicar_voo();
                    self.modo = Modo::Criar;
                    self.limpar_campos();
                }
                None
            }
            Pedido::Criar => match self.criar_conta() {
                Ok(login) => Some(login),
                Err(e) => {
                    self.erro = Some(e);
                    None
                }
            },
        }
    }

    /// Grava idioma, teclado e fuso escolhidos no Primeiro Voo.
    fn aplicar_voo(&mut self) {
        let (idioma, teclado, fuso) = (IDIOMAS[self.voo.0].1, TECLADOS[self.voo.1], FUSOS[self.voo.2].1);
        let _ = std::fs::write("/etc/locale.conf", format!("LANG={idioma}\n"));
        let _ = std::fs::write("/etc/vconsole.conf", format!("KEYMAP={}\nFONT=eurlatgr\n", teclado.3));
        let _ = std::fs::create_dir_all("/etc/plumagem");
        let _ = std::fs::write("/etc/plumagem/teclado", format!("{} {}\n", teclado.1, teclado.2));
        let _ = std::fs::remove_file("/etc/localtime");
        let _ = std::os::unix::fs::symlink(format!("../usr/share/zoneinfo/{fuso}"), "/etc/localtime");
        let _ = std::fs::create_dir_all("/var/lib/ararinha");
        let _ = std::fs::write(MARCA_VOO, "");
        // O relógio da tela passa a usar o fuso novo já no próximo minuto.
        self.relogio.0 = 0;
        info!(idioma, teclado = teclado.1, fuso, "Primeiro Voo aplicado");
    }

    fn criar_conta(&mut self) -> Result<String, String> {
        let nome = self.campos[0].trim().to_string();
        let login = login_de(&nome);
        if nome.is_empty() || login.is_empty() {
            self.foco = 0;
            return Err("Digite o seu nome.".into());
        }
        if self.campos[1].chars().count() < 4 {
            self.foco = 1;
            return Err("A senha precisa de pelo menos 4 caracteres.".into());
        }
        if self.campos[1] != self.campos[2] {
            self.campos[2].clear();
            self.foco = 2;
            return Err("As senhas não são iguais.".into());
        }
        let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
        if passwd.lines().any(|l| l.split(':').next() == Some(login.as_str())) {
            return Err(format!("O usuário \"{login}\" já existe."));
        }
        // Grupos de acesso: tela/teclado (seat), vídeo, som, entrada e admin.
        let grupos_existentes = std::fs::read_to_string("/etc/group").unwrap_or_default();
        let grupos: Vec<&str> = ["seat", "video", "audio", "input", "render", "wheel", "netdev"]
            .into_iter()
            .filter(|g| grupos_existentes.lines().any(|l| l.split(':').next() == Some(*g)))
            .collect();
        let mut cmd = Command::new("useradd");
        cmd.args(["-m", "-s", "/bin/bash", "-c", &nome]);
        if !grupos.is_empty() {
            cmd.args(["-G", &grupos.join(",")]);
        }
        let ok = cmd.arg(&login).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err("Não deu para criar a conta (useradd).".into());
        }
        let mut chpasswd = Command::new("chpasswd")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|_| "Não deu para definir a senha (chpasswd).".to_string())?;
        if let Some(mut entrada) = chpasswd.stdin.take() {
            let _ = writeln!(entrada, "{login}:{}", self.campos[1]);
        }
        if !chpasswd.wait().map(|s| s.success()).unwrap_or(false) {
            return Err("Não deu para definir a senha (chpasswd).".into());
        }
        // Primeira conta: a mesma senha passa a valer para o administrador
        // (root), que até aqui estava sem senha.
        let shadow = std::fs::read_to_string("/etc/shadow").unwrap_or_default();
        let root_sem_senha = shadow
            .lines()
            .find_map(|l| l.strip_prefix("root:"))
            .map(|r| r.split(':').next().unwrap_or("").is_empty())
            .unwrap_or(false);
        if root_sem_senha {
            if let Ok(mut c) = Command::new("chpasswd").stdin(Stdio::piped()).spawn() {
                if let Some(mut e) = c.stdin.take() {
                    let _ = writeln!(e, "root:{}", self.campos[1]);
                }
                let _ = c.wait();
                let _ = Command::new("chage").args(["-d", "-1", "root"]).status();
            }
        }
        info!(login, "conta criada");
        Ok(login)
    }

    fn atualizar_relogio(&mut self) {
        let minuto = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() / 60)
            .unwrap_or(0);
        if minuto == self.relogio.0 {
            return;
        }
        let saida = Command::new("date")
            .env("LC_TIME", "pt_BR.UTF-8")
            .arg("+%H:%M|%A, %-d de %B")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let mut p = saida.trim().splitn(2, '|');
        let hora = p.next().unwrap_or("").to_string();
        let mut data = p.next().unwrap_or("").to_string();
        if let Some(c) = data.chars().next() {
            data = c.to_uppercase().collect::<String>() + &data[c.len_utf8()..];
        }
        self.relogio = (minuto, hora, data);
    }

    fn saudacao(&self) -> &'static str {
        let h: u32 = self.relogio.1.split(':').next().and_then(|h| h.parse().ok()).unwrap_or(12);
        match h {
            5..=11 => "Bom dia!",
            12..=17 => "Boa tarde!",
            _ => "Boa noite!",
        }
    }

    pub fn elementos<R>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        space: &Space<WindowElement>,
        ponteiro: Point<f64, Logical>,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        let Some(geo) = space.output_geometry(output) else { return Vec::new() };
        let escala = output.current_scale().fractional_scale();
        let tela = geo.size;
        desenho::preparar_vidro(tela.w, tela.h);
        self.atualizar_relogio();
        marcar_tempo_de_boot();
        let hover = self.alvo(ponteiro - geo.loc.to_f64(), tela);
        let o = self.cartao_origem(tela);

        let partes: [(String, Point<i32, Logical>, u8); 3] = [
            (
                format!(
                    "c|{:?}|{:?}|{}|{:?}|{:?}|{}|{o:?}|{}|{:?}",
                    self.modo, self.campos, self.foco, self.erro, hover, self.sel, self.relogio.1, self.voo
                ),
                o,
                0,
            ),
            (
                format!("r|{}|{}|{}", self.relogio.1, self.relogio.2, tela.w),
                ((tela.w - 600) / 2, o.y - RELOGIO_A - 8).into(),
                1,
            ),
            (
                format!("e|{hover:?}|{tela:?}"),
                (tela.w - ENERGIA_L - 16, tela.h - ENERGIA_A - 16).into(),
                2,
            ),
        ];
        self.cache.resize_with(3, || (String::new(), Tela::nova(1, 1).em_buffer()));
        let mut v = Vec::new();
        for (chave, pos, i) in partes {
            let i = i as usize;
            if self.cache[i].0 != chave {
                let t = match i {
                    0 => self.desenhar_cartao(hover, o),
                    1 => self.desenhar_relogio(),
                    _ => self.desenhar_energia(hover, pos),
                };
                self.cache[i] = (chave, t.em_buffer());
            }
            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                pos.to_f64().to_physical(escala),
                &self.cache[i].1,
                None,
                None,
                None,
                Kind::Unspecified,
            ) {
                v.push(e);
            }
        }
        v
    }

    fn desenhar_relogio(&self) -> Tela {
        let mut t = Tela::nova(600, RELOGIO_A);
        let sombra = Cor(11, 26, 46, 90);
        let (hora, data) = (&self.relogio.1, &self.relogio.2);
        let lh = desenho::medir(64.0, hora);
        t.texto(300.0 - lh / 2.0 + 1.0, 66.0, 64.0, sombra, hora);
        t.texto(300.0 - lh / 2.0, 64.0, 64.0, tema::BRANCO(), hora);
        let ld = desenho::medir(18.0, data);
        t.texto(300.0 - ld / 2.0 + 1.0, 97.0, 18.0, sombra, data);
        t.texto(300.0 - ld / 2.0, 96.0, 18.0, tema::BRANCO(), data);
        t
    }

    fn desenhar_energia(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let mut t = Tela::nova(ENERGIA_L, ENERGIA_A);
        let bl = ENERGIA_L as f32 / 2.0;
        for (i, (alvo, nome)) in [(Alvo::Reiniciar, "Reiniciar"), (Alvo::Desligar, "Desligar")].iter().enumerate() {
            let x = i as f32 * bl;
            let tinta = if hover == Some(*alvo) {
                if *alvo == Alvo::Desligar { tema::ERRO().com_alfa(170) } else { tema::BRANCO().com_alfa(170) }
            } else {
                tema::BRANCO().com_alfa(90)
            };
            t.vidro(x + 3.0, 4.0, bl - 6.0, 40.0, 20.0, (origem.x + x as i32 + 3, origem.y + 4), tinta);
            let cor = if hover == Some(Alvo::Desligar) && *alvo == Alvo::Desligar { tema::BRANCO() } else { tema::TEXTO() };
            let lt = desenho::medir(13.0, nome);
            t.texto_com_brilho(x + bl / 2.0 - lt / 2.0, 29.0, 13.0, cor, nome);
        }
        t
    }

    fn campo(&self, t: &mut Tela, i: usize, y: f32, rotulo: &str, segredo: bool, hover: Option<Alvo>) {
        let focado = self.foco == i;
        let borda = if focado { tema::DESTAQUE() } else if hover == Some(Alvo::Campo(i)) { tema::TEXTO_SECUNDARIO() } else { tema::BORDA() };
        t.retangulo(CAMPO_X, y, CAMPO_L, CAMPO_A, 8.0, borda);
        let esp = if focado { 2.0 } else { 1.0 };
        t.retangulo(CAMPO_X + esp, y + esp, CAMPO_L - 2.0 * esp, CAMPO_A - 2.0 * esp, 8.0 - esp, tema::JANELA());
        let texto = &self.campos[i];
        let base = y + CAMPO_A / 2.0 + 5.0;
        let fim = if texto.is_empty() {
            t.texto(CAMPO_X + 14.0, base, 14.0, tema::TEXTO_SECUNDARIO().com_alfa(170), rotulo);
            CAMPO_X + 14.0
        } else if segredo {
            let n = texto.chars().count().min(26);
            for k in 0..n {
                let cx = CAMPO_X + 18.0 + k as f32 * 11.0;
                t.retangulo(cx - 4.0, y + CAMPO_A / 2.0 - 4.0, 8.0, 8.0, 4.0, tema::TEXTO());
            }
            CAMPO_X + 14.0 + n as f32 * 11.0
        } else {
            let visivel = desenho::encurtar(14.0, texto, CAMPO_L - 30.0);
            CAMPO_X + 14.0 + t.texto(CAMPO_X + 14.0, base, 14.0, tema::TEXTO(), &visivel)
        };
        if focado {
            t.retangulo(fim + 1.0, y + 10.0, 1.5, CAMPO_A - 20.0, 0.0, tema::DESTAQUE());
        }
    }

    fn botao(&self, t: &mut Tela, y: f32, nome: &str, hover: Option<Alvo>) {
        let cor = if hover == Some(Alvo::Botao) { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() };
        t.retangulo(CAMPO_X, y, CAMPO_L, CAMPO_A, 8.0, cor);
        let l = desenho::medir(15.0, nome);
        t.texto(CAMPO_X + CAMPO_L / 2.0 - l / 2.0, y + CAMPO_A / 2.0 + 5.5, 15.0, tema::BRANCO(), nome);
    }

    fn centralizado(t: &mut Tela, y: f32, tam: f32, cor: Cor, texto: &str) {
        let l = desenho::medir(tam, texto);
        t.texto(CARTAO_L as f32 / 2.0 - l / 2.0, y, tam, cor, texto);
    }

    fn desenhar_cartao(&self, hover: Option<Alvo>, origem: Point<i32, Logical>) -> Tela {
        let a = self.cartao_a();
        let mut t = Tela::nova(CARTAO_L, a);
        t.vidro(0.0, 0.0, CARTAO_L as f32, a as f32, 18.0, (origem.x, origem.y), tema::FUNDO().com_alfa(185));
        // Ao entrar, a foto da conta (se houver) no lugar do Mingau.
        let foto = if self.modo == Modo::Entrar { self.usuario().and_then(|u| u.foto.as_ref()) } else { None };
        if let Some(f) = foto {
            t.imagem((CARTAO_L - f.width() as i32) / 2, 22, f);
        } else if let Some(m) = &self.mingau {
            t.imagem((CARTAO_L - m.width() as i32) / 2, 18, m);
        }
        let (campos, botao) = self.campos_y();
        match self.modo {
            Modo::Entrar => {
                Self::centralizado(&mut t, 152.0, 22.0, tema::TEXTO(), self.saudacao());
                let nome = self.usuario().map(|u| u.nome.clone()).unwrap_or_default();
                let nome = desenho::encurtar(17.0, &nome, CAMPO_L - 80.0);
                Self::centralizado(&mut t, 200.0, 17.0, tema::TEXTO(), &nome);
                if self.usuarios.len() > 1 {
                    for (alvo, x, s) in [(Alvo::Anterior, CAMPO_X + 18.0, -1.0), (Alvo::Proximo, CAMPO_X + CAMPO_L - 18.0, 1.0)] {
                        let cor = if hover == Some(alvo) { tema::DESTAQUE() } else { tema::TEXTO_SECUNDARIO() };
                        t.linha(x + 3.0 * s * -1.0, 187.0, x + 3.0 * s, 194.0, 2.0, cor);
                        t.linha(x + 3.0 * s, 194.0, x + 3.0 * s * -1.0, 201.0, 2.0, cor);
                    }
                }
                self.campo(&mut t, 0, campos[0], "Senha", true, hover);
                self.botao(&mut t, botao, "Entrar", hover);
            }
            Modo::Idioma | Modo::Fuso => {
                let (titulo, sub) = if self.modo == Modo::Idioma {
                    ("Bem-vindo ao Ararinha!", "Escolha o idioma e o teclado.")
                } else {
                    ("Qual é o seu fuso horário?", "O relógio usa este fuso.")
                };
                Self::centralizado(&mut t, 136.0, 21.0, tema::TEXTO(), titulo);
                Self::centralizado(&mut t, 158.0, 13.0, tema::TEXTO_SECUNDARIO(), sub);
                if self.modo == Modo::Idioma {
                    t.texto(CAMPO_X, 190.0, 12.0, tema::TEXTO_SECUNDARIO(), "Idioma");
                    t.texto(CAMPO_X, 300.0, 12.0, tema::TEXTO_SECUNDARIO(), "Teclado");
                }
                for (g, i, oy, nome) in self.opcoes() {
                    let sel = match g {
                        0 => self.voo.0 == i,
                        1 => self.voo.1 == i,
                        _ => self.voo.2 == i,
                    };
                    let oy = if g == 2 { oy } else { oy + 4.0 };
                    if sel {
                        t.retangulo(CAMPO_X, oy, CAMPO_L, 30.0, 7.0, tema::DESTAQUE_SUAVE());
                    } else if hover == Some(Alvo::Opcao(g, i)) {
                        t.retangulo(CAMPO_X, oy, CAMPO_L, 30.0, 7.0, tema::SUPERFICIE_ALTA());
                    }
                    // Bolinha de escolha.
                    t.retangulo(CAMPO_X + 10.0, oy + 8.0, 14.0, 14.0, 7.0, if sel { tema::DESTAQUE() } else { tema::BORDA() });
                    t.retangulo(CAMPO_X + 12.0, oy + 10.0, 10.0, 10.0, 5.0, tema::JANELA());
                    if sel {
                        t.retangulo(CAMPO_X + 14.0, oy + 12.0, 6.0, 6.0, 3.0, tema::DESTAQUE());
                    }
                    t.texto(CAMPO_X + 34.0, oy + 20.0, 13.5, tema::TEXTO(), nome);
                }
                self.botao(&mut t, botao, "Continuar", hover);
            }
            Modo::Criar => {
                Self::centralizado(&mut t, 148.0, 21.0, tema::TEXTO(), "Bem-vindo ao Ararinha!");
                Self::centralizado(&mut t, 172.0, 13.0, tema::TEXTO_SECUNDARIO(), "Vamos criar a sua conta.");
                self.campo(&mut t, 0, campos[0], "Seu nome", false, hover);
                let login = login_de(&self.campos[0]);
                if !login.is_empty() {
                    t.texto(CAMPO_X + 4.0, campos[0] + CAMPO_A + 18.0, 12.0, tema::TEXTO_SECUNDARIO(), &format!("Nome de usuário: {login}"));
                }
                self.campo(&mut t, 1, campos[1], "Senha", true, hover);
                self.campo(&mut t, 2, campos[2], "Confirme a senha", true, hover);
                self.botao(&mut t, botao, "Criar conta", hover);
                t.texto(
                    CAMPO_X,
                    botao + CAMPO_A + 50.0,
                    11.5,
                    tema::TEXTO_SECUNDARIO(),
                    "Esta senha também vale para o administrador.",
                );
            }
        }
        if let Some(e) = &self.erro {
            let e = desenho::encurtar(13.0, e, CAMPO_L);
            Self::centralizado(&mut t, botao + CAMPO_A + 28.0, 13.0, tema::ERRO(), &e);
        }
        t
    }
}

/// No primeiro quadro do Ninho, guarda em /run/ararinha/ninho-pronto quantos
/// segundos se passaram desde o boot (o Sobre das Configurações mostra).
fn marcar_tempo_de_boot() {
    static FEITO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if FEITO.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let uptime = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
    let segundos = uptime.split_whitespace().next().unwrap_or("0").to_string();
    tracing::info!(segundos, "Ninho na tela");
    let _ = std::fs::create_dir_all("/run/ararinha");
    let _ = std::fs::write("/run/ararinha/ninho-pronto", format!("{segundos}\n"));
}
