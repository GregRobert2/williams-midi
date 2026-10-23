//! Apprentissage sur le corpus Williams : harmonie (accords, transitions,
//! rythme harmonique, basse), mélodie et contre-chant (intervalles, durées,
//! complémentarité), mesures et modulations selon l'ambiance, rôles
//! d'orchestration, doublures, tessitures, voicings, figures rythmiques,
//! timbales et percussions, ouvertures.

use crate::corpus::{Inst, Piece, N};
use std::collections::HashMap;

// ---------------------------------------------------------------- familles
pub const FAMS: [Inst; 19] = [
    Inst::Piccolo, Inst::Flute, Inst::Oboe, Inst::Clarinet, Inst::Bassoon,
    Inst::Horn, Inst::Trumpet, Inst::Trombone, Inst::Tuba,
    Inst::Timpani, Inst::Harp, Inst::Glock, Inst::Celesta, Inst::Xylo, Inst::Perc,
    Inst::Violin, Inst::Viola, Inst::Cello, Inst::Bass,
];
pub const NF: usize = FAMS.len();
pub fn fam(i: Inst) -> usize {
    FAMS.iter().position(|&f| f == i).unwrap()
}
fn fam_opt(i: Inst) -> Option<usize> {
    FAMS.iter().position(|&f| f == i)
}
pub fn fname(f: usize) -> String {
    format!("{:?}", FAMS[f])
}
pub fn pitched(f: usize) -> bool {
    !matches!(FAMS[f], Inst::Perc | Inst::Timpani)
}

// ---------------------------------------------------------------- accords
pub const NQ: usize = 9;
pub const QN: [&str; NQ] = ["", "m", "sus4", "7", "maj7", "m7", "dim", "aug", "sus2"];
pub const QT: [&[u8]; NQ] = [&[0, 4, 7], &[0, 3, 7], &[0, 5, 7], &[0, 4, 7, 10], &[0, 4, 7, 11], &[0, 3, 7, 10], &[0, 3, 6], &[0, 4, 8], &[0, 2, 7]];
pub const PCN: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
pub fn chname(r: u8, q: u8) -> String {
    format!("{}{}", PCN[r as usize % 12], QN[q as usize])
}
pub fn tones(r: u8, q: u8) -> Vec<u8> {
    QT[q as usize].iter().map(|i| (r + i) % 12).collect()
}

/// Meilleur accord pour un vecteur de classes de hauteurs, basse favorisée.
pub fn best_chord(w: &[f32; 12], bass: Option<u8>) -> Option<(u8, u8, f32)> {
    let tot: f32 = w.iter().sum();
    if tot <= 0. {
        return None;
    }
    let mut best = (0, 0, f32::MIN);
    for r in 0..12u8 {
        for (q, t) in QT.iter().enumerate() {
            let inw: f32 = t.iter().map(|&i| w[((r + i) % 12) as usize]).sum();
            let mut sc = (inw - 1.2 * (tot - inw)) / tot;
            if t.len() == 4 {
                let fourth = w[((r + t[3]) % 12) as usize] / tot;
                sc -= if fourth < 0.08 { 0.3 } else { 0.03 };
            }
            if matches!(q, 2 | 6 | 7 | 8) {
                sc -= 0.06;
            }
            if w[r as usize] / tot < 0.06 {
                sc -= 0.25;
            }
            if bass == Some(r) {
                sc += 0.08;
            }
            if sc > best.2 {
                best = (r, q as u8, sc);
            }
        }
    }
    Some(best)
}

// ---------------------------------------------------------------- tonalité
const KMAJ: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const KMIN: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
pub fn key_of(w: &[f32; 12]) -> (u8, bool, f32) {
    let mut best = (0, true, f32::MIN);
    let mw = w.iter().sum::<f32>() / 12.;
    for t in 0..12 {
        for (maj, prof) in [(true, &KMAJ), (false, &KMIN)] {
            let mp = prof.iter().sum::<f32>() / 12.;
            let (mut num, mut a, mut b) = (0., 0., 0.);
            for i in 0..12 {
                let x = w[(i + t) % 12] - mw;
                let y = prof[i] - mp;
                num += x * y;
                a += x * x;
                b += y * y;
            }
            let r = num / (a * b).sqrt().max(1e-9);
            if r > best.2 {
                best = (t as u8, maj, r);
            }
        }
    }
    best
}

/// Découpage harmonique d'une mesure (en temps) selon la signature.
pub fn slots(num: u8, den: u8) -> Vec<f32> {
    let unit = 4. / den as f32;
    if den == 8 && num % 3 == 0 {
        return vec![1.5; (num / 3) as usize];
    }
    match (num, den) {
        (3, 4) => vec![3.],
        (5, 4) => vec![3., 2.],
        (6, 4) => vec![3., 3.],
        (3, 2) => vec![2., 2., 2.],
        (n, 4) if n % 2 == 0 => vec![2.; (n / 2) as usize],
        (n, 2) => vec![2.; n as usize],
        (n, _) => vec![n as f32 * unit],
    }
}

// ---------------------------------------------------------------- rôles
pub const ROLES: [&str; 7] = ["rest", "melody", "bass", "sustain", "ostinato", "counter", "hits"];
pub const R_REST: usize = 0;
pub const R_MEL: usize = 1;
pub const R_BASS: usize = 2;
pub const R_SUS: usize = 3;
pub const R_OST: usize = 4;
pub const R_CTR: usize = 5;
pub const R_HIT: usize = 6;

pub fn dyn_of(v: f32) -> usize {
    if v < 70. { 0 } else if v < 95. { 1 } else { 2 }
}
pub const DYN: [&str; 3] = ["p", "mf", "f"];

pub fn mood(dyn_: usize, bpm: f32, dens: f32) -> String {
    let speed = if bpm < 80. { "lent" } else if bpm < 125. { "modéré" } else { "rapide" };
    let act = if dens > 5. { "dense" } else { "léger" };
    format!("{}/{}/{}", DYN[dyn_], speed, act)
}

