//! Arrangeur : forme, harmonisation, orchestration et réalisation, pilotés par
//! le modèle appris sur Williams. Entrée : thème(s), ostinato(s), accords,
//! basse de l'utilisateur. Sortie : partition orchestrale (palette fixe).

use crate::corpus::{Inst, N};
use crate::harmony::{chord_at, key_of_notes, Chord, End, Harmonizer, Key};
use crate::input::{mono, Input, Kind};
use crate::melody;
use crate::train::{fam, pitched, slots, Bar, Model, NF, R_BASS, R_CTR, R_MEL, R_OST, R_SUS};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    Majestic,
    Lyrical,
    Action,
    Mysterious,
}

pub struct Opts {
    pub mood: Mood,
    pub seconds: f32,
    pub bpm: Option<f32>,
    pub key: Option<Key>,
    pub hr: f32,
    pub modulate: bool,
    pub add_theme: bool,
    pub carrier: Option<Inst>,
}

// ---------------------------------------------------------------- palette
pub struct Desk {
    pub name: &'static str,
    pub inst: Inst,
    pub max: usize,
    pub prog: u8,
    pub perc: Option<u8>,
}

pub const PALETTE: [Desk; 22] = [
    Desk { name: "Piccolo", inst: Inst::Piccolo, max: 1, prog: 72, perc: None },
    Desk { name: "Flutes", inst: Inst::Flute, max: 2, prog: 73, perc: None },
    Desk { name: "Oboes", inst: Inst::Oboe, max: 2, prog: 68, perc: None },
    Desk { name: "Clarinets", inst: Inst::Clarinet, max: 2, prog: 71, perc: None },
    Desk { name: "Bassoons", inst: Inst::Bassoon, max: 2, prog: 70, perc: None },
    Desk { name: "Horns", inst: Inst::Horn, max: 4, prog: 60, perc: None },
    Desk { name: "Trumpets", inst: Inst::Trumpet, max: 3, prog: 56, perc: None },
    Desk { name: "Trombones", inst: Inst::Trombone, max: 3, prog: 57, perc: None },
    Desk { name: "Tuba", inst: Inst::Tuba, max: 1, prog: 58, perc: None },
    Desk { name: "Timpani", inst: Inst::Timpani, max: 2, prog: 47, perc: None },
    Desk { name: "Bass Drum", inst: Inst::Perc, max: 1, prog: 0, perc: Some(35) },
    Desk { name: "Piatti", inst: Inst::Perc, max: 1, prog: 0, perc: Some(49) },
    Desk { name: "Harp", inst: Inst::Harp, max: 8, prog: 46, perc: None },
    Desk { name: "Glockenspiel", inst: Inst::Glock, max: 2, prog: 9, perc: None },
    Desk { name: "Celesta", inst: Inst::Celesta, max: 4, prog: 8, perc: None },
    Desk { name: "Xylophone", inst: Inst::Xylo, max: 2, prog: 13, perc: None },
    Desk { name: "Marimba", inst: Inst::Xylo, max: 4, prog: 12, perc: None },
    Desk { name: "Violins 1", inst: Inst::Violin, max: 1, prog: 48, perc: None },
    Desk { name: "Violins 2", inst: Inst::Violin, max: 1, prog: 48, perc: None },
    Desk { name: "Violas", inst: Inst::Viola, max: 1, prog: 48, perc: None },
    Desk { name: "Cellos", inst: Inst::Cello, max: 1, prog: 48, perc: None },
    Desk { name: "Basses", inst: Inst::Bass, max: 1, prog: 43, perc: None },
];
pub fn desk(name: &str) -> usize {
    PALETTE.iter().position(|d| d.name == name).unwrap()
}

pub struct Score {
    pub parts: Vec<Vec<N>>,
    /// courbe de dynamique par pupitre (temps, valeur 0-127)
    pub cc: Vec<Vec<(f32, f32)>>,
    pub bars: Vec<Bar>,
    pub tempos: Vec<(f32, f32)>,
    pub plan: Vec<String>,
}

// ---------------------------------------------------------------- sections
#[derive(Clone, Copy, PartialEq, Debug)]
enum Sec {
    Intro,
    Statement,
    Extension,
    Coda,
}

struct Section {
    kind: Sec,
    name: String,
    t: f32,
    bars: Vec<Bar>,
    d0: usize,
    d1: usize,
    carrier: Option<usize>,
    mel: Vec<N>,
    chords: Vec<Chord>,
}

fn shift(ns: &[N], dt: f32, dk: i32) -> Vec<N> {
    ns.iter().map(|n| N { t: n.t + dt, l: n.l, k: (n.k as i32 + dk).clamp(0, 127) as u8, v: n.v }).collect()
}

fn bars_from(meters: &[(u8, u8)], t0: f32) -> Vec<Bar> {
    let mut t = t0;
    meters
        .iter()
        .map(|&(num, den)| {
            let len = num as f32 * 4. / den as f32;
            let b = Bar { t, len, num, den };
            t += len;
            b
        })
        .collect()
}

fn end_of(b: &[Bar]) -> f32 {
    b.last().map(|x| x.t + x.len).unwrap_or(0.)
}

/// Décalage d'octave pour qu'une ligne tienne dans un registre appris.
fn fit_octave(ns: &[N], lo: u8, hi: u8, prefer: i32) -> i32 {
    let mut best = (f32::MAX, 0);
    for s in [-24, -12, 0, 12, 24] {
        let out: f32 = ns.iter().map(|n| {
            let k = n.k as i32 + s;
            ((lo as i32 - k).max(0) + (k - hi as i32).max(0)) as f32
        }).sum();
        let med = {
            let mut v: Vec<i32> = ns.iter().map(|n| n.k as i32 + s).collect();
            v.sort();
            v[v.len() / 2]
        };
        let c = out * 3. + ((med - (lo as i32 + hi as i32) / 2).abs() as f32) * 0.5 - if s == prefer { 1. } else { 0. };
        if c < best.0 {
            best = (c, s);
        }
    }
    best.1
}

