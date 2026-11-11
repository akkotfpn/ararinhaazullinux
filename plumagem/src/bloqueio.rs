// SPDX-License-Identifier: GPL-3.0-or-later
//! Tela de bloqueio (Win+L): cobre tudo com o papel de parede desfocado,
//! mostra o relógio e pede a senha da conta. A senha é conferida pelo
//! ajudante `ararinha-senha` (a sessão não lê o /etc/shadow), numa linha
//! separada para a tela não travar.

use std::{
    io::Write,
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver},
};

use smithay::{
    backend::renderer::{
        element::{memory::MemoryRenderBufferRenderElement, Kind},
        ImportMem, Renderer,
    },
    input::keyboard::Keysym,
    utils::{Logical, Point, Size},
};

use crate::desenho::{self, tema, Cor, Tela};

pub struct Bloqueio {
    senha: String,
    erro: Option<String>,
    conferindo: Option<Receiver<bool>>,
    usuario: String,
    hora: (std::time::Instant, String, String),
    buf: Option<(String, smithay::backend::renderer::element::memory::MemoryRenderBuffer)>,
}

fn agora() -> (String, String) {
    let saida = Command::new("date")
        .arg("+%H:%M|%A, %-d de %B")
        .env("LC_TIME", "pt_BR.UTF-8")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let (h, d) = saida.trim().split_once('|').unwrap_or(("", ""));
    (h.to_string(), d.to_string())
}

impl Bloqueio {
    pub fn novo() -> Bloqueio {
        let (h, d) = agora();
        Bloqueio {
            senha: String::new(),
            erro: None,
            conferindo: None,
            usuario: std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_default(),
            hora: (std::time::Instant::now(), h, d),
            buf: None,
        }
    }

    /// Tecla na tela de bloqueio.
    pub fn tecla(&mut self, sym: Keysym) {
        if self.conferindo.is_some() {
            return;
        }
        match sym {
            Keysym::Return | Keysym::KP_Enter => self.conferir(),
            Keysym::BackSpace => {
                self.senha.pop();
            }
            Keysym::Escape => self.senha.clear(),
            s => {
                if let Some(c) = s.key_char() {
                    if !c.is_control() && self.senha.chars().count() < 128 {
                        self.senha.push(c);
                        self.erro = None;
                    }
                }
            }
        }
    }

    fn conferir(&mut self) {
        let senha = std::mem::take(&mut self.senha);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let ok = Command::new("ararinha-senha")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .and_then(|mut f| {
                    if let Some(mut e) = f.stdin.take() {
                        let _ = e.write_all(senha.as_bytes());
                    }
                    f.wait()
                })
                .map(|s| s.success())
                .unwrap_or(false);
            let _ = tx.send(ok);
        });
        self.conferindo = Some(rx);
    }

    /// `true` quando a senha foi aceita (a tela some).
    pub fn atualizar(&mut self) -> bool {
        if let Some(rx) = &self.conferindo {
            if let Ok(ok) = rx.try_recv() {
                self.conferindo = None;
                if ok {
                    return true;
                }
                self.erro = Some("Senha incorreta. Tente de novo.".into());
            }
        }
        if self.hora.0.elapsed().as_secs() >= 5 {
            let (h, d) = agora();
            self.hora = (std::time::Instant::now(), h, d);
        }
        false
    }

    fn desenhar(&self, tela: Size<i32, Logical>) -> Tela {
        let (l, a) = (tela.w as f32, tela.h as f32);
        let mut t = Tela::nova(tela.w, tela.h);
        t.vidro(0.0, 0.0, l, a, 0.0, (0, 0), Cor(11, 26, 46, 120));
        let (hora, data) = (&self.hora.1, &self.hora.2);
        let lh = desenho::medir(84.0, hora);
        t.texto_com_brilho(l / 2.0 - lh / 2.0, a * 0.3, 84.0, tema::BRANCO(), hora);
        let ld = desenho::medir(20.0, data);
        t.texto(l / 2.0 - ld / 2.0, a * 0.3 + 40.0, 20.0, tema::BRANCO(), data);
        // Cartão da senha
        let (cl, ca) = (380.0, 170.0);
        let (cx, cy) = (l / 2.0 - cl / 2.0, a * 0.52);
        t.vidro(cx, cy, cl, ca, 16.0, (cx as i32, cy as i32), Cor(244, 247, 250, 150));
        let lu = desenho::medir(18.0, &self.usuario);
        t.texto(cx + cl / 2.0 - lu / 2.0, cy + 40.0, 18.0, desenho::Cor::hex(0x0B1A2E), &self.usuario);
        let (fx, fy, fl) = (cx + 30.0, cy + 62.0, cl - 60.0);
        t.retangulo(fx, fy, fl, 40.0, 8.0, tema::DESTAQUE());
        t.retangulo(fx + 1.5, fy + 1.5, fl - 3.0, 37.0, 7.0, Cor::hex(0xFFFFFF));
        if self.senha.is_empty() {
            t.texto(fx + 14.0, fy + 26.0, 14.0, Cor::hex(0x4A5A6A), if self.conferindo.is_some() { "Conferindo…" } else { "Senha" });
        } else {
            let pontos = "•".repeat(self.senha.chars().count().min(30));
            t.texto(fx + 14.0, fy + 27.0, 16.0, Cor::hex(0x0B1A2E), &pontos);
        }
        let msg = self.erro.as_deref().unwrap_or("Digite a senha e tecle Enter");
        let cor = if self.erro.is_some() { Cor::hex(0xC2413B) } else { Cor::hex(0x4A5A6A) };
        let lm = desenho::medir(12.5, msg);
        t.texto(cx + cl / 2.0 - lm / 2.0, cy + 132.0, 12.5, cor, msg);
        t
    }

    pub fn elementos<R>(
        &mut self,
        renderer: &mut R,
        tela: Size<i32, Logical>,
        escala: f64,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Send + Clone + 'static,
    {
        desenho::preparar_vidro(tela.w, tela.h);
        let chave = format!(
            "{tela:?}|{}|{:?}|{}|{}|{}",
            self.senha.chars().count(),
            self.erro,
            self.conferindo.is_some(),
            self.hora.1,
            self.hora.2
        );
        if self.buf.as_ref().map(|(c, _)| c != &chave).unwrap_or(true) {
            let t = self.desenhar(tela);
            self.buf = Some((chave, t.em_buffer()));
        }
        let Some((_, buf)) = &self.buf else { return Vec::new() };
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            Point::<f64, Logical>::from((0.0, 0.0)).to_physical(escala),
            buf,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .map(|e| vec![e])
        .unwrap_or_default()
    }
}

impl std::fmt::Debug for Bloqueio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nunca mostra a senha.
        f.debug_struct("Bloqueio").field("usuario", &self.usuario).finish_non_exhaustive()
    }
}
