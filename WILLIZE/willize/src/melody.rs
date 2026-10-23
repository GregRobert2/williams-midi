//! Lignes apprises : contre-chant (Viterbi sur les hauteurs, intervalles et
//! complémentarité tirés du corpus) et thème généré sur une harmonie
//! (par exemple au-dessus d'un ostinato).

use crate::corpus::N;
use crate::harmony::{chord_at, Chord, Key};
use crate::train::{slots, Bar, Model, R_CTR, R_MEL};

fn ln_iv(hist: &[f32; 25], iv: i32) -> f32 {
    if iv.abs() > 12 {
        return -9.;
    }
    let t: f32 = hist.iter().sum();
    ((hist[(iv + 12) as usize] + 0.5) / (t + 12.)).ln()
}

fn strong_times(bars: &[Bar]) -> Vec<f32> {
    let mut v = vec![];
    for b in bars {
        let mut t = b.t;
        for l in slots(b.num, b.den) {
            v.push(t);
            t += l;
        }
    }
    v
}

fn sounding(mel: &[N], t: f32) -> Option<&N> {
    mel.iter().rev().find(|n| n.t <= t + 0.01 && n.t + n.l > t + 0.01)
}

/// Rythme d'une ligne secondaire : figures apprises du pupitre, complétées
/// là où la mélodie tient (complémentarité).
fn ctr_rhythm(m: &Model, f: usize, d: usize, bars: &[Bar], mel: &[N], role: usize) -> Vec<f32> {
    let pats = m.top_rhythms(f, role, d, 12);
    let hold = m.ctr_when_hold.0 / m.ctr_when_hold.1.max(1.);
    let mut on: Vec<f32> = vec![];
    for (bi, b) in bars.iter().enumerate() {
        if (b.num, b.den) == (4, 4) && !pats.is_empty() {
            // figure la plus fréquente avec 2 à 6 attaques, alternée par phrase
            let ok: Vec<u16> = pats.iter().map(|p| p.0).filter(|p| (2..=6).contains(&p.count_ones()) && p & 1 == 1).collect();
            let mask = if ok.is_empty() { 0x1111 } else { ok[(bi / 2) % ok.len().min(2)] };
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    on.push(b.t + i as f32 * 0.25);
                }
            }
        } else {
            let mut t = b.t;
            for l in slots(b.num, b.den) {
                on.push(t);
                t += l;
            }
        }
        // mélodie tenue sur la 2e moitié de la mesure : le contre-chant répond
        let mid = b.t + b.len / 2.;
        if let Some(n) = sounding(mel, mid) {
            if n.t < mid - 0.5 && hold > 0.1 && !on.iter().any(|&x| (x - mid).abs() < 0.3) {
                on.push(mid);
                on.push(mid + b.len / 4.);
            }
        }
    }
    on.sort_by(|a, b| a.partial_cmp(b).unwrap());
    on.dedup_by(|a, b| (*a - *b).abs() < 0.2);
    on
}

/// Contre-chant sous (ou sur) la mélodie dans le registre [lo, hi].
pub fn counter(m: &Model, f: usize, d: usize, bars: &[Bar], ch: &[Chord], mel: &[N], lo: u8, hi: u8, below: bool) -> Vec<N> {
    let on = ctr_rhythm(m, f, d, bars, mel, R_CTR);
    let end = bars.last().map(|b| b.t + b.len).unwrap_or(0.);
    line_viterbi(m, &on, end, bars, ch, mel, lo, hi, below, &m.ctr_iv, None)
}