/// Conduite des voix (notes de l'accord les plus proches, sans croisement).
fn lead(tones: &[u8], prev: &mut [u8], lo: u8, hi: u8, avoid: &[u8]) -> Vec<u8> {
    let n = prev.len();
    let span = (hi - lo) as f32 / n as f32;
    let mut out: Vec<u8> = vec![];
    for i in 0..n {
        let zlo = (hi as f32 - span * (i as f32 + 1.)).max(lo as f32) as u8;
        let zhi = (hi as f32 - span * i as f32).min(hi as f32) as u8 + 2;
        let best = (lo..=hi)
            .filter(|k| tones.contains(&(k % 12)))
            .min_by_key(|&k| {
                let mut c = (k as i32 - prev[i] as i32).abs() * 2;
                if out.iter().any(|o| o % 12 == k % 12) {
                    c += 8;
                }
                if out.last().is_some_and(|&o| k >= o) {
                    c += 40;
                }
                if k < zlo || k > zhi {
                    c += 6;
                }
                if avoid.contains(&(k % 12)) {
                    c += 5;
                }
                c
            })
            .unwrap_or(prev[i]);
        out.push(best);
    }
    prev.copy_from_slice(&out);
    out
}

// ---------------------------------------------------------------- arrangeur
pub struct Arranger<'a> {
    m: &'a Model,
    o: Opts,
    parts: Vec<Vec<N>>,
    cc: Vec<Vec<(f32, f32)>>,
    plan: Vec<String>,
    a_carrier: Option<usize>,
}

const VEL: [u8; 3] = [58, 82, 104];
const CCV: [f32; 3] = [55., 85., 115.];

impl<'a> Arranger<'a> {
    pub fn new(m: &'a Model, o: Opts) -> Self {
        Arranger { m, o, parts: vec![vec![]; PALETTE.len()], cc: vec![vec![]; PALETTE.len()], plan: vec![], a_carrier: None }
    }

    fn put(&mut self, d: &str, t: f32, l: f32, k: u8, v: u8) {
        self.parts[desk(d)].push(N { t, l: l.max(0.1), k, v: v.clamp(1, 127) });
    }

    fn reg(&self, f: usize, role: usize) -> (u8, u8) {
        self.m.reg_range(f, role, 0.1, 0.9)
    }

    /// Sélection apprise des rôles par pupitre pour une section.
    fn roles(&self, carrier: Option<usize>, d: usize) -> Vec<(usize, [f32; 7])> {
        let c = carrier.unwrap_or(NF);
        (0..PALETTE.len())
            .map(|i| {
                let f = fam(PALETTE[i].inst);
                (i, self.m.role_p(c, d, f))
            })
            .collect()
    }

