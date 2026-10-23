//! Échantillonneur orchestral : rend la partition avec les vrais échantillons
//! VSCO-2 CE (CC0). Couches de vélocité, articulations tenues / courtes,
//! alternance (round robin), boucle de tenue, relâchement, dynamique CC1,
//! placement stéréo de l'orchestre et réverbération de salle (FDN).

use crate::arrange::{Score, PALETTE};
use crate::corpus::N;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SR: f32 = 44100.;

pub struct Sample {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
    pub sr: f32,
    pub key: u8,
    pub layer: usize,
}

pub struct Zone {
    pub samples: Vec<Sample>,
    pub layers: usize,
}

pub struct Instrument {
    pub sus: Option<Zone>,
    pub short: Option<Zone>,
    /// échantillons joués entièrement (harpe, percussions)
    pub oneshot: bool,
    pub release: f32,
    pub gain: f32,
    pub pan: f32,
}

// ---------------------------------------------------------------- WAV
fn read_wav(p: &Path) -> Option<(Vec<f32>, Vec<f32>, f32)> {
    let d = std::fs::read(p).ok()?;
    if d.len() < 44 || &d[0..4] != b"RIFF" {
        return None;
    }
    let mut i = 12;
    let (mut fmt, mut ch, mut sr, mut bits) = (1u16, 2u16, 44100u32, 16u16);
    while i + 8 <= d.len() {
        let id = &d[i..i + 4];
        let len = u32::from_le_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        let body = &d[i + 8..(i + 8 + len).min(d.len())];
        if id == b"fmt " {
            fmt = u16::from_le_bytes([body[0], body[1]]);
            ch = u16::from_le_bytes([body[2], body[3]]);
            sr = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            bits = u16::from_le_bytes([body[14], body[15]]);
            if fmt == 0xfffe && body.len() >= 26 {
                fmt = u16::from_le_bytes([body[24], body[25]]);
            }
        } else if id == b"data" {
            let bps = (bits / 8) as usize;
            let frame = bps * ch as usize;
            let n = body.len() / frame;
            let mut l = Vec::with_capacity(n);
            let mut r = Vec::with_capacity(n);
            for f in 0..n {
                let mut get = |c: usize| -> f32 {
                    let o = f * frame + c * bps;
                    match (fmt, bits) {
                        (1, 16) => i16::from_le_bytes([body[o], body[o + 1]]) as f32 / 32768.,
                        (1, 24) => ((((body[o] as i32) << 8) | ((body[o + 1] as i32) << 16) | ((body[o + 2] as i32) << 24)) >> 8) as f32 / 8388608.,
                        (1, 32) => i32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]) as f32 / 2147483648.,
                        (3, 32) => f32::from_le_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]),
                        _ => 0.,
                    }
                };
                let a = get(0);
                let b = if ch > 1 { get(1) } else { a };
                l.push(a);
                r.push(b);
            }
            return Some((l, r, sr as f32));
        }
        i += 8 + len + (len & 1);
    }
    None
}

