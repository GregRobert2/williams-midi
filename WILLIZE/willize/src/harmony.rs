//! Harmonisation apprise : Viterbi sur les accords (transitions, rythme
//! harmonique, émission mélodie/accord tirés du corpus), puis ligne de basse
//! (renversements et pédales appris).

use crate::corpus::N;
use crate::train::{slots, tones, Bar, Model, NQ, QT};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chord {
    pub t: f32,
    pub l: f32,
    pub r: u8,
    pub q: u8,
    /// classe de hauteur de la basse
    pub bass: u8,
}

impl Chord {
    pub fn tones(&self) -> Vec<u8> {
        tones(self.r, self.q)
    }
    pub fn has(&self, k: u8) -> bool {
        self.tones().contains(&(k % 12))
    }
}

pub fn chord_at(ch: &[Chord], t: f32) -> &Chord {
    ch.iter().rev().find(|c| c.t <= t + 0.01).unwrap_or(&ch[0])
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub tonic: u8,
    pub maj: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum End {
    Free,
    Tonic,
    /// accord imposé (fondamentale, qualité)
    Is(u8, u8),
    /// fondamentale imposée, qualité libre
    Root(u8),
}

const NS: usize = 12 * NQ;
fn st(s: usize) -> (u8, u8) {
    ((s / NQ) as u8, (s % NQ) as u8)
}

pub struct Harmonizer<'a> {
    m: &'a Model,
    key: Key,
    prior: Vec<f32>,
    trans: Vec<Vec<f32>>,
    emis: [[[f32; 12]; NQ]; 2],
    /// échelle du rythme harmonique (1 = Williams moyen, < 1 = plus lent)
    pub hr: f32,
    /// réharmonisation : accords d'un exposé précédent (temps relatif) à ne pas reprendre
    pub avoid: Vec<Chord>,
    /// bonus de couleur (tétrades, sus) pour une réexposition plus riche
    pub color: f32,
    /// préférence pour les accords majeurs (écriture héroïque)
    pub major_bias: f32,
}

impl<'a> Harmonizer<'a> {
    pub fn new(m: &'a Model, key: Key) -> Self {
        Self::with_stats(m, key, false)
    }

    /// `forte` : statistiques des seuls passages forte (majestueux / tutti).
    /// Les statistiques sont ramenées aux triades (les 7e détectées dans des
    /// MIDI amateurs sont surtout des doublures / notes de passage), puis la
    /// couleur (7e, sus) est un réglage explicite.
    pub fn with_stats(m: &'a Model, key: Key, forte: bool) -> Self {
        let red = |q: u8| -> u8 {
            match q {
                3 | 4 | 7 | 8 => 0,
                5 => 1,
                x => x,
            }
        };
        let (cf, tr) = if forte { (&m.chord_freq_f, &m.trans_f) } else { (&m.chord_freq, &m.trans) };
        let mut cfr: std::collections::HashMap<(u8, u8), f32> = Default::default();
        for ((mj, r, q), c) in cf {
            if *mj == key.maj {
                *cfr.entry((*r, red(*q))).or_default() += c;
            }
        }
        let mut trr: std::collections::HashMap<(u8, u8), std::collections::HashMap<(u8, u8), f32>> = Default::default();
        for ((mj, r, q), row) in tr {
            if *mj != key.maj {
                continue;
            }
            for ((r2, q2), c) in row {
                let a = (*r, red(*q));
                let b = (*r2, red(*q2));
                if a != b {
                    *trr.entry(a).or_default().entry(b).or_default() += c;
                }
            }
        }
        let tot: f32 = cfr.values().sum::<f32>().max(1.);
        let mut prior = vec![0f32; NS];
        for s in 0..NS {
            let (r, q) = st(s);
            let rel = (r + 12 - key.tonic) % 12;
            let c = cfr.get(&(rel, red(q))).copied().unwrap_or(0.);
            // les états non-triades partagent la masse de leur triade
            let share = if red(q) == q { 1. } else { 0.25 };
            prior[s] = ((c * share + 0.02 * tot / NS as f32) / tot).ln();
        }
        let mut trans = vec![vec![0f32; NS]; NS];
        for a in 0..NS {
            let (r, q) = st(a);
            let rel = (r + 12 - key.tonic) % 12;
            let row = trr.get(&(rel, red(q)));
            let rt: f32 = row.map(|x| x.values().sum()).unwrap_or(0.);
            for b in 0..NS {
                if a == b {
                    trans[a][b] = f32::NEG_INFINITY;
                    continue;
                }
                let (r2, q2) = st(b);
                let rel2 = (r2 + 12 - key.tonic) % 12;
                let c = if (rel2, red(q2)) == (rel, red(q)) {
                    // même triade, couleur différente (ex. C → Cmaj7) : rare mais permis
                    0.05 * rt
                } else {
                    row.and_then(|x| x.get(&(rel2, red(q2)))).copied().unwrap_or(0.)
                };
                let beta = 3.;
                trans[a][b] = ((c + beta * prior[b].exp()) / (rt + beta)).ln();
            }
        }
        let mut emis = [[[0f32; 12]; NQ]; 2];
        for s in 0..2 {
            for q in 0..NQ {
                let t: f32 = m.emis[s][q].iter().sum();
                for i in 0..12 {
                    emis[s][q][i] = ((m.emis[s][q][i] + 0.002 * t + 0.1) / (t * 1.024 + 1.2)).ln();
                }
            }
        }
        Harmonizer { m, key, prior, trans, emis, hr: 1., avoid: vec![], color: 0., major_bias: 0. }
    }

