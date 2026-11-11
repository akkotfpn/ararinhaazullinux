// SPDX-License-Identifier: GPL-3.0-or-later
//! Painel de Controle, como o do Windows ("Exibir por: Categoria"): cada
//! categoria tem um ícone e atalhos para o que mexe nela.

use std::process::Command;

use penas::{desenho, tema, App, Cor, Opcoes, Tela, Ui};

struct Categoria {
    titulo: &'static str,
    cor: u32,
    simbolo: Simbolo,
    links: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy)]
enum Simbolo {
    Escudo,
    Globo,
    Engrenagem,
    Caixa,
    Pessoa,
    Pincel,
    Relogio,
    Alto,
}

const CATEGORIAS: [Categoria; 8] = [
    Categoria {
        titulo: "Sistema e Segurança",
        cor: 0x2B7BC4,
        simbolo: Simbolo::Escudo,
        links: &[
            ("Informações do sistema", "configuracoes sobre"),
            ("Opções de energia", "configuracoes energia"),
            ("Gerenciador de Tarefas", "tarefas"),
            ("Bloquear a tela (Win+L)", "#bloquear"),
        ],
    },
    Categoria {
        titulo: "Rede e Internet",
        cor: 0x2E9E6A,
        simbolo: Simbolo::Globo,
        links: &[
            ("Wi-Fi e conexão", "configuracoes rede"),
            ("Mapear unidade de rede", "explorador"),
        ],
    },
    Categoria {
        titulo: "Hardware e Sons",
        cor: 0x6B7C8F,
        simbolo: Simbolo::Alto,
        links: &[
            ("Sons do sistema", "configuracoes"),
            ("Dispositivos e unidades", "explorador"),
        ],
    },
    Categoria {
        titulo: "Programas",
        cor: 0xD9732B,
        simbolo: Simbolo::Caixa,
        links: &[
            ("Instalar um programa", "central"),
            ("Desinstalar um programa", "central instalados"),
            ("Programas padrão", "#padrao"),
        ],
    },
    Categoria {
        titulo: "Contas de Usuário",
        cor: 0x8B3FB8,
        simbolo: Simbolo::Pessoa,
        links: &[("Alterar a senha", "configuracoes conta"), ("Bloquear a tela", "#bloquear")],
    },
    Categoria {
        titulo: "Aparência e Personalização",
        cor: 0xC62F80,
        simbolo: Simbolo::Pincel,
        links: &[
            ("Papel de parede", "configuracoes"),
            ("Modo claro ou escuro", "configuracoes"),
            ("Barra de tarefas", "configuracoes"),
        ],
    },
    Categoria {
        titulo: "Relógio e Região",
        cor: 0x1F8A9E,
        simbolo: Simbolo::Relogio,
        links: &[("Data, hora e fuso horário", "configuracoes sobre")],
    },
    Categoria {
        titulo: "Facilidade de Acesso",
        cor: 0x3B5BA8,
        simbolo: Simbolo::Engrenagem,
        links: &[("Tamanho da barra e dos ícones", "configuracoes"), ("Atalhos de teclado", "#atalhos")],
    },
];

const ATALHOS: [(&str, &str); 10] = [
    ("Win", "Abrir o menu Iniciar"),
    ("Win+E", "Explorador de Arquivos"),
    ("Win+D", "Mostrar a área de trabalho"),
    ("Win+L", "Bloquear a tela"),
    ("Win+← / →", "Encaixar a janela na metade da tela"),
    ("Win+↑ / ↓", "Maximizar / restaurar ou minimizar"),
    ("Alt+Tab", "Trocar de janela"),
    ("Alt+F4", "Fechar a janela"),
    ("Print Screen", "Captura de tela"),
    ("Ctrl+Alt+T", "Terminal"),
];