// ---------------------------------------------------------------- noms et hauteurs
/// Note du nom de fichier : (classe+octave) en convention scientifique C4 = 60.
fn note_in_name(name: &str) -> Option<i32> {
    let b = name.as_bytes();
    for i in 0..b.len() {
        let c = b[i];
        if !(b'A'..=b'G').contains(&c) {
            continue;
        }
        if i > 0 && (b[i - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let mut j = i + 1;
        let mut acc = 0;
        if j < b.len() && b[j] == b'#' {
            acc = 1;
            j += 1;
        } else if j < b.len() && b[j] == b'b' {
            acc = -1;
            j += 1;
        }
        let neg = j < b.len() && b[j] == b'-';
        if neg {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() && (j + 1 >= b.len() || !b[j + 1].is_ascii_digit()) {
            let oct = (b[j] - b'0') as i32 * if neg { -1 } else { 1 };
            let pc = [9, 11, 0, 2, 4, 5, 7][(c - b'A') as usize];
            return Some((oct + 1) * 12 + pc + acc);
        }
    }
    None
}

fn layer_in_name(name: &str) -> usize {
    let n = name.to_lowercase();
    for (i, w) in ["ppp", "pp", "mp", "mf", "fff", "ff"].iter().enumerate() {
        if n.contains(&format!("_{}_", w)) || n.contains(&format!("_{}.", w)) {
            return [0, 1, 3, 4, 7, 6][i];
        }
    }
    if n.contains("_p_") {
        return 2;
    }
    if n.contains("_f_") {
        return 5;
    }
    let b = n.as_bytes();
    for i in 0..b.len().saturating_sub(2) {
        if b[i] == b'_' && b[i + 1] == b'v' && b[i + 2].is_ascii_digit() {
            return (b[i + 2] - b'0') as usize;
        }
    }
    0
}

/// Fréquence fondamentale (YIN simplifié) → note MIDI.
fn detect_pitch(x: &[f32], sr: f32) -> Option<f32> {
    let start = (0.15 * sr) as usize;
    let w = 4096usize;
    if x.len() < start + w + 2000 {
        return None;
    }
    let s = &x[start..start + w + 2000];
    let (min_lag, max_lag) = ((sr / 2000.) as usize, ((sr / 30.) as usize).min(1990));
    let mut d = vec![0f32; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut acc = 0.;
        for i in 0..w {
            let v = s[i] - s[i + lag];
            acc += v * v;
        }
        d[lag] = acc;
    }
    let mut cum = 0.;
    let mut best = None;
    let mut dn = vec![1f32; max_lag + 1];
    for lag in 1..=max_lag {
        cum += d[lag];
        if lag >= min_lag && cum > 0. {
            dn[lag] = d[lag] * lag as f32 / cum;
        }
    }
    for lag in min_lag + 1..max_lag {
        if dn[lag] < 0.15 && dn[lag] <= dn[lag - 1] && dn[lag] <= dn[lag + 1] {
            best = Some(lag);
            break;
        }
    }
    let lag = best.or_else(|| (min_lag..max_lag).min_by(|&a, &b| dn[a].partial_cmp(&dn[b]).unwrap()))?;
    let f0 = sr / lag as f32;
    Some(69. + 12. * (f0 / 440.).log2())
}

fn load_zone(dirs: &[PathBuf], pitched: bool, only: &str) -> Option<Zone> {
    let mut files: Vec<PathBuf> = vec![];
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                let nm = p.file_name().unwrap().to_string_lossy().to_lowercase();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) && nm.contains(only) {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    if files.is_empty() {
        return None;
    }
    let mut raw: Vec<(Vec<f32>, Vec<f32>, f32, Option<i32>, Option<f32>, usize)> = vec![];
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let Some((l, r, sr)) = read_wav(f) else { continue };
        let named = note_in_name(&name);
        let det = if pitched { detect_pitch(&l.iter().zip(&r).map(|(a, b)| a + b).collect::<Vec<_>>(), sr) } else { None };
        raw.push((l, r, sr, named, det, layer_in_name(&name)));
    }
    // convention d'octave du dossier : celle qui s'accorde le mieux à la détection
    let mut best_off = 0;
    if pitched {
        let mut score = HashMap::new();
        for x in &raw {
            if let (Some(nm), Some(dt)) = (x.3, x.4) {
                for off in [-24, -12, 0, 12, 24] {
                    if ((nm + off) as f32 - dt).abs() < 0.6 {
                        *score.entry(off).or_insert(0) += 1;
                    }
                }
            }
        }
        best_off = score.into_iter().max_by_key(|x| x.1).map(|x| x.0).unwrap_or(0);
    }
    let mut layers: Vec<usize> = raw.iter().map(|x| x.5).collect();
    layers.sort();
    layers.dedup();
    let samples: Vec<Sample> = raw
        .into_iter()
        .map(|(l, r, sr, nm, dt, ly)| {
            let key = match (nm, dt) {
                (Some(n), _) => (n + best_off).clamp(0, 127) as u8,
                (None, Some(d)) => d.round().clamp(0., 127.) as u8,
                _ => 60,
            };
            Sample { l, r, sr, key, layer: layers.iter().position(|&x| x == ly).unwrap_or(0) }
        })
        .collect();
    Some(Zone { samples, layers: layers.len().max(1) })
}

// ---------------------------------------------------------------- orchestre
struct Def {
    desk: &'static str,
    sus: &'static [&'static str],
    short: &'static [&'static str],
    oneshot: bool,
    release: f32,
    gain: f32,
    pan: f32,
}