    pub fn run(mut self, inp: &Input) -> Score {
        let m = self.m;
        let theme_src = inp.items.iter().find(|x| x.kind == Kind::Theme).map(|x| mono(&x.notes));
        let osts: Vec<&crate::input::Material> = inp.items.iter().filter(|x| x.kind == Kind::Ostinato).collect();
        let bed: Vec<N> = inp.items.iter().filter(|x| matches!(x.kind, Kind::Chords | Kind::Bass | Kind::Ostinato)).flat_map(|x| x.notes.clone()).collect();
        for it in &inp.items {
            self.plan.push(format!("entrée « {} » : {:?} ({} notes)", it.name, it.kind, it.notes.len()));
        }
        let all: Vec<N> = inp.items.iter().flat_map(|x| x.notes.clone()).collect();
        let key = self.o.key.unwrap_or_else(|| key_of_notes(&all));
        let bpm = self.o.bpm.unwrap_or(inp.bpm);
        self.plan.push(format!("tonalité {}{} · tempo {:.0} · mesure {}/{}", crate::train::PCN[key.tonic as usize], if key.maj { "" } else { "m" }, bpm, inp.bars[0].num, inp.bars[0].den));

        // matériau : nombre de mesures utiles
        let mat_end = all.iter().map(|n| n.t + n.l).fold(0f32, f32::max);
        let last_on = all.iter().map(|n| n.t).fold(0f32, f32::max);
        let mut nb = inp.bars.iter().take_while(|b| b.t <= last_on + 0.01).count().max(1);
        if let Some(b) = inp.bars.get(nb) {
            if mat_end - b.t > b.len / 2. {
                nb += 1;
            }
        }
        let meters: Vec<(u8, u8)> = inp.bars[..nb].iter().map(|b| (b.num, b.den)).collect();
        let m0 = meters[0];
        let bar_sec = |b: &(u8, u8)| b.0 as f32 * 4. / b.1 as f32 * 60. / bpm;
        let stmt_sec: f32 = meters.iter().map(bar_sec).sum();
        let unit = bar_sec(&m0);

        // ---------- forme
        let mood = self.o.mood;
        let (d_intro, d_a, d_b, d_coda) = match mood {
            Mood::Majestic => ((0, 1), 1, 2, 2),
            Mood::Lyrical => ((0, 0), 0, 1, 0),
            Mood::Action => ((1, 2), 2, 2, 2),
            Mood::Mysterious => ((0, 0), 0, 1, 0),
        };
        let n_stmt = if self.o.seconds >= 2.6 * stmt_sec { 2 } else { 1 };
        let rest = (self.o.seconds - n_stmt as f32 * stmt_sec).max(4. * unit);
        let mut n_intro = ((rest * 0.6 / unit) / 2.).round() as usize * 2;
        n_intro = n_intro.clamp(2, 8);
        let n_coda = (((rest / unit) as usize).saturating_sub(n_intro)).clamp(2, 6);
        // mesure isolée apprise (ex. 2/4 dans 4/4) entre dernière exposition et coda
        let iso = {
            let from = format!("{}/{}", m0.0, m0.1);
            let mut best: Option<(String, usize)> = None;
            let mut cnt: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for c in m.meter_change_ctx.iter().filter(|c| c.from == from && c.back_after == 1) {
                *cnt.entry(c.to.clone()).or_default() += 1;
            }
            for (k, v) in cnt {
                if best.as_ref().is_none_or(|b| v > b.1) {
                    best = Some((k, v));
                }
            }
            best.and_then(|(s, _)| {
                let mut it = s.split('/');
                Some((it.next()?.parse::<u8>().ok()?, it.next()?.parse::<u8>().ok()?))
            })
        };

        // carriers appris par nuance, filtrés par le mood
        let carrier_for = |d: usize, sec: usize| -> usize {
            let pref: &[Inst] = match (mood, sec) {
                (Mood::Majestic, 0) | (Mood::Action, 0) => &[Inst::Horn, Inst::Trumpet],
                (Mood::Majestic, _) => &[Inst::Violin, Inst::Trumpet, Inst::Horn],
                (Mood::Action, _) => &[Inst::Trumpet, Inst::Violin, Inst::Horn],
                (Mood::Lyrical, 0) => &[Inst::Oboe, Inst::Flute, Inst::Clarinet, Inst::Violin],
                (Mood::Lyrical, _) => &[Inst::Violin, Inst::Cello, Inst::Horn],
                (Mood::Mysterious, 0) => &[Inst::Clarinet, Inst::Flute, Inst::Horn],
                (Mood::Mysterious, _) => &[Inst::Violin, Inst::Cello, Inst::Horn],
            };
            *pref
                .iter()
                .map(|&i| fam(i))
                .max_by(|&a, &b| {
                    let fa = m.carriers[d].get(&a).copied().unwrap_or(0.) * (1. + 0.5 * (pref[0] == crate::train::FAMS[a]) as i32 as f32);
                    let fb = m.carriers[d].get(&b).copied().unwrap_or(0.) * (1. + 0.5 * (pref[0] == crate::train::FAMS[b]) as i32 as f32);
                    fa.partial_cmp(&fb).unwrap()
                })
                .as_ref()
                .unwrap()
        };

        // thème : celui de l'utilisateur, ou généré au-dessus des ostinatos
        let mut sections: Vec<Section> = vec![];
        let mut t = 0.;
        let intro_bars = bars_from(&vec![m0; n_intro], t);
        t = end_of(&intro_bars);
        sections.push(Section { kind: Sec::Intro, name: "Intro".into(), t: 0., bars: intro_bars, d0: d_intro.0, d1: d_intro.1, carrier: None, mel: vec![], chords: vec![] });
        let mut theme: Option<Vec<N>> = theme_src.clone();
        for s in 0..n_stmt {
            let bars = bars_from(&meters, t);
            let d = if s == 0 { d_a } else { d_b };
            let c = if s == 0 { self.o.carrier.map(fam).unwrap_or_else(|| carrier_for(d, 0)) } else { carrier_for(d, 1) };
            let tr = if s == 1 && self.o.modulate { self.modulation() } else { 0 };
            let key_s = Key { tonic: ((key.tonic as i32 + tr).rem_euclid(12)) as u8, maj: key.maj };
            let mut h = Harmonizer::new(m, key_s);
            h.hr = self.o.hr * if s == 0 { 0.85 } else { 1.1 };
            let bed_s = shift(&bed, t, tr);
            let mel = match &theme {
                Some(th) => shift(th, t, tr),
                None => vec![],
            };
            let chords = h.harmonize(&bars, &mel, &bed_s, None, End::Free);
            let mel = if mel.is_empty() || (self.o.add_theme && s == 1 && theme_src.is_some()) {
                // thème généré (sur ostinato, ou nouveau thème par-dessus)
                let (lo, hi) = self.reg(c, R_MEL);
                let generated = melody::theme(m, c, d, &bars, &chords, key_s, lo.max(55), hi.min(81), if mood == Mood::Action { 6 } else { 4 });
                if theme.is_none() {
                    theme = Some(shift(&generated, -t, -tr));
                }
                generated
            } else {
                mel
            };
            let nm = if s == 0 { "A" } else { "B" };
            sections.push(Section { kind: Sec::Statement, name: format!("Thème {}", nm), t, bars, d0: d, d1: d, carrier: Some(c), mel, chords });
            t = end_of(&sections.last().unwrap().bars);
        }
        if let Some(iso) = iso {
            if mood != Mood::Lyrical && mood != Mood::Mysterious {
                let bars = bars_from(&[iso], t);
                t = end_of(&bars);
                sections.push(Section { kind: Sec::Extension, name: format!("Extension {}/{}", iso.0, iso.1), t: bars[0].t, bars, d0: d_b, d1: d_coda, carrier: None, mel: vec![], chords: vec![] });
            }
        }
        let coda_bars = bars_from(&vec![m0; n_coda], t);
        sections.push(Section { kind: Sec::Coda, name: "Coda".into(), t, bars: coda_bars, d0: d_coda, d1: d_coda, carrier: None, mel: vec![], chords: vec![] });

        // harmonie de l'intro, de l'extension et de la coda
        let hz = Harmonizer::new(m, key);
        let dom = (key.tonic + 7) % 12;
        let tonic_q = if key.maj { 0 } else { 1 };
        let first_a = sections[1].chords.first().map(|c| (c.r, c.q));
        for i in 0..sections.len() {
            let s = &sections[i];
            match s.kind {
                Sec::Intro => {
                    let mut h = Harmonizer::new(m, key);
                    h.hr = self.o.hr * 0.6;
                    let half = s.bars.len() / 2;
                    // pédale de tonique sur la première moitié
                    let ped: Vec<N> = s.bars[..half.max(1)].iter().map(|b| N { t: b.t, l: b.len, k: 40 + key.tonic, v: 80 }).collect();
                    let motif: Vec<N> = theme.as_ref().map(|th| head(th, 2.min(meters.len()), &inp.bars)).unwrap_or_default();
                    let mot = shift(&motif, s.bars[half.min(s.bars.len() - 1)].t, 0);
                    let end = match first_a {
                        Some((r, _)) if r == key.tonic => End::Root(dom),
                        _ => End::Root(dom),
                    };
                    let ch = h.harmonize(&s.bars, &mot, &ped, Some((key.tonic, tonic_q)), end);
                    sections[i].chords = ch;
                    sections[i].mel = mot;
                }
                Sec::Extension => {
                    let last = sections[i - 1].chords.last().copied().unwrap();
                    let b = &sections[i].bars[0];
                    sections[i].chords = vec![Chord { t: b.t, l: b.len, ..last }];
                }
                Sec::Coda => {
                    let s = &sections[i];
                    let nbar = s.bars.len();
                    // cadence apprise vers la tonique (majeure pour un final majestueux)
                    let end = if mood == Mood::Majestic || mood == Mood::Action { End::Is(key.tonic, 0) } else { End::Tonic };
                    let mut h = Harmonizer::new(m, key);
                    h.hr = self.o.hr * 1.2;
                    let cad_bars: Vec<Bar> = s.bars[..nbar.saturating_sub(1).max(1)].iter().map(|b| Bar { ..*b }).collect();
                    let motif: Vec<N> = theme.as_ref().map(|th| augment(&head(th, 1, &inp.bars), 2.)).unwrap_or_default();
                    let mot = shift(&motif, s.t, 0);
                    let mut ch = h.harmonize(&cad_bars, &mot, &[], None, end);
                    let lb = s.bars.last().unwrap();
                    let fin = Chord { t: lb.t, l: lb.len, r: key.tonic, q: if end == End::Tonic { tonic_q } else { 0 }, bass: key.tonic };
                    if nbar > 1 {
                        ch.push(fin);
                    } else {
                        ch = vec![fin];
                    }
                    sections[i].chords = ch;
                    sections[i].mel = mot;
                }
                Sec::Statement => {}
            }
        }
        let _ = hz;

        // ---------- plan lisible
        for s in &sections {
            let chs: Vec<String> = s.chords.iter().map(|c| {
                let mut x = format!("{}:{}", c.t - s.t, crate::train::chname(c.r, c.q));
                if c.bass != c.r {
                    x += &format!("/{}", crate::train::PCN[c.bass as usize]);
                }
                x
            }).collect();
            self.plan.push(format!(
                "{} — {} mes. ({}), {}{} : {}",
                s.name,
                s.bars.len(),
                s.bars.iter().map(|b| format!("{}/{}", b.num, b.den)).collect::<Vec<_>>().join(" "),
                crate::train::DYN[s.d0],
                s.carrier.map(|c| format!(", thème aux {:?}", crate::train::FAMS[c])).unwrap_or_default(),
                chs.join(" | ")
            ));
        }

        // ---------- réalisation
        self.a_carrier = sections.iter().find(|s| s.kind == Sec::Statement).and_then(|s| s.carrier);
        let ost_mat: Vec<Vec<N>> = osts.iter().map(|o| o.notes.clone()).collect();
        for i in 0..sections.len() {
            let next_d = sections.get(i + 1).map(|s| s.d0).unwrap_or(0);
            self.realize(&sections[i], next_d, key, &ost_mat, &meters);
        }

        // tempo : ritenuto sur les deux dernières mesures
        let total = end_of(&sections.last().unwrap().bars);
        let lb = &sections.last().unwrap().bars;
        let mut tempos = vec![(0., bpm)];
        if lb.len() >= 2 {
            tempos.push((lb[lb.len() - 2].t, bpm * 0.92));
            tempos.push((lb[lb.len() - 1].t, bpm * 0.8));
        }
        let mut bars: Vec<Bar> = vec![];
        for s in &sections {
            bars.extend(s.bars.iter().map(|b| Bar { ..*b }));
        }
        let _ = total;
        self.enforce();
        {
            let all_ch: Vec<Chord> = sections.iter().flat_map(|s| s.chords.clone()).collect();
            let mut bad = 0;
            let mut tot = 0;
            for (i, ns) in self.parts.iter().enumerate() {
                if PALETTE[i].perc.is_some() || PALETTE[i].inst == Inst::Timpani {
                    continue;
                }
                for n in ns {
                    if all_ch.iter().any(|c| (c.t - n.t).abs() < 0.02) {
                        tot += 1;
                        if !chord_at(&all_ch, n.t).has(n.k) {
                            bad += 1;
                        }
                    }
                }
            }
            self.plan.push(format!("contrôle : {} notes étrangères à l'accord sur {} attaques aux changements d'accord", bad, tot));
        }
        Score { parts: self.parts, cc: self.cc, bars, tempos, plan: self.plan }
    }