// ---------------------------------------------------------------- modèle
#[derive(Default)]
pub struct Model {
    pub pieces: usize,
    pub bars: usize,
    // harmonie (relative à la tonique)
    pub trans: HashMap<(bool, u8, u8), HashMap<(u8, u8), f32>>,
    pub chord_freq: HashMap<(bool, u8, u8), f32>,
    pub root_motion: [f32; 12],
    /// mélodie / accord : [temps fort][qualité][intervalle à la fondamentale]
    pub emis: [[[f32; 12]; NQ]; 2],
    /// durée des accords (en tranches harmoniques) par nuance
    pub hr: [HashMap<u32, f32>; 3],
    /// (changements, tranches) au premier temps / ailleurs
    pub change_down: (f32, f32),
    pub change_mid: (f32, f32),
    pub inversion: [f32; 4],
    pub pedal: (f32, f32),
    // lignes mélodiques
    pub mel_iv: [f32; 25],
    pub mel_dur: HashMap<u8, f32>,
    pub ctr_iv: [f32; 25],
    pub ctr_dur: HashMap<u8, f32>,
    /// contre-chant : attaque quand la mélodie tient / quand elle bouge
    pub ctr_when_hold: (f32, f32),
    pub ctr_when_move: (f32, f32),
    /// contre-chant : mouvement contraire / parallèle / oblique
    pub ctr_motion: [f32; 3],
    // orchestration
    pub roles: HashMap<(usize, usize), Vec<[f32; 7]>>,
    pub role_bars: HashMap<(usize, usize), f32>,
    pub dbl_oct: HashMap<(usize, usize), [f32; 5]>,
    pub reg: HashMap<(usize, usize), Vec<u8>>,
    pub poly: HashMap<(usize, usize), (f32, f32)>,
    pub rhythm: HashMap<(usize, usize, usize), HashMap<u16, f32>>,
    pub spacing: Vec<Vec<u8>>,
    pub timp_roll: [(f32, f32); 3],
    pub roll_before_louder: (f32, f32),
    pub crash: [[(f32, f32); 2]; 3],
    pub bdrum: [[(f32, f32); 2]; 3],
    pub density: [HashMap<usize, f32>; 3],
    pub carriers: [HashMap<usize, f32>; 3],
    // forme
    pub meter_mood: HashMap<(String, String), f32>,
    pub meter_change: HashMap<(String, String), f32>,
    pub meter_change_ctx: Vec<MeterChange>,
    pub modulation: HashMap<i8, f32>,
    pub modulation_ctx: Vec<(String, i8, usize, usize)>,
    pub openings: Vec<(usize, usize, bool, Vec<usize>)>,
}

pub struct MeterChange {
    pub from: String,
    pub to: String,
    pub dyn_before: usize,
    pub dyn_after: usize,
    pub bpm_before: f32,
    pub bpm_after: f32,
    pub back_after: usize,
    pub dens_before: f32,
    pub dens_after: f32,
}

pub struct Bar {
    pub t: f32,
    pub len: f32,
    pub num: u8,
    pub den: u8,
}

pub fn bars_of(sigs: &[(f32, u8, u8)], total: f32) -> Vec<Bar> {
    let mut out = vec![];
    let mut t = 0.;
    let mut si = 0;
    while t < total - 0.01 {
        while si + 1 < sigs.len() && sigs[si + 1].0 <= t + 0.01 {
            si += 1;
        }
        let (_, num, den) = sigs[si];
        let mut len = num as f32 * 4. / den as f32;
        if si + 1 < sigs.len() && sigs[si + 1].0 < t + len - 0.01 {
            len = sigs[si + 1].0 - t;
        }
        if len <= 0.01 {
            break;
        }
        out.push(Bar { t, len, num, den });
        t += len;
    }
    out
}

fn bpm_at(p: &Piece, t: f32) -> f32 {
    p.tempos.iter().rev().find(|x| x.0 <= t + 0.01).map(|x| x.1).unwrap_or(120.)
}

fn overlap(n: &N, a: f32, b: f32) -> f32 {
    (n.t + n.l).min(b) - n.t.max(a)
}

/// Notes (triées par début) qui recouvrent [a, e).
fn window<'a>(ns: &'a [N], a: f32, e: f32) -> impl Iterator<Item = &'a N> {
    let i = ns.partition_point(|n| n.t < a - 16.);
    ns[i..].iter().take_while(move |n| n.t < e).filter(move |n| n.t + n.l > a)
}

/// Ligne supérieure (une hauteur par attaque).
fn topline(ns: &[N]) -> Vec<N> {
    let mut v: Vec<N> = vec![];
    for n in ns {
        match v.last_mut() {
            Some(l) if (l.t - n.t).abs() < 0.07 => {
                if n.k > l.k {
                    *l = *n;
                }
            }
            _ => v.push(*n),
        }
    }
    v
}

/// Analyse harmonique d'une pièce : accords par tranche, puis fusion.
/// Retourne des segments (début, fin, fondamentale, qualité, basse).
pub fn harmonize_notes(harm: &[N], bars: &[Bar]) -> Vec<(f32, f32, u8, u8, Option<u8>)> {
    let analyze = |a: f32, e: f32| {
        let mut w = [0f32; 12];
        let mut low: Option<u8> = None;
        for n in window(harm, a, e) {
            let o = overlap(n, a, e);
            if o > 0.05 {
                let strong = if (n.t - a).abs() < 0.05 { 1.5 } else { 1. };
                w[(n.k % 12) as usize] += o * n.v as f32 * strong;
                if low.is_none_or(|l| n.k < l) {
                    low = Some(n.k);
                }
            }
        }
        (best_chord(&w, low.map(|l| l % 12)), low)
    };
    let mut segs: Vec<(f32, f32, u8, u8, Option<u8>)> = vec![];
    for b in bars {
        let sl = slots(b.num, b.den);
        let mut parts = vec![];
        let mut t = b.t;
        for s in &sl {
            let s = s.min(b.t + b.len - t);
            if s <= 0.01 {
                break;
            }
            parts.push((t, t + s, analyze(t, t + s)));
            t += s;
        }
        let (whole, wl) = analyze(b.t, b.t + b.len);
        let mean = parts.iter().filter_map(|p| p.2 .0.map(|c| c.2)).sum::<f32>() / parts.len().max(1) as f32;
        let one = matches!(whole, Some(c) if c.2 > 0.15 && c.2 >= mean - 0.12);
        let items: Vec<(f32, f32, Option<(u8, u8, f32)>, Option<u8>)> = if one || parts.len() <= 1 {
            vec![(b.t, b.t + b.len, whole, wl)]
        } else {
            parts.into_iter().map(|p| (p.0, p.1, p.2 .0, p.2 .1)).collect()
        };
        for (a, e, c, low) in items {
            let Some(c) = c.filter(|c| c.2 > 0.15) else { continue };
            match segs.last_mut() {
                Some(s) if (s.2, s.3) == (c.0, c.1) && (s.1 - a).abs() < 0.01 => s.1 = e,
                _ => segs.push((a, e, c.0, c.1, low)),
            }
        }
    }
    segs
}