    /// Accords pour les mesures données. `mel` : ligne principale ;
    /// `bed` : autres hauteurs imposées (ostinato, accords de l'utilisateur).
    pub fn harmonize(&self, bars: &[Bar], mel: &[N], bed: &[N], start: Option<(u8, u8)>, end: End) -> Vec<Chord> {
        // tranches
        let mut sl: Vec<(f32, f32, bool)> = vec![];
        for b in bars {
            let mut t = b.t;
            for (i, l) in slots(b.num, b.den).into_iter().enumerate() {
                let l = l.min(b.t + b.len - t);
                if l <= 0.01 {
                    break;
                }
                sl.push((t, l, i == 0));
                t += l;
            }
        }
        let n = sl.len();
        if n == 0 {
            return vec![];
        }
        let pd = (self.m.change_down.0 / self.m.change_down.1.max(1.) * self.hr).clamp(0.05, 0.95);
        let pm = (self.m.change_mid.0 / self.m.change_mid.1.max(1.) * self.hr * self.hr).clamp(0.01, 0.9);
        let strong_at = |t: f32| sl.iter().any(|s| (s.0 - t).abs() < 0.05) as usize;
        let mut em = vec![vec![0f32; NS]; n];
        for (i, &(a, l, _)) in sl.iter().enumerate() {
            let e = a + l;
            for s in 0..NS {
                let (r, q) = st(s);
                let mut sc = 0.;
                for x in mel.iter().filter(|x| x.t < e - 0.01 && x.t + x.l > a + 0.01) {
                    let o = (x.t + x.l).min(e) - x.t.max(a);
                    let strong = if x.t >= a - 0.01 { strong_at(x.t) } else { 1 };
                    let iv = ((x.k % 12) + 12 - r) % 12;
                    let w = if strong == 1 { 1.5 } else { 1. };
                    sc += 2.2 * w * o.min(2.) * self.emis[strong][q as usize][iv as usize];
                }
                if !bed.is_empty() {
                    let t = QT[q as usize];
                    let (mut inw, mut tot) = (0., 0.);
                    for x in bed.iter().filter(|x| x.t < e - 0.01 && x.t + x.l > a + 0.01) {
                        let o = (x.t + x.l).min(e) - x.t.max(a);
                        tot += o;
                        if t.iter().any(|&iv| (r + iv) % 12 == x.k % 12) {
                            inw += o;
                        }
                    }
                    if tot > 0. {
                        sc += 6. * ((inw / tot) - 0.85);
                    }
                }
                // réharmonisation : ne pas reprendre l'accord de l'exposé précédent
                let rel = a - sl[0].0;
                if let Some(c) = self.avoid.iter().rev().find(|c| c.t <= rel + 0.01 && c.t + c.l > rel + 0.01) {
                    if (c.r, c.q) == (r, q) {
                        sc -= 4.;
                    } else if c.r == r {
                        sc -= 1.5;
                    }
                }
                if matches!(q, 3 | 4 | 5 | 8) {
                    sc += self.color;
                }
                if matches!(q, 6 | 7) {
                    sc += self.color.min(0.) * 2. - if self.major_bias > 0. { 2. } else { 0. };
                }
                if q == 0 {
                    sc += self.major_bias;
                }
                em[i][s] = sc + 0.35 * self.prior[s];
            }
        }
        // Viterbi
        let mut v = vec![vec![f32::NEG_INFINITY; NS]; n];
        let mut bp = vec![vec![0usize; NS]; n];
        for s in 0..NS {
            let ok = start.is_none_or(|c| st(s) == c);
            v[0][s] = if ok { em[0][s] + self.prior[s] } else { f32::NEG_INFINITY };
        }
        for i in 1..n {
            let pc = if sl[i].2 { pd } else { pm };
            let (stay, chg) = (pc.ln_1p_neg(), pc.ln());
            for b in 0..NS {
                let mut best = (f32::NEG_INFINITY, 0);
                for a in 0..NS {
                    if v[i - 1][a] == f32::NEG_INFINITY {
                        continue;
                    }
                    let c = v[i - 1][a] + if a == b { stay } else { chg + self.trans[a][b] };
                    if c > best.0 {
                        best = (c, a);
                    }
                }
                v[i][b] = best.0 + em[i][b];
                bp[i][b] = best.1;
            }
        }
        let fin = |s: usize| -> bool {
            let (r, q) = st(s);
            match end {
                End::Free => true,
                End::Tonic => r == self.key.tonic && (q == if self.key.maj { 0 } else { 1 } || q == 0),
                End::Is(rr, qq) => r == rr && q == qq,
                End::Root(rr) => r == rr,
            }
        };
        let mut s = (0..NS).filter(|&s| fin(s)).max_by(|&a, &b| v[n - 1][a].partial_cmp(&v[n - 1][b]).unwrap()).unwrap();
        let mut path = vec![0; n];
        for i in (0..n).rev() {
            path[i] = s;
            s = bp[i][s];
        }
        let mut out: Vec<Chord> = vec![];
        for (i, &s) in path.iter().enumerate() {
            let (r, q) = st(s);
            match out.last_mut() {
                Some(c) if (c.r, c.q) == (r, q) => c.l += sl[i].1,
                _ => out.push(Chord { t: sl[i].0, l: sl[i].1, r, q, bass: r }),
            }
        }
        self.bass_line(&mut out, mel);
        out
    }