fn simbolo(t: &mut Tela, s: Simbolo, cx: f32, cy: f32, r: f32) {
    let b = Cor::hex(0xFFFFFF);
    match s {
        Simbolo::Escudo => {
            t.poligono(
                &[(cx, cy - r), (cx + r * 0.8, cy - r * 0.6), (cx + r * 0.7, cy + r * 0.3), (cx, cy + r), (cx - r * 0.7, cy + r * 0.3), (cx - r * 0.8, cy - r * 0.6)],
                b,
            );
        }
        Simbolo::Globo => {
            t.retangulo(cx - r, cy - r, r * 2.0, r * 2.0, r, b);
            t.retangulo(cx - r * 0.8, cy - r * 0.8, r * 1.6, r * 1.6, r * 0.8, Cor(0, 0, 0, 40));
            t.linha(cx - r, cy, cx + r, cy, 1.6, b);
            t.linha(cx, cy - r, cx, cy + r, 1.6, b);
        }
        Simbolo::Engrenagem => {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                t.linha(cx, cy, cx + a.cos() * r, cy + a.sin() * r, r * 0.35, b);
            }
            t.retangulo(cx - r * 0.65, cy - r * 0.65, r * 1.3, r * 1.3, r * 0.65, b);
        }
        Simbolo::Caixa => {
            t.retangulo(cx - r, cy - r * 0.5, r * 2.0, r * 1.5, 3.0, b);
            t.poligono(&[(cx - r, cy - r * 0.5), (cx - r * 0.7, cy - r), (cx + r * 0.7, cy - r), (cx + r, cy - r * 0.5)], b);
        }
        Simbolo::Pessoa => {
            t.retangulo(cx - r * 0.4, cy - r, r * 0.8, r * 0.8, r * 0.4, b);
            t.retangulo(cx - r * 0.8, cy, r * 1.6, r, r * 0.5, b);
        }
        Simbolo::Pincel => {
            t.linha(cx - r * 0.7, cy + r * 0.7, cx + r * 0.6, cy - r * 0.6, r * 0.35, b);
            t.retangulo(cx - r, cy + r * 0.4, r * 0.6, r * 0.6, r * 0.3, b);
        }
        Simbolo::Relogio => {
            t.retangulo(cx - r, cy - r, r * 2.0, r * 2.0, r, b);
            t.linha(cx, cy, cx, cy - r * 0.65, 2.0, Cor(0, 0, 0, 120));
            t.linha(cx, cy, cx + r * 0.5, cy, 2.0, Cor(0, 0, 0, 120));
        }
        Simbolo::Alto => {
            t.poligono(&[(cx - r, cy - r * 0.35), (cx - r * 0.4, cy - r * 0.35), (cx + r * 0.2, cy - r), (cx + r * 0.2, cy + r), (cx - r * 0.4, cy + r * 0.35), (cx - r, cy + r * 0.35)], b);
            t.linha(cx + r * 0.5, cy - r * 0.4, cx + r * 0.5, cy + r * 0.4, 1.8, b);
            t.linha(cx + r * 0.8, cy - r * 0.7, cx + r * 0.8, cy + r * 0.7, 1.8, b);
        }
    }
}

struct Painel {
    aviso: Option<String>,
    atalhos: bool,
    rolagem: f32,
}

impl Painel {
    fn executar(&mut self, cmd: &str) {
        match cmd {
            "#atalhos" => self.atalhos = true,
            "#bloquear" => {
                self.aviso = Some("Para bloquear a tela, use Win+L.".into());
            }
            "#padrao" => {
                self.aviso = Some("Pastas abrem no Explorador, imagens no Visualizador, textos no Bloco de Notas e músicas no Músicas.".into());
            }
            c => {
                let mut p = c.split_whitespace();
                if let Some(prog) = p.next() {
                    if let Err(e) = Command::new(prog).args(p).spawn() {
                        self.aviso = Some(format!("Não deu para abrir: {e}"));
                    }
                }
            }
        }
    }
}