const DEFS: [Def; 21] = [
    Def { desk: "Piccolo", sus: &["Woodwinds/Piccolo/Sus"], short: &["Woodwinds/Piccolo/Stac"], oneshot: false, release: 0.2, gain: 0.55, pan: -0.15 },
    Def { desk: "Flutes", sus: &["Woodwinds/Flute/susvib"], short: &["Woodwinds/Flute/stac"], oneshot: false, release: 0.25, gain: 0.7, pan: -0.1 },
    Def { desk: "Oboes", sus: &["Woodwinds/Oboe/Vib"], short: &["Woodwinds/Oboe/Stacc"], oneshot: false, release: 0.2, gain: 0.6, pan: 0.1 },
    Def { desk: "Clarinets", sus: &["Woodwinds/Clarinet/susLong"], short: &["Woodwinds/Clarinet/stac"], oneshot: false, release: 0.2, gain: 0.65, pan: -0.05 },
    Def { desk: "Bassoons", sus: &["Woodwinds/Bassoon/sus"], short: &["Woodwinds/Bassoon/stac"], oneshot: false, release: 0.2, gain: 0.65, pan: 0.12 },
    Def { desk: "Horns", sus: &["Brass/F Horn/sus"], short: &["Brass/F Horn/stac"], oneshot: false, release: 0.35, gain: 1.0, pan: -0.35 },
    Def { desk: "Trumpets", sus: &["Brass/Trumpet/sus"], short: &["Brass/Trumpet/stac"], oneshot: false, release: 0.25, gain: 0.75, pan: 0.15 },
    Def { desk: "Trombones", sus: &["Brass/Tenor Trombone/sus"], short: &["Brass/Tenor Trombone/stac"], oneshot: false, release: 0.3, gain: 0.8, pan: 0.35 },
    Def { desk: "Tuba", sus: &["Brass/Tuba/sus"], short: &["Brass/Tuba/stac"], oneshot: false, release: 0.3, gain: 0.8, pan: 0.45 },
    Def { desk: "Timpani", sus: &["Percussion/Timpani"], short: &[], oneshot: true, release: 1.5, gain: 0.8, pan: 0.0 },
    Def { desk: "Bass Drum", sus: &["VSCO 1 Percussion/drums/bass"], short: &[], oneshot: true, release: 2.0, gain: 0.9, pan: 0.1 },
    Def { desk: "Piatti", sus: &["VSCO 1 Percussion/varMetal/Cymbals/susp"], short: &[], oneshot: true, release: 3.0, gain: 0.5, pan: 0.15 },
    Def { desk: "Harp", sus: &["Strings/Harp"], short: &[], oneshot: true, release: 2.0, gain: 0.8, pan: -0.7 },
    Def { desk: "Glockenspiel", sus: &["Percussion/Glock"], short: &[], oneshot: true, release: 2.0, gain: 0.35, pan: 0.25 },
    Def { desk: "Xylophone", sus: &["Percussion/Xylo"], short: &[], oneshot: true, release: 1.0, gain: 0.4, pan: 0.3 },
    Def { desk: "Marimba", sus: &["Percussion/Marimba"], short: &[], oneshot: true, release: 1.5, gain: 0.5, pan: 0.3 },
    Def { desk: "Violins 1", sus: &["Strings/Violin Section/susVib"], short: &["Strings/Violin Section/Spic"], oneshot: false, release: 0.35, gain: 1.0, pan: -0.6 },
    Def { desk: "Violins 2", sus: &["Strings/Violin Section/susVib"], short: &["Strings/Violin Section/Spic"], oneshot: false, release: 0.35, gain: 0.9, pan: -0.3 },
    Def { desk: "Violas", sus: &["Strings/Viola Section/susvib"], short: &["Strings/Viola Section/spic"], oneshot: false, release: 0.35, gain: 0.9, pan: 0.25 },
    Def { desk: "Cellos", sus: &["Strings/Cello Section/susvib"], short: &["Strings/Cello Section/spic"], oneshot: false, release: 0.35, gain: 1.0, pan: 0.45 },
    Def { desk: "Basses", sus: &["Strings/Solo Contrabass/SusVib"], short: &["Strings/Solo Contrabass/Spic"], oneshot: false, release: 0.35, gain: 1.1, pan: 0.6 },
];