pub fn train(corpus: &[Piece]) -> Model {
    let mut m = Model { spacing: vec![vec![]; 6], ..Default::default() };
    for p in corpus {
        let tot: usize = p.parts.iter().map(|x| x.notes.len()).sum();
        let red: usize = p.parts.iter().filter(|x| matches!(x.inst, Inst::Piano | Inst::Synth | Inst::Other)).map(|x| x.notes.len()).sum();
        if tot < 50 {
            continue;
        }
        let orch = (red as f32) < 0.3 * tot as f32;
        m.pieces += 1;
        let mut fams: Vec<Vec<N>> = vec![vec![]; NF];
        let mut harm: Vec<N> = vec![];
        for pt in &p.parts {
            if pt.inst != Inst::Perc {
                harm.extend(pt.notes.iter().copied());
            }
            if let Some(f) = fam_opt(pt.inst) {
                fams[f].extend(pt.notes.iter().copied());
            }
        }
        for f in fams.iter_mut() {
            f.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        }
        harm.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        let bars = bars_of(&p.sigs, p.len);
        if bars.is_empty() {
            continue;
        }
        let bar_dyn = |a: f32, e: f32| -> f32 {
            let (mut s, mut c) = (0., 0.);
            for n in window(&harm, a, e).filter(|n| n.t >= a) {
                s += n.v as f32;
                c += 1.;
            }
            if c > 0. { s / c } else { 0. }
        };
        let bar_feat: Vec<(usize, f32, f32)> = bars
            .iter()
            .map(|b| {
                let on = window(&harm, b.t, b.t + b.len).filter(|n| n.t >= b.t).count() as f32;
                (dyn_of(bar_dyn(b.t, b.t + b.len)), bpm_at(p, b.t), on / b.len.max(0.5) / 3.)
            })
            .collect();

        // ---------- tonalité, accords
        let mut gw = [0f32; 12];
        for n in &harm {
            gw[(n.k % 12) as usize] += n.l.min(4.) * n.v as f32;
        }
        let (gt, gmaj, _) = key_of(&gw);
        let segs = harmonize_notes(&harm, &bars);
        let rel = |r: u8, q: u8| ((r + 12 - gt) % 12, q);
        for w in segs.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.1 - b.0).abs() > 0.01 {
                continue;
            }
            *m.trans.entry((gmaj, rel(a.2, a.3).0, a.3)).or_default().entry(rel(b.2, b.3)).or_default() += 1.;
            m.root_motion[((b.2 + 12 - a.2) % 12) as usize] += 1.;
            if let (Some(x), Some(y)) = (a.4, b.4) {
                m.pedal.1 += 1.;
                if x == y {
                    m.pedal.0 += 1.;
                }
            }
        }
        // tranches : changement d'accord au 1er temps ou en cours de mesure
        let bar_at = |t: f32| bars.iter().rposition(|b| b.t <= t + 0.01).unwrap_or(0);
        for s in &segs {
            let r = rel(s.2, s.3);
            *m.chord_freq.entry((gmaj, r.0, r.1)).or_default() += s.1 - s.0;
            let bi = bar_at(s.0);
            let b = &bars[bi];
            let nslots = {
                let mut n = 0.;
                let mut t = s.0;
                let mut bj = bi;
                while t < s.1 - 0.01 && bj < bars.len() {
                    let bb = &bars[bj];
                    let sl = slots(bb.num, bb.den);
                    let mut x = bb.t;
                    for l in sl {
                        if x >= t - 0.01 && x < s.1 - 0.01 {
                            n += 1.;
                        }
                        x += l;
                    }
                    bj += 1;
                    t = bars.get(bj).map(|b| b.t).unwrap_or(f32::MAX);
                }
                n
            };
            *m.hr[bar_feat[bi].0].entry((nslots as u32).clamp(1, 16)).or_default() += 1.;
            let down = (s.0 - b.t).abs() < 0.05;
            if down {
                m.change_down.0 += 1.;
            } else {
                m.change_mid.0 += 1.;
            }
            if let Some(bk) = s.4 {
                let iv = (bk % 12 + 12 - s.2) % 12;
                let t = QT[s.3 as usize];
                let i = if iv == 0 { 0 } else if iv == t[1] { 1 } else if iv == t[2] { 2 } else { 3 };
                m.inversion[i] += 1.;
            }
            // émission : notes de dessus pendant l'accord
            let top = topline(&window(&harm, s.0, s.1).filter(|n| n.t >= s.0 - 0.01).copied().collect::<Vec<_>>());
            for n in top {
                let bb = &bars[bar_at(n.t)];
                let strong = slots(bb.num, bb.den)
                    .iter()
                    .scan(bb.t, |x, l| { let r = *x; *x += l; Some(r) })
                    .any(|x| (x - n.t).abs() < 0.05) as usize;
                let iv = ((n.k % 12) + 12 - s.2) % 12;
                m.emis[strong][s.3 as usize][iv as usize] += n.l.min(2.);
            }
        }
        for b in &bars {
            let n = slots(b.num, b.den).len() as f32;
            m.change_down.1 += 1.;
            m.change_mid.1 += n - 1.;
        }

        // ---------- modulations
        let mut prev_key: Option<(u8, bool)> = None;
        let mut i = 0;
        while i + 8 <= bars.len() {
            let (a, e) = (bars[i].t, bars[i + 7].t + bars[i + 7].len);
            let mut w = [0f32; 12];
            for n in window(&harm, a, e) {
                w[(n.k % 12) as usize] += overlap(n, a, e) * n.v as f32;
            }
            let (t, maj, r) = key_of(&w);
            if r > 0.75 {
                if let Some((pt, pm)) = prev_key {
                    let relative = pm != maj && ((pm && (t + 3) % 12 == pt) || (!pm && (pt + 3) % 12 == t));
                    if pt != t && !relative {
                        let mut iv = ((t + 12 - pt) % 12) as i8;
                        if iv > 6 {
                            iv -= 12;
                        }
                        *m.modulation.entry(iv).or_default() += 1.;
                        let d0 = dyn_of(bar_dyn(a - 16., a));
                        let d1 = dyn_of(bar_dyn(a, a + 16.));
                        m.modulation_ctx.push((p.file.clone(), iv, d0, d1));
                    }
                }
                prev_key = Some((t, maj));
            }
            i += 4;
        }

        // ---------- mesures et changements de signature
        for (bi, b) in bars.iter().enumerate() {
            let sig = format!("{}/{}", b.num, b.den);
            let (d, bpm, dens) = bar_feat[bi];
            *m.meter_mood.entry((mood(d, bpm, dens), sig.clone())).or_default() += 1.;
            if bi > 0 && (bars[bi - 1].num, bars[bi - 1].den) != (b.num, b.den) {
                let from = format!("{}/{}", bars[bi - 1].num, bars[bi - 1].den);
                *m.meter_change.entry((from.clone(), sig.clone())).or_default() += 1.;
                let back = bars[bi..].iter().position(|x| format!("{}/{}", x.num, x.den) == from).unwrap_or(0);
                let avg = |r: std::ops::Range<usize>, k: usize| -> f32 {
                    let r = r.start..r.end.min(bars.len());
                    let n = r.len().max(1) as f32;
                    r.map(|j| match k { 0 => bar_feat[j].0 as f32, 1 => bar_feat[j].1, _ => bar_feat[j].2 }).sum::<f32>() / n
                };
                m.meter_change_ctx.push(MeterChange {
                    from,
                    to: sig,
                    dyn_before: avg(bi.saturating_sub(2)..bi, 0).round() as usize,
                    dyn_after: avg(bi..bi + 2, 0).round() as usize,
                    bpm_before: bpm_at(p, bars[bi - 1].t),
                    bpm_after: bpm_at(p, b.t),
                    back_after: back,
                    dens_before: avg(bi.saturating_sub(2)..bi, 2),
                    dens_after: avg(bi..bi + 2, 2),
                });
            }
        }

        if !orch {
            continue;
        }

        // ---------- orchestration mesure par mesure
        let present: Vec<bool> = (0..NF).map(|f| fams[f].len() >= 8).collect();
        let mut last_line: [Option<u8>; NF] = [None; NF];
        for (bi, b) in bars.iter().enumerate() {
            let (a, e) = (b.t, b.t + b.len);
            m.bars += 1;
            let notes: Vec<Vec<N>> = (0..NF)
                .map(|f| window(&fams[f], a, e).filter(|n| n.t >= a - 0.01).copied().collect())
                .collect();
            let sounding = |f: usize| window(&fams[f], a + 0.1, e).next().is_some();
            let d = bar_feat[bi].0;
            let mut sky: Vec<(f32, u8)> = vec![];
            for f in 0..NF {
                if !pitched(f) || matches!(FAMS[f], Inst::Piccolo | Inst::Glock | Inst::Celesta | Inst::Xylo) {
                    continue;
                }
                for n in notes[f].iter().filter(|n| n.l >= 0.12) {
                    match sky.iter_mut().find(|s| (s.0 - n.t).abs() < 0.07) {
                        Some(s) => s.1 = s.1.max(n.k),
                        None => sky.push((n.t, n.k)),
                    }
                }
            }
            let mut mel: Vec<(usize, f32)> = vec![];
            if sky.len() >= 2 {
                for f in 0..NF {
                    if !pitched(f) || notes[f].len() < 2 {
                        continue;
                    }
                    let ons = topline(&notes[f]);
                    let hit = ons.iter().filter(|o| sky.iter().any(|s| (s.0 - o.t).abs() < 0.07 && s.1 % 12 == o.k % 12)).count();
                    if hit as f32 >= 0.7 * ons.len() as f32 && hit as f32 >= 0.6 * sky.len() as f32 {
                        let mp = ons.iter().map(|o| o.k as f32).sum::<f32>() / ons.len() as f32;
                        mel.push((f, mp));
                    }
                }
            }
            let carrier = mel.iter().min_by(|x, y| x.1.partial_cmp(&y.1).unwrap()).copied();
            let ck = (carrier.map(|c| c.0).unwrap_or(NF), d);
            *m.role_bars.entry(ck).or_default() += 1.;
            if let Some(c) = carrier {
                *m.carriers[d].entry(c.0).or_default() += 1.;
            }
            let lowest = (0..NF)
                .filter(|&f| pitched(f) && !notes[f].is_empty() && !mel.iter().any(|x| x.0 == f))
                .min_by_key(|&f| notes[f].iter().map(|n| n.k).min().unwrap());
            let mel_line: Vec<N> = carrier.map(|c| topline(&notes[c.0])).unwrap_or_default();
            let mut active = 0;
            let mut role_of = [R_REST; NF];
            for f in 0..NF {
                let ns = &notes[f];
                let role = if ns.is_empty() {
                    if sounding(f) && pitched(f) { R_SUS } else { R_REST }
                } else if let Some(x) = mel.iter().find(|x| x.0 == f) {
                    if let Some(c) = carrier {
                        let o = ((x.1 - c.1) / 12.).round().clamp(-2., 2.) as i32;
                        m.dbl_oct.entry((c.0, f)).or_insert([0.; 5])[(o + 2) as usize] += 1.;
                    }
                    R_MEL
                } else if !pitched(f) || matches!(FAMS[f], Inst::Glock | Inst::Xylo) && ns.len() <= 3 {
                    R_HIT
                } else if Some(f) == lowest && FAMS[f] != Inst::Harp {
                    R_BASS
                } else {
                    let avg = ns.iter().map(|n| n.l).sum::<f32>() / ns.len() as f32;
                    let ons = topline(ns).len() as f32;
                    // contre-chant = vraie ligne : monodique, peu de notes répétées,
                    // au moins 3 hauteurs ; sinon figure d'accompagnement
                    let tl = topline(ns);
                    let poly = ns.len() as f32 / tl.len().max(1) as f32;
                    let reps = tl.windows(2).filter(|w| w[0].k == w[1].k).count() as f32 / (tl.len().max(2) - 1) as f32;
                    let mut ks: Vec<u8> = tl.iter().map(|n| n.k).collect();
                    ks.sort();
                    ks.dedup();
                    if avg >= 1.4 || ons <= 1. {
                        R_SUS
                    } else if ons >= 1.5 * b.len || poly >= 1.6 || reps > 0.4 || ks.len() < 3 {
                        R_OST
                    } else {
                        R_CTR
                    }
                };
                role_of[f] = role;
                // rôle conditionné à la présence du pupitre dans la pièce
                if present[f] {
                    m.roles.entry(ck).or_insert_with(|| vec![[0.; 7]; NF])[f][role] += 1.;
                }
                if role != R_REST {
                    active += 1;
                }
                if role != R_REST && role != R_HIT && !ns.is_empty() {
                    m.reg.entry((f, role)).or_default().extend(ns.iter().map(|n| n.k));
                    let mut ons: HashMap<i32, u32> = HashMap::new();
                    for n in ns {
                        *ons.entry((n.t * 12.).round() as i32).or_default() += 1;
                    }
                    let pv = ons.values().map(|&c| c as f32).sum::<f32>() / ons.len() as f32;
                    let e = m.poly.entry((f, role)).or_insert((0., 0.));
                    e.0 += pv;
                    e.1 += 1.;
                }
                if role != R_REST && !ns.is_empty() && (b.num, b.den) == (4, 4) {
                    let mut mask = 0u16;
                    for n in ns {
                        let x = (n.t - a) * 4.;
                        let s = x.round() as i32;
                        if (0..16).contains(&s) && (x - s as f32).abs() < 0.15 {
                            mask |= 1 << s;
                        }
                    }
                    *m.rhythm.entry((f, role, d)).or_default().entry(mask).or_default() += 1.;
                }
                // lignes : intervalles et durées
                if (role == R_MEL && Some(f) == carrier.map(|c| c.0)) || role == R_CTR {
                    let line = topline(ns);
                    let (iv, du) = if role == R_MEL { (&mut m.mel_iv, &mut m.mel_dur) } else { (&mut m.ctr_iv, &mut m.ctr_dur) };
                    let mut prev = last_line[f];
                    for (i, n) in line.iter().enumerate() {
                        if let Some(pk) = prev {
                            let x = n.k as i32 - pk as i32;
                            if x.abs() <= 12 {
                                iv[(x + 12) as usize] += 1.;
                            }
                        }
                        prev = Some(n.k);
                        let ioi = line.get(i + 1).map(|x| x.t - n.t).unwrap_or(n.l).min(n.l.max(0.25));
                        *du.entry(((ioi * 4.).round() as u8).clamp(1, 32)).or_default() += 1.;
                    }
                    last_line[f] = prev;
                } else {
                    last_line[f] = None;
                }
            }
            // contre-chant vs mélodie
            if mel_line.len() >= 2 {
                for f in (0..NF).filter(|&f| role_of[f] == R_CTR) {
                    let ctr = topline(&notes[f]);
                    let mut t = a;
                    while t < e - 0.01 {
                        let mel_on = mel_line.iter().any(|n| (n.t - t).abs() < 0.07);
                        let ctr_on = ctr.iter().any(|n| (n.t - t).abs() < 0.07);
                        let slot = if mel_on { &mut m.ctr_when_move } else { &mut m.ctr_when_hold };
                        slot.1 += 1.;
                        if ctr_on {
                            slot.0 += 1.;
                        }
                        t += 0.5;
                    }
                    let at = |l: &[N], t: f32| l.iter().rev().find(|n| n.t <= t + 0.01).map(|n| n.k as i32);
                    for w in ctr.windows(2) {
                        if let (Some(m0), Some(m1)) = (at(&mel_line, w[0].t), at(&mel_line, w[1].t)) {
                            let dc = w[1].k as i32 - w[0].k as i32;
                            let dm = m1 - m0;
                            let k = if dc == 0 || dm == 0 { 2 } else if dc.signum() != dm.signum() { 0 } else { 1 };
                            m.ctr_motion[k] += 1.;
                        }
                    }
                }
            }
            *m.density[d].entry(active).or_default() += 1.;
            let mut v: Vec<u8> = (0..NF)
                .filter(|&f| pitched(f))
                .flat_map(|f| window(&fams[f], a + 0.1, a + 0.11).filter(|n| n.t <= a + 0.05).map(|n| n.k).collect::<Vec<_>>())
                .collect();
            v.sort();
            v.dedup();
            if v.len() >= 4 {
                for (i, w) in v.windows(2).take(6).enumerate() {
                    m.spacing[i].push(w[1] - w[0]);
                }
            }
            let tn = &notes[fam(Inst::Timpani)];
            let roll = tn.windows(4).any(|w| w.iter().all(|n| n.k == w[0].k) && w[3].t - w[0].t <= 1.0);
            m.timp_roll[d].1 += 1.;
            if roll {
                m.timp_roll[d].0 += 1.;
                if bi + 1 < bars.len() {
                    m.roll_before_louder.1 += 1.;
                    if bar_feat[bi + 1].0 > d {
                        m.roll_before_louder.0 += 1.;
                    }
                }
            }
            let pn = &notes[fam(Inst::Perc)];
            let head = (bi % 4 == 0) as usize;
            let has = |ks: &[u8]| pn.iter().any(|n| ks.contains(&n.k));
            m.crash[d][head].1 += 1.;
            m.bdrum[d][head].1 += 1.;
            if has(&[49, 57, 55, 52]) {
                m.crash[d][head].0 += 1.;
            }
            if has(&[35, 36]) {
                m.bdrum[d][head].0 += 1.;
            }
            if bi == 0 {
                let b0 = segs.first().and_then(|s| s.4).map(|x| x % 12);
                let ped = segs.iter().take_while(|s| s.0 < bars.get(4).map(|b| b.t).unwrap_or(p.len)).all(|s| s.4.map(|x| x % 12) == b0);
                let act: Vec<usize> = (0..NF).filter(|&f| !notes[f].is_empty()).collect();
                m.openings.push((d, act.len(), ped, act));
            }
        }
    }
    m
}