    /// Basse : fondamentale / renversement / pédale, coûts appris + conduite.
    pub fn bass_line(&self, ch: &mut [Chord], mel: &[N]) {
        let inv: f32 = self.m.inversion.iter().sum();
        let lp = |i: usize| (self.m.inversion[i] / inv).max(0.01).ln();
        let ped = (self.m.pedal.0 / self.m.pedal.1.max(1.)).max(0.01).ln();
        let n = ch.len();
        if n == 0 {
            return;
        }
        // candidats : (classe, coût)
        let cands: Vec<Vec<(u8, f32)>> = (0..n)
            .map(|i| {
                let c = &ch[i];
                let t = QT[c.q as usize];
                let mut v = vec![(c.r, -lp(0)), ((c.r + t[1]) % 12, -lp(1)), ((c.r + t[2]) % 12, -lp(2))];
                // pédale de tonique / dominante possible
                for p in [self.key.tonic, (self.key.tonic + 7) % 12] {
                    if !v.iter().any(|x| x.0 == p) {
                        v.push((p, -lp(3) + 1.5));
                    }
                }
                // la basse ne double pas la note de mélodie au temps fort (sauf fondamentale)
                for x in v.iter_mut() {
                    if mel.iter().any(|m| (m.t - c.t).abs() < 0.05 && m.k % 12 == x.0) && x.0 != c.r {
                        x.1 += 1.5;
                    }
                }
                v
            })
            .collect();
        let mut cost = vec![cands[0].iter().map(|x| x.1).collect::<Vec<f32>>()];
        let mut bp: Vec<Vec<usize>> = vec![vec![0; cands[0].len()]];
        for i in 1..n {
            let mut row = vec![];
            let mut brow = vec![];
            for &(pc, c) in cands[i].iter() {
                let mut best = (f32::MAX, 0);
                for (k, &(pp, _)) in cands[i - 1].iter().enumerate() {
                    let d = ((pc as i32 - pp as i32).rem_euclid(12)).min((pp as i32 - pc as i32).rem_euclid(12));
                    let mut t = match d {
                        0 => -ped,
                        1 | 2 => 0.2,
                        5 => 0.4,
                        3 | 4 => 0.7,
                        _ => 1.2,
                    };
                    // une pédale (note étrangère) doit être préparée
                    if pc != ch[i].r && !ch[i].has(pc) && pc != pp {
                        t += 4.;
                    }
                    let c2 = cost[i - 1][k] + t + c;
                    if c2 < best.0 {
                        best = (c2, k);
                    }
                }
                row.push(best.0);
                brow.push(best.1);
            }
            cost.push(row);
            bp.push(brow);
        }
        let mut j = (0..cands[n - 1].len()).min_by(|&a, &b| cost[n - 1][a].partial_cmp(&cost[n - 1][b]).unwrap()).unwrap();
        // la dernière basse est la fondamentale
        if let Some(r) = cands[n - 1].iter().position(|x| x.0 == ch[n - 1].r) {
            j = r;
        }
        for i in (0..n).rev() {
            ch[i].bass = cands[i][j].0;
            j = bp[i][j];
        }
    }
}