    fn modulation(&self) -> i32 {
        // modulation ascendante la plus fréquente quand la nuance monte
        let mut cnt: std::collections::HashMap<i8, usize> = std::collections::HashMap::new();
        for c in self.m.modulation_ctx.iter().filter(|c| c.1 > 0 && c.3 > c.2) {
            *cnt.entry(c.1).or_default() += 1;
        }
        cnt.into_iter().max_by_key(|x| x.1).map(|x| x.0 as i32).unwrap_or(2)
    }

    /// Réalise une section : rôles appris → notes.
    fn realize(&mut self, s: &Section, next_d: usize, key: Key, osts: &[Vec<N>], meters: &[(u8, u8)]) {
        let m = self.m;
        let d = s.d0;
        let (a, e) = (s.t, end_of(&s.bars));
        let roles = self.roles(s.carrier, d);
        let p = |name: &str| roles[desk(name)].1;
        let vel = VEL[d];
        // dynamique (CC1) : niveau de section + crescendo vers la suivante
        let curve = |x: f32| -> f32 {
            let base = CCV[s.d0] + (CCV[s.d1] - CCV[s.d0]) * ((x - a) / (e - a)).clamp(0., 1.);
            let last = s.bars.last().map(|b| b.t).unwrap_or(e);
            if next_d > s.d1 && x > last {
                base + (CCV[next_d] - base) * ((x - last) / (e - last)).clamp(0., 1.)
            } else {
                base
            }
        };
        let mut x = a;
        while x < e - 0.01 {
            for i in 0..PALETTE.len() {
                self.cc[i].push((x, curve(x)));
            }
            x += 0.5;
        }
        let mut lines = vec![];
        let ch = &s.chords;
        let melk = |t: f32| -> Vec<u8> { s.mel.iter().filter(|n| n.t <= t + 0.05 && n.t + n.l > t + 0.05).map(|n| n.k % 12).collect() };

        // ----- affectation des rôles : nombre de pupitres par rôle = espérance
        // apprise (somme des probabilités Williams), puis meilleurs candidats.
        let kmul = match (s.kind, d) {
            (Sec::Statement, 2) | (Sec::Coda, _) | (Sec::Extension, _) => 1.35,
            (Sec::Intro, _) => 0.9,
            _ => 1.0,
        };
        let carrier_desks: Vec<usize> = match (s.kind, s.carrier.map(|c| crate::train::FAMS[c])) {
            (Sec::Statement, Some(Inst::Violin)) => vec![desk("Violins 1"), desk("Violins 2")],
            (Sec::Statement, Some(i)) => vec![PALETTE.iter().position(|x| x.inst == i).unwrap()],
            _ => vec![],
        };
        let eligible = |i: usize, r: usize| -> bool {
            let dk = &PALETTE[i];
            if carrier_desks.contains(&i) || dk.name == "Marimba" || matches!(dk.inst, Inst::Perc | Inst::Timpani) {
                return false;
            }
            match dk.inst {
                Inst::Glock | Inst::Xylo | Inst::Celesta => r == R_MEL && s.kind == Sec::Statement,
                Inst::Harp => r == R_OST,
                Inst::Bass => r == R_BASS,
                Inst::Piccolo => r == R_MEL || r == R_SUS,
                // basse : instruments graves seulement ; pas de contre-chant au tuba
                Inst::Tuba => r == R_BASS || r == R_SUS,
                Inst::Violin | Inst::Viola | Inst::Flute | Inst::Oboe | Inst::Clarinet | Inst::Horn | Inst::Trumpet => r != R_BASS && (r != R_MEL || s.kind == Sec::Statement),
                _ => r != R_MEL || s.kind == Sec::Statement,
            }
        };
        let rlist = [R_MEL, R_BASS, R_CTR, R_OST, R_SUS];
        let mut quota = [0f32; 7];
        for &r in &rlist {
            quota[r] = kmul * (0..PALETTE.len()).filter(|&i| eligible(i, r)).map(|i| roles[i].1[r]).sum::<f32>();
        }
        quota[R_CTR] = quota[R_CTR].min(if s.kind == Sec::Statement { 2. } else { 0. });
        quota[R_OST] = quota[R_OST].min(2.);
        if d < 2 {
            quota[R_MEL] = quota[R_MEL].min(1.);
        }
        if std::env::var("WILLIZE_DEBUG").is_ok() {
            eprintln!("{} quotas mel {:.1} bass {:.1} ctr {:.1} ost {:.1} sus {:.1}", s.name, quota[R_MEL], quota[R_BASS], quota[R_CTR], quota[R_OST], quota[R_SUS]);
            for i in 0..PALETTE.len() {
                let r = roles[i].1;
                eprintln!("   {:<12} rest {:.2} mel {:.2} bass {:.2} sus {:.2} ost {:.2} ctr {:.2}", PALETTE[i].name, r[0], r[1], r[2], r[3], r[4], r[5]);
            }
        }
        let mut role_of: Vec<Option<usize>> = vec![None; PALETTE.len()];
        role_of[desk("Basses")] = Some(R_BASS);
        quota[R_BASS] -= 1.;
        let mut pairs: Vec<(usize, usize, f32)> = vec![];
        for i in 0..PALETTE.len() {
            for &r in &rlist {
                if eligible(i, r) && role_of[i].is_none() {
                    pairs.push((i, r, roles[i].1[r]));
                }
            }
        }
        pairs.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap());
        for (i, r, pr) in pairs {
            if role_of[i].is_some() || quota[r] < 0.5 || pr < 0.02 {
                continue;
            }
            // cuivres graves : pas de tenue en nuance piano
            if d == 0 && r == R_SUS && matches!(PALETTE[i].inst, Inst::Trumpet | Inst::Trombone | Inst::Tuba) {
                continue;
            }
            if r == R_OST && d < 2 && matches!(PALETTE[i].inst, Inst::Trumpet | Inst::Trombone | Inst::Tuba) {
                continue;
            }
            if r == R_MEL && d < 2 && s.carrier.is_some_and(|c| matches!(crate::train::FAMS[c], Inst::Horn | Inst::Trumpet | Inst::Trombone)) && matches!(PALETTE[i].inst, Inst::Trumpet | Inst::Trombone | Inst::Horn) {
                continue;
            }
            role_of[i] = Some(r);
            quota[r] -= 1.;
        }
        // les cordes ne se taisent pas (sauf mystère / ouverture piano) : rôle le plus probable restant
        if d >= 1 || s.kind == Sec::Statement {
            for name in ["Violins 1", "Violins 2", "Violas", "Cellos"] {
                let i = desk(name);
                if role_of[i].is_none() && !carrier_desks.contains(&i) {
                    let r = roles[i].1;
                    let best = [R_SUS, R_OST, R_BASS].into_iter().filter(|&x| eligible(i, x)).max_by(|&x, &y| r[x].partial_cmp(&r[y]).unwrap()).unwrap();
                    role_of[i] = Some(best);
                }
            }
        }

