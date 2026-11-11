// SPDX-License-Identifier: GPL-3.0-or-later
//! Página "Conta": foto da conta (~/.face) e trocar a senha da própria
//! conta (pelo ararinha-senha).

use std::{
    io::Write,
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

use penas::{desenho, tema, Tecla, Ui};

#[derive(Default)]
pub struct PaginaConta {
    campos: [String; 3],
    foco: usize,
    pub aviso: Option<String>,
    rx: Option<Receiver<i32>>,
    foto: Option<penas::desenho::Imagem>,
    foto_lida: bool,
    rx_foto: Option<Receiver<Result<(), String>>>,
}

fn casa() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// Lê a foto da conta, recortada num círculo.
fn ler_foto(lado: u32) -> Option<penas::desenho::Imagem> {
    let bytes = std::fs::read(format!("{}/.face", casa())).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let mut r = image::imageops::resize(&img, lado, lado, image::imageops::FilterType::Triangle);
    let c = lado as f32 / 2.0;
    for (x, y, p) in r.enumerate_pixels_mut() {
        let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
        p[3] = (p[3] as f32 * (c - d).clamp(0.0, 1.0)) as u8;
    }
    Some(r)
}

/// Pede a foto ao Explorador (modo escolher), recorta o centro quadrado e
/// grava ~/.face em PNG de 256 px (o Ninho, o menu e o bloqueio usam).
fn escolher_foto() -> Result<(), String> {
    let saida = Command::new("explorador")
        .args(["--escolher", "abrir", "--titulo", "Escolha a foto da conta", "--pasta"])
        .arg(format!("{}/Imagens", casa()))
        .output()
        .map_err(|e| format!("Não deu para abrir o Explorador: {e}"))?;
    let caminho = String::from_utf8_lossy(&saida.stdout).lines().next().unwrap_or("").trim().to_string();
    if caminho.is_empty() {
        return Ok(());
    }
    let img = image::ImageReader::open(&caminho)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| format!("Não deu para abrir a imagem: {e}"))?
        .decode()
        .map_err(|_| "Não deu para ler essa imagem (use PNG, JPEG ou WebP).".to_string())?
        .to_rgba8();
    let q = img.width().min(img.height());
    let recorte = image::imageops::crop_imm(&img, (img.width() - q) / 2, (img.height() - q) / 2, q, q).to_image();
    let foto = image::imageops::resize(&recorte, 256, 256, image::imageops::FilterType::Lanczos3);
    let destino = format!("{}/.face", casa());
    let tmp = format!("{destino}.novo");
    foto.save_with_format(&tmp, image::ImageFormat::Png).map_err(|e| format!("Não deu para salvar a foto: {e}"))?;
    std::fs::rename(&tmp, &destino).map_err(|e| format!("Não deu para salvar a foto: {e}"))
}

