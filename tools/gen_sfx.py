#!/usr/bin/env python3
"""Synthesise every sound DayDreams ships: `assets/sfx/*.flac` and `assets/music/*.flac`.

Nothing here is a recording. Every file is built from noise, sine partials and envelopes by
the small DSP kit below, so the shipped audio carries no third-party rights at all
(THIRD_PARTY.md, "Generated and original assets"). Seeded and deterministic: the same numpy,
the same ffmpeg and the same seed give byte-identical files, so a regenerated tree is a
no-op in `git status`.

    python3 tools/gen_sfx.py                     # write assets/ and the contact sheets
    python3 tools/gen_sfx.py --only footstep     # just the sounds whose stem contains that
    python3 tools/gen_sfx.py --plots DIR         # where the contact sheets go

Every file is measured after it has been encoded and decoded again -- what the game actually
plays, not what the synthesiser produced -- and the numbers are asserted against the table
below. A sound that has drifted out of its band stops the run rather than shipping.

FLAC, not Vorbis. The plan was mono Ogg Vorbis, and this machine's ffmpeg cannot write it:
`ffmpeg -encoders` offers no `libvorbis`, and the built-in `vorbis` encoder is experimental
and refuses anything but two channels ("Current FFmpeg Vorbis encoder only supports 2
channels"). FLAC is what is left that kira decodes (`ogg`, `vorbis`, `flac`, `mp3`, `wav`,
`pcm` are its default features), and it is the better fit anyway: lossless, so the loops wrap
on the exact samples they were built to wrap on rather than on whatever a lossy codec left
there, and bit-exact from one run to the next. The cost is size, which is why the loops are
30 s rather than 60 -- the whole of `assets/` has a 15 MB budget.

MANIFEST -- 42 effects (mono 44.1 kHz) and 5 loops (mono 32 kHz), all FLAC:

    footstep_carpet_1..4   lowpassed noise burst over a damped 55 Hz body -- soft, dull, low
    footstep_tile_1..4     bright noise slap through a small tiled room impulse
    footstep_water_1..4    broadband splash plus rising bubble chirps
    footstep_moss_1..4     heavily damped burst with a few quiet crackles
    footstep_grass_1..4    dry band-passed rustle, amplitude fluttered by slow noise
    jump                   cloth push-off: lowpassed noise over a 120->70 Hz body sweep
    land_soft              a carpet tread an octave down, longer decay
    land_hard              45->30 Hz body, broadband impact, gently saturated
    grab                   soft resonant pick, pitch rising a little
    release                the same pick, duller and falling
    stow                   muffled swallow: a downward noise sweep into a soft close
    retrieve               the swallow reversed, ending on a bright tick
    drop                   dull thud plus one small bounce
    refuse                 short dead thunk: a 90 Hz body damped almost flat
    key_take               small metallic ring: four slightly inharmonic partials
    key_use                a lock turning: three dry clicks and a low clunk
    window_grow            low glassy swell: glass partials rising under a slow lowpass sweep
    portal                 band-passed noise swept up and back, into a long tail
    elevator_button        tactile click plus a short 1.4 kHz beep
    elevator_ding          bell partials (1, 2.76, 5.40, 8.93) decaying at their own rates
    elevator_doors         rolling slide of filtered noise, ending on a soft thunk
    elevator_ride          motor rumble: 58 Hz sawtooth partials under an amplitude wobble
    ui_move                60 ms band-passed tick
    ui_confirm             two blips rising 660 -> 990 Hz
    ui_back                two blips falling 880 -> 550 Hz
    door_open              wood body thump, then a creak: a resonance wandering 210-520 Hz
    door_close             the swing back, then a latch clunk on a 120 Hz body
    16-meadow              wind through a wandering lowpass, gusts, two distant thunders
    17-backrooms           100 Hz fluorescent buzz with its harmonics over dead air
    18-pool                drips into a long bright tiled reverb, faint water surface
    19-overgrown           insect band pulsing at 19 Hz, leaf rustles, the buzz further off
    ambient                fallback: two beating 55 Hz sines under filtered air

The number in a music filename is the scene's position in the registry, ONE-BASED, which is
what `Audio::set_scene` matches (`ext/audio.rs`, `scene_of_stem`). `src/level16.rs` is the
SEVENTEENTH entry in `ext::scenes::SCENES`, so the Backrooms track is `17-backrooms.flac`.
The name after the number is what makes the file readable; the number is only the binding.
"""

from __future__ import annotations

import argparse
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import numpy as np
from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parents[1]

SR = 44100
"""Effects: 44.1 kHz, so the bright transients (tile, grass, keys) keep their top octave."""

LOOP_SR = 32000
"""Loops: 32 kHz. Ambience has nothing above 15 kHz, and the lower rate is a quarter off the
file size of material that cannot use the bandwidth."""

LOOP_SECS = 30.0
"""Every loop's length. The low end of what does not sound like a loop; the high end is what
lossless ambience costs (`assets/` has a 15 MB budget and the portrait sheets take 7 MB)."""

LOOP_RMS_DBFS = -20.0
"""Loops are levelled by RMS, not by peak: they are wallpaper, and what matters is how loud
they sit under everything else, not how tall their loudest drip is."""

LOOP_CEILING = 0.5
"""And their peaks are rounded off here (-6 dBFS) so a single drip cannot startle."""

BASE_SEED = 0x0DA7DBEA
"""One seed for the whole tool; each sound derives its own from this and its name."""

PEAK_DBFS = -3.0
"""Every effect is peak-normalised here: loud enough to sit in the mix, 3 dB of headroom
for the overshoot a lossy encoder adds."""


# ─────────────────────────────────────────────────────────────────────────────
# The DSP kit.
# ─────────────────────────────────────────────────────────────────────────────


def rng_for(name: str, salt: int = 0) -> np.random.Generator:
    """A generator keyed by name, not by call order: adding a sound cannot move another one.

    `zlib.crc32` rather than `hash`, which is salted per process and would make the tool
    non-deterministic between runs.
    """
    return np.random.default_rng((BASE_SEED ^ zlib.crc32(name.encode()) ^ (salt * 0x9E3779B1)) & 0xFFFFFFFF)


def secs(n: int, sr: int = SR) -> np.ndarray:
    """The time axis for `n` samples."""
    return np.arange(n) / sr


def spectral(x: np.ndarray, sr: int, response: Callable[[np.ndarray], np.ndarray]) -> np.ndarray:
    """Zero-phase filter: multiply the spectrum by a magnitude response.

    Zero-phase smears a transient backwards as well as forwards, which is why every sound
    below filters its noise FIRST and applies the amplitude envelope afterwards -- the
    envelope's silent head cuts the pre-ring off.
    """
    n = len(x)
    spec = np.fft.rfft(x)
    freq = np.fft.rfftfreq(n, 1.0 / sr)
    return np.fft.irfft(spec * response(np.maximum(freq, 1e-6)), n)


def lowpass(x: np.ndarray, sr: int, fc: float, order: int = 2) -> np.ndarray:
    """Butterworth magnitude, `order` poles."""
    return spectral(x, sr, lambda f: 1.0 / np.sqrt(1.0 + (f / fc) ** (2 * order)))


def highpass(x: np.ndarray, sr: int, fc: float, order: int = 2) -> np.ndarray:
    """The mirror of `lowpass`."""
    return spectral(x, sr, lambda f: 1.0 / np.sqrt(1.0 + (fc / f) ** (2 * order)))