pub fn load_orchestra(root: &Path) -> HashMap<&'static str, Instrument> {
    let mut out = HashMap::new();
    for d in DEFS.iter() {
        let pitched = !matches!(d.desk, "Bass Drum" | "Piatti");
        let dirs = |v: &[&str]| v.iter().map(|x| root.join(x)).collect::<Vec<_>>();
        let only = if d.desk == "Piatti" { "hit" } else { "" };
        let sus = load_zone(&dirs(d.sus), pitched, only);
        let short = if d.short.is_empty() { None } else { load_zone(&dirs(d.short), pitched, "") };
        if sus.is_none() && short.is_none() {
            continue;
        }
        // timbales : hauteurs détectées ramenées dans la tessiture réelle (ré2–la3)
        let sus = sus.map(|mut z| {
            if d.desk == "Timpani" {
                for x in z.samples.iter_mut() {
                    while x.key < 38 {
                        x.key += 12;
                    }
                    while x.key > 57 {
                        x.key -= 12;
                    }
                }
            }
            z
        });
        // normalisation par instrument : pic médian de la couche la plus forte
        let mut peaks: Vec<f32> = vec![];
        for z in [&sus, &short].into_iter().flatten() {
            for s in z.samples.iter().filter(|s| s.layer + 1 == z.layers) {
                peaks.push(s.l.iter().chain(&s.r).fold(0f32, |m, x| m.max(x.abs())));
            }
        }
        peaks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let norm = if peaks.is_empty() { 1. } else { 0.5 / peaks[peaks.len() / 2].max(1e-4) };
        out.insert(d.desk, Instrument { sus, short, oneshot: d.oneshot, release: d.release, gain: d.gain * norm, pan: d.pan });
    }
    out
}

// ---------------------------------------------------------------- rendu
fn beat_to_sec(tempos: &[(f32, f32)], b: f32) -> f32 {
    let mut s = 0.;
    for (i, &(t, bpm)) in tempos.iter().enumerate() {
        let end = tempos.get(i + 1).map(|x| x.0).unwrap_or(f32::MAX);
        if b <= t {
            break;
        }
        s += (b.min(end) - t) * 60. / bpm;
    }
    s
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % 10000) as f32 / 10000.
    }
}

fn pick<'a>(z: &'a Zone, key: u8, vel01: f32, rng: &mut Rng) -> Option<&'a Sample> {
    let best_d = z.samples.iter().map(|s| (s.key as i32 - key as i32).abs()).min()?;
    let near: Vec<&Sample> = z.samples.iter().filter(|s| (s.key as i32 - key as i32).abs() == best_d).collect();
    let mut ls: Vec<usize> = near.iter().map(|s| s.layer).collect();
    ls.sort();
    ls.dedup();
    let want = ((vel01 * ls.len() as f32).floor() as usize).min(ls.len() - 1);
    let layer = ls[want];
    let c: Vec<&&Sample> = near.iter().filter(|s| s.layer == layer).collect();
    let i = ((rng.next() * c.len() as f32) as usize).min(c.len() - 1);
    Some(c[i])
}