pub fn pct(v: &[u8], p: f32) -> u8 {
    if v.is_empty() {
        return 0;
    }
    let mut v = v.to_vec();
    v.sort();
    v[((v.len() - 1) as f32 * p) as usize]
}

impl Model {
    pub fn reg_range(&self, f: usize, role: usize, lo: f32, hi: f32) -> (u8, u8) {
        let v = self.reg.get(&(f, role)).filter(|v| v.len() > 200).or_else(|| self.reg.get(&(f, R_SUS))).unwrap();
        (pct(v, lo), pct(v, hi))
    }
    pub fn role_p(&self, carrier: usize, d: usize, f: usize) -> [f32; 7] {
        let mut out = [0.; 7];
        for dd in [d, 1, 0, 2] {
            if let Some(r) = self.roles.get(&(carrier, dd)) {
                if self.role_bars[&(carrier, dd)] >= 60. {
                    let t: f32 = r[f].iter().sum();
                    for i in 0..7 {
                        out[i] = r[f][i] / t.max(1.);
                    }
                    return out;
                }
            }
        }
        out
    }
    pub fn top_rhythms(&self, f: usize, role: usize, d: usize, n: usize) -> Vec<(u16, f32)> {
        let mut acc: HashMap<u16, f32> = HashMap::new();
        for dd in 0..3 {
            if let Some(h) = self.rhythm.get(&(f, role, dd)) {
                let w = if dd == d { 3. } else { 1. };
                for (k, v) in h {
                    if *k != 0 {
                        *acc.entry(*k).or_default() += v * w;
                    }
                }
            }
        }
        let t: f32 = acc.values().sum();
        let mut v: Vec<(u16, f32)> = acc.into_iter().map(|(k, c)| (k, c / t.max(1.))).collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        v.truncate(n);
        v
    }
}

