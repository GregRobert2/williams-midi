//! "WILLIZE" — 1 mn, thème des 6 horns, écriture à la Williams.
//! Intro (6 mes.) → Thème A aux cors (8) → Thème B tutti (8) → Coda bVI-bVII-I.

use crate::midi::{Ev, Note, Song, Track};

const PPQ: u32 = 480;

/// Thème extrait de la piste "6horns leg" (temps relatifs, durée, note).
const THEME: [(f32, f32, u8); 12] = [
    (0., 2., 64), (2., 2., 69), (4., 3., 66), (7., 1., 62), (8., 2., 62), (10., 6., 59),
    (16., 2., 64), (18., 2., 71), (20., 2., 69), (22., 2., 64), (24., 4., 72), (28., 4., 66),
];

fn chord(n: &str) -> Vec<u8> {
    match n {
        "Em" => vec![4, 7, 11],
        "A" => vec![9, 1, 4],
        "D" => vec![2, 6, 9],
        "G" => vec![7, 11, 2],
        "C" => vec![0, 4, 7],
        "Cmaj7" => vec![0, 4, 7, 11],
        "Am" => vec![9, 0, 4],
        "B" => vec![11, 3, 6],
        "E" => vec![4, 8, 11],
        _ => unreachable!(),
    }
}

/// Harmonie du thème (temps relatif, durée, accord).
const THEME_H: [(f32, f32, &str); 13] = [
    (0., 2., "Em"), (2., 2., "A"), (4., 3., "D"), (7., 3., "G"), (10., 3., "Em"), (13., 3., "Cmaj7"),
    (16., 2., "C"), (18., 2., "G"), (20., 2., "D"), (22., 2., "A"), (24., 2., "C"), (26., 2., "Am"),
    (28., 4., "B"),
];

struct Part {
    name: &'static str,
    ch: u8,
    prog: u8,
    vol: u8,
    sustained: bool,
    notes: Vec<Note>,
}

struct Score {
    parts: Vec<Part>,
}

impl Score {
    fn p(&mut self, name: &str) -> &mut Part {
        self.parts.iter_mut().find(|p| p.name == name).unwrap()
    }
    fn n(&mut self, name: &str, beat: f32, len: f32, key: u8, vel: u8) {
        let p = self.p(name);
        let ch = p.ch;
        p.notes.push(Note {
            start: (beat * PPQ as f32) as u32,
            len: (len * PPQ as f32).max(30.) as u32,
            key,
            vel: vel.min(127),
            ch,
        });
    }
    fn chord_notes(&mut self, name: &str, beat: f32, len: f32, keys: &[u8], vel: u8) {
        for &k in keys {
            self.n(name, beat, len, k, vel);
        }
    }
}

/// Conduite des voix : chaque voix prend la note de l'accord la plus proche,
/// en évitant doublures et croisements.
fn lead(ch: &[u8], prev: &mut [u8], ranges: &[(u8, u8)]) -> Vec<u8> {
    let mut out: Vec<u8> = vec![];
    for i in 0..prev.len() {
        let (lo, hi) = ranges[i];
        let best = (lo..=hi)
            .filter(|k| ch.contains(&(k % 12)))
            .min_by_key(|&k| {
                let mut c = (k as i32 - prev[i] as i32).abs();
                if out.iter().any(|o| o % 12 == k % 12) {
                    c += 6;
                }
                if out.last().is_some_and(|&o| k >= o) {
                    c += 30;
                }
                c
            })
            .unwrap();
        out.push(best);
    }
    prev.copy_from_slice(&out);
    out
}

fn root_in(ch: &[u8], lo: u8, hi: u8, prev: u8) -> u8 {
    (lo..=hi)
        .filter(|k| k % 12 == ch[0])
        .min_by_key(|&k| (k as i32 - prev as i32).abs())
        .unwrap()
}

/// Notes de l'accord sous une mélodie (pour harmoniser trompettes/cors).
fn below(ch: &[u8], mel: u8, prev: &mut [u8]) -> Vec<u8> {
    let r: Vec<(u8, u8)> = prev.iter().map(|_| (mel.saturating_sub(10), mel - 1)).collect();
    lead(ch, prev, &r)
}