/// Rendu complet → (gauche, droite) à 44,1 kHz.
pub fn render(sc: &Score, orch: &HashMap<&'static str, Instrument>) -> (Vec<f32>, Vec<f32>) {
    let end_b = sc.parts.iter().flatten().map(|n| n.t + n.l).fold(0f32, f32::max);
    let total = ((beat_to_sec(&sc.tempos, end_b) + 6.) * SR) as usize;
    let (mut ol, mut or) = (vec![0f32; total], vec![0f32; total]);
    let mut rng = Rng(0x9e3779b97f4a7c15);
    for (di, dk) in PALETTE.iter().enumerate() {
        let Some(ins) = orch.get(dk.name) else { continue };
        let cc = &sc.cc[di];
        let dyn_at = |b: f32| -> f32 {
            let i = cc.partition_point(|x| x.0 <= b);
            let v = if i == 0 { cc.first().map(|x| x.1).unwrap_or(90.) } else { cc[i - 1].1 };
            0.3 + 0.7 * (v / 127.).powf(1.4)
        };
        let (gl, gr) = {
            let a = (ins.pan + 1.) * std::f32::consts::FRAC_PI_4;
            (a.cos() * ins.gain, a.sin() * ins.gain)
        };
        for n in &sc.parts[di] {
            let note: &N = n;
            let jitter = if ins.oneshot { 0. } else { (rng.next() - 0.5) * 0.012 };
            let t0 = beat_to_sec(&sc.tempos, note.t) + jitter + 0.01;
            let t1 = beat_to_sec(&sc.tempos, note.t + note.l);
            let dur = (t1 - t0).max(0.03);
            let dynv = if ins.oneshot { 1. } else { dyn_at(note.t) };
            let vel01 = (note.v as f32 / 127. * (0.6 + 0.4 * dynv)).clamp(0., 0.999);
            let short = dur < 0.28 && ins.short.is_some() && !ins.oneshot;
            let zone = if short { ins.short.as_ref() } else { ins.sus.as_ref().or(ins.short.as_ref()) };
            let Some(z) = zone else { continue };
            let Some(s) = pick(z, note.k, vel01, &mut rng) else { continue };
            let ratio = 2f32.powf((note.k as f32 - s.key as f32) / 12.) * s.sr / SR;
            let amp = (0.25 + 0.75 * (note.v as f32 / 127.).powf(1.5)) * if ins.oneshot { 1. } else { dynv };
            let play = if ins.oneshot || short { s.l.len() as f32 / ratio / SR } else { dur + ins.release };
            let n_out = (play.min(12.) * SR) as usize;
            let start = (t0 * SR) as usize;
            let rel_start = if ins.oneshot || short { usize::MAX } else { (dur * SR) as usize };
            let rel_len = (ins.release * SR) as usize;
            // boucle de tenue si l'échantillon est trop court
            let slen = s.l.len() as f32;
            let (lp0, lp1) = (slen * 0.45, slen * 0.9);
            for j in 0..n_out {
                let o = start + j;
                if o >= total {
                    break;
                }
                let mut pos = j as f32 * ratio;
                if pos >= slen - 2. {
                    if ins.oneshot || short || lp1 - lp0 < 2000. {
                        break;
                    }
                    pos = lp0 + (pos - lp0) % (lp1 - lp0);
                }
                let i = pos as usize;
                let fr = pos - i as f32;
                let (a, b) = (s.l[i] * (1. - fr) + s.l[(i + 1).min(s.l.len() - 1)] * fr, s.r[i] * (1. - fr) + s.r[(i + 1).min(s.r.len() - 1)] * fr);
                let mut env = amp;
                if j < 220 && !ins.oneshot {
                    env *= j as f32 / 220.;
                }
                if j >= rel_start {
                    let k = (j - rel_start) as f32 / rel_len.max(1) as f32;
                    if k >= 1. {
                        break;
                    }
                    env *= (1. - k) * (1. - k);
                }
                ol[o] += a * env * gl;
                or[o] += b * env * gr;
            }
        }
    }
    reverb(&mut ol, &mut or);
    (ol, or)
}