// ---------------------------------------------------------------- rapport
pub fn report(m: &Model) -> String {
    let mut o = String::new();
    let w = |o: &mut String, s: String| {
        o.push_str(&s);
        o.push('\n');
    };
    w(&mut o, format!("# Ce que Williams enseigne — {} pièces, {} mesures orchestrales analysées\n", m.pieces, m.bars));
    w(&mut o, "## Rythme harmonique\n".into());
    for d in 0..3 {
        let tot: f32 = m.hr[d].values().sum();
        let mean: f32 = m.hr[d].iter().map(|(k, c)| *k as f32 * c).sum::<f32>() / tot.max(1.);
        let one: f32 = m.hr[d].get(&1).copied().unwrap_or(0.) / tot.max(1.);
        w(&mut o, format!("- **{}** : un accord dure en moyenne {:.1} tranches harmoniques (demi-mesure en 4/4) ; {:.0}% ne durent qu'une tranche", DYN[d], mean, one * 100.));
    }
    w(&mut o, format!("- Probabilité de changer d'accord : au 1er temps {:.0}%, en milieu de mesure {:.0}%",
        100. * m.change_down.0 / m.change_down.1.max(1.), 100. * m.change_mid.0 / m.change_mid.1.max(1.)));
    let it: f32 = m.inversion.iter().sum();
    w(&mut o, format!("- Basse : fondamentale {:.0}%, tierce {:.0}%, quinte {:.0}%, autre (pédale) {:.0}%",
        100. * m.inversion[0] / it, 100. * m.inversion[1] / it, 100. * m.inversion[2] / it, 100. * m.inversion[3] / it));
    w(&mut o, format!("- Basse tenue sous un changement d'accord (pédale) : {:.0}%\n", 100. * m.pedal.0 / m.pedal.1.max(1.)));

    w(&mut o, "## Mouvements de fondamentale\n".into());
    let rt: f32 = m.root_motion[1..].iter().sum();
    let names = ["", "+1 (b2)", "+2", "+3 (tierce m)", "+4 (tierce M)", "+5 (quarte)", "triton", "+7 (quinte)", "+8 (6te m)", "+9 (6te M)", "+10 (b7)", "+11 (7)"];
    let mut rm: Vec<(usize, f32)> = m.root_motion.iter().copied().enumerate().skip(1).collect();
    rm.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let s: Vec<String> = rm.iter().map(|(i, c)| format!("{} {:.1}%", names[*i], 100. * c / rt)).collect();
    w(&mut o, s.join(" · "));

    for maj in [true, false] {
        w(&mut o, format!("\n## Accords ({}) — degrés relatifs, tonique = C\n", if maj { "majeur" } else { "mineur" }));
        let tot: f32 = m.chord_freq.iter().filter(|x| x.0 .0 == maj).map(|x| x.1).sum();
        let mut v: Vec<_> = m.chord_freq.iter().filter(|x| x.0 .0 == maj).collect();
        v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        let line: Vec<String> = v.iter().take(20).map(|(k, c)| format!("{} {:.1}%", chname(k.1, k.2), 100. * *c / tot)).collect();
        w(&mut o, line.join(" · "));
        let mut tr: Vec<(String, f32)> = vec![];
        for (k, nx) in m.trans.iter().filter(|x| x.0 .0 == maj) {
            for (n, c) in nx {
                tr.push((format!("{} → {}", chname(k.1, k.2), chname(n.0, n.1)), *c));
            }
        }
        tr.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let tt: f32 = tr.iter().map(|x| x.1).sum();
        let s: Vec<String> = tr.iter().take(24).map(|(s, c)| format!("{} {:.1}%", s, 100. * c / tt)).collect();
        w(&mut o, format!("\nEnchaînements : {}", s.join(" · ")));
    }

    w(&mut o, "\n## Mélodie et contre-chant\n".into());
    let ivn = ["1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7"];
    for (q, name) in [(0usize, "majeur"), (1, "mineur")] {
        for strong in [1usize, 0] {
            let t: f32 = m.emis[strong][q].iter().sum();
            let mut v: Vec<(usize, f32)> = m.emis[strong][q].iter().copied().enumerate().collect();
            v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let s: Vec<String> = v.iter().take(7).map(|(i, c)| format!("{} {:.0}%", ivn[*i], 100. * c / t)).collect();
            w(&mut o, format!("- note de mélodie sur accord {} ({}) : {}", name, if strong == 1 { "temps fort" } else { "temps faible" }, s.join(", ")));
        }
    }
    let line_stats = |iv: &[f32; 25], du: &HashMap<u8, f32>| -> String {
        let t: f32 = iv.iter().sum();
        let step = (iv[10] + iv[11] + iv[13] + iv[14]) / t;
        let rep = iv[12] / t;
        let leap = 1. - step - rep;
        let up = iv[13..].iter().sum::<f32>() / t;
        let mut v: Vec<(i32, f32)> = iv.iter().enumerate().map(|(i, c)| (i as i32 - 12, *c)).collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let top: Vec<String> = v.iter().take(8).map(|(i, c)| format!("{:+} {:.0}%", i, 100. * c / t)).collect();
        let dt: f32 = du.values().sum();
        let mut dv: Vec<_> = du.iter().collect();
        dv.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        let dtop: Vec<String> = dv.iter().take(6).map(|(k, c)| format!("{} {:.0}%", **k as f32 / 4., 100. * **c / dt)).collect();
        format!("conjoint {:.0}%, répété {:.0}%, sauts {:.0}%, montant {:.0}% ; intervalles {} ; durées (temps) {}", step * 100., rep * 100., leap * 100., up * 100., top.join(", "), dtop.join(", "))
    };
    w(&mut o, format!("- **Mélodie** : {}", line_stats(&m.mel_iv, &m.mel_dur)));
    w(&mut o, format!("- **Contre-chant** : {}", line_stats(&m.ctr_iv, &m.ctr_dur)));
    w(&mut o, format!("- Le contre-chant attaque quand la mélodie tient : {:.0}% / quand elle bouge : {:.0}%",
        100. * m.ctr_when_hold.0 / m.ctr_when_hold.1.max(1.), 100. * m.ctr_when_move.0 / m.ctr_when_move.1.max(1.)));
    let mt: f32 = m.ctr_motion.iter().sum();
    w(&mut o, format!("- Mouvement contre-chant / mélodie : contraire {:.0}%, parallèle {:.0}%, oblique {:.0}%",
        100. * m.ctr_motion[0] / mt, 100. * m.ctr_motion[1] / mt, 100. * m.ctr_motion[2] / mt));

    w(&mut o, "\n## Mesures et ambiance\n".into());
    let mut moods: HashMap<String, Vec<(String, f32)>> = HashMap::new();
    for ((md, sig), c) in &m.meter_mood {
        moods.entry(md.clone()).or_default().push((sig.clone(), *c));
    }
    let mut mk: Vec<_> = moods.keys().cloned().collect();
    mk.sort();
    for k in mk {
        let v = moods.get_mut(&k).unwrap();
        let t: f32 = v.iter().map(|x| x.1).sum();
        if t < 200. {
            continue;
        }
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let s: Vec<String> = v.iter().take(5).map(|(s, c)| format!("{} {:.0}%", s, 100. * c / t)).collect();
        w(&mut o, format!("- {} ({} mes.) : {}", k, t, s.join(", ")));
    }
    let mut mc: Vec<_> = m.meter_change.iter().collect();
    mc.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    let s: Vec<String> = mc.iter().take(12).map(|((a, b), c)| format!("{}→{} ({})", a, b, c)).collect();
    w(&mut o, format!("\nChangements : {}", s.join(" · ")));
    let n = m.meter_change_ctx.len() as f32;
    if n > 0. {
        let ret = m.meter_change_ctx.iter().filter(|c| c.back_after > 0 && c.back_after <= 2).count() as f32;
        let louder = m.meter_change_ctx.iter().filter(|c| c.dyn_after > c.dyn_before).count() as f32;
        let softer = m.meter_change_ctx.iter().filter(|c| c.dyn_after < c.dyn_before).count() as f32;
        let tempo = m.meter_change_ctx.iter().filter(|c| (c.bpm_after - c.bpm_before).abs() > 3.).count() as f32;
        w(&mut o, format!("\n- {} changements : {:.0}% sont des mesures isolées (retour en ≤ 2 mes.) ; nuance plus forte après {:.0}%, plus douce {:.0}% ; tempo modifié {:.0}%",
            n, 100. * ret / n, 100. * louder / n, 100. * softer / n, 100. * tempo / n));
        let mut sh: HashMap<String, f32> = HashMap::new();
        for c in m.meter_change_ctx.iter().filter(|c| c.back_after == 1) {
            *sh.entry(format!("{} dans {}", c.to, c.from)).or_default() += 1.;
        }
        let mut sh: Vec<_> = sh.into_iter().collect();
        sh.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let s: Vec<String> = sh.iter().take(6).map(|(k, c)| format!("{} ({})", k, c)).collect();
        w(&mut o, format!("- mesure isolée typique : {}", s.join(", ")));
    }

    w(&mut o, "\n## Modulations\n".into());
    let mut md: Vec<_> = m.modulation.iter().collect();
    md.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    let t: f32 = md.iter().map(|x| *x.1).sum();
    let s: Vec<String> = md.iter().take(8).map(|(k, c)| format!("{:+} {:.0}%", k, 100. * **c / t)).collect();
    w(&mut o, s.join(" · "));

    w(&mut o, "\n## Qui porte la mélodie\n".into());
    for d in 0..3 {
        let t: f32 = m.carriers[d].values().sum();
        let mut v: Vec<_> = m.carriers[d].iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        let s: Vec<String> = v.iter().take(7).map(|(f, c)| format!("{} {:.0}%", fname(**f), 100. * **c / t)).collect();
        w(&mut o, format!("- {} : {}", DYN[d], s.join(", ")));
    }

    w(&mut o, "\n## Orchestration selon le porteur du thème\n".into());
    for carrier in [Inst::Horn, Inst::Violin, Inst::Trumpet, Inst::Flute, Inst::Oboe, Inst::Cello, Inst::Clarinet] {
        let c = fam(carrier);
        for d in 0..3 {
            let Some(r) = m.roles.get(&(c, d)) else { continue };
            let nb = m.role_bars[&(c, d)];
            if nb < 60. {
                continue;
            }
            w(&mut o, format!("\n### Thème aux {:?} — {} ({} mesures)\n", carrier, DYN[d], nb));
            w(&mut o, "| pupitre | joue | mel | basse | tenue | ostinato | contre-chant | frappe | doublure |".into());
            w(&mut o, "|---|---|---|---|---|---|---|---|---|".into());
            for f in 0..NF {
                let t: f32 = r[f].iter().sum();
                let play = 1. - r[f][R_REST] / t;
                if play < 0.05 {
                    continue;
                }
                let oc = m.dbl_oct.get(&(c, f)).map(|o| {
                    let t: f32 = o.iter().sum();
                    let best = o.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
                    format!("{:+} ({:.0}%)", (best.0 as i32 - 2) * 12, 100. * best.1 / t)
                }).unwrap_or_default();
                let p = |i| format!("{:.0}%", 100. * r[f][i] / t);
                w(&mut o, format!("| {} | {:.0}% | {} | {} | {} | {} | {} | {} | {} |", fname(f), 100. * play, p(R_MEL), p(R_BASS), p(R_SUS), p(R_OST), p(R_CTR), p(R_HIT), oc));
            }
        }
    }

    w(&mut o, "\n## Tessitures (5% / médiane / 95%) et polyphonie\n".into());
    w(&mut o, "| pupitre | rôle | grave | médiane | aigu | notes simultanées |".into());
    w(&mut o, "|---|---|---|---|---|---|".into());
    let mut keys: Vec<_> = m.reg.keys().copied().collect();
    keys.sort();
    for k in keys {
        let v = &m.reg[&k];
        if v.len() < 300 {
            continue;
        }
        let po = m.poly[&k];
        w(&mut o, format!("| {} | {} | {} | {} | {} | {:.1} |", fname(k.0), ROLES[k.1], pct(v, 0.05), pct(v, 0.5), pct(v, 0.95), po.0 / po.1));
    }
    let s: Vec<String> = m.spacing.iter().map(|v| pct(v, 0.5).to_string()).collect();
    w(&mut o, format!("\nEspacement des voix (médiane, basse → aigu) : {} demi-tons", s.join(" · ")));

    w(&mut o, "\n## Figures rythmiques (4/4, x = attaque en double-croche)\n".into());
    for (inst, role) in [(Inst::Violin, R_OST), (Inst::Viola, R_OST), (Inst::Cello, R_OST), (Inst::Harp, R_OST), (Inst::Horn, R_OST), (Inst::Horn, R_CTR), (Inst::Trumpet, R_CTR), (Inst::Trombone, R_CTR), (Inst::Bass, R_BASS), (Inst::Timpani, R_HIT), (Inst::Perc, R_HIT)] {
        let f = fam(inst);
        let v = m.top_rhythms(f, role, 2, 4);
        let s: Vec<String> = v.iter().map(|(mask, c)| {
            let g: String = (0..16).map(|i| if mask & (1 << i) != 0 { 'x' } else { '.' }).collect();
            format!("`{}` {:.0}%", g, 100. * c)
        }).collect();
        w(&mut o, format!("- {:?} {} : {}", inst, ROLES[role], s.join(" ")));
    }

    w(&mut o, "\n## Percussions et densité\n".into());
    for d in 0..3 {
        let r = m.timp_roll[d];
        let t: f32 = m.density[d].values().sum();
        let mean: f32 = m.density[d].iter().map(|(k, c)| *k as f32 * c).sum::<f32>() / t.max(1.);
        w(&mut o, format!("- {} : {:.1} pupitres ; roulement de timbales {:.0}% des mesures ; cymbales tête de phrase {:.0}% / ailleurs {:.0}% ; grosse caisse {:.0}% / {:.0}%",
            DYN[d], mean, 100. * r.0 / r.1.max(1.),
            100. * m.crash[d][1].0 / m.crash[d][1].1.max(1.), 100. * m.crash[d][0].0 / m.crash[d][0].1.max(1.),
            100. * m.bdrum[d][1].0 / m.bdrum[d][1].1.max(1.), 100. * m.bdrum[d][0].0 / m.bdrum[d][0].1.max(1.)));
    }
    w(&mut o, format!("- roulement suivi d'une mesure plus forte : {:.0}%", 100. * m.roll_before_louder.0 / m.roll_before_louder.1.max(1.)));

    w(&mut o, "\n## Ouvertures (4 premières mesures)\n".into());
    let n = m.openings.len() as f32;
    let soft = m.openings.iter().filter(|x| x.0 == 0).count() as f32;
    let ped = m.openings.iter().filter(|x| x.2).count() as f32;
    let fm = m.openings.iter().map(|x| x.1 as f32).sum::<f32>() / n.max(1.);
    w(&mut o, format!("- {:.0}% commencent piano, {:.0}% sur une pédale, {:.1} pupitres", 100. * soft / n, 100. * ped / n, fm));
    let mut fc = [0f32; NF];
    for x in &m.openings {
        for &f in &x.3 {
            fc[f] += 1.;
        }
    }
    let mut v: Vec<(usize, f32)> = fc.iter().copied().enumerate().collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let s: Vec<String> = v.iter().take(10).map(|(f, c)| format!("{} {:.0}%", fname(*f), 100. * c / n)).collect();
    w(&mut o, format!("- présents dès le début : {}", s.join(", ")));
    o
}
