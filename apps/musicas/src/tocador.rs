// SPDX-License-Identifier: GPL-3.0-or-later
//! O motor do tocador: decodifica com o Symphonia e toca pelo ALSA (que, no
//! Ararinha, vai para o PipeWire). Roda numa linha separada e recebe
//! comandos por um canal; o estado fica num Mutex para a janela ler.

use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        mpsc::{Receiver, TryRecvError},
        Arc, Mutex,
    },
};

use alsa::{
    pcm::{Access, Format, HwParams, PCM},
    Direction, ValueOr,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as ErroSymphonia,
    formats::{FormatOptions, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::{MetadataOptions, StandardTagKey},
    probe::Hint,
    units::Time,
};

pub enum Comando {
    Tocar(PathBuf),
    Pausar,
    Continuar,
    Parar,
    /// Ir para a posição, em segundos.
    Buscar(f64),
}

#[derive(Debug, Clone, Default)]
pub struct Estado {
    pub arquivo: Option<PathBuf>,
    pub titulo: String,
    pub artista: String,
    pub posicao: f64,
    pub duracao: Option<f64>,
    pub tocando: bool,
    pub pausado: bool,
    /// A faixa terminou sozinha (a janela passa para a próxima).
    pub terminou: bool,
    pub erro: Option<String>,
}

fn abrir_saida(taxa: u32, canais: usize) -> Result<PCM, String> {
    let pcm = PCM::new("default", Direction::Playback, false).map_err(|e| format!("som: {e}"))?;
    {
        let hw = HwParams::any(&pcm).map_err(|e| format!("som: {e}"))?;
        hw.set_channels(canais as u32).map_err(|e| format!("canais: {e}"))?;
        hw.set_rate(taxa, ValueOr::Nearest).map_err(|e| format!("taxa: {e}"))?;
        hw.set_format(Format::s16()).map_err(|e| format!("formato: {e}"))?;
        hw.set_access(Access::RWInterleaved).map_err(|e| format!("acesso: {e}"))?;
        let _ = hw.set_buffer_time_near(250_000, ValueOr::Nearest);
        pcm.hw_params(&hw).map_err(|e| format!("som: {e}"))?;
    }
    Ok(pcm)
}

/// Título e artista das etiquetas do arquivo (ID3, Vorbis...).
fn etiquetas(meta: Option<symphonia::core::meta::MetadataRevision>) -> (Option<String>, Option<String>) {
    let mut titulo = None;
    let mut artista = None;
    if let Some(rev) = meta {
        for tag in rev.tags() {
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) => titulo = Some(tag.value.to_string()),
                Some(StandardTagKey::Artist) => artista = Some(tag.value.to_string()),
                _ => {}
            }
        }
    }
    (titulo, artista)
}