        // ----- mélodie et doublures
        match s.kind {
            Sec::Statement => {
                let c = s.carrier.unwrap();
                let (lo, hi) = m.reg_range(c, R_MEL, 0.05, 0.95);
                let sh = fit_octave(&s.mel, lo, hi, 0);
                let mel: Vec<N> = shift(&s.mel, 0., sh);
                let cd = carrier_desks[0];
                for n in &mel {
                    self.put(PALETTE[cd].name, n.t, n.l + 0.06, n.k, vel + 10);
                }
                lines.push(format!("thème : {} ({:+})", PALETTE[cd].name, sh));
                if carrier_desks.len() > 1 {
                    // violons en octaves
                    let (l2, _) = m.reg_range(fam(Inst::Violin), R_MEL, 0.03, 0.97);
                    let o = if mel.iter().all(|n| n.k >= l2 + 12) { -12 } else { 0 };
                    for n in &mel {
                        self.put("Violins 2", n.t, n.l + 0.06, (n.k as i32 + o) as u8, vel + 6);
                    }
                    lines.push(format!("Violins 2 {:+}", o));
                }
                for i in with(&role_of, R_MEL) {
                    let f = fam(PALETTE[i].inst);
                    let (lo, hi) = m.reg_range(f, R_MEL, 0.03, 0.97);
                    let oct = m.dbl_oct.get(&(c, f)).copied().unwrap_or([0., 0., 1., 0., 0.]);
                    let mut order: Vec<(i32, f32)> = oct.iter().enumerate().map(|(j, w)| ((j as i32 - 2) * 12, *w)).collect();
                    order.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
                    let fit = order.iter().find(|(o, w)| *w > 0. && mel.iter().all(|n| (lo as i32..=hi as i32).contains(&(n.k as i32 + o))));
                    let Some(&(o, _)) = fit else {
                        role_of[i] = Some(R_SUS);
                        continue;
                    };
                    let short = matches!(PALETTE[i].inst, Inst::Glock | Inst::Xylo | Inst::Celesta);
                    for n in &mel {
                        let l = if short { 1.0f32.min(n.l) } else { n.l };
                        self.put(PALETTE[i].name, n.t, l, (n.k as i32 + o) as u8, vel.saturating_sub(4));
                    }
                    lines.push(format!("doublure : {} {:+}", PALETTE[i].name, o));
                }
            }
            Sec::Intro | Sec::Coda => {
                if !s.mel.is_empty() {
                    let name = if s.kind == Sec::Intro {
                        // instrument d'ouverture appris, autre que le porteur du thème A
                        let mut v: Vec<(usize, f32)> = (0..NF).map(|f| (f, 0.)).collect();
                        for o in &m.openings {
                            for &f in &o.3 {
                                v[f].1 += 1.;
                            }
                        }
                        v.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
                        v.iter()
                            .filter(|(f, _)| Some(*f) != self.a_carrier)
                            .find_map(|(f, _)| match crate::train::FAMS[*f] {
                                Inst::Oboe => Some("Oboes"),
                                Inst::Clarinet => Some("Clarinets"),
                                Inst::Flute => Some("Flutes"),
                                Inst::Horn => Some("Horns"),
                                _ => None,
                            })
                            .unwrap_or("Oboes")
                    } else {
                        "Trumpets"
                    };
                    let i = desk(name);
                    let f = fam(PALETTE[i].inst);
                    let (lo, hi) = m.reg_range(f, R_MEL, 0.05, 0.95);
                    let sh = fit_octave(&s.mel, lo, hi, 0);
                    for n in &s.mel {
                        self.put(name, n.t, n.l + 0.05, (n.k as i32 + sh) as u8, vel + 6);
                    }
                    if s.kind == Sec::Coda {
                        let (hl, hh) = m.reg_range(fam(Inst::Horn), R_MEL, 0.05, 0.95);
                        let hs = fit_octave(&s.mel, hl, hh, -12);
                        for n in &s.mel {
                            self.put("Horns", n.t, n.l + 0.05, (n.k as i32 + hs) as u8, vel + 4);
                        }
                        role_of[desk("Horns")] = None;
                    }
                    role_of[i] = None;
                    lines.push(format!("motif : {}", name));
                }
            }
            Sec::Extension => {}
        }

