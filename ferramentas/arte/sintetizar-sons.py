# SPDX-License-Identifier: GPL-3.0-or-later
# Sintetiza os sons do sistema (Grito) inspirados no chamado da ararinha-azul.
# Os sons gerados ficam em arte/sons/ sob CC BY-SA 4.0.
#
# Uso: python3 ferramentas/arte/sintetizar-sons.py <raiz-do-repo>
# Requer: Python 3 e numpy. Se o ffmpeg existir, também gera .oga (Opus).
#
# Ideia: o chamado de araras e ararinhas é um "piado" curto com varredura de
# frequência rápida e harmônicos ásperos. Para sons de sistema, suavizamos:
# varredura em FM, poucos harmônicos, envelope curto, sem cliques.

import os
import shutil
import subprocess
import sys
import wave

import numpy as np

TAXA = 48000


def envelope(n, ataque=0.008, liberacao=0.06):
    t = np.arange(n) / TAXA
    dur = n / TAXA
    env = np.minimum(1.0, t / ataque)
    env *= np.clip((dur - t) / liberacao, 0.0, 1.0)
    return env


def piado(f_ini, f_fim, dur, aspereza=0.25, vibrato=0.0):
    """Um piado: varredura exponencial de f_ini a f_fim com harmônicos."""
    n = int(dur * TAXA)
    t = np.arange(n) / TAXA
    k = np.log(f_fim / f_ini) / dur
    freq = f_ini * np.exp(k * t)
    if vibrato:
        freq *= 1 + vibrato * np.sin(2 * np.pi * 38 * t)
    fase = 2 * np.pi * np.cumsum(freq) / TAXA
    sinal = np.sin(fase)
    sinal += aspereza * np.sin(2 * fase) + (aspereza / 2) * np.sin(3 * fase)
    return sinal * envelope(n)


def silencio(dur):
    return np.zeros(int(dur * TAXA))


def mix(*partes):
    s = np.concatenate(partes)
    # leve eco, como num vale da Caatinga
    atraso = int(0.09 * TAXA)
    eco = np.zeros_like(s)
    eco[atraso:] = s[:-atraso] * 0.18
    s = s + eco
    return 0.6 * s / np.max(np.abs(s))


SONS = {
    # sessão iniciada: três piados subindo, como a ave levantando voo
    "sessao-iniciada": lambda: mix(
        piado(1400, 2100, 0.11), silencio(0.05),
        piado(1700, 2600, 0.11), silencio(0.05),
        piado(2100, 3300, 0.18, vibrato=0.01), silencio(0.25)),
    # notificação: dois piados curtos
    "notificacao": lambda: mix(
        piado(2200, 2900, 0.07), silencio(0.04),
        piado(2600, 3200, 0.09), silencio(0.2)),
    # erro: piado descendente e mais grave
    "erro": lambda: mix(
        piado(1500, 900, 0.16, aspereza=0.45), silencio(0.04),
        piado(1200, 700, 0.18, aspereza=0.45), silencio(0.2)),
    # dispositivo conectado (pendrive, unidade de rede)
    "dispositivo-conectado": lambda: mix(
        piado(1300, 2400, 0.12), silencio(0.2)),
    # dispositivo removido
    "dispositivo-removido": lambda: mix(
        piado(2400, 1300, 0.12), silencio(0.2)),
    # desligar: piado longo descendo, a ave voltando ao ninho
    "desligar": lambda: mix(
        piado(2600, 1500, 0.22, vibrato=0.01), silencio(0.06),
        piado(1800, 1000, 0.3), silencio(0.3)),
}


def gravar_wav(caminho, amostras):
    dados = (amostras * 32767).astype("<i2").tobytes()
    with wave.open(caminho, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(TAXA)
        w.writeframes(dados)


def main():
    raiz = sys.argv[1] if len(sys.argv) > 1 else "."
    saida = os.path.join(raiz, "arte", "sons", "grito-padrao")
    os.makedirs(saida, exist_ok=True)
    ffmpeg = shutil.which("ffmpeg")
    for nome, gerar in SONS.items():
        wav = os.path.join(saida, nome + ".wav")
        gravar_wav(wav, gerar())
        if ffmpeg:
            oga = os.path.join(saida, nome + ".oga")
            subprocess.run([ffmpeg, "-loglevel", "error", "-y", "-i", wav,
                            "-c:a", "libopus", "-b:a", "64k", oga], check=True)
            os.remove(wav)
        print("gerado:", nome)


if __name__ == "__main__":
    main()