pub fn nome_da_faixa(p: &Path) -> String {
    p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Toca um arquivo até acabar ou chegar outro comando que não seja de
/// pausa/busca. Devolve o comando que interrompeu, se houver.
fn tocar(caminho: &Path, rx: &Receiver<Comando>, estado: &Arc<Mutex<Estado>>) -> Result<Option<Comando>, String> {
    let arquivo = File::open(caminho).map_err(|e| format!("abrir: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(arquivo), Default::default());
    let mut dica = Hint::new();
    if let Some(ext) = caminho.extension().and_then(|e| e.to_str()) {
        dica.with_extension(ext);
    }
    let mut sondado = symphonia::default::get_probe()
        .format(&dica, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("formato não reconhecido: {e}"))?;
    let (mut titulo, mut artista) = etiquetas(sondado.metadata.get().and_then(|m| m.current().cloned()));
    let mut formato = sondado.format;
    if titulo.is_none() {
        let (t, a) = etiquetas(formato.metadata().current().cloned());
        titulo = t;
        artista = artista.or(a);
    }
    let faixa = formato
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("o arquivo não tem áudio")?;
    let id = faixa.id;
    let base = faixa.codec_params.time_base;
    let taxa = faixa.codec_params.sample_rate.unwrap_or(44_100);
    let duracao = match (faixa.codec_params.n_frames, base) {
        (Some(n), Some(tb)) => {
            let t = tb.calc_time(n);
            Some(t.seconds as f64 + t.frac)
        }
        (Some(n), None) => Some(n as f64 / taxa as f64),
        _ => None,
    };
    let mut decodificador = symphonia::default::get_codecs()
        .make(&faixa.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("codec sem suporte: {e}"))?;

    {
        let mut e = estado.lock().unwrap();
        e.arquivo = Some(caminho.to_path_buf());
        e.titulo = titulo.unwrap_or_else(|| nome_da_faixa(caminho));
        e.artista = artista.unwrap_or_default();
        e.duracao = duracao;
        e.posicao = 0.0;
        e.tocando = true;
        e.pausado = false;
        e.terminou = false;
        e.erro = None;
    }

    let mut saida: Option<(PCM, usize)> = None;
    let mut amostras: Option<(SampleBuffer<i16>, usize, usize)> = None;
    loop {
        // Comandos entre um pacote e outro.
        let cmd = match rx.try_recv() {
            Ok(c) => Some(c),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => return Ok(None),
        };
        if let Some(c) = cmd {
            match c {
                Comando::Pausar => {
                    estado.lock().unwrap().pausado = true;
                    if let Some((pcm, _)) = &saida {
                        let _ = pcm.drop();
                    }
                    // Espera o próximo comando parado.
                    loop {
                        match rx.recv() {
                            Ok(Comando::Continuar) => break,
                            Ok(Comando::Pausar) => continue,
                            Ok(Comando::Buscar(s)) => {
                                let _ = formato.seek(
                                    SeekMode::Coarse,
                                    SeekTo::Time { time: Time::new(s as u64, s.fract()), track_id: Some(id) },
                                );
                                decodificador.reset();
                                estado.lock().unwrap().posicao = s;
                            }
                            Ok(outro) => {
                                estado.lock().unwrap().pausado = false;
                                return Ok(Some(outro));
                            }
                            Err(_) => return Ok(None),
                        }
                    }
                    estado.lock().unwrap().pausado = false;
                    if let Some((pcm, _)) = &saida {
                        let _ = pcm.prepare();
                    }
                }
                Comando::Continuar => {}
                Comando::Buscar(s) => {
                    let _ = formato.seek(
                        SeekMode::Coarse,
                        SeekTo::Time { time: Time::new(s as u64, s.fract()), track_id: Some(id) },
                    );
                    decodificador.reset();
                    if let Some((pcm, _)) = &saida {
                        let _ = pcm.drop();
                        let _ = pcm.prepare();
                    }
                    estado.lock().unwrap().posicao = s;
                }
                outro => return Ok(Some(outro)),
            }
        }

        let pacote = match formato.next_packet() {
            Ok(p) => p,
            Err(ErroSymphonia::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(ErroSymphonia::ResetRequired) => {
                decodificador.reset();
                continue;
            }
            Err(e) => return Err(format!("leitura: {e}")),
        };
        if pacote.track_id() != id {
            continue;
        }
        if let Some(tb) = base {
            let t = tb.calc_time(pacote.ts());
            estado.lock().unwrap().posicao = t.seconds as f64 + t.frac;
        }
        let decodificado = match decodificador.decode(&pacote) {
            Ok(d) => d,
            Err(ErroSymphonia::DecodeError(_)) => continue,
            Err(e) => return Err(format!("decodificação: {e}")),
        };
        let spec = *decodificado.spec();
        let canais = spec.channels.count();
        if saida.as_ref().map(|(_, c)| *c != canais).unwrap_or(true) {
            saida = Some((abrir_saida(spec.rate, canais)?, canais));
        }
        let quadros = decodificado.capacity();
        if amostras.as_ref().map(|(_, cap, c)| *cap < quadros || *c != canais).unwrap_or(true) {
            amostras = Some((SampleBuffer::<i16>::new(quadros as u64, spec), quadros, canais));
        }
        let Some((buf, _, _)) = amostras.as_mut() else { continue };
        buf.copy_interleaved_ref(decodificado);
        if let Some((pcm, canais)) = &saida {
            let io = pcm.io_i16().map_err(|e| format!("som: {e}"))?;
            let mut resto: &[i16] = buf.samples();
            while !resto.is_empty() {
                match io.writei(resto) {
                    Ok(quadros) => resto = &resto[quadros * canais..],
                    Err(e) => {
                        if pcm.try_recover(e, true).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
    // Deixa o fim da música sair pela caixa de som.
    if let Some((pcm, _)) = &saida {
        let _ = pcm.drain();
    }
    let mut e = estado.lock().unwrap();
    e.tocando = false;
    e.terminou = true;
    Ok(None)
}

/// Linha do tocador: espera comandos e toca.
pub fn rodar(rx: Receiver<Comando>, estado: Arc<Mutex<Estado>>) {
    let mut proximo: Option<Comando> = None;
    loop {
        let cmd = match proximo.take() {
            Some(c) => c,
            None => match rx.recv() {
                Ok(c) => c,
                Err(_) => return,
            },
        };
        if let Comando::Tocar(p) = cmd {
            match tocar(&p, &rx, &estado) {
                Ok(c) => proximo = c,
                Err(e) => {
                    let mut st = estado.lock().unwrap();
                    st.erro = Some(format!("{}: {e}", nome_da_faixa(&p)));
                    st.tocando = false;
                    st.terminou = true;
                }
            }
            if matches!(proximo, Some(Comando::Parar)) {
                let mut st = estado.lock().unwrap();
                st.tocando = false;
                st.terminou = false;
                st.posicao = 0.0;
                proximo = None;
            }
        }
    }
}