fn harmony(start: f32, list: &[(f32, f32, &'static str)]) -> Vec<(f32, f32, Vec<u8>)> {
    list.iter().map(|&(b, l, c)| (start + b, l, chord(c))).collect()
}

fn theme(start: f32, transpose: i8) -> Vec<(f32, f32, u8)> {
    THEME.iter().map(|&(b, l, k)| (start + b, l, (k as i8 + transpose) as u8)).collect()
}

fn chord_at(h: &[(f32, f32, Vec<u8>)], beat: f32) -> Vec<u8> {
    h.iter().rev().find(|x| x.0 <= beat + 0.01).unwrap().2.clone()
}

fn timp_roll(s: &mut Score, beat: f32, len: f32, key: u8, v0: u8, v1: u8) {
    let steps = (len * 6.) as usize;
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let v = v0 as f32 + (v1 as f32 - v0 as f32) * f;
        s.n("Timpani", beat + i as f32 / 6., 0.16, key, (v as u8).saturating_sub((i % 2) as u8 * 6));
    }
}

fn harp_arp(s: &mut Score, beat: f32, len: f32, ch: &[u8], lo: u8, hi: u8, vel: u8, step: f32) {
    let tones: Vec<u8> = (lo..=hi).filter(|k| ch[..3].contains(&(k % 12))).collect();
    let n = (len / step) as usize;
    for i in 0..n {
        let k = tones[i % tones.len()];
        s.n("Harp", beat + i as f32 * step, 1.5, k, vel - (i % 2) as u8 * 8);
    }
}

pub fn build(render: bool) -> Song {
    let mut s = Score { parts: vec![] };
    let defs: [(&str, u8, u8, u8, bool); 22] = [
        ("Piccolo", 7, 72, 80, true),
        ("Flutes", 6, 73, 90, true),
        ("Oboes", 8, 68, 85, true),
        ("Clarinets", 10, 71, 90, true),
        ("Bassoons", 11, 70, 95, true),
        ("Horns", 2, 60, 115, true),
        ("Trumpets", 3, 56, 95, true),
        ("Trombones", 4, 57, 95, true),
        ("Tuba", 5, 58, 100, true),
        ("Timpani", 12, 47, 110, false),
        ("Bass Drum", 9, 0, 110, false),
        ("Piatti", 9, 0, 100, false),
        ("Glockenspiel", 14, 9, 75, false),
        ("Celesta", 15, 8, 80, false),
        ("Harp", 13, 46, 105, false),
        ("Violins 1", 0, 48, 100, true),
        ("Violins 2", 0, 48, 100, true),
        ("Violas", 0, 48, 100, true),
        ("Cellos", 0, 48, 100, true),
        ("Basses", 1, 43, 105, true),
        ("Xylophone", 14, 13, 0, false),
        ("Marimba", 15, 12, 0, false),
    ];
    for (name, ch, prog, vol, sustained) in defs {
        s.parts.push(Part { name, ch, prog, vol, sustained, notes: vec![] });
    }
    s.parts.retain(|p| p.vol > 0); // xylo/marimba dispo mais non utilisés

    // ---------- Harmonie globale ----------
    let mut h: Vec<(f32, f32, Vec<u8>)> = vec![(0., 8., chord("Em"))];
    h.extend(harmony(8., &THEME_H[..4]));
    h.extend(harmony(16., &[(0., 2., "C"), (2., 2., "D"), (4., 4., "B")]));
    h.extend(harmony(24., &THEME_H));
    h.extend(harmony(56., &THEME_H));
    h.extend(harmony(88., &[(0., 2., "C"), (2., 2., "D"), (4., 6., "E")]));

    // ---------- INTRO (mes. 1-6) ----------
    timp_roll(&mut s, 0., 7.5, 40, 30, 70);
    s.n("Basses", 0., 16., 28, 55);
    s.n("Cellos", 0., 8., 40, 50);
    s.n("Cellos", 0., 8., 47, 45);
    s.chord_notes("Violas", 0., 8., &[55, 59], 40);
    s.n("Violins 2", 0., 8., 64, 38);
    s.n("Violins 1", 4., 4., 71, 35);
    harp_arp(&mut s, 0., 8., &chord("Em"), 40, 71, 62, 0.5);
    for (b, k) in [(1., 83), (3., 88), (4.5, 79), (6., 83), (7., 91)] {
        s.n("Celesta", b, 1., k, 55);
    }
    // Appel lointain des trompettes : tête du thème (mes. 3-4)
    let mut tp = [59u8, 55];
    for (b, l, k) in theme(8., 0).into_iter().take(5) {
        if b >= 16. {
            break;
        }
        let ch = chord_at(&h, b);
        s.n("Trumpets", b, l + 0.05, k, 72);
        let v = below(&ch, k, &mut tp);
        s.chord_notes("Trumpets", b, l + 0.05, &v, 62);
    }
    // Réponse des trombones + cordes graves
    let mut tb = [55u8, 52, 40];
    let mut lowstr = [59u8, 55];
    let mut vc = 40u8;
    for (b, l, ch) in h.iter().filter(|x| x.0 >= 8. && x.0 < 24.) {
        let v = lead(ch, &mut tb, &[(50, 62), (45, 57), (36, 50)]);
        s.chord_notes("Trombones", *b, *l, &v, if *b < 16. { 55 } else { 70 + (*b - 16.) as u8 * 4 });
        let v = lead(ch, &mut lowstr, &[(55, 67), (48, 60)]);
        s.chord_notes("Violas", *b, *l, &v, 50 + (*b - 8.) as u8 * 2);
        vc = root_in(ch, 36, 48, vc);
        s.n("Cellos", *b, *l, vc, 55 + (*b - 8.) as u8 * 2);
        s.n("Bassoons", *b, *l, vc, 50);
        if *b >= 16. {
            s.n("Basses", *b, *l, vc - 12, 75);
            s.n("Tuba", *b, *l, vc - 12, 70 + (*b - 16.) as u8 * 3);
        }
        harp_arp(&mut s, *b, *l, ch, 43, 74, 60, 0.5);
    }
    s.n("Violins 2", 8., 8., 67, 45);
    s.n("Violins 1", 8., 8., 71, 45);
    // Bois : écho doux de la tête (mes. 4)
    for (b, l, k) in [(12., 2., 76), (14., 2., 81)] {
        s.n("Oboes", b, l, k, 55);
    }
    // Montée des violons (mes. 5-6), mineur harmonique de mi
    let run = [59u8, 60, 62, 64, 66, 67, 69, 71, 72, 74, 76, 78, 79, 81, 83, 84];
    for (i, &k) in run.iter().enumerate() {
        let b = 16. + i as f32 * 0.5;
        let k = if b >= 20. && k % 12 == 2 { k + 1 } else { k }; // ré# sur l'accord de si
        s.n("Violins 1", b, 0.55, k, 70 + i as u8 * 3);
        s.n("Violins 2", b, 0.55, k - 12, 65 + i as u8 * 3);
        if i >= 8 {
            s.n("Flutes", b, 0.55, k, 60 + i as u8 * 3);
        }
    }
    let mut cl = [64u8, 59];
    for (b, l, ch) in h.iter().filter(|x| x.0 >= 16. && x.0 < 24.) {
        let v = lead(ch, &mut cl, &[(60, 72), (55, 67)]);
        s.chord_notes("Clarinets", *b, *l, &v, 60 + (*b - 16.) as u8 * 4);
    }
    timp_roll(&mut s, 20., 4., 47, 50, 105);

    // ---------- THÈME A (mes. 7-14) : cors a4 ----------
    let a = 24.;
    s.n("Bass Drum", a, 2., 35, 70);
    s.n("Piatti", a, 2., 49, 65);
    for (b, l, k) in theme(a, 0) {
        s.n("Horns", b, l + 0.1, k, 100);
        s.n("Horns", b, l + 0.1, k - 12, 88);
    }
    let mut str3 = [71u8, 67, 59];
    let mut cl = [64u8, 59];
    let mut fl = [76u8, 71];
    let mut tb = [55u8, 52, 47];
    let mut bass = 40u8;
    for (b, l, ch) in h.clone().iter().filter(|x| x.0 >= a && x.0 < a + 32.) {
        let rel = b - a;
        let v = lead(ch, &mut str3, &[(64, 76), (57, 69), (50, 62)]);
        s.n("Violins 1", *b, *l, v[0] + 12, 50 + rel as u8);
        s.n("Violins 2", *b, *l, v[0], 55 + rel as u8);
        s.chord_notes("Violas", *b, *l, &v[1..], 58 + rel as u8);
        bass = root_in(ch, 36, 47, bass);
        s.n("Cellos", *b, *l, bass, 70);
        s.n("Basses", *b, *l, bass - 12, 70);
        s.n("Bassoons", *b, *l, bass + 12, 55);
        let v = lead(ch, &mut cl, &[(60, 70), (55, 65)]);
        s.chord_notes("Clarinets", *b, *l, &v, 50 + rel as u8);
        if rel >= 16. {
            let v = lead(ch, &mut fl, &[(74, 86), (69, 81)]);
            s.chord_notes("Flutes", *b, *l, &v, 45 + rel as u8);
        }
        if rel >= 20. {
            let v = lead(ch, &mut tb, &[(50, 62), (45, 57), (38, 50)]);
            s.chord_notes("Trombones", *b, *l, &v, 55 + (rel - 20.) as u8 * 3);
            s.n("Tuba", *b, *l, bass - 12, 60 + (rel - 20.) as u8 * 3);
        }
        harp_arp(&mut s, *b, *l, ch, 40, 76, 65, if rel >= 16. { 0.25 } else { 0.5 });
        if [0., 10., 16., 24.].contains(&rel) {
            s.n("Timpani", *b, 1., root_in(ch, 38, 50, 40), 85);
        }
    }
    timp_roll(&mut s, a + 28., 4., 47, 60, 115);

    // ---------- THÈME B (mes. 15-22) : tutti ----------
    let t = 56.;
    for b in [t, t + 16.] {
        s.n("Bass Drum", b, 2., 35, 110);
        s.n("Piatti", b, 3., 49, 110);
    }
    s.n("Piatti", t + 24., 3., 57, 100);
    for (b, l, k) in theme(t, 0) {
        let rel = b - t;
        s.n("Horns", b, l + 0.1, k, 120);
        s.n("Horns", b, l + 0.1, k - 12, 110);
        s.n("Violins 1", b, l + 0.1, k + 12, 115);
        s.n("Violins 1", b, l + 0.1, k, 105);
        s.n("Flutes", b, l, k + 12, 105);
        s.n("Oboes", b, l, k + 12, 100);
        s.n("Clarinets", b, l, k, 100);
        s.n("Glockenspiel", b, 1., k + 24, 90);
        if rel >= 16. {
            s.n("Piccolo", b, l, k + 24, 95);
            s.n("Trumpets", b, l + 0.05, k, 112);
        }
    }
    let mut v2 = [67u8, 59];
    let mut tb = [59u8, 55, 48];
    let mut tp = [64u8, 60];
    let mut cl2 = [59u8];
    bass = 40;
    for (b, l, ch) in h.clone().iter().filter(|x| x.0 >= t && x.0 < t + 32.) {
        let rel = b - t;
        // Violons 2 + altos : ostinato en croches sur les notes de l'accord
        let v = lead(ch, &mut v2, &[(62, 74), (55, 67)]);
        let steps = (*l / 0.5) as usize;
        for i in 0..steps {
            let bb = b + i as f32 * 0.5;
            let acc = if bb.fract() == 0. { 12 } else { 0 };
            s.n("Violins 2", bb, 0.45, v[i % 2], 88 + acc);
            s.n("Violas", bb, 0.45, v[(i + 1) % 2] - 12, 85 + acc);
        }
        let v = lead(ch, &mut tb, &[(52, 64), (47, 59), (40, 52)]);
        s.chord_notes("Trombones", *b, *l, &v, 100);
        bass = root_in(ch, 36, 47, bass);
        s.n("Cellos", *b, *l, bass, 105);
        s.n("Cellos", *b, *l, bass + 12, 95);
        s.n("Basses", *b, *l, bass - 12, 105);
        s.n("Tuba", *b, *l, bass - 12, 100);
        s.chord_notes("Bassoons", *b, *l, &[bass, bass + 12], 90);
        let v = lead(ch, &mut cl2, &[(52, 62)]);
        s.n("Clarinets", *b, *l, v[0], 80);
        if rel < 16. {
            let v = lead(ch, &mut tp, &[(60, 72), (55, 67)]);
            s.chord_notes("Trumpets", *b, *l, &v, 85);
        }
        harp_arp(&mut s, *b, *l, ch, 40, 84, 80, 0.25);
        s.n("Timpani", *b, 1., root_in(ch, 38, 50, 40), 105);
    }
    timp_roll(&mut s, t + 28., 4., 47, 80, 120);

    // ---------- CODA (mes. 23-24) : bVI - bVII - I ----------
    let c = 88.;
    let fan = [(0., 67u8), (1. / 3., 67), (2. / 3., 67), (1., 72), (2., 69), (2. + 1. / 3., 69), (2. + 2. / 3., 69), (3., 74)];
    let mut tp = [64u8, 60];
    for (b, k) in fan {
        let ch = chord_at(&h, c + b);
        let l = if b.fract() == 0. && b >= 1. { 1. } else { 0.3 };
        s.n("Trumpets", c + b, l, k, 118);
        let v = below(&ch, k, &mut tp);
        s.chord_notes("Trumpets", c + b, l, &v, 108);
    }
    for (b, l, k) in [(0., 2., 64u8), (2., 2., 66)] {
        s.chord_notes("Horns", c + b, l + 0.1, &[k, k - 12], 120);
        s.chord_notes("Violins 1", c + b, l + 0.1, &[k + 12, k], 118);
        s.chord_notes("Flutes", c + b, l, &[k + 12], 105);
        s.n("Piccolo", c + b, l, k + 24, 100);
    }
    let mut tb = [55u8, 52, 48];
    for (b, l, ch) in h.clone().iter().filter(|x| x.0 >= c && x.0 < c + 4.) {
        let v = lead(ch, &mut tb, &[(52, 64), (47, 59), (40, 52)]);
        s.chord_notes("Trombones", *b, *l, &v, 110);
        let r = root_in(ch, 36, 47, 40);
        s.chord_notes("Cellos", *b, *l, &[r, r + 12], 110);
        s.n("Basses", *b, *l, r - 12, 110);
        s.n("Tuba", *b, *l, r - 12, 108);
        s.n("Bassoons", *b, *l, r, 95);
        let v = lead(ch, &mut [67u8, 60], &[(62, 74), (55, 67)]);
        s.chord_notes("Violins 2", *b, *l, &v, 105);
        s.chord_notes("Violas", *b, *l, &[v[1] - 12, v[0] - 12], 100);
        s.chord_notes("Clarinets", *b, *l, &v, 95);
        s.chord_notes("Oboes", *b, *l, &[v[0] + 12], 95);
        harp_arp(&mut s, *b, *l, ch, 40, 88, 85, 0.25);
    }
    timp_roll(&mut s, c + 2., 2., 47, 90, 120);
    // Accord final de mi majeur
    let f = c + 4.;
    let fin = 5.5;
    s.n("Bass Drum", f, 3., 35, 125);
    s.n("Piatti", f, 4., 49, 125);
    s.n("Piatti", f, 4., 57, 110);
    s.chord_notes("Horns", f, fin, &[68, 64, 59, 56], 125);
    s.chord_notes("Trumpets", f, fin, &[76, 71, 68], 120);
    s.chord_notes("Trombones", f, fin, &[59, 56, 52], 115);
    s.n("Tuba", f, fin, 28, 115);
    s.chord_notes("Violins 1", f, fin, &[80, 88], 120);
    s.chord_notes("Violins 2", f, fin, &[76, 71], 115);
    s.chord_notes("Violas", f, fin, &[64, 56], 110);
    s.chord_notes("Cellos", f, fin, &[40, 52], 115);
    s.n("Basses", f, fin, 28, 115);
    s.chord_notes("Flutes", f, fin, &[88, 83], 105);
    s.n("Piccolo", f, fin, 92, 100);
    s.chord_notes("Oboes", f, fin, &[80, 76], 100);
    s.chord_notes("Clarinets", f, fin, &[71, 68], 100);
    s.chord_notes("Bassoons", f, fin, &[40, 52], 100);
    s.chord_notes("Glockenspiel", f, 2., &[88, 92, 95], 95);
    harp_arp(&mut s, f, 2., &chord("E"), 40, 88, 95, 1. / 8.);
    timp_roll(&mut s, f, fin - 0.5, 40, 80, 120);
    s.n("Timpani", f + fin - 0.5, 1., 40, 127);
    s.n("Bass Drum", f + fin - 0.5, 2., 35, 127);

    // ---------- Assemblage ----------
    // Courbe de dynamique globale (CC1 pour les banques, CC11 pour le rendu GM)
    let dynamics: [(f32, f32); 10] = [
        (0., 40.), (8., 70.), (16., 70.), (24., 100.), (40., 95.), (48., 100.), (56., 120.), (88., 122.), (92., 127.), (100., 127.),
    ];
    let cc = if render { 11 } else { 1 };
    let mut tracks = vec![Track {
        name: "WILLIZE".into(),
        evs: vec![],
        tempos: vec![(0, 625_000), (88 * PPQ, 666_667), (92 * PPQ, 750_000)],
    }];
    for p in s.parts {
        let mut evs = vec![Ev::Cc { t: 0, ch: p.ch, cc: 7, val: p.vol }];
        if p.ch != 9 {
            evs.push(Ev::Prog { t: 0, ch: p.ch, prog: p.prog });
        }
        if p.sustained {
            let mut q = 0.;
            while q < 100. {
                let i = dynamics.iter().rposition(|d| d.0 <= q).unwrap();
                let (b0, v0) = dynamics[i];
                let (b1, v1) = dynamics[(i + 1).min(dynamics.len() - 1)];
                let v = if b1 > b0 { v0 + (v1 - v0) * (q - b0) / (b1 - b0) } else { v0 };
                evs.push(Ev::Cc { t: (q * PPQ as f32) as u32, ch: p.ch, cc, val: v as u8 });
                q += 0.5;
            }
        }
        evs.extend(p.notes.into_iter().map(Ev::Note));
        tracks.push(Track { name: p.name.into(), evs, tempos: vec![] });
    }
    Song { ppq: PPQ as u16, tracks }
}
