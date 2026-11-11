// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-admin: "Permitir que este app faça alterações?" (como o UAC).
//!
//! Um app pede uma ação de administrador e o usuário é padrão: o app abre
//! esta janela com a descrição da ação. Ela mostra os administradores do
//! computador e pede a senha de um deles; "Sim" escreve no stdout
//! `login` e a senha em hex, uma por linha (nada = Não). Quem confere a
//! senha é o ararinha-sistema, não esta janela.
//!
//!   ararinha-admin "Criar a conta joao"

use penas::{componentes as c, desenho, tema, App, Opcoes, Tecla, Ui};

struct Pedido {
    acao: String,
    admins: Vec<(String, String)>,
    sel: usize,
    senha: String,
}

/// Administradores (grupo wheel) com o nome completo.
fn administradores() -> Vec<(String, String)> {
    let grupos = std::fs::read_to_string("/etc/group").unwrap_or_default();
    let membros: Vec<String> = grupos
        .lines()
        .find(|l| l.split(':').next() == Some("wheel"))
        .and_then(|l| l.split(':').nth(3))
        .map(|m| m.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    membros
        .into_iter()
        .filter(|m| m != "root")
        .map(|m| {
            let nome = passwd
                .lines()
                .find(|l| l.split(':').next() == Some(m.as_str()))
                .and_then(|l| l.split(':').nth(4))
                .map(|g| g.split(',').next().unwrap_or("").to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| m.clone());
            (m, nome)
        })
        .collect()
}

fn responder(login: &str, senha: &str) {
    let hex: String = senha.bytes().map(|b| format!("{b:02x}")).collect();
    println!("{login}\n{hex}");
}

impl App for Pedido {
    fn desenhar(&mut self, ui: &mut Ui) {
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());
        ui.tela.retangulo(0.0, 0.0, l, 64.0, 0.0, tema::DESTAQUE());
        ui.tela.texto(24.0, 40.0, 17.0, tema::BRANCO(), "Permitir esta alteração no computador?");
        let mut y = 98.0;
        let alt = c::paragrafo(ui, 24.0, y, l - 48.0, &self.acao);
        y += alt.max(20.0) + 8.0;
        ui.tela.texto(24.0, y, 12.5, tema::TEXTO_SECUNDARIO(), "Isso precisa de um administrador. Escolha a conta e digite a senha dela.");
        y += 22.0;
        if self.admins.is_empty() {
            ui.tela.texto(24.0, y + 10.0, 13.0, tema::ERRO(), "Nenhuma conta de administrador encontrada.");
        }
        for (i, (_, nome)) in self.admins.clone().iter().enumerate() {
            let sel = i == self.sel;
            let iy = y + i as f32 * 40.0;
            if sel {
                ui.tela.retangulo(24.0, iy, l - 48.0, 36.0, 6.0, tema::DESTAQUE_SUAVE());
            } else if ui.sobre(24.0, iy, l - 48.0, 36.0) {
                ui.tela.retangulo(24.0, iy, l - 48.0, 36.0, 6.0, tema::SUPERFICIE_ALTA());
            }
            ui.tela.retangulo(34.0, iy + 6.0, 24.0, 24.0, 12.0, tema::DESTAQUE());
            let ini: String = nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let li = desenho::medir(12.0, &ini);
            ui.tela.texto(46.0 - li / 2.0, iy + 23.0, 12.0, tema::BRANCO(), &ini);
            ui.tela.texto(68.0, iy + 23.0, 13.5, tema::TEXTO(), &desenho::encurtar(13.5, nome, l - 120.0));
            if ui.clicou(24.0, iy, l - 48.0, 36.0) {
                self.sel = i;
            }
        }
        y += self.admins.len().max(1) as f32 * 40.0 + 10.0;
        ui.tela.texto(24.0, y, 12.5, tema::TEXTO_SECUNDARIO(), "Senha");
        let teclas = std::mem::take(&mut ui.teclas);
        let mut confirmar = false;
        for t in c::editar(&mut self.senha, teclas) {
            match t {
                Tecla::Enter => confirmar = true,
                Tecla::Esc => ui.fechar = true,
                _ => {}
            }
        }
        c::campo(ui, 24.0, y + 8.0, l - 48.0, &self.senha, "Senha do administrador", true, true);
        let by = a - 60.0;
        let pode = !self.senha.is_empty() && !self.admins.is_empty();
        if c::botao_direita(ui, l - 24.0, by, "Sim", c::Estilo::Principal, pode) || (confirmar && pode) {
            if let Some((login, _)) = self.admins.get(self.sel) {
                responder(login, &self.senha);
            }
            ui.fechar = true;
        }
        let lb = c::largura_botao("Sim") + 10.0;
        if c::botao_direita(ui, l - 24.0 - lb, by, "Não", c::Estilo::Comum, true) {
            ui.fechar = true;
        }
        ui.titulo = Some("Controle de conta".into());
    }
}

fn main() {
    let acao = std::env::args().nth(1).unwrap_or_else(|| "Um app quer fazer uma alteração no computador.".into());
    let admins = administradores();
    let altura = 300 + admins.len().max(1) as u32 * 40;
    penas::executar(
        Opcoes { titulo: "Controle de conta".into(), app_id: "ararinha-admin".into(), tamanho: (480, altura), minimo: (420, altura) },
        Box::new(Pedido { acao, admins, sel: 0, senha: String::new() }),
    );
}
