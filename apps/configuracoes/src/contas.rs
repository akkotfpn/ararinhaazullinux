// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Outras contas": quem usa este computador. Criar conta
//! (administrador ou padrão), tornar ou deixar de ser administrador e
//! apagar. Tudo pelo ararinha-sistema, que exige um administrador.

use std::{
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{componentes as c, desenho, tema, Tecla, Ui};

#[derive(Clone)]
struct Conta {
    login: String,
    nome: String,
    admin: bool,
}

#[derive(Default)]
pub struct PaginaContas {
    contas: Vec<Conta>,
    pub aviso: Option<String>,
    rx: Option<Receiver<Result<String, String>>>,
    lido: bool,
    /// Formulário de conta nova: nome, login, senha, confirmação.
    novo: Option<[String; 4]>,
    novo_admin: bool,
    foco: usize,
    /// Login com "Apagar" pedido (espera confirmação).
    apagar: Option<String>,
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// Login a partir do nome: "João da Silva" → "joao".
fn sugerir_login(nome: &str) -> String {
    let primeiro = nome.split_whitespace().next().unwrap_or("").to_lowercase();
    primeiro
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            o => o,
        })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .take(24)
        .collect()
}

impl PaginaContas {
    fn pedir(&mut self, p: String) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(penas::sistema::pedir_admin(&p));
        });
        self.rx = Some(rx);
    }

    fn aplicar(&mut self, t: &str) {
        self.contas = t
            .split(';')
            .filter_map(|c| {
                let p: Vec<&str> = c.split('|').collect();
                Some(Conta { login: p.first()?.to_string(), nome: p.get(1).unwrap_or(&"").to_string(), admin: p.get(2) == Some(&"sim") })
            })
            .filter(|c| !c.login.is_empty())
            .collect();
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        if !self.lido {
            self.lido = true;
            self.pedir("contas listar".into());
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx = None;
                    match r {
                        Ok(t) => self.aplicar(&t),
                        Err(e) => self.aviso = Some(format!("Não deu: {e}.")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(150)),
            }
        }
        let eu = std::env::var("USER").unwrap_or_default();
        let x = x0 + 32.0;
        let w = (l - 64.0).min(620.0);
        ui.tela.texto(x, 52.0, 22.0, tema::TEXTO(), "Contas");
        ui.tela.texto(x, 84.0, 13.0, tema::TEXTO_SECUNDARIO(), "Quem usa este computador. Administradores podem instalar e mudar o sistema.");

        if let Some(mut campos) = self.novo.take() {
            // Formulário de conta nova.
            let teclas = std::mem::take(&mut ui.teclas);
            for t in c::editar(&mut campos[self.foco], teclas) {
                if matches!(t, Tecla::Outra(penas::Keysym::Tab) | Tecla::Enter | Tecla::Baixo) {
                    self.foco = (self.foco + 1) % 4;
                } else if t == Tecla::Cima {
                    self.foco = (self.foco + 3) % 4;
                }
            }
            if self.foco == 0 {
                campos[1] = sugerir_login(&campos[0]);
            }
            let rotulos = ["Nome completo", "Nome de usuário", "Senha", "Confirme a senha"];
            for (i, r) in rotulos.iter().enumerate() {
                let y = 116.0 + i as f32 * 66.0;
                ui.tela.texto(x, y, 12.5, tema::TEXTO_SECUNDARIO(), r);
                if c::campo(ui, x, y + 8.0, w.min(360.0), &campos[i], "", self.foco == i, i >= 2) {
                    self.foco = i;
                }
            }
            let y = 116.0 + 4.0 * 66.0;
            c::caixa_marcar(ui, x, y, "Administrador (pode instalar e mudar o sistema)", &mut self.novo_admin);
            let ok = !campos[0].trim().is_empty() && !campos[1].is_empty() && campos[2].chars().count() >= 4 && campos[2] == campos[3];
            if !campos[3].is_empty() && campos[2] != campos[3] {
                ui.tela.texto(x, y + 44.0, 12.0, tema::ERRO(), "As senhas não são iguais.");
            }
            if c::botao(ui, x, y + 60.0, "Criar conta", c::Estilo::Principal, ok && self.rx.is_none()) {
                let tipo = if self.novo_admin { "admin" } else { "padrao" };
                self.pedir(format!("contas criar {} {tipo} {} {}", campos[1], hex(campos[0].trim()), hex(&campos[2])));
                return;
            }
            if c::botao(ui, x + c::largura_botao("Criar conta") + 10.0, y + 60.0, "Cancelar", c::Estilo::Comum, true) {
                return;
            }
            self.novo = Some(campos);
            return;
        }

        let mut y = 116.0;
        let mut acao: Option<String> = None;
        for conta in self.contas.clone() {
            ui.tela.retangulo(x, y, w, 64.0, 8.0, tema::SUPERFICIE_ALTA());
            ui.tela.retangulo(x + 14.0, y + 14.0, 36.0, 36.0, 18.0, tema::DESTAQUE());
            let ini: String = conta.nome.chars().chain(conta.login.chars()).next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let li = desenho::medir(16.0, &ini);
            ui.tela.texto(x + 32.0 - li / 2.0, y + 38.0, 16.0, tema::BRANCO(), &ini);
            let titulo = if conta.nome.is_empty() { conta.login.clone() } else { conta.nome.clone() };
            let titulo = if conta.login == eu { format!("{titulo} (você)") } else { titulo };
            ui.tela.texto(x + 62.0, y + 28.0, 14.5, tema::TEXTO(), &desenho::encurtar(14.5, &titulo, w - 320.0));
            let sub = format!("{} · {}", conta.login, if conta.admin { "Administrador" } else { "Padrão" });
            ui.tela.texto(x + 62.0, y + 48.0, 12.0, tema::TEXTO_SECUNDARIO(), &sub);
            if conta.login != eu {
                let livre = self.rx.is_none();
                let r1 = if conta.admin { "Tornar padrão" } else { "Tornar admin" };
                let bx = x + w - 12.0;
                if c::botao_direita(ui, bx, y + 14.0, "Apagar", c::Estilo::Comum, livre) {
                    self.apagar = Some(conta.login.clone());
                }
                let l2 = c::largura_botao("Apagar") + 8.0;
                if c::botao_direita(ui, bx - l2, y + 14.0, r1, c::Estilo::Comum, livre) {
                    acao = Some(format!("contas admin {} {}", conta.login, if conta.admin { "nao" } else { "sim" }));
                }
            }
            y += 72.0;
        }
        if let Some(a) = acao {
            self.pedir(a);
        }
        if c::botao(ui, x, y + 8.0, "Adicionar conta", c::Estilo::Principal, self.rx.is_none()) {
            self.novo = Some(Default::default());
            self.novo_admin = false;
            self.foco = 0;
        }

        // Confirmação de apagar.
        if let Some(login) = self.apagar.clone() {
            let (dl, da) = (w.min(460.0), 180.0);
            let (dx, dy) = (x + (w - dl) / 2.0, 150.0);
            ui.tela.retangulo(0.0, 0.0, ui.largura(), ui.altura(), 0.0, penas::Cor(0, 0, 0, 90));
            ui.tela.retangulo(dx - 1.0, dy - 1.0, dl + 2.0, da + 2.0, 13.0, tema::BORDA());
            ui.tela.retangulo(dx, dy, dl, da, 12.0, tema::JANELA());
            ui.tela.texto(dx + 24.0, dy + 38.0, 17.0, tema::TEXTO(), &format!("Apagar a conta \"{login}\"?"));
            c::paragrafo(ui, dx + 24.0, dy + 66.0, dl - 48.0, "Os arquivos dessa conta (Documentos, Imagens, tudo da pasta pessoal) serão apagados para sempre.");
            let by = dy + da - 56.0;
            if c::botao_direita(ui, dx + dl - 24.0, by, "Apagar", c::Estilo::Perigo, true) {
                self.apagar = None;
                self.pedir(format!("contas remover {login}"));
            }
            let lv = c::largura_botao("Apagar") + 10.0;
            if c::botao_direita(ui, dx + dl - 24.0 - lv, by, "Cancelar", c::Estilo::Comum, true) {
                self.apagar = None;
            }
        }
    }
}