def bandpass(x: np.ndarray, sr: int, fc: float, q: float = 2.0) -> np.ndarray:
    """One resonant band; `q` is the usual centre-over-width."""
    return spectral(x, sr, lambda f: 1.0 / np.sqrt(1.0 + (q * (f / fc - fc / f)) ** 2))


def resonate(x: np.ndarray, sr: int, fc: float, q: float, gain_db: float) -> np.ndarray:
    """A peaking boost: the body resonance a box or a pane rings at, left in place."""
    g = 10.0 ** (gain_db / 20.0) - 1.0
    return spectral(x, sr, lambda f: 1.0 + g / np.sqrt(1.0 + (q * (f / fc - fc / f)) ** 2))


def tilt(x: np.ndarray, sr: int, db_per_octave: float, pivot: float = 1000.0) -> np.ndarray:
    """A constant slope across the spectrum -- pink noise out of white, and gentler slopes."""
    return spectral(x, sr, lambda f: 10.0 ** (db_per_octave * np.log2(f / pivot) / 20.0))


def env(n: int, sr: int, attack: float, decay: float, curve: float = 3.0, hold: float = 0.0) -> np.ndarray:
    """Linear attack, exponential decay; `curve` sets how many time constants fit in `decay`."""
    t = secs(n, sr)
    rise = np.clip(t / max(attack, 1e-6), 0.0, 1.0)
    fall = np.exp(-curve * np.clip((t - attack - hold) / max(decay, 1e-6), 0.0, None))
    return rise * fall


def sweep(n: int, sr: int, f0: float, f1: float, log: bool = True) -> np.ndarray:
    """A sine whose frequency runs f0 -> f1 over the whole span, phase-continuous."""
    t = secs(n, sr)
    span = max(n / sr, 1e-9)
    if log and f0 > 0 and f1 > 0:
        k = np.log(f1 / f0)
        phase = 2 * np.pi * f0 * span / k * (np.exp(k * t / span) - 1.0) if abs(k) > 1e-9 else 2 * np.pi * f0 * t
    else:
        phase = 2 * np.pi * (f0 * t + 0.5 * (f1 - f0) * t * t / span)
    return np.sin(phase)


def partials(
    n: int,
    sr: int,
    f0: float,
    ratios: list[float],
    amps: list[float],
    decays: list[float],
    inharm: float = 0.0,
    rng: np.random.Generator | None = None,
) -> np.ndarray:
    """Additive stack: each partial its own ratio, level and decay, detuned by `inharm`.

    The detuning is the point. Exact integer ratios sound like a synthesiser; a fraction of
    a percent of scatter is what makes a struck object sound struck.
    """
    t = secs(n, sr)
    out = np.zeros(n)
    for ratio, amp, decay in zip(ratios, amps, decays):
        detune = 1.0 + (inharm * rng.standard_normal() if rng is not None else 0.0)
        phase = rng.uniform(0.0, 2 * np.pi) if rng is not None else 0.0
        out += amp * np.sin(2 * np.pi * f0 * ratio * detune * t + phase) * np.exp(-t / max(decay, 1e-6))
    return out


def room_ir(rng: np.random.Generator, sr: int, rt60: float, damping: float, taps: int = 9) -> np.ndarray:
    """A synthetic room: sparse early reflections, then exponentially decaying damped noise."""
    n = max(8, int(rt60 * sr))
    t = secs(n, sr)
    tail = rng.standard_normal(n) * np.exp(-6.9078 * t / rt60)
    tail = lowpass(tail, sr, damping, 2)
    for k in range(taps):
        at = int((0.004 + 0.055 * rng.random() * (k + 1) / taps) * sr)
        if at < n:
            tail[at] += (0.6 / (1 + k)) * (1 if rng.random() > 0.35 else -1)
    tail[0] += 1.0
    return tail / np.sqrt(np.sum(tail * tail))


def convolve(x: np.ndarray, ir: np.ndarray) -> np.ndarray:
    """FFT convolution, trimmed to the input length plus the tail."""
    n = len(x) + len(ir) - 1
    size = 1 << (n - 1).bit_length()
    y = np.fft.irfft(np.fft.rfft(x, size) * np.fft.rfft(ir, size), size)[:n]
    return y


def circular_convolve(x: np.ndarray, ir: np.ndarray) -> np.ndarray:
    """Convolution that wraps: the reverb tail of the last event lands on the first samples.

    This is what makes a reverberant loop seamless rather than nearly seamless -- there is
    no seam to fade, because the signal is one period of a periodic function.
    """
    n = len(x)
    return np.fft.irfft(np.fft.rfft(x) * np.fft.rfft(ir, n), n)


def saturate(x: np.ndarray, drive: float) -> np.ndarray:
    """`tanh` soft clip, normalised back so the drive changes the shape and not the level."""
    return np.tanh(drive * x) / np.tanh(drive)


def dc_block(x: np.ndarray, sr: int) -> np.ndarray:
    """Kill the mean and everything under 20 Hz: inaudible, and it eats headroom."""
    return highpass(x - x.mean(), sr, 20.0, 2)


def fade(x: np.ndarray, sr: int, head: float = 0.002, tail: float = 0.006) -> np.ndarray:
    """Short ramps at both ends, so a file never starts or stops on a step."""
    y = x.copy()
    h, t = int(head * sr), int(tail * sr)
    if h > 1:
        y[:h] *= np.linspace(0.0, 1.0, h)
    if t > 1:
        y[-t:] *= np.linspace(1.0, 0.0, t)
    return y


def normalise(x: np.ndarray, peak_db: float = PEAK_DBFS) -> np.ndarray:
    """Peak-normalise to `peak_db`."""
    peak = np.max(np.abs(x))
    return x * (10.0 ** (peak_db / 20.0) / peak) if peak > 0 else x


def set_rms(x: np.ndarray, rms_db: float) -> np.ndarray:
    """Set the RMS instead of the peak: how the loops are levelled, being wallpaper."""
    rms = np.sqrt(np.mean(x * x))
    return x * (10.0 ** (rms_db / 20.0) / rms) if rms > 0 else x


def soft_ceiling(x: np.ndarray, ceiling: float, knee: float = 0.6) -> np.ndarray:
    """Round off everything above `knee * ceiling` and leave the rest exactly alone.

    A hard normalise would drag a whole loop down to fit one drip; a `tanh` over the whole
    signal would fatten the quiet bed it spends its life being. This touches only the peaks.
    """
    k = knee * ceiling
    mag = np.abs(x)
    over = mag > k
    out = x.copy()
    out[over] = np.sign(x[over]) * (k + (ceiling - k) * np.tanh((mag[over] - k) / (ceiling - k)))
    return out


def level_loop(x: np.ndarray) -> np.ndarray:
    """RMS to [`LOOP_RMS_DBFS`], peaks under [`LOOP_CEILING`]. Three passes, because rounding
    the peaks lowers the RMS a little; it ends on the ceiling so nothing can be over it."""
    x = dc_block(x, LOOP_SR)
    for _ in range(3):
        x = soft_ceiling(set_rms(x, LOOP_RMS_DBFS), LOOP_CEILING)
    return x