        // ----- basse
        let bass_desks: Vec<usize> = with(&role_of, R_BASS);
        for &i in &bass_desks {
            let name = PALETTE[i].name;
            let f = fam(PALETTE[i].inst);
            let (lo, hi) = self.reg(f, R_BASS);
            let rh = m.top_rhythms(f, R_BASS, d, 6);
            let mask = rh.iter().map(|r| r.0).find(|mk| mk & 1 == 1 && mk.count_ones() <= if d == 2 { 4 } else { 2 }).unwrap_or(1);
            let mut prev = (lo as i32 + hi as i32) / 2;
            for c in ch {
                let mut ons: Vec<f32> = vec![];
                let mut t = c.t;
                while t < c.t + c.l - 0.01 {
                    let bar = s.bars.iter().rev().find(|b| b.t <= t + 0.01).unwrap();
                    let bend = (bar.t + bar.len).min(c.t + c.l);
                    if (bar.num, bar.den) == (4, 4) {
                        for j in 0..16 {
                            let x = bar.t + j as f32 * 0.25;
                            if mask & (1 << j) != 0 && x >= t - 0.01 && x < bend - 0.01 {
                                ons.push(x);
                            }
                        }
                    }
                    ons.push(t);
                    t = bend;
                }
                ons.sort_by(|x, y| x.partial_cmp(y).unwrap());
                ons.dedup_by(|x, y| (*x - *y).abs() < 0.01);
                let k = (lo as i32..=hi as i32).filter(|k| (k % 12) as u8 == c.bass).min_by_key(|k| (k - prev).abs()).unwrap_or(prev);
                prev = k;
                for (j, o) in ons.iter().enumerate() {
                    let nx = ons.get(j + 1).copied().unwrap_or(c.t + c.l);
                    self.put(name, *o, nx - o, k as u8, vel);
                }
            }
        }
        lines.push(format!("basse : {}", bass_desks.iter().map(|&i| PALETTE[i].name).collect::<Vec<_>>().join(", ")));

        // ----- contre-chants
        for i in with(&role_of, R_CTR) {
            let f = fam(PALETTE[i].inst);
            let (lo, hi) = self.reg(f, R_CTR);
            let mel_med = s.mel.iter().map(|n| n.k as f32).sum::<f32>() / s.mel.len().max(1) as f32;
            let below = ((lo as f32 + hi as f32) / 2.) < mel_med + 3.;
            let line = melody::counter(m, f, d, &s.bars, ch, &s.mel, lo, hi, below);
            for n in &line {
                self.put(PALETTE[i].name, n.t, n.l + 0.04, n.k, vel);
            }
            lines.push(format!("contre-chant : {}", PALETTE[i].name));
        }