trait Ln1p {
    fn ln_1p_neg(self) -> f32;
}
impl Ln1p for f32 {
    fn ln_1p_neg(self) -> f32 {
        (1. - self).ln()
    }
}

/// Tonalité d'un matériau : corrélation de Krumhansl + appartenance des
/// notes à la gamme (décisif sur un thème court).
pub fn key_of_notes(ns: &[N]) -> Key {
    let mut w = [0f32; 12];
    for n in ns {
        w[(n.k % 12) as usize] += n.l.min(4.);
    }
    let tot: f32 = w.iter().sum::<f32>().max(1e-6);
    let maj = [0u8, 2, 4, 5, 7, 9, 11];
    let min = [0u8, 2, 3, 5, 7, 8, 10, 11];
    let mut best = (Key { tonic: 0, maj: true }, f32::MIN);
    for t in 0..12u8 {
        for (is_maj, sc) in [(true, &maj[..]), (false, &min[..])] {
            let cov: f32 = sc.iter().map(|i| w[((t + i) % 12) as usize]).sum::<f32>() / tot;
            let tonic_w = (w[t as usize] + 0.5 * w[((t + 7) % 12) as usize]) / tot;
            let mut ks = [0f32; 12];
            for i in 0..12 {
                ks[i] = w[i];
            }
            let s = 3. * cov + tonic_w;
            if s > best.1 {
                best = (Key { tonic: t, maj: is_maj }, s);
            }
        }
    }
    best.0
}