/// Réverbération de salle : réseau de retards à rétroaction (8 lignes,
/// matrice de Householder, amortissement des aigus), pré-délai 25 ms.
fn reverb(l: &mut [f32], r: &mut [f32]) {
    let lens = [1557usize, 1617, 1491, 1422, 1277, 1356, 1188, 1116].map(|x| x * 2);
    let mut lines: Vec<Vec<f32>> = lens.iter().map(|&n| vec![0.; n]).collect();
    let mut idx = [0usize; 8];
    let mut lp = [0f32; 8];
    let rt60 = 2.3f32;
    let g: Vec<f32> = lens.iter().map(|&n| 10f32.powf(-3. * n as f32 / SR / rt60)).collect();
    let pre = (0.025 * SR) as usize;
    let damp = 0.35;
    let wet = 0.28;
    let n = l.len();
    let mut out_l = vec![0f32; n];
    let mut out_r = vec![0f32; n];
    for t in 0..n {
        let x = if t >= pre { (l[t - pre] + r[t - pre]) * 0.5 } else { 0. };
        let mut y = [0f32; 8];
        for i in 0..8 {
            y[i] = lines[i][idx[i]];
        }
        let s: f32 = y.iter().sum::<f32>() * 0.25;
        for i in 0..8 {
            let fb = (y[i] - s) * g[i];
            lp[i] = lp[i] + (1. - damp) * (fb - lp[i]);
            lines[i][idx[i]] = x + lp[i];
            idx[i] = (idx[i] + 1) % lens[i];
        }
        out_l[t] = (y[0] + y[2] + y[4] + y[6]) * 0.35;
        out_r[t] = (y[1] + y[3] + y[5] + y[7]) * 0.35;
    }
    for t in 0..n {
        l[t] = l[t] * (1. - wet * 0.5) + out_l[t] * wet;
        r[t] = r[t] * (1. - wet * 0.5) + out_r[t] * wet;
    }
}

pub fn write_wav(path: &Path, l: &[f32], r: &[f32]) {
    let peak = l.iter().chain(r).fold(0f32, |m, x| m.max(x.abs())).max(1e-6);
    let g = 0.9 / peak;
    let n = l.len();
    let mut d = Vec::with_capacity(44 + n * 8);
    d.extend(b"RIFF");
    d.extend(((36 + n * 8) as u32).to_le_bytes());
    d.extend(b"WAVEfmt ");
    d.extend(16u32.to_le_bytes());
    d.extend(3u16.to_le_bytes());
    d.extend(2u16.to_le_bytes());
    d.extend((SR as u32).to_le_bytes());
    d.extend((SR as u32 * 8).to_le_bytes());
    d.extend(8u16.to_le_bytes());
    d.extend(32u16.to_le_bytes());
    d.extend(b"data");
    d.extend(((n * 8) as u32).to_le_bytes());
    for i in 0..n {
        d.extend((l[i] * g).to_le_bytes());
        d.extend((r[i] * g).to_le_bytes());
    }
    std::fs::write(path, d).unwrap();
}

/// Diagnostic : hauteurs et couches chargées par instrument.
pub fn describe(orch: &HashMap<&'static str, Instrument>) -> String {
    let mut o = String::new();
    for d in DEFS.iter() {
        if let Some(i) = orch.get(d.desk) {
            for (nm, z) in [("tenu", &i.sus), ("court", &i.short)] {
                if let Some(z) = z {
                    let mut ks: Vec<u8> = z.samples.iter().map(|s| s.key).collect();
                    ks.sort();
                    ks.dedup();
                    o += &format!("{:<13} {:<5} {} éch., {} couches, notes {}–{}\n", d.desk, nm, z.samples.len(), z.layers, ks.first().unwrap_or(&0), ks.last().unwrap_or(&0));
                }
            }
        }
    }
    o
}