        // ----- ostinato : celui de l'utilisateur (pupitre dont la tessiture le contient), sinon figure apprise
        let mut ost_desks = with(&role_of, R_OST);
        ost_desks.retain(|&i| PALETTE[i].inst != Inst::Harp);
        if !osts.is_empty() && s.kind != Sec::Extension {
            for o in osts {
                let med = { let mut v: Vec<u8> = o.iter().map(|n| n.k).collect(); v.sort(); v[v.len() / 2] };
                let pool: Vec<usize> = (0..PALETTE.len()).filter(|&i| ost_desks.contains(&i) || role_of[i] == Some(R_SUS)).collect();
                let Some(&i) = pool.iter().chain([desk("Violas"), desk("Violins 2"), desk("Harp")].iter()).find(|&&i| {
                    let (lo, hi) = m.reg_range(fam(PALETTE[i].inst), R_OST, 0.05, 0.95);
                    (lo..=hi).contains(&med)
                }) else { continue };
                let len = o.iter().map(|n| n.t + n.l).fold(0f32, f32::max);
                let bl: f32 = meters.first().map(|m| m.0 as f32 * 4. / m.1 as f32).unwrap_or(4.);
                let plen = ((len / bl).ceil() * bl).max(bl);
                let mut t = a;
                while t < e - 0.01 {
                    for n in o.iter().filter(|n| n.t < plen) {
                        if t + n.t < e - 0.01 {
                            self.put(PALETTE[i].name, t + n.t, n.l.min(e - t - n.t), n.k, vel.saturating_sub(6));
                        }
                    }
                    t += plen;
                }
                role_of[i] = Some(R_OST);
                ost_desks.retain(|&x| x != i);
                lines.push(format!("ostinato (utilisateur) : {}", PALETTE[i].name));
            }
        }
        if s.kind != Sec::Extension {
            for &i in &ost_desks {
                let f = fam(PALETTE[i].inst);
                let (lo, hi) = self.reg(f, R_OST);
                let rh = m.top_rhythms(f, R_OST, d, 8);
                let mask = rh.iter().map(|r| r.0).find(|mk| mk & 1 == 1 && mk.count_ones() >= 4 && mk.count_ones() <= 8).unwrap_or(0x5555);
                let mut prev = [((lo as u16 + hi as u16) / 2) as u8, ((lo as u16 + hi as u16) / 2 - 4) as u8];
                for b in &s.bars {
                    let steps: Vec<f32> = if (b.num, b.den) == (4, 4) {
                        (0..16).filter(|x| mask & (1 << x) != 0).map(|x| x as f32 * 0.25).collect()
                    } else {
                        (0..(b.len * 2.) as usize).map(|x| x as f32 * 0.5).collect()
                    };
                    let mut cur: Option<Chord> = None;
                    let mut pool = vec![];
                    for (si, st) in steps.iter().enumerate() {
                        let t = b.t + st;
                        let cc = *chord_at(ch, t);
                        if cur != Some(cc) {
                            pool = lead(&cc.tones(), &mut prev, lo, hi, &[]);
                            cur = Some(cc);
                        }
                        let k = pool[si % pool.len()];
                        let nx = steps.get(si + 1).map(|x| b.t + x).unwrap_or(b.t + b.len);
                        let acc = if st.fract() == 0. && (*st as i32) % 2 == 0 { 8 } else { 0 };
                        self.put(PALETTE[i].name, t, (nx - t) * 0.9, k, vel.saturating_sub(12) + acc);
                    }
                }
                lines.push(format!("ostinato : {}", PALETTE[i].name));
            }
        }

        // ----- tenues : voix menées dans la tessiture apprise
        let mut sus_names = vec![];
        for i in with(&role_of, R_SUS) {
            let f = fam(PALETTE[i].inst);
            let (lo, hi) = self.reg(f, R_SUS);
            let po = m.poly.get(&(f, R_SUS)).map(|x| x.0 / x.1).unwrap_or(1.5);
            let mut nv = (po.round() as usize).clamp(1, PALETTE[i].max);
            if matches!(PALETTE[i].inst, Inst::Horn | Inst::Trombone | Inst::Trumpet) {
                nv = PALETTE[i].max.min(if d == 0 { 2 } else { PALETTE[i].max });
            }
            let mut prev: Vec<u8> = (0..nv).map(|j| hi.saturating_sub(2 + 4 * j as u8)).collect();
            for c in ch {
                let v = lead(&c.tones(), &mut prev, lo, hi, &melk(c.t));
                for k in v {
                    self.put(PALETTE[i].name, c.t, c.l + 0.03, k, vel.saturating_sub(10));
                }
            }
            sus_names.push(PALETTE[i].name);
        }
        lines.push(format!("tenues : {}", sus_names.join(", ")));


        // ----- harpe : arpèges (figure apprise) ou glissando vers une section plus forte
        if role_of[desk("Harp")].is_some() || s.kind == Sec::Intro {
            let rh = m.top_rhythms(fam(Inst::Harp), R_OST, d, 6);
            let mask = rh.iter().map(|r| r.0).find(|mk| mk & 1 == 1).unwrap_or(0x5555);
            for b in &s.bars {
                let c = *chord_at(ch, b.t);
                let tones: Vec<u8> = (36..90).filter(|k| c.has(*k)).filter(|k| *k >= 36 + c.bass.min(11)).collect();
                let steps: Vec<f32> = if (b.num, b.den) == (4, 4) { (0..16).filter(|x| mask & (1 << x) != 0).map(|x| x as f32 * 0.25).collect() } else { (0..(b.len * 2.) as usize).map(|x| x as f32 * 0.5).collect() };
                let base = tones.iter().position(|&k| k % 12 == c.bass).unwrap_or(0);
                for (j, st) in steps.iter().enumerate() {
                    let cc = chord_at(ch, b.t + st);
                    let k = tones.get(base + j).copied().unwrap_or(tones[tones.len() - 1]);
                    let k = if cc.has(k) { k } else { (k.saturating_sub(2)..=k + 2).find(|x| cc.has(*x)).unwrap_or(k) };
                    self.put("Harp", b.t + st, 1.5, k, vel.saturating_sub(15));
                }
            }
            lines.push("harpe : arpèges".into());
        }
        if next_d > s.d1 {
            let lb = s.bars.last().unwrap();
            let c = *chord_at(ch, lb.t);
            let tones: Vec<u8> = (43..86).filter(|k| c.has(*k) || (*k % 12 + 12 - c.r) % 12 == 2).collect();
            let n = tones.len().min(16);
            for (j, k) in tones.iter().take(n).enumerate() {
                self.put("Harp", lb.t + lb.len - 1.2 + j as f32 * 1.2 / n as f32, 1.2, *k, 60 + j as u8 * 2);
            }
        }

        // ----- claviers : célesta en ouverture piano (scintillement), glock à l'apogée
        if s.kind == Sec::Intro && d == 0 && p("Celesta")[R_OST] + p("Celesta")[R_MEL] + p("Celesta")[R_SUS] > 0.0 {
            for b in s.bars.iter().take((s.bars.len() / 2).max(1)) {
                let c = *chord_at(ch, b.t);
                let tones: Vec<u8> = (76..96).filter(|k| c.has(*k)).collect();
                let pat = [0usize, 2, 1, 2];
                let mut j = 0;
                let mut t = b.t;
                while t < b.t + b.len - 0.01 {
                    self.put("Celesta", t, 0.5, tones[pat[j % 4].min(tones.len() - 1)], 45);
                    j += 1;
                    t += 0.5;
                }
            }
            lines.push("célesta : scintillement".into());
        }