def noise_loop(rng: np.random.Generator, n: int) -> np.ndarray:
    """Noise that is exactly periodic in `n` samples: random phase on a flat spectrum."""
    spec = np.exp(2j * np.pi * rng.random(n // 2 + 1))
    spec[0] = 0.0
    if n % 2 == 0:
        spec[-1] = spec[-1].real
    x = np.fft.irfft(spec, n)
    return x / np.std(x)


def snap(freq: float, n: int, sr: int) -> float:
    """The nearest frequency that fits a whole number of cycles in `n` samples."""
    return max(1, round(freq * n / sr)) * sr / n


def place(dst: np.ndarray, src: np.ndarray, start: int) -> None:
    """Add `src` into `dst` at `start`, wrapping round the end. Keeps a loop periodic."""
    n, m = len(dst), len(src)
    s = start % n
    if s + m <= n:
        dst[s : s + m] += src
        return
    head = n - s
    dst[s:] += src[:head]
    rest = src[head:]
    while len(rest):
        take = min(len(rest), n)
        dst[:take] += rest[:take]
        rest = rest[take:]


# ─────────────────────────────────────────────────────────────────────────────
# Effects.
# ─────────────────────────────────────────────────────────────────────────────


def footstep_carpet(rng: np.random.Generator, v: int) -> np.ndarray:
    """Noise under 700 Hz over a damped 55 Hz body: the soft dull tread of the Backrooms."""
    n = int(0.20 * SR)
    cut = 700 * rng.uniform(0.85, 1.15)
    body_f = 55 * rng.uniform(0.92, 1.08)
    noise = lowpass(rng.standard_normal(n), SR, cut, 3) * env(n, SR, 0.006, 0.085 * rng.uniform(0.85, 1.2))
    body = np.sin(2 * np.pi * body_f * secs(n, SR)) * env(n, SR, 0.003, 0.06)
    return dc_block(noise * 1.0 + body * 0.65, SR)


def footstep_tile(rng: np.random.Generator, v: int) -> np.ndarray:
    """A bright 2 kHz slap, very short, through a small tiled room: the pool's hard floor."""
    n = int(0.32 * SR)
    hit = int(0.035 * SR)
    slap = bandpass(rng.standard_normal(hit), SR, 2000 * rng.uniform(0.85, 1.2), 1.1)
    slap *= env(hit, SR, 0.0008, 0.010 * rng.uniform(0.8, 1.3))
    body = np.sin(2 * np.pi * 90 * secs(hit, SR)) * env(hit, SR, 0.001, 0.012) * 0.35
    dry = np.zeros(n)
    dry[:hit] = slap + body
    wet = convolve(dry, room_ir(rng, SR, 0.42, 5200))[:n]
    return dc_block(dry * 0.8 + wet * 0.45, SR)


def footstep_water(rng: np.random.Generator, v: int) -> np.ndarray:
    """A broadband splash with two rising bubble chirps under it: wading, not walking."""
    n = int(0.30 * SR)
    splash = bandpass(rng.standard_normal(n), SR, 1400 * rng.uniform(0.8, 1.25), 0.7)
    splash *= env(n, SR, 0.004, 0.075 * rng.uniform(0.9, 1.25))
    out = splash * 0.8
    for _ in range(2):
        m = int(rng.uniform(0.03, 0.06) * SR)
        f0 = rng.uniform(500, 900)
        bub = sweep(m, SR, f0, f0 * rng.uniform(1.8, 2.6)) * env(m, SR, 0.002, 0.018)
        place(out, bub * rng.uniform(0.12, 0.22), int(rng.uniform(0.01, 0.12) * SR))
    low = np.sin(2 * np.pi * 70 * secs(n, SR)) * env(n, SR, 0.005, 0.05) * 0.35
    return dc_block(out + low, SR)


def footstep_moss(rng: np.random.Generator, v: int) -> np.ndarray:
    """Damp and muffled -- everything above 900 Hz gone -- with a few dry crackles on top."""
    n = int(0.24 * SR)
    cut = 900 * rng.uniform(0.85, 1.15)
    soft = lowpass(rng.standard_normal(n), SR, cut, 4) * env(n, SR, 0.010, 0.10 * rng.uniform(0.85, 1.2))
    body = np.sin(2 * np.pi * 48 * secs(n, SR)) * env(n, SR, 0.004, 0.055) * 0.7
    out = soft * 0.75 + body
    for _ in range(rng.integers(2, 5)):
        m = int(0.006 * SR)
        tick = bandpass(rng.standard_normal(m), SR, rng.uniform(3000, 5200), 3.0) * env(m, SR, 0.0004, 0.002)
        place(out, tick * rng.uniform(0.05, 0.12), int(rng.uniform(0.005, 0.13) * SR))
    return dc_block(out, SR)


def footstep_grass(rng: np.random.Generator, v: int) -> np.ndarray:
    """A dry rustle: 1.5-6 kHz noise whose level flutters with slow noise of its own."""
    n = int(0.22 * SR)
    band = bandpass(rng.standard_normal(n), SR, 2100 * rng.uniform(0.85, 1.2), 1.0)
    flutter = 0.45 + 0.55 * np.abs(lowpass(rng.standard_normal(n), SR, 90.0, 2))
    flutter /= flutter.max()
    body = np.sin(2 * np.pi * 62 * secs(n, SR)) * env(n, SR, 0.004, 0.04) * 0.30
    return dc_block(band * flutter * env(n, SR, 0.005, 0.075 * rng.uniform(0.85, 1.2)) + body, SR)


def jump(rng: np.random.Generator, v: int) -> np.ndarray:
    """Cloth and effort: lowpassed noise over a 120 -> 70 Hz push-off."""
    n = int(0.20 * SR)
    cloth = lowpass(rng.standard_normal(n), SR, 1100, 2) * env(n, SR, 0.008, 0.055)
    push = sweep(n, SR, 120, 70) * env(n, SR, 0.004, 0.070)
    return dc_block(cloth * 0.45 + push * 0.9, SR)


def land_soft(rng: np.random.Generator, v: int) -> np.ndarray:
    """A carpet tread taken an octave down and let ring twice as long."""
    n = int(0.28 * SR)
    noise = lowpass(rng.standard_normal(n), SR, 520, 3) * env(n, SR, 0.005, 0.10)
    body = sweep(n, SR, 62, 44) * env(n, SR, 0.003, 0.085)
    return dc_block(noise * 0.85 + body * 0.85, SR)


def land_hard(rng: np.random.Generator, v: int) -> np.ndarray:
    """A 45 -> 30 Hz body with a broadband crack over it, driven into a little saturation."""
    n = int(0.40 * SR)
    crack = lowpass(rng.standard_normal(n), SR, 2600, 2) * env(n, SR, 0.0015, 0.030)
    body = sweep(n, SR, 45, 30) * env(n, SR, 0.002, 0.10)
    mid = bandpass(rng.standard_normal(n), SR, 220, 1.4) * env(n, SR, 0.003, 0.10)
    return dc_block(saturate(crack * 0.7 + body * 0.6 + mid * 1.2, 1.7), SR)


def grab(rng: np.random.Generator, v: int) -> np.ndarray:
    """A soft resonant pick, pitch lifting a little: something leaving the floor."""
    n = int(0.14 * SR)
    click = bandpass(rng.standard_normal(n), SR, 900, 1.6) * env(n, SR, 0.002, 0.022)
    tone = sweep(n, SR, 320, 430) * env(n, SR, 0.004, 0.045)
    return dc_block(click * 0.6 + tone * 0.5, SR)


def release(rng: np.random.Generator, v: int) -> np.ndarray:
    """The pick again, duller and falling: something set down."""
    n = int(0.16 * SR)
    click = bandpass(rng.standard_normal(n), SR, 620, 1.4) * env(n, SR, 0.003, 0.028)
    tone = sweep(n, SR, 380, 250) * env(n, SR, 0.005, 0.050)
    return dc_block(click * 0.6 + tone * 0.5, SR)


def stow(rng: np.random.Generator, v: int) -> np.ndarray:
    """A muffled swallow: noise sweeping down into a soft close."""
    n = int(0.24 * SR)
    swell = lowpass(rng.standard_normal(n), SR, 1600, 2) * env(n, SR, 0.030, 0.055)
    down = sweep(n, SR, 520, 180) * env(n, SR, 0.010, 0.060) * 0.5
    close = np.zeros(n)
    m = int(0.05 * SR)
    close[-m:] = lowpass(rng.standard_normal(m), SR, 400, 3) * env(m, SR, 0.002, 0.020)
    return dc_block(swell * 0.6 + down + close * 0.7, SR)


def retrieve(rng: np.random.Generator, v: int) -> np.ndarray:
    """The swallow reversed: noise rising out of nothing onto a bright tick."""
    n = int(0.22 * SR)
    swell = lowpass(rng.standard_normal(n), SR, 1800, 2) * env(n, SR, 0.090, 0.035)
    up = sweep(n, SR, 200, 560) * env(n, SR, 0.060, 0.045) * 0.5
    tick = np.zeros(n)
    m = int(0.02 * SR)
    tick[-m:] = bandpass(rng.standard_normal(m), SR, 2600, 2.2) * env(m, SR, 0.0008, 0.006)
    return dc_block(swell * 0.6 + up + tick * 0.55, SR)


def drop(rng: np.random.Generator, v: int) -> np.ndarray:
    """A dull thud and one small bounce 90 ms behind it."""
    n = int(0.34 * SR)
    out = np.zeros(n)
    for at, level in ((0.0, 1.0), (0.09, 0.32)):
        m = int(0.14 * SR)
        thud = lowpass(rng.standard_normal(m), SR, 700, 3) * env(m, SR, 0.002, 0.035)
        body = np.sin(2 * np.pi * 78 * secs(m, SR)) * env(m, SR, 0.002, 0.045)
        place(out, (thud * 0.55 + body * 0.9) * level, int(at * SR))
    return dc_block(out, SR)


def refuse(rng: np.random.Generator, v: int) -> np.ndarray:
    """A short dead thunk for "no": a 90 Hz body damped almost flat, nothing bright on it."""
    n = int(0.17 * SR)
    body = np.sin(2 * np.pi * 90 * secs(n, SR)) * env(n, SR, 0.002, 0.028, curve=4.0)
    knock = lowpass(rng.standard_normal(n), SR, 450, 4) * env(n, SR, 0.0015, 0.020)
    return dc_block(body * 0.9 + knock * 0.5, SR)


def key_take(rng: np.random.Generator, v: int) -> np.ndarray:
    """A small metallic ring: four inharmonic partials over 2 kHz on a tiny attack click."""
    n = int(0.55 * SR)
    ring = partials(
        n, SR, 2100,
        ratios=[1.0, 1.51, 2.30, 2.96], amps=[1.0, 0.62, 0.40, 0.24],
        decays=[0.16, 0.11, 0.075, 0.05], inharm=0.006, rng=rng,
    )
    click = highpass(rng.standard_normal(n), SR, 2500, 2) * env(n, SR, 0.0004, 0.004)
    return dc_block(ring * 0.8 + click * 0.35, SR)


def key_use(rng: np.random.Generator, v: int) -> np.ndarray:
    """A lock turning over: three dry clicks a few tens of ms apart, then a low clunk."""
    n = int(0.50 * SR)
    out = np.zeros(n)
    for k, at in enumerate((0.00, 0.075, 0.145)):
        m = int(0.03 * SR)
        clk = bandpass(rng.standard_normal(m), SR, 1900 * (1.0 + 0.12 * k), 2.4)
        place(out, clk * env(m, SR, 0.0006, 0.006) * (0.9 - 0.15 * k), int(at * SR))
    m = int(0.20 * SR)
    clunk = np.sin(2 * np.pi * 130 * secs(m, SR)) * env(m, SR, 0.002, 0.045)
    clunk += lowpass(rng.standard_normal(m), SR, 900, 3) * env(m, SR, 0.002, 0.030) * 0.5
    place(out, clunk * 0.5, int(0.24 * SR))
    return dc_block(out, SR)


def window_grow(rng: np.random.Generator, v: int) -> np.ndarray:
    """A low glassy swell: glass-ratio partials rising under a lowpass that opens as it goes."""
    n = int(0.90 * SR)
    t = secs(n, SR)
    glass = np.zeros(n)
    for ratio, amp in ((1.0, 1.0), (2.41, 0.45), (3.83, 0.28), (5.62, 0.16)):
        f = 165 * ratio * (1.0 + 0.35 * t / t[-1])
        glass += amp * np.sin(2 * np.pi * f * t)
    swell = np.sin(np.pi * np.clip(t / 0.80, 0, 1)) ** 1.6
    air = spectral(rng.standard_normal(n), SR, lambda f: 1.0 / np.sqrt(1.0 + (f / 2200.0) ** 4))
    return dc_block((glass * 0.30 + air * 0.20) * swell, SR)


def portal(rng: np.random.Generator, v: int) -> np.ndarray:
    """Band-passed noise swept 400 -> 2600 -> 500 Hz, then a tail from a big soft room."""
    n = int(0.85 * SR)
    t = secs(n, SR)
    src = noise_loop(rng, n)
    centre = 400 * np.exp(np.log(6.5) * np.sin(np.pi * t / t[-1]) ** 1.4)
    # A moving band, done as a weighted blend of four fixed bands: cheaper than a time-varying
    # filter and indistinguishable on noise.
    out = np.zeros(n)
    banks = [260.0, 600.0, 1200.0, 2000.0]
    for fc in banks:
        weight = np.exp(-((np.log(centre / fc)) ** 2) / (2 * 0.30**2))
        out += bandpass(src, SR, fc, 1.1) * weight
    out = lowpass(out, SR, 2600, 2) * np.sin(np.pi * np.clip(t / (0.72 * t[-1]), 0, 1)) ** 1.2
    tail = convolve(out * 0.5, room_ir(rng, SR, 0.55, 3000))[:n]
    return dc_block(out * 0.8 + tail * 0.6, SR)


def elevator_button(rng: np.random.Generator, v: int) -> np.ndarray:
    """A tactile click and a short 1.4 kHz beep behind it: a button that answers."""
    n = int(0.16 * SR)
    click = bandpass(rng.standard_normal(n), SR, 2400, 2.0) * env(n, SR, 0.0005, 0.005)
    beep = np.sin(2 * np.pi * 1400 * secs(n, SR)) * env(n, SR, 0.004, 0.018, hold=0.030)
    return dc_block(click * 0.55 + beep * 0.7, SR)


def elevator_ding(rng: np.random.Generator, v: int) -> np.ndarray:
    """Bell partials 1 : 2.76 : 5.40 : 8.93 on 660 Hz, each with its own decay."""
    n = int(0.90 * SR)
    bell = partials(
        n, SR, 660,
        ratios=[1.0, 2.76, 5.40, 8.93], amps=[1.0, 0.55, 0.28, 0.14],
        decays=[0.34, 0.22, 0.14, 0.09], inharm=0.003, rng=rng,
    )
    strike = highpass(rng.standard_normal(n), SR, 3000, 2) * env(n, SR, 0.0004, 0.003)
    hum = np.sin(2 * np.pi * 330 * secs(n, SR)) * np.exp(-secs(n, SR) / 0.45) * 0.25
    return dc_block(bell * 0.75 + strike * 0.25 + hum, SR)


def elevator_doors(rng: np.random.Generator, v: int) -> np.ndarray:
    """A rolling slide -- noise round 600 Hz with a rumble under it -- ending on a soft thunk."""
    n = int(0.90 * SR)
    t = secs(n, SR)
    roll = bandpass(noise_loop(rng, n), SR, 600, 1.0)
    rumble = lowpass(noise_loop(rng, n), SR, 120, 2)
    shape = np.sin(np.pi * np.clip(t / (0.78 * t[-1]), 0, 1)) ** 0.8
    out = (roll * 0.55 + rumble * 0.9) * shape
    m = int(0.16 * SR)
    thunk = np.sin(2 * np.pi * 105 * secs(m, SR)) * env(m, SR, 0.002, 0.035)
    thunk += lowpass(rng.standard_normal(m), SR, 800, 3) * env(m, SR, 0.002, 0.018) * 0.5
    place(out, thunk * 0.8, int(0.72 * SR))
    return dc_block(out, SR)


def elevator_ride(rng: np.random.Generator, v: int) -> np.ndarray:
    """Motor rumble: 58 Hz and its first harmonics, wobbling, fading in and out again."""
    n = int(0.90 * SR)
    t = secs(n, SR)
    motor = np.zeros(n)
    for k, amp in enumerate((1.0, 0.5, 0.28, 0.16, 0.09), start=1):
        motor += amp * np.sin(2 * np.pi * 58 * k * t + rng.uniform(0, 2 * np.pi))
    wobble = 1.0 + 0.10 * np.sin(2 * np.pi * 7.3 * t)
    hiss = lowpass(rng.standard_normal(n), SR, 700, 2) * 0.25
    shape = np.sin(np.pi * np.clip(t / t[-1], 0, 1)) ** 0.7
    return dc_block(saturate((motor * 0.35 * wobble + hiss) * shape, 1.4), SR)


def ui_move(rng: np.random.Generator, v: int) -> np.ndarray:
    """A 60 ms band-passed tick at 1.2 kHz: the smallest sound in the game."""
    n = int(0.09 * SR)
    tick = bandpass(rng.standard_normal(n), SR, 1200, 2.6) * env(n, SR, 0.0008, 0.008)
    tone = np.sin(2 * np.pi * 1200 * secs(n, SR)) * env(n, SR, 0.001, 0.010) * 0.4
    return dc_block(tick + tone, SR)


def ui_confirm(rng: np.random.Generator, v: int) -> np.ndarray:
    """Two blips, 660 then 990 Hz: a step forward."""
    n = int(0.20 * SR)
    out = np.zeros(n)
    for at, freq in ((0.00, 660.0), (0.055, 990.0)):
        m = int(0.10 * SR)
        blip = np.sin(2 * np.pi * freq * secs(m, SR)) * env(m, SR, 0.002, 0.022)
        blip += np.sin(2 * np.pi * 2 * freq * secs(m, SR)) * env(m, SR, 0.002, 0.012) * 0.25
        place(out, blip, int(at * SR))
    return dc_block(out, SR)


def ui_back(rng: np.random.Generator, v: int) -> np.ndarray:
    """The same two blips falling, 880 then 550 Hz: a step back."""
    n = int(0.20 * SR)
    out = np.zeros(n)
    for at, freq in ((0.00, 880.0), (0.055, 550.0)):
        m = int(0.10 * SR)
        blip = np.sin(2 * np.pi * freq * secs(m, SR)) * env(m, SR, 0.002, 0.022)
        blip += np.sin(2 * np.pi * 2 * freq * secs(m, SR)) * env(m, SR, 0.002, 0.012) * 0.20
        place(out, blip, int(at * SR))
    return dc_block(out, SR)


def door_open(rng: np.random.Generator, v: int) -> np.ndarray:
    """A wood thump, then a creak: a resonance wandering 210 -> 520 Hz over a rough envelope."""
    n = int(0.90 * SR)
    t = secs(n, SR)
    thump = np.zeros(n)
    m = int(0.18 * SR)
    thump[:m] = np.sin(2 * np.pi * 118 * secs(m, SR)) * env(m, SR, 0.002, 0.040)
    thump[:m] += lowpass(rng.standard_normal(m), SR, 900, 3) * env(m, SR, 0.002, 0.022) * 0.55
    src = noise_loop(rng, n)
    creak = np.zeros(n)
    centre = 210 * (520 / 210) ** np.clip((t - 0.15) / 0.55, 0, 1)
    for fc in (230.0, 340.0, 480.0):
        weight = np.exp(-((np.log(centre / fc)) ** 2) / (2 * 0.22**2))
        creak += bandpass(src, SR, fc, 5.0) * weight
    rough = 0.35 + 0.65 * np.abs(lowpass(rng.standard_normal(n), SR, 45.0, 2))
    rough /= rough.max()
    creak *= rough * np.clip(np.sin(np.pi * np.clip((t - 0.12) / 0.70, 0, 1)), 0, None) ** 1.1
    return dc_block(thump * 0.9 + creak * 0.55, SR)


def door_close(rng: np.random.Generator, v: int) -> np.ndarray:
    """The swing back as a short whoosh, then a latch clunk on a 120 Hz wooden body."""
    n = int(0.72 * SR)
    t = secs(n, SR)
    whoosh = lowpass(noise_loop(rng, n), SR, 900, 2)
    whoosh *= np.clip(np.sin(np.pi * np.clip(t / 0.44, 0, 1)), 0, None) ** 1.4
    out = whoosh * 0.30
    m = int(0.28 * SR)
    clunk = np.sin(2 * np.pi * 120 * secs(m, SR)) * env(m, SR, 0.0015, 0.050)
    clunk += np.sin(2 * np.pi * 197 * secs(m, SR)) * env(m, SR, 0.0015, 0.028) * 0.45
    clunk += lowpass(rng.standard_normal(m), SR, 1600, 3) * env(m, SR, 0.0015, 0.016) * 0.6
    place(out, saturate(clunk, 1.3), int(0.42 * SR))
    return dc_block(out, SR)


# ─────────────────────────────────────────────────────────────────────────────
# Loops. Every one is built to be exactly periodic in its own length -- sines snapped to a
# whole number of cycles, noise made from a random-phase spectrum, reverb applied circularly
# and events wrapped by `place` -- so the head IS the continuation of the tail and there is
# no seam to fade. `measure` proves it: the reported wrap discontinuity is the step between
# two adjacent samples, not the step across a cut.
# ─────────────────────────────────────────────────────────────────────────────


def loop_meadow(rng: np.random.Generator, n: int) -> np.ndarray:
    """Wind through a wandering lowpass, gusts on a slow envelope, two distant thunders."""
    sr = LOOP_SR
    air = tilt(noise_loop(rng, n), sr, -4.5)
    gust = np.abs(lowpass(noise_loop(rng, n), sr, 0.22, 2))
    gust = 0.25 + 0.75 * gust / gust.max()
    wind = np.zeros(n)
    for fc, w in ((320.0, 0.55), (900.0, 0.30), (2400.0, 0.12)):
        wind += lowpass(air, sr, fc, 2) * w
    wind *= gust
    out = wind * 0.9
    for k in range(2):
        m = int(4.5 * sr)
        boom = lowpass(noise_loop(rng, m), sr, 110, 3)
        boom *= np.sin(np.pi * np.linspace(0, 1, m)) ** 2.2
        place(out, boom * 0.55, int((7.0 + 21.0 * k) * sr))
    return out


def loop_backrooms(rng: np.random.Generator, n: int) -> np.ndarray:
    """The fluorescent 100 Hz line and its harmonics over dead air: the room is the hum."""
    sr = LOOP_SR
    t = secs(n, sr)
    hum = np.zeros(n)
    for mult, amp in ((1, 1.0), (2, 0.42), (3, 0.22), (4, 0.11), (6, 0.05)):
        hum += amp * np.sin(2 * np.pi * snap(100.0 * mult, n, sr) * t + rng.uniform(0, 2 * np.pi))
    # The ballast is never quite steady: a slow 0.13 Hz breath on the level, and a 50 Hz
    # subharmonic murmur under it.
    hum *= 1.0 + 0.06 * np.sin(2 * np.pi * snap(0.13, n, sr) * t)
    hum += 0.10 * np.sin(2 * np.pi * snap(50.0, n, sr) * t)
    dead = lowpass(noise_loop(rng, n), sr, 480, 2) * 0.22
    drift = 1.0 + 0.15 * lowpass(noise_loop(rng, n), sr, 0.09, 2) / 0.02
    return hum * 0.30 + dead * np.clip(drift, 0.4, 1.6)


def loop_pool(rng: np.random.Generator, n: int) -> np.ndarray:
    """Drips into a 3.4 s tiled reverb, applied circularly, over a faint water surface."""
    sr = LOOP_SR
    drips = np.zeros(n)
    at = 0.0
    while at < n / sr:
        m = int(0.09 * sr)
        f0 = rng.uniform(760, 1500)
        plink = sweep(m, sr, f0, f0 * rng.uniform(1.5, 2.2)) * env(m, sr, 0.0008, 0.014)
        plink += bandpass(rng.standard_normal(m), sr, f0 * 2.2, 2.0) * env(m, sr, 0.0006, 0.004) * 0.3
        place(drips, plink * rng.uniform(0.45, 1.0), int(at * sr))
        at += rng.uniform(0.7, 3.4)
    wet = circular_convolve(drips, room_ir(rng, sr, 3.4, 6000))
    surface = lowpass(noise_loop(rng, n), sr, 900, 2) * 0.10
    swash = 0.55 + 0.45 * np.abs(lowpass(noise_loop(rng, n), sr, 0.30, 2))
    swash /= swash.max()
    return wet * 1.0 + surface * swash


def loop_overgrown(rng: np.random.Generator, n: int) -> np.ndarray:
    """An insect band pulsing at 19 Hz, leaf rustles now and then, the fluorescent buzz further off."""
    sr = LOOP_SR
    t = secs(n, sr)
    insects = bandpass(noise_loop(rng, n), sr, 4800, 3.5)
    pulse = 0.5 + 0.5 * np.sin(2 * np.pi * snap(19.0, n, sr) * t)
    swarm = np.abs(lowpass(noise_loop(rng, n), sr, 0.12, 2))
    swarm = 0.15 + 0.85 * swarm / swarm.max()
    insects *= pulse**2 * swarm
    buzz = np.zeros(n)
    for mult, amp in ((1, 1.0), (2, 0.35), (3, 0.15)):
        buzz += amp * np.sin(2 * np.pi * snap(100.0 * mult, n, sr) * t + rng.uniform(0, 2 * np.pi))
    leaves = np.zeros(n)
    for k in range(9):
        m = int(rng.uniform(0.5, 1.4) * sr)
        rustle = bandpass(noise_loop(rng, m), sr, rng.uniform(2200, 4200), 1.0)
        rustle *= np.sin(np.pi * np.linspace(0, 1, m)) ** 1.5
        place(leaves, rustle * rng.uniform(0.25, 0.6), int(rng.uniform(0, n)))
    room = lowpass(noise_loop(rng, n), sr, 400, 2) * 0.18
    return insects * 0.55 + buzz * 0.10 + leaves * 0.30 + room


def loop_ambient(rng: np.random.Generator, n: int) -> np.ndarray:
    """Two 55 Hz sines a third of a hertz apart, beating slowly, under filtered air."""
    sr = LOOP_SR
    t = secs(n, sr)
    drone = np.sin(2 * np.pi * snap(55.0, n, sr) * t) + np.sin(2 * np.pi * snap(55.33, n, sr) * t)
    drone += 0.35 * np.sin(2 * np.pi * snap(110.0, n, sr) * t + 1.1)
    drone += 0.16 * np.sin(2 * np.pi * snap(164.5, n, sr) * t + 2.3)
    air = lowpass(noise_loop(rng, n), sr, 700, 2)
    breathe = 0.6 + 0.4 * np.sin(2 * np.pi * snap(0.07, n, sr) * t)
    return drone * 0.22 + air * 0.14 * breathe


# ─────────────────────────────────────────────────────────────────────────────
# Specifications: what each sound must measure once it has been encoded and decoded again.
# ─────────────────────────────────────────────────────────────────────────────


@dataclass
class Effect:
    stem: str
    build: Callable[[np.random.Generator, int], np.ndarray]
    variants: int = 1
    dur: tuple[float, float] = (0.08, 0.90)
    centroid: tuple[float, float] = (60.0, 12000.0)


@dataclass
class Loop:
    stem: str
    build: Callable[[np.random.Generator, int], np.ndarray]
    rms_db: tuple[float, float] = (-23.0, -18.0)
    peak_db: tuple[float, float] = (-18.0, -5.0)


EFFECTS: list[Effect] = [
    Effect("footstep_carpet", footstep_carpet, 4, (0.15, 0.25), (150, 700)),
    Effect("footstep_tile", footstep_tile, 4, (0.25, 0.40), (700, 2600)),
    Effect("footstep_water", footstep_water, 4, (0.25, 0.35), (700, 3200)),
    Effect("footstep_moss", footstep_moss, 4, (0.18, 0.30), (250, 700)),
    Effect("footstep_grass", footstep_grass, 4, (0.18, 0.28), (2000, 5000)),
    Effect("jump", jump, 1, (0.15, 0.25), (60, 400)),
    Effect("land_soft", land_soft, 1, (0.22, 0.34), (60, 400)),
    Effect("land_hard", land_hard, 1, (0.34, 0.46), (60, 400)),
    Effect("grab", grab, 1, (0.10, 0.20), (200, 900)),
    Effect("release", release, 1, (0.12, 0.22), (150, 900)),
    Effect("stow", stow, 1, (0.19, 0.30), (400, 1600)),
    Effect("retrieve", retrieve, 1, (0.17, 0.28), (350, 1600)),
    Effect("drop", drop, 1, (0.28, 0.40), (50, 350)),
    Effect("refuse", refuse, 1, (0.12, 0.22), (50, 350)),
    Effect("key_take", key_take, 1, (0.50, 0.62), (1500, 4000)),
    Effect("key_use", key_use, 1, (0.44, 0.56), (150, 1200)),
    Effect("window_grow", window_grow, 1, (0.84, 0.96), (200, 900)),
    Effect("portal", portal, 1, (0.80, 0.92), (600, 3000)),
    Effect("elevator_button", elevator_button, 1, (0.12, 0.22), (900, 2200)),
    Effect("elevator_ding", elevator_ding, 1, (0.84, 0.96), (450, 1400)),
    Effect("elevator_doors", elevator_doors, 1, (0.84, 0.96), (400, 1800)),
    Effect("elevator_ride", elevator_ride, 1, (0.84, 0.96), (50, 250)),
    Effect("ui_move", ui_move, 1, (0.08, 0.14), (900, 2000)),
    Effect("ui_confirm", ui_confirm, 1, (0.17, 0.26), (700, 1600)),
    Effect("ui_back", ui_back, 1, (0.17, 0.26), (350, 1000)),
    Effect("door_open", door_open, 1, (0.84, 0.96), (300, 900)),
    Effect("door_close", door_close, 1, (0.66, 0.80), (80, 400)),
]

LOOPS: list[Loop] = [
    Loop("16-meadow", loop_meadow),
    Loop("17-backrooms", loop_backrooms),
    Loop("18-pool", loop_pool),
    Loop("19-overgrown", loop_overgrown),
    Loop("ambient", loop_ambient),
]

SEAM_LIMIT = 0.02
"""The largest step across the wrap that still counts as no click: -34 dBFS."""


# ─────────────────────────────────────────────────────────────────────────────
# WAV, encoding, measurement.
# ─────────────────────────────────────────────────────────────────────────────


def to_int16(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    """Quantise with TPDF dither -- one LSB of triangular noise, so the truncation error is
    noise rather than a correlated distortion on the quiet tails."""
    dither = rng.random(len(x)) - rng.random(len(x))
    y = np.clip(x * 32767.0 + dither, -32768.0, 32767.0)
    return np.rint(y).astype("<i2")


def write_wav(path: Path, x: np.ndarray, sr: int, rng: np.random.Generator) -> None:
    """16-bit mono RIFF, written by hand: no dependency, and every byte is accounted for."""
    pcm = to_int16(x, rng).tobytes()
    header = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt "
    header += struct.pack("<IHHIIHH", 16, 1, 1, sr, sr * 2, 2, 16)
    header += b"data" + struct.pack("<I", len(pcm))
    path.write_bytes(header + pcm)


def read_wav(raw: bytes) -> tuple[np.ndarray, int]:
    """Read back a 16-bit mono RIFF (what ffmpeg hands us when it decodes an ogg)."""
    assert raw[:4] == b"RIFF" and raw[8:12] == b"WAVE", "not a RIFF/WAVE stream"
    pos, sr, data = 12, 0, b""
    while pos + 8 <= len(raw):
        cid, size = raw[pos : pos + 4], struct.unpack("<I", raw[pos + 4 : pos + 8])[0]
        body = raw[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            sr = struct.unpack("<I", body[4:8])[0]
        elif cid == b"data":
            data = body
        pos += 8 + size + (size & 1)
    x = np.frombuffer(data, dtype="<i2").astype(np.float64) / 32768.0
    return x, sr


def encode(wav: Path, flac: Path) -> None:
    """WAV -> FLAC. `-map_metadata -1 -fflags +bitexact` so no tool version or date lands in
    the file: two runs a month apart must produce the same bytes."""
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", str(wav), "-map_metadata", "-1",
         "-fflags", "+bitexact", "-c:a", "flac", "-compression_level", "12", "-ac", "1", str(flac)],
        check=True,
    )


def decode(path: Path) -> tuple[np.ndarray, int]:
    """Decode the shipped file back to samples: everything below is measured on THIS."""
    raw = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(path), "-f", "wav", "-c:a", "pcm_s16le", "-"],
        check=True, capture_output=True,
    ).stdout
    return read_wav(raw)