impl PaginaConta {
    fn enviar(&mut self) {
        let [atual, nova, confirma] = &self.campos;
        if nova != confirma {
            self.aviso = Some("A senha nova e a confirmação estão diferentes.".into());
            return;
        }
        if nova.chars().count() < 4 {
            self.aviso = Some("A senha nova precisa ter pelo menos 4 caracteres.".into());
            return;
        }
        let entrada = format!("{atual}\n{nova}\n");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = Command::new("ararinha-senha")
                .arg("--trocar")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .and_then(|mut f| {
                    if let Some(mut e) = f.stdin.take() {
                        let _ = e.write_all(entrada.as_bytes());
                    }
                    f.wait()
                })
                .ok()
                .and_then(|s| s.code())
                .unwrap_or(2);
            let _ = tx.send(r);
        });
        self.rx = Some(rx);
        self.aviso = Some("Trocando a senha…".into());
    }

    fn desenhar_foto(&mut self, ui: &mut Ui, x: f32, y: f32) {
        if !self.foto_lida {
            self.foto_lida = true;
            self.foto = ler_foto(112);
        }
        if let Some(rx) = &self.rx_foto {
            match rx.try_recv() {
                Ok(r) => {
                    self.rx_foto = None;
                    match r {
                        Ok(()) => {
                            self.foto = ler_foto(112);
                            if self.foto.is_some() {
                                self.aviso = Some("Foto da conta atualizada.".into());
                            }
                        }
                        Err(e) => self.aviso = Some(format!("Não deu: {e}")),
                    }
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(250)),
            }
        }
        ui.tela.texto(x, y, 12.5, tema::TEXTO_SECUNDARIO(), "Foto da conta");
        let (fx, fy) = (x, y + 14.0);
        if let Some(f) = &self.foto {
            ui.tela.imagem(fx as i32, fy as i32, f);
        } else {
            let usuario = std::env::var("USER").unwrap_or_default();
            ui.tela.retangulo(fx, fy, 112.0, 112.0, 56.0, tema::DESTAQUE());
            let ini: String = usuario.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let li = desenho::medir(44.0, &ini);
            ui.tela.texto(fx + 56.0 - li / 2.0, fy + 72.0, 44.0, tema::BRANCO(), &ini);
        }
        let livre = self.rx_foto.is_none();
        if penas::componentes::botao(ui, x, fy + 128.0, "Escolher foto…", penas::componentes::Estilo::Comum, livre) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(escolher_foto());
            });
            self.rx_foto = Some(rx);
        }
        if self.foto.is_some() && penas::componentes::botao(ui, x, fy + 172.0, "Remover foto", penas::componentes::Estilo::Comum, livre) {
            let _ = std::fs::remove_file(format!("{}/.face", casa()));
            self.foto = None;
            self.aviso = Some("Foto removida.".into());
        }
    }

    pub fn desenhar(&mut self, ui: &mut Ui, x0: f32, l: f32) {
        // Foto ao lado dos campos de senha (ou embaixo, se a janela for estreita).
        if l >= 600.0 {
            self.desenhar_foto(ui, x0 + 410.0, 120.0);
        } else {
            self.desenhar_foto(ui, x0 + 32.0, 400.0);
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(c) => {
                    self.rx = None;
                    self.aviso = Some(match c {
                        0 => {
                            self.campos = Default::default();
                            "Senha trocada. Use a nova senha para entrar e desbloquear.".into()
                        }
                        1 => "A senha atual está errada.".into(),
                        _ => "Não deu para trocar a senha.".into(),
                    });
                }
                Err(_) => ui.acordar_em = Some(Duration::from_millis(200)),
            }
        }
        for t in std::mem::take(&mut ui.teclas) {
            match t {
                Tecla::Texto(s) => self.campos[self.foco].push_str(&s),
                Tecla::Apagar => {
                    self.campos[self.foco].pop();
                }
                Tecla::Outra(penas::Keysym::Tab) | Tecla::Baixo => self.foco = (self.foco + 1) % 3,
                Tecla::Cima => self.foco = (self.foco + 2) % 3,
                Tecla::Enter => {
                    if self.foco < 2 {
                        self.foco += 1;
                    } else if self.rx.is_none() {
                        self.enviar();
                    }
                }
                _ => {}
            }
        }
        let usuario = std::env::var("USER").unwrap_or_default();
        ui.tela.texto(x0 + 32.0, 52.0, 22.0, tema::TEXTO(), "Conta");
        ui.tela.texto(x0 + 32.0, 84.0, 13.0, tema::TEXTO_SECUNDARIO(), &format!("Você entrou como \"{usuario}\". Troque aqui a sua senha."));
        let rotulos = ["Senha atual", "Senha nova", "Confirme a senha nova"];
        for (i, r) in rotulos.iter().enumerate() {
            let y = 120.0 + i as f32 * 70.0;
            ui.tela.texto(x0 + 32.0, y, 12.5, tema::TEXTO_SECUNDARIO(), r);
            let (fx, fy, fl) = (x0 + 32.0, y + 8.0, 340.0);
            let foco = self.foco == i;
            ui.tela.retangulo(fx, fy, fl, 38.0, 7.0, if foco { tema::DESTAQUE() } else { tema::BORDA() });
            ui.tela.retangulo(fx + 1.0, fy + 1.0, fl - 2.0, 36.0, 6.0, tema::JANELA());
            let pontos = "•".repeat(self.campos[i].chars().count().min(40));
            let fim = ui.tela.texto(fx + 12.0, fy + 25.0, 14.0, tema::TEXTO(), &pontos);
            if foco {
                ui.tela.retangulo(fx + 13.0 + fim, fy + 9.0, 1.5, 20.0, 0.0, tema::DESTAQUE());
            }
            if ui.clicou(fx, fy, fl, 38.0) {
                self.foco = i;
            }
        }
        let (bx, by) = (x0 + 32.0, 330.0);
        let ativo = self.rx.is_none() && !self.campos[1].is_empty();
        let s = ativo && ui.sobre(bx, by, 150.0, 38.0);
        ui.tela.retangulo(bx, by, 150.0, 38.0, 8.0, if !ativo { tema::SUPERFICIE_ALTA() } else if s { tema::DESTAQUE_FORTE() } else { tema::DESTAQUE() });
        let rot = "Alterar senha";
        let lr = desenho::medir(13.5, rot);
        ui.tela.texto(bx + 75.0 - lr / 2.0, by + 24.0, 13.5, if ativo { tema::BRANCO() } else { tema::TEXTO_SECUNDARIO() }, rot);
        if ativo && ui.clicou(bx, by, 150.0, 38.0) {
            self.enviar();
        }
    }
}