        // ----- timbales et percussions (probabilités apprises)
        let tk = |r: u8| -> u8 { (38..=50).find(|k| k % 12 == r).unwrap_or(40) };
        let roll_p = m.timp_roll[d].0 / m.timp_roll[d].1.max(1.);
        if d >= 1 || s.kind == Sec::Intro {
            for c in ch {
                if (c.r == key.tonic || c.r == (key.tonic + 7) % 12) && s.bars.iter().any(|b| (b.t - c.t).abs() < 0.01) && d >= 1 {
                    self.put("Timpani", c.t, 1., tk(c.r), vel);
                }
            }
        }
        let roll = |me: &mut Self, t: f32, l: f32, k: u8, v0: u8, v1: u8| {
            let steps = (l * 6.) as usize;
            for j in 0..steps {
                let v = v0 as f32 + (v1 as f32 - v0 as f32) * j as f32 / steps as f32;
                me.put("Timpani", t + j as f32 / 6., 0.16, k, v as u8 - (j % 2) as u8 * 5);
            }
        };
        if s.kind == Sec::Intro {
            let b0 = &s.bars[0];
            roll(self, b0.t, b0.len * (s.bars.len() as f32 / 2.).max(1.), tk(key.tonic), 25, 45);
        }
        if next_d > s.d1 || (roll_p > 0.03 && s.kind == Sec::Extension) {
            let lb = s.bars.last().unwrap();
            let c = *chord_at(ch, lb.t);
            roll(self, lb.t, lb.len, tk(c.r), VEL[d].saturating_sub(20), VEL[next_d.max(d)] + 10);
        }
        let head_crash = m.crash[d][1].0 / m.crash[d][1].1.max(1.);
        if d == 2 && head_crash > 0.05 && s.kind == Sec::Statement {
            for (bi, b) in s.bars.iter().enumerate() {
                if bi % 4 == 0 && (bi == 0 || bi == s.bars.len() / 2) {
                    self.put("Piatti", b.t, 3., 49, vel);
                    self.put("Bass Drum", b.t, 2., 35, vel);
                }
            }
            lines.push("piatti + grosse caisse : têtes de phrase".into());
        }

        // ----- coda : accord final tutti
        if s.kind == Sec::Coda {
            let lb = s.bars.last().unwrap();
            let c = *chord_at(ch, lb.t);
            let l = lb.len;
            for i in 0..PALETTE.len() {
                let dk = &PALETTE[i];
                let f = fam(dk.inst);
                if !pitched(f) || matches!(dk.inst, Inst::Xylo | Inst::Celesta | Inst::Harp) {
                    continue;
                }
                self.parts[i].retain(|n| n.t < lb.t - 0.01 || n.t >= lb.t + l);
                for n in self.parts[i].iter_mut() {
                    if n.t < lb.t && n.t + n.l > lb.t {
                        n.l = lb.t - n.t;
                    }
                }
                let (lo, hi) = self.m.reg_range(f, R_SUS, 0.15, 0.85);
                let nv = match dk.inst {
                    Inst::Horn => 4,
                    Inst::Trumpet | Inst::Trombone => 3,
                    Inst::Glock => 1,
                    _ => dk.max.min(2),
                };
                let mut prev: Vec<u8> = (0..nv).map(|j| hi.saturating_sub(3 * j as u8)).collect();
                let tones = if dk.inst == Inst::Bass || dk.inst == Inst::Tuba { vec![c.r] } else { c.tones() };
                let v = lead(&tones, &mut prev, lo, hi, &[]);
                for k in v {
                    let ll = if dk.inst == Inst::Glock { 2. } else { l };
                    self.put(dk.name, lb.t, ll, k, vel + 12);
                }
            }
            roll(self, lb.t, l - 0.5, tk(key.tonic), 80, 120);
            self.put("Timpani", lb.t + l - 0.5, 1., tk(key.tonic), 127);
            self.put("Piatti", lb.t, 4., 49, 120);
            self.put("Bass Drum", lb.t, 3., 35, 120);
            self.put("Bass Drum", lb.t + l - 0.5, 2., 35, 120);
            lines.push("accord final tutti".into());
        }

        self.plan.push(format!("  {} : {}", s.name, lines.join(" · ")));
    }

    /// Respecte le nombre de voix maximal de chaque pupitre.
    fn enforce(&mut self) {
        for (i, dk) in PALETTE.iter().enumerate() {
            let ns = &mut self.parts[i];
            ns.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap().then(b.v.cmp(&a.v)));
            // unissons identiques fusionnés
            ns.dedup_by(|b, a| (a.t - b.t).abs() < 0.02 && a.k == b.k);
            let mut keep: Vec<N> = vec![];
            for n in ns.iter() {
                let active = keep.iter().filter(|o| o.t + o.l > n.t + 0.12 && o.t < n.t + 0.01).count();
                if active < dk.max {
                    keep.push(*n);
                } else if dk.max == 1 {
                    // monophonique : la note précédente s'arrête (legato court)
                    if let Some(o) = keep.iter_mut().rev().find(|o| o.t + o.l > n.t) {
                        o.l = (n.t - o.t + 0.06).max(0.1);
                    }
                    keep.push(*n);
                }
            }
            *ns = keep;
        }
    }
}

fn with(role_of: &[Option<usize>], r: usize) -> Vec<usize> {
    (0..role_of.len()).filter(|&i| role_of[i] == Some(r)).collect()
}

/// Tête du thème : notes des `nb` premières mesures.
fn head(th: &[N], nb: usize, bars: &[Bar]) -> Vec<N> {
    let end = bars.get(nb).map(|b| b.t).unwrap_or(f32::MAX);
    let mut v: Vec<N> = th.iter().filter(|n| n.t < end - 0.01).copied().collect();
    if let Some(l) = v.last_mut() {
        l.l = l.l.max(end - l.t).min(4.);
    }
    v
}

fn augment(ns: &[N], f: f32) -> Vec<N> {
    let t0 = ns.first().map(|n| n.t).unwrap_or(0.);
    ns.iter().map(|n| N { t: (n.t - t0) * f, l: n.l * f, k: n.k, v: n.v }).collect()
}

#[allow(dead_code)]
fn _unused(_: &[Bar]) -> Vec<f32> {
    slots(4, 4)
}