def db(x: float) -> float:
    return 20.0 * np.log10(max(x, 1e-12))


def centroid_hz(x: np.ndarray, sr: int) -> float:
    """Spectral centroid, weighted by POWER.

    Weighted by magnitude it is useless here: a dithered 16-bit file carries a -90 dBFS floor
    across the whole band, and 20 kHz of that floor drags the mean of a sound whose content
    stops at 900 Hz up past 2 kHz. Squaring puts the floor a further 90 dB down, and the
    number then says what the ear says.
    """
    power = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    freq = np.fft.rfftfreq(len(x), 1.0 / sr)
    total = power.sum()
    return float((power * freq).sum() / total) if total > 0 else 0.0


def measure(x: np.ndarray, sr: int) -> dict:
    """Duration, peak, RMS, DC, brightness and the wrap discontinuity, of the decoded file."""
    step = np.abs(np.diff(x))
    return {
        "secs": len(x) / sr,
        "peak_db": db(float(np.max(np.abs(x)))),
        "rms_db": db(float(np.sqrt(np.mean(x * x)))),
        "dc": float(np.mean(x)),
        "centroid": centroid_hz(x, sr),
        "seam": float(abs(x[0] - x[-1])),
        # The 99th percentile of the step between adjacent samples: what the wrap has to
        # beat. A wrap smaller than the steps the signal takes anyway is not a wrap at all.
        "step_p99": float(np.percentile(step, 99.0)) if len(step) else 0.0,
    }