#[allow(clippy::too_many_arguments)]
fn line_viterbi(
    m: &Model,
    on: &[f32],
    end: f32,
    bars: &[Bar],
    ch: &[Chord],
    mel: &[N],
    lo: u8,
    hi: u8,
    below: bool,
    ivh: &[f32; 25],
    contour: Option<&dyn Fn(f32) -> f32>,
) -> Vec<N> {
    let strong = strong_times(bars);
    let n = on.len();
    if n == 0 || hi <= lo {
        return vec![];
    }
    let ks: Vec<u8> = (lo..=hi).collect();
    let nk = ks.len();
    let _ = m;
    let emit = |i: usize, k: u8| -> f32 {
        let t = on[i];
        let c = chord_at(ch, t);
        let st = strong.iter().any(|&s| (s - t).abs() < 0.05);
        let mut e = 0.;
        if !c.has(k) {
            e += if st { 5. } else { 1.2 };
        }
        if let Some(mn) = sounding(mel, t) {
            let iv = (k as i32 - mn.k as i32).rem_euclid(12);
            if iv == 0 {
                e += 2.5;
            }
            if iv == 1 || iv == 11 {
                e += 3.;
            }
            if below && k > mn.k {
                e += 4.;
            }
            if !below && k < mn.k {
                e += 4.;
            }
            if (k as i32 - mn.k as i32).abs() < 3 {
                e += 1.5;
            }
        }
        if let Some(cf) = contour {
            e += 0.15 * (k as f32 - cf(t)).abs();
        } else {
            let mid = (lo as f32 + hi as f32) / 2.;
            e += 0.05 * (k as f32 - mid).abs();
        }
        e
    };
    let mut cost = vec![vec![f32::MAX; nk]; n];
    let mut bp = vec![vec![0usize; nk]; n];
    for j in 0..nk {
        cost[0][j] = emit(0, ks[j]);
    }
    for i in 1..n {
        let (t0, t1) = (on[i - 1], on[i]);
        let m0 = sounding(mel, t0).map(|x| x.k as i32);
        let m1 = sounding(mel, t1).map(|x| x.k as i32);
        let c0 = *chord_at(ch, t0);
        for j in 0..nk {
            let e = emit(i, ks[j]);
            let mut best = (f32::MAX, 0);
            for p in 0..nk {
                if cost[i - 1][p] == f32::MAX {
                    continue;
                }
                let iv = ks[j] as i32 - ks[p] as i32;
                let mut t = -ln_iv(ivh, iv);
                // note étrangère : résolution conjointe
                if !c0.has(ks[p]) && iv.abs() > 2 {
                    t += 3.;
                }
                if let (Some(a), Some(b)) = (m0, m1) {
                    let i0 = (ks[p] as i32 - a).rem_euclid(12);
                    let i1 = (ks[j] as i32 - b).rem_euclid(12);
                    let dm = b - a;
                    if iv != 0 && dm != 0 && i0 == i1 && (i1 == 0 || i1 == 7) {
                        t += 4.; // quintes / octaves parallèles
                    }
                    if iv != 0 && dm != 0 && iv.signum() != dm.signum() {
                        t -= 0.4; // mouvement contraire
                    }
                }
                let c = cost[i - 1][p] + t;
                if c < best.0 {
                    best = (c, p);
                }
            }
            cost[i][j] = best.0 + e;
            bp[i][j] = best.1;
        }
    }
    let mut j = (0..nk).min_by(|&a, &b| cost[n - 1][a].partial_cmp(&cost[n - 1][b]).unwrap()).unwrap();
    let mut ks_out = vec![0u8; n];
    for i in (0..n).rev() {
        ks_out[i] = ks[j];
        j = bp[i][j];
    }
    let mut out = vec![];
    for i in 0..n {
        let next = on.get(i + 1).copied().unwrap_or(end);
        out.push(N { t: on[i], l: (next - on[i]).max(0.25), k: ks_out[i], v: 90 });
    }
    // notes répétées liées (les lignes de Williams respirent)
    let mut merged: Vec<N> = vec![];
    for n in out {
        match merged.last_mut() {
            Some(l) if l.k == n.k && n.t - l.t < 2. && !strong.iter().any(|&s| (s - n.t).abs() < 0.05 && bars.iter().any(|b| (b.t - s).abs() < 0.05)) => l.l = n.t + n.l - l.t,
            _ => merged.push(n),
        }
    }
    merged
}

/// Génère un thème sur une harmonie : phrases 4+4, motif répété, arche,
/// cadence ; intervalles et figures appris du porteur.
pub fn theme(m: &Model, f: usize, d: usize, bars: &[Bar], ch: &[Chord], key: Key, lo: u8, hi: u8, max_onsets: u32) -> Vec<N> {
    let pats: Vec<u16> = m
        .top_rhythms(f, R_MEL, d, 60)
        .into_iter()
        .map(|p| p.0)
        .filter(|p| p & 1 == 1 && (1..=max_onsets).contains(&p.count_ones()))
        .collect();
    let long: Vec<u16> = pats.iter().copied().filter(|p| p.count_ones() <= 2).collect();
    let pick = |i: usize, v: &[u16], dflt: u16| v.get(i).copied().unwrap_or(dflt);
    let (a, b, c) = (pick(0, &pats, 0x0101), pick(1, &pats, 0x1111), pick(0, &long, 0x0001));
    let mut on: Vec<f32> = vec![];
    for (bi, bar) in bars.iter().enumerate() {
        let pos = bi % 8;
        let mask = match pos {
            0 | 4 => a,
            1 | 5 => b,
            2 | 6 => a,
            _ => c,
        };
        if (bar.num, bar.den) == (4, 4) {
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    on.push(bar.t + i as f32 * 0.25);
                }
            }
        } else {
            let mut t = bar.t;
            for l in slots(bar.num, bar.den) {
                on.push(t);
                t += l;
            }
        }
    }
    let total = bars.last().map(|b| b.t + b.len).unwrap_or(0.);
    let start = bars.first().map(|b| b.t).unwrap_or(0.);
    let span = (total - start).max(1.);
    let (lo_f, hi_f) = (lo as f32, hi as f32);
    // arche : départ au milieu, sommet aux 2/3, retour
    let arch = move |t: f32| -> f32 {
        let x = ((t - start) / span).clamp(0., 1.);
        let h = if x < 0.65 { x / 0.65 } else { (1. - x) / 0.35 };
        lo_f + (hi_f - lo_f) * (0.3 + 0.55 * h)
    };
    let mut line = line_viterbi(m, &on, total, bars, ch, &[], lo, hi, true, &m.mel_iv, Some(&arch));
    // cadence : dernière note sur la tonique (ou la quinte si l'accord l'exige)
    if let Some(l) = line.last_mut() {
        let c = chord_at(ch, l.t);
        let target = if c.has(key.tonic) { key.tonic } else { c.r };
        let base = l.k - l.k % 12;
        let cand = [base + target, base + target + 12, base.saturating_sub(12) + target];
        l.k = *cand.iter().filter(|k| (lo..=hi).contains(*k)).min_by_key(|k| (**k as i32 - l.k as i32).abs()).unwrap_or(&l.k);
    }
    line
}