impl App for Painel {
    fn desenhar(&mut self, ui: &mut Ui) {
        let (l, a) = (ui.largura(), ui.altura());
        ui.tela.limpar(tema::JANELA());
        if self.atalhos {
            ui.tela.texto(32.0, 52.0, 22.0, tema::TEXTO(), "Atalhos de teclado");
            for (i, (k, d)) in ATALHOS.iter().enumerate() {
                let y = 100.0 + i as f32 * 34.0;
                let lk = desenho::medir(13.0, k) + 20.0;
                ui.tela.retangulo(32.0, y - 18.0, lk, 26.0, 6.0, tema::SUPERFICIE_ALTA());
                ui.tela.texto(42.0, y, 13.0, tema::TEXTO(), k);
                ui.tela.texto(200.0, y, 13.5, tema::TEXTO(), d);
            }
            let s = ui.sobre(32.0, a - 56.0, 100.0, 36.0);
            ui.tela.retangulo(32.0, a - 56.0, 100.0, 36.0, 8.0, if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
            ui.tela.texto(58.0, a - 33.0, 13.5, tema::BRANCO(), "Voltar");
            if ui.clicou(32.0, a - 56.0, 100.0, 36.0) {
                self.atalhos = false;
            }
            return;
        }
        ui.tela.texto(32.0, 52.0, 22.0, tema::TEXTO(), "Ajuste as configurações do computador");
        let colunas = if l > 860.0 { 2 } else { 1 };
        let cl = (l - 64.0 - (colunas - 1) as f32 * 24.0) / colunas as f32;
        let ca = 132.0;
        let linhas = CATEGORIAS.len().div_ceil(colunas);
        let total = 90.0 + linhas as f32 * (ca + 12.0);
        self.rolagem = (self.rolagem + ui.rolagem).clamp(0.0, (total - a + 50.0).max(0.0));
        let mut clicado = None;
        for (i, c) in CATEGORIAS.iter().enumerate() {
            let x = 32.0 + (i % colunas) as f32 * (cl + 24.0);
            let y = 84.0 + (i / colunas) as f32 * (ca + 12.0) - self.rolagem;
            // Ícone grande colorido
            ui.tela.retangulo(x, y + 6.0, 56.0, 56.0, 12.0, Cor::hex(c.cor));
            simbolo(ui.tela, c.simbolo, x + 28.0, y + 34.0, 15.0);
            let tx = x + 74.0;
            let st = ui.sobre(tx, y + 6.0, desenho::medir(16.0, c.titulo), 24.0);
            ui.tela.texto(tx, y + 24.0, 16.0, if st { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() }, c.titulo);
            if ui.clicou(tx, y + 6.0, desenho::medir(16.0, c.titulo), 24.0) {
                clicado = c.links.first().map(|(_, cmd)| *cmd);
            }
            for (k, (rot, cmd)) in c.links.iter().enumerate() {
                let ly = y + 50.0 + k as f32 * 21.0;
                let lr = desenho::medir(13.0, rot);
                let s = ui.sobre(tx, ly - 15.0, lr, 20.0);
                ui.tela.texto(tx, ly, 13.0, if s { tema::DESTAQUE() } else { tema::TEXTO() }, rot);
                if s {
                    ui.tela.retangulo(tx, ly + 3.0, lr, 1.0, 0.0, tema::DESTAQUE());
                }
                if ui.clicou(tx, ly - 15.0, lr, 20.0) {
                    clicado = Some(*cmd);
                }
            }
        }
        if let Some(cmd) = clicado {
            self.executar(cmd);
        }
        if let Some(av) = &self.aviso {
            ui.tela.retangulo(0.0, a - 36.0, l, 36.0, 0.0, tema::FUNDO());
            ui.tela.texto(32.0, a - 13.0, 12.5, tema::TEXTO_SECUNDARIO(), &desenho::encurtar(12.5, av, l - 64.0));
        }
    }
}

fn main() {
    penas::executar(
        Opcoes {
            titulo: "Painel de Controle".into(),
            app_id: "painel-de-controle".into(),
            tamanho: (940, 640),
            minimo: (560, 420),
        },
        Box::new(Painel { aviso: None, atalhos: false, rolagem: 0.0 }),
    );
}