# ─────────────────────────────────────────────────────────────────────────────
# Contact sheets.
# ─────────────────────────────────────────────────────────────────────────────

RAMP = np.array([(8, 10, 24), (38, 24, 96), (150, 46, 92), (232, 138, 48), (255, 244, 214)], dtype=float)


def colourise(norm: np.ndarray) -> np.ndarray:
    """Five-stop ramp, dark to hot: enough separation to read a spectrogram by eye."""
    pos = np.clip(norm, 0, 1) * (len(RAMP) - 1)
    lo = np.clip(pos.astype(int), 0, len(RAMP) - 2)
    frac = (pos - lo)[..., None]
    return (RAMP[lo] * (1 - frac) + RAMP[lo + 1] * frac).astype(np.uint8)


def spectrogram(x: np.ndarray, sr: int, width: int, height: int, fmin: float = 40.0) -> np.ndarray:
    """Log-frequency, log-magnitude STFT, resampled to exactly `width` x `height` pixels."""
    win = max(256, 1 << int(np.log2(max(len(x) / width, 64))))
    hop = max(1, (len(x) - win) // max(width - 1, 1)) if len(x) > win else 1
    frames = max(1, 1 + (max(len(x) - win, 0)) // hop)
    window = np.hanning(win)
    padded = np.concatenate([x, np.zeros(win)])
    cols = np.empty((win // 2 + 1, frames))
    for i in range(frames):
        cols[:, i] = np.abs(np.fft.rfft(padded[i * hop : i * hop + win] * window))
    freq = np.fft.rfftfreq(win, 1.0 / sr)
    edges = np.geomspace(fmin, sr / 2, height + 1)
    rows = np.empty((height, frames))
    for b in range(height):
        sel = (freq >= edges[b]) & (freq < edges[b + 1])
        rows[b] = cols[sel].mean(axis=0) if sel.any() else cols[np.argmin(np.abs(freq - edges[b]))]
    mag = 20 * np.log10(rows + 1e-9)
    mag = np.clip((mag - (mag.max() - 72.0)) / 72.0, 0, 1)
    idx = np.clip((np.arange(width) * frames / width).astype(int), 0, frames - 1)
    return colourise(mag[::-1][:, idx])


def waveform(x: np.ndarray, width: int, height: int, colour=(120, 220, 200)) -> np.ndarray:
    """Min/max envelope per pixel column, on the same dark ground as the spectrogram."""
    img = np.full((height, width, 3), (10, 12, 20), dtype=np.uint8)
    edges = np.linspace(0, len(x), width + 1).astype(int)
    mid = height // 2
    img[mid] = (44, 48, 64)
    for i in range(width):
        seg = x[edges[i] : max(edges[i + 1], edges[i] + 1)]
        if not len(seg):
            continue
        top = int(mid - np.max(seg) * (mid - 1))
        bot = int(mid - np.min(seg) * (mid - 1))
        img[np.clip(top, 0, height - 1) : np.clip(bot, 0, height - 1) + 1, i] = colour
    return img


def contact_sheet(items: list[tuple[str, np.ndarray, int, dict]], cols: int, cell: tuple[int, int], path: Path,
                  title: str, seam: bool = False) -> None:
    """One cell per sound: name and numbers, the waveform, then the log-spectrogram."""
    cw, ch = cell
    pad, head = 6, 26
    rows = (len(items) + cols - 1) // cols
    img = Image.new("RGB", (cols * cw + pad, rows * ch + pad + head), (18, 18, 24))
    draw = ImageDraw.Draw(img)
    font = ImageFont.load_default()
    draw.text((pad, 6), title, fill=(230, 230, 240), font=font)
    for k, (name, x, sr, m) in enumerate(items):
        ox, oy = pad + (k % cols) * cw, head + pad + (k // cols) * ch
        inner_w = cw - pad
        wave_h, spec_h = int((ch - pad - 22) * 0.34), int((ch - pad - 22) * 0.66)
        seam_w = inner_w // 5 if seam else 0
        main_w = inner_w - seam_w - (4 if seam else 0)
        draw.text((ox, oy), name, fill=(215, 220, 235), font=font)
        draw.text(
            (ox, oy + 10),
            f"{m['secs']:.2f}s pk{m['peak_db']:+.1f} rms{m['rms_db']:+.1f} dc{m['dc']:+.0e} c{m['centroid']:.0f}Hz",
            fill=(140, 150, 175), font=font,
        )
        img.paste(Image.fromarray(waveform(x, main_w, wave_h)), (ox, oy + 22))
        img.paste(Image.fromarray(spectrogram(x, sr, main_w, spec_h)), (ox, oy + 22 + wave_h))
        if seam:
            # The wrap, magnified: the last 15 ms joined to the first 15 ms. A click would be
            # a vertical step in the middle of this strip.
            k15 = int(0.015 * sr)
            joined = np.concatenate([x[-k15:], x[:k15]])
            strip = waveform(joined, seam_w, wave_h + spec_h, colour=(240, 190, 110))
            strip[:, seam_w // 2] = (255, 90, 90)
            img.paste(Image.fromarray(strip), (ox + main_w + 4, oy + 22))
            draw.text((ox + main_w + 4, oy + 10), "wrap", fill=(240, 190, 110), font=font)
    img.save(path)


# ─────────────────────────────────────────────────────────────────────────────


def build(args: argparse.Namespace) -> int:
    """Everything is built, encoded and measured in a temp directory; `assets/` is only
    written once EVERY sound has passed its band, so a failed run leaves the shipped set
    exactly as it was rather than half replaced."""
    args.plots.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="gen_sfx."))
    rows: list[str] = []
    faults: list[str] = []
    fx_plots: list[tuple[str, np.ndarray, int, dict]] = []
    loop_plots: list[tuple[str, np.ndarray, int, dict]] = []
    staged: list[tuple[Path, Path]] = []

    try:
        for spec in EFFECTS:
            for v in range(1, spec.variants + 1):
                stem = spec.stem if spec.variants == 1 else f"{spec.stem}_{v}"
                if args.only and args.only not in stem:
                    continue
                x = normalise(fade(spec.build(rng_for(spec.stem, v), v), SR))
                wav, out = tmp / f"{stem}.wav", tmp / f"{stem}.flac"
                write_wav(wav, x, SR, rng_for(stem, 1000))
                encode(wav, out)
                y, sr = decode(out)
                m = measure(y, sr)
                faults += check(stem, m, dur=spec.dur, centroid=spec.centroid, peak_db=(-4.5, -1.0))
                rows.append(report_row(f"sfx/{stem}.flac", out, m))
                fx_plots.append((stem, y, sr, m))
                staged.append((out, args.assets / "sfx" / out.name))

        for spec in LOOPS:
            if args.only and args.only not in spec.stem:
                continue
            n = int(LOOP_SECS * LOOP_SR)
            x = level_loop(spec.build(rng_for(spec.stem), n))
            wav, out = tmp / f"{spec.stem}.wav", tmp / f"{spec.stem}.flac"
            write_wav(wav, x, LOOP_SR, rng_for(spec.stem, 1000))
            encode(wav, out)
            y, sr = decode(out)
            m = measure(y, sr)
            faults += check(spec.stem, m, dur=(LOOP_SECS - 0.05, LOOP_SECS + 0.05),
                            centroid=(20.0, 12000.0), peak_db=spec.peak_db, rms_db=spec.rms_db,
                            seam=SEAM_LIMIT)
            rows.append(report_row(f"music/{spec.stem}.flac", out, m))
            loop_plots.append((spec.stem, y, sr, m))
            staged.append((out, args.assets / "music" / out.name))

        if fx_plots:
            contact_sheet(fx_plots, 6, (272, 190), args.plots / "effects.png",
                          "DayDreams effects -- waveform and log spectrogram")
        if loop_plots:
            contact_sheet(loop_plots, 1, (1180, 210), args.plots / "loops.png",
                          "DayDreams ambience loops -- waveform, spectrogram, and the wrap magnified",
                          seam=True)

        print(f"{'file':30} {'bytes':>8} {'secs':>7} {'peak':>7} {'rms':>7} {'dc':>10} "
              f"{'centroid':>9} {'wrap':>8} {'p99step':>8}")
        for row in rows:
            print(row)
        if faults:
            print("\n" + "\n".join(faults), file=sys.stderr)
            print(f"{len(faults)} measurement(s) out of band; assets/ left untouched", file=sys.stderr)
            return 1

        for src, dst in staged:
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dst)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    total = sum(p.stat().st_size for p in args.assets.rglob("*") if p.is_file())
    print(f"\n{len(staged)} file(s) written; assets/ totals {total / 1e6:.2f} MB")
    return 0


def report_row(name: str, path: Path, m: dict) -> str:
    return (f"{name:30} {path.stat().st_size:8d} {m['secs']:7.3f} {m['peak_db']:+7.2f} "
            f"{m['rms_db']:+7.2f} {m['dc']:+10.2e} {m['centroid']:9.0f} {db(m['seam']):+8.1f} "
            f"{db(m['step_p99']):+8.1f}")


def check(stem: str, m: dict, dur: tuple[float, float], centroid: tuple[float, float],
          peak_db: tuple[float, float], rms_db: tuple[float, float] | None = None,
          seam: float | None = None) -> list[str]:
    """The gate. A sound that measures outside its band never reaches `assets/`; every fault
    in the run is collected rather than only the first, so one pass shows all the damage."""
    faults: list[str] = []

    def band(what: str, value: float, lo: float, hi: float) -> None:
        if not lo <= value <= hi:
            faults.append(f"{stem}: {what} {value:.4g} outside [{lo:.4g}, {hi:.4g}]")

    band("duration", m["secs"], *dur)
    band("peak dBFS", m["peak_db"], *peak_db)
    band("centroid Hz", m["centroid"], *centroid)
    band("DC offset", abs(m["dc"]), 0.0, 2e-3)
    band("headroom dBFS", m["peak_db"], -60.0, -0.01)  # anything at 0 dBFS has clipped
    if rms_db is not None:
        band("RMS dBFS", m["rms_db"], *rms_db)
    if seam is not None:
        # Either the wrap is inaudibly small outright, or it is no bigger than the steps the
        # signal takes between adjacent samples everywhere else -- in which case there is
        # nothing at the wrap to hear.
        band("wrap step", m["seam"], 0.0, max(seam, m["step_p99"]))
    return faults


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--assets", type=Path, default=REPO / "assets", help="asset root to write into")
    ap.add_argument("--plots", type=Path, default=REPO / "target" / "sfx-plots", help="contact sheets go here")
    ap.add_argument("--only", help="build only the sounds whose stem contains this")
    args = ap.parse_args(argv)
    if shutil.which("ffmpeg") is None:
        print("gen_sfx: ffmpeg is not on PATH; it does the FLAC encoding", file=sys.stderr)
        return 1
    return build(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
