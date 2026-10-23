//! "WILLIZE" v2 — 1 mn sur le thème des 6 horns, écriture à la Williams :
//! rythme harmonique lent (1 accord/mesure), pédales, textures transparentes,
//! contre-chants (violoncelles, flûte, hautbois), tutti réservé à l'apogée.
//!
//! Intro (6 mes.) → Thème A cors a4 (8) → Thème B cordes + bois, cuivres en choral (8)
//! → Coda bVI-bVII-I (3).

use crate::midi::{Ev, Note, Song, Track};

const PPQ: u32 = 480;

/// Thème extrait de la piste "6horns leg" (temps relatif, durée, note).
const THEME: [(f32, f32, u8); 12] = [
    (0., 2., 64), (2., 2., 69), (4., 3., 66), (7., 1., 62), (8., 2., 62), (10., 6., 59),
    (16., 2., 64), (18., 2., 71), (20., 2., 69), (22., 2., 64), (24., 4., 72), (28., 4., 66),
];

/// Un accord par mesure : A/E – D – G/B – Em – Cmaj7 – Am – F – B.
const TH: [&str; 8] = ["A", "D", "G", "Em", "Cmaj7", "Am", "F", "B"];
/// Basse (violoncelles ; contrebasses à l'octave inférieure).
const BASS_A: [u8; 8] = [52, 52, 47, 52, 48, 45, 41, 47];
const BASS_B: [u8; 8] = [52, 50, 47, 52, 48, 45, 41, 47];

fn chord(n: &str) -> Vec<u8> {
    match n {
        "Em" => vec![4, 7, 11],
        "A" => vec![9, 1, 4],
        "D" => vec![2, 6, 9],
        "G" => vec![7, 11, 2],
        "C" => vec![0, 4, 7],
        "Cmaj7" => vec![0, 4, 7, 11],
        "Am" => vec![9, 0, 4],
        "F" => vec![5, 9, 0],
        "B" => vec![11, 3, 6],
        "Bsus4" => vec![11, 4, 6],
        "E" => vec![4, 8, 11],
        _ => unreachable!(),
    }
}

struct Part {
    name: &'static str,
    ch: u8,
    prog: u8,
    vol: u8,
    curve: Vec<(f32, f32)>,
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
            start: (beat * PPQ as f32).round() as u32,
            len: (len * PPQ as f32).max(30.) as u32,
            key,
            vel: vel.min(127),
            ch,
        });
    }
    fn ns(&mut self, name: &str, beat: f32, len: f32, keys: &[u8], vel: u8) {
        for &k in keys {
            self.n(name, beat, len, k, vel);
        }
    }
    /// Ligne mélodique : (durée, note) enchaînées, 0 = silence.
    fn line(&mut self, name: &str, mut beat: f32, notes: &[(f32, u8)], vel: u8, legato: f32) {
        for &(l, k) in notes {
            if k > 0 {
                self.n(name, beat, l + legato, k, vel);
            }
            beat += l;
        }
    }
    fn curve(&mut self, name: &str, c: &[(f32, f32)]) {
        self.p(name).curve = c.to_vec();
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

fn below(ch: &[u8], mel: u8, prev: &mut [u8]) -> Vec<u8> {
    let r: Vec<(u8, u8)> = prev.iter().map(|_| (mel.saturating_sub(9), mel - 2)).collect();
    lead(ch, prev, &r)
}

fn theme(start: f32, tr: i8) -> Vec<(f32, f32, u8)> {
    THEME.iter().map(|&(b, l, k)| (start + b, l, (k as i8 + tr) as u8)).collect()
}

fn timp_roll(s: &mut Score, beat: f32, len: f32, key: u8, v0: u8, v1: u8) {
    let steps = (len * 6.) as usize;
    for i in 0..steps {
        let v = v0 as f32 + (v1 as f32 - v0 as f32) * i as f32 / steps as f32;
        s.n("Timpani", beat + i as f32 / 6., 0.16, key, (v as u8).saturating_sub((i % 2) as u8 * 5));
    }
}

/// Arpège de harpe montant depuis la basse.
fn harp_up(s: &mut Score, beat: f32, ch: &[u8], from: u8, count: usize, step: f32, vel: u8) {
    let tones: Vec<u8> = (from..from + 40).filter(|k| ch[..3].contains(&(k % 12))).take(count).collect();
    for (i, &k) in tones.iter().enumerate() {
        s.n("Harp", beat + i as f32 * step, 3., k, vel + i as u8 * 2);
    }
}

/// Arpège montant puis descendant sur une mesure.
fn harp_wave(s: &mut Score, beat: f32, ch: &[u8], from: u8, vel: u8) {
    let tones: Vec<u8> = (from..from + 40).filter(|k| ch[..3].contains(&(k % 12))).take(5).collect();
    let order = [0, 1, 2, 3, 4, 3, 2, 1];
    for (i, &o) in order.iter().enumerate() {
        s.n("Harp", beat + i as f32 * 0.5, 1.5, tones[o], vel - (i % 2) as u8 * 6);
    }
}

pub fn build(render: bool) -> Song {
    let mut s = Score { parts: vec![] };
    // nom, canal (unique par instrument tenu pour le rendu GM), programme GM, volume
    let defs: [(&str, u8, u8, u8); 20] = [
        ("Piccolo", 4, 73, 85),
        ("Flutes", 4, 73, 85),
        ("Oboes", 5, 68, 80),
        ("Clarinets", 6, 71, 85),
        ("Bassoons", 7, 70, 90),
        ("Horns", 8, 60, 112),
        ("Trumpets", 10, 56, 90),
        ("Trombones", 11, 57, 90),
        ("Tuba", 12, 58, 95),
        ("Timpani", 13, 47, 105),
        ("Bass Drum", 9, 0, 105),
        ("Piatti", 9, 0, 90),
        ("Celesta", 15, 8, 75),
        ("Glockenspiel", 15, 9, 70),
        ("Harp", 14, 46, 100),
        ("Violins 1", 0, 48, 100),
        ("Violins 2", 1, 48, 95),
        ("Violas", 2, 48, 95),
        ("Cellos", 3, 48, 100),
        ("Basses", 3, 48, 100),
    ];
    for (name, ch, prog, vol) in defs {
        s.parts.push(Part { name, ch, prog, vol, curve: vec![], notes: vec![] });
    }

    // ================= INTRO (mes. 1-6, temps 0-24) =================
    // Pédale de mi, quinte à vide, scintillement célesta, tête du thème au hautbois.
    s.n("Basses", 0., 12., 28, 50);
    s.ns("Cellos", 0., 12., &[40, 47], 48);
    timp_roll(&mut s, 0., 6., 40, 22, 40);
    s.n("Violins 2", 0., 12., 76, 40);
    s.n("Violins 1", 2., 10., 83, 38);
    for bar in 0..3 {
        let b = bar as f32 * 4.;
        s.ns("Harp", b, 4., &[40, 47], 55);
        for (i, k) in [76u8, 83, 78, 83, 79, 83, 78, 83].iter().enumerate() {
            s.n("Celesta", b + i as f32 * 0.5, 0.5, *k, 42 + (i % 2 == 0) as u8 * 8);
        }
    }
    s.line("Oboes", 4., &[(2., 76), (2., 81), (3., 78), (1., 74)], 58, 0.05);

    // Choral des cuivres graves : C/E – D/E – Bsus4 – B
    let chorale = [(12., 4., "C", 40u8), (16., 4., "D", 40), (20., 2., "Bsus4", 35), (22., 2., "B", 35)];
    let mut tb = [55u8, 52, 48];
    let mut hn = [64u8, 59, 55];
    let mut va = [60u8, 55];
    let mut cl = [67u8, 64];
    let mut vn = [79u8, 76];
    for (i, &(b, l, c, bass)) in chorale.iter().enumerate() {
        let ch = chord(c);
        let v = lead(&ch, &mut tb, &[(52, 62), (47, 57), (42, 52)]);
        s.ns("Trombones", b, l, &v, 62 + i as u8 * 8);
        s.n("Tuba", b, l, bass, 60 + i as u8 * 8);
        s.n("Basses", b, l, if bass == 40 { 28 } else { 35 }, 60 + i as u8 * 8);
        s.n("Cellos", b, l, bass + if bass == 40 { 0 } else { 12 }, 60 + i as u8 * 8);
        s.n("Bassoons", b, l, v[2], 55 + i as u8 * 6);
        let v = lead(&ch, &mut va, &[(55, 67), (50, 62)]);
        s.ns("Violas", b, l, &v, 55 + i as u8 * 8);
        let v = lead(&ch, &mut cl, &[(62, 72), (57, 67)]);
        s.ns("Clarinets", b, l, &v, 50 + i as u8 * 8);
        let v = lead(&ch, &mut vn, &[(74, 86), (67, 79)]);
        s.n("Violins 1", b, l, v[0], 55 + i as u8 * 10);
        s.n("Violins 2", b, l, v[1], 55 + i as u8 * 10);
        if b >= 16. {
            let v = lead(&ch, &mut hn, &[(62, 70), (57, 65), (52, 60)]);
            s.ns("Horns", b, l, &v, 70 + i as u8 * 8);
        }
        harp_up(&mut s, b, &ch, bass, 6, 1. / 3., 50 + i as u8 * 5);
    }
    timp_roll(&mut s, 20., 4., 47, 45, 100);
    // Glissando de harpe vers le thème
    let gl = [47u8, 48, 51, 52, 54, 55, 57, 59, 60, 63, 64, 66, 67, 69, 71];
    for (i, &k) in gl.iter().enumerate() {
        s.n("Harp", 22.5 + i as f32 * 0.1, 1.5, k, 60 + i as u8 * 2);
    }

    // ================= THÈME A (mes. 7-14, temps 24-56) : cors a4 =================
    let a = 24.;
    for (b, l, k) in theme(a, 0) {
        let rel = b - a;
        let v = if rel >= 24. { 100 } else if rel >= 16. { 95 } else { 90 };
        s.n("Horns", b, l + 0.1, k, v);
    }
    let mut vn = [83u8, 76];
    let mut va = [61u8, 57];
    let mut cl = [64u8, 60];
    for bar in 0..8 {
        let b = a + bar as f32 * 4.;
        let ch = chord(TH[bar]);
        let bass = BASS_A[bar];
        let v = lead(&ch, &mut vn, &[(76, 88), (69, 79)]);
        s.n("Violins 1", b, 4.05, v[0], 48 + bar as u8 * 3);
        s.n("Violins 2", b, 4.05, v[1], 50 + bar as u8 * 3);
        let v = lead(&ch, &mut va, &[(55, 66), (50, 60)]);
        s.ns("Violas", b, 4.05, &v, 55 + bar as u8 * 3);
        s.n("Cellos", b, 4.05, bass, 62 + bar as u8 * 2);
        s.n("Basses", b, 4.05, bass - 12, 60 + bar as u8 * 2);
        harp_up(&mut s, b, &ch, bass, 6, 1. / 3., 55);
        if bar >= 4 {
            let v = lead(&ch, &mut cl, &[(60, 70), (55, 65)]);
            s.ns("Clarinets", b, 4.05, &v, 52 + bar as u8 * 2);
            s.n("Bassoons", b, 4.05, bass, 55);
        }
    }
    s.n("Timpani", a, 2., 40, 70);
    // Flûte : filigrane pendant le si tenu des cors (mes. 9-10)
    s.line("Flutes", a + 10., &[(0.5, 74), (0.5, 76), (0.5, 78), (0.5, 79), (2., 83), (0.5, 81), (0.5, 79), (0.5, 78), (1.5, 76)], 62, 0.02);
    // Hautbois : réponse sur le do et le fa# tenus (mes. 13-14)
    s.line("Oboes", a + 25., &[(1., 81), (1., 79), (1., 77), (2., 78), (1., 75), (1., 78)], 60, 0.05);
    // Montée vers le tutti : trombones + tuba sur l'accord de si
    s.ns("Trombones", a + 28., 4., &[54, 51, 47], 60);
    s.n("Tuba", a + 28., 4., 35, 60);
    timp_roll(&mut s, a + 28., 4., 47, 40, 105);
    for (i, &k) in [71u8, 73, 75, 76, 78, 80, 81, 83].iter().enumerate() {
        s.n("Violins 1", a + 30. + i as f32 * 0.25, 0.3, k, 70 + i as u8 * 5); // élan vers le tutti
    }

    // ================= THÈME B (mes. 15-22, temps 56-88) : cordes + bois =================
    let t = 56.;
    s.n("Bass Drum", t, 2., 35, 90);
    s.n("Piatti", t, 3., 49, 85);
    s.n("Bass Drum", t + 24., 2., 35, 105);
    s.n("Piatti", t + 24., 3., 57, 100);
    for (b, l, k) in theme(t, 0) {
        let rel = b - t;
        s.n("Violins 1", b, l + 0.1, k + 12, 105);
        s.n("Violins 2", b, l + 0.1, k, 100);
        s.n("Flutes", b, l, k + 12, 95);
        s.n("Clarinets", b, l, k, 90);
        if rel >= 16. {
            s.n("Oboes", b, l, k + 12, 95);
            s.n("Trumpets", b, l + 0.05, k, 105);
        }
        if rel >= 24. {
            s.n("Piccolo", b, l, k + 24, 92);
            s.n("Glockenspiel", b, 1.5, k + 24, 85);
        }
    }
    // Contre-chant des violoncelles (doublé bassons)
    let cello: [(f32, u8); 18] = [
        (2., 61), (2., 64), (3., 62), (1., 54), (2., 55), (2., 57), (2., 55), (2., 54),
        (2., 52), (2., 55), (2., 60), (1., 59), (1., 57), (2., 57), (2., 60), (2., 63), (1., 59), (1., 54),
    ];
    s.line("Cellos", t, &cello, 100, 0.05);
    s.line("Bassoons", t, &cello, 85, 0.05);
    let mut hn = [64u8, 61, 57, 52];
    let mut va = [61u8, 57];
    let mut tb = [57u8, 52, 45];
    for bar in 0..8 {
        let b = t + bar as f32 * 4.;
        let ch = chord(TH[bar]);
        let bass = BASS_B[bar];
        // Cors : choral à 4 voix, tenu
        let v = lead(&ch, &mut hn, &[(60, 70), (55, 66), (52, 62), (45, 57)]);
        s.ns("Horns", b, 4.05, &v, 92 + bar as u8 * 2);
        let v = lead(&ch, &mut va, &[(55, 67), (50, 62)]);
        s.ns("Violas", b, 4.05, &v, 88);
        s.n("Basses", b, 4.05, bass - 12, 100);
        s.n("Tuba", b, 4.05, bass - 12, 85 + bar as u8 * 3);
        if bar >= 4 {
            let v = lead(&ch, &mut tb, &[(52, 62), (47, 57), (40, 52)]);
            s.ns("Trombones", b, 4.05, &v, 90 + (bar - 4) as u8 * 6);
        }
        harp_wave(&mut s, b, &ch, bass, 75);
    }
    s.n("Timpani", t, 2., 40, 100);
    s.n("Timpani", t + 16., 2., 45, 95);
    s.n("Timpani", t + 24., 2., 41, 110);
    timp_roll(&mut s, t + 28., 4., 47, 70, 120);

    // ================= CODA (temps 88-100) : C – D – E =================
    let c = 88.;
    let mut tp = [64u8, 60];
    let fan = [(0., 0.33, 67u8), (1. / 3., 0.33, 67), (2. / 3., 0.33, 67), (1., 1., 72), (2., 0.33, 69), (2. + 1. / 3., 0.33, 69), (2. + 2. / 3., 0.33, 69), (3., 1., 74)];
    for (b, l, k) in fan {
        let ch = chord(if b < 2. { "C" } else { "D" });
        s.n("Trumpets", c + b, l, k, 115);
        let v = below(&ch, k, &mut tp);
        s.ns("Trumpets", c + b, l, &v, 105);
    }
    s.line("Horns", c, &[(2., 64), (2., 69)], 118, 0.1);
    s.line("Horns", c, &[(2., 52), (2., 57)], 110, 0.1);
    s.line("Violins 1", c, &[(2., 79), (1., 81), (0.25, 81), (0.25, 83), (0.25, 85), (0.25, 86)], 115, 0.02);
    s.line("Violins 2", c, &[(2., 76), (2., 78)], 110, 0.05);
    s.line("Flutes", c, &[(2., 79), (2., 81)], 105, 0.);
    s.line("Oboes", c, &[(2., 76), (2., 78)], 100, 0.);
    s.line("Clarinets", c, &[(2., 67), (2., 69)], 100, 0.);
    for (b, ch, bass) in [(c, "C", 48u8), (c + 2., "D", 50)] {
        let v = lead(&chord(ch), &mut tb, &[(52, 62), (47, 57), (40, 52)]);
        s.ns("Trombones", b, 2., &v, 110);
        s.ns("Cellos", b, 2.05, &[bass - 12, bass], 110);
        s.n("Basses", b, 2.05, bass - 24, 110);
        s.n("Tuba", b, 2.05, bass - 12, 108);
        s.ns("Violas", b, 2.05, &[bass + 7, bass + 4], 100);
        s.n("Bassoons", b, 2.05, bass - 12, 95);
    }
    timp_roll(&mut s, c + 2., 2., 45, 85, 115);
    let gl = [52u8, 54, 56, 57, 59, 61, 63, 64, 66, 68, 69, 71, 73, 75, 76, 78];
    for (i, &k) in gl.iter().enumerate() {
        s.n("Harp", c + 2.5 + i as f32 * 0.09, 2., k, 70 + i as u8 * 2);
    }
    // Accord final de mi majeur
    let f = c + 4.;
    let fin = 7.5;
    s.n("Bass Drum", f, 3., 35, 120);
    s.n("Piatti", f, 4., 49, 118);
    s.ns("Horns", f, fin, &[68, 64, 59, 52], 122);
    s.ns("Trumpets", f, fin, &[76, 71, 68], 115);
    s.ns("Trombones", f, fin, &[59, 56, 52], 112);
    s.n("Tuba", f, fin, 28, 112);
    s.n("Violins 1", f, fin, 88, 120);
    s.n("Violins 2", f, fin, 80, 115);
    s.ns("Violas", f, fin, &[71, 64], 110);
    s.ns("Cellos", f, fin, &[40, 52], 115);
    s.n("Basses", f, fin, 28, 115);
    s.ns("Flutes", f, fin, &[88, 83], 105);
    s.n("Piccolo", f, fin, 92, 95);
    s.ns("Oboes", f, fin, &[80, 76], 100);
    s.ns("Clarinets", f, fin, &[71, 68], 100);
    s.ns("Bassoons", f, fin, &[40, 52], 100);
    s.ns("Glockenspiel", f, 2., &[88, 92, 95], 90);
    harp_up(&mut s, f, &chord("E"), 40, 10, 0.125, 85);
    timp_roll(&mut s, f, fin - 0.5, 40, 75, 118);
    s.n("Timpani", f + fin - 0.5, 1., 40, 127);
    s.n("Bass Drum", f + fin - 0.5, 2., 35, 120);

    // ================= Dynamiques (CC1 banques / CC11 rendu GM) =================
    let strings = [(0., 45.), (12., 55.), (22., 95.), (24., 65.), (44., 75.), (52., 100.), (56., 110.), (80., 120.), (92., 127.)];
    for n in ["Violins 1", "Violins 2", "Violas", "Cellos", "Basses"] {
        s.curve(n, &strings);
    }
    s.curve("Horns", &[(0., 60.), (16., 65.), (23., 95.), (24., 88.), (40., 95.), (48., 105.), (56., 100.), (80., 112.), (92., 127.)]);
    s.curve("Trombones", &[(12., 55.), (23., 95.), (52., 60.), (56., 95.), (72., 100.), (88., 118.), (92., 127.)]);
    s.curve("Tuba", &[(12., 55.), (23., 95.), (52., 60.), (56., 95.), (72., 100.), (88., 118.), (92., 127.)]);
    s.curve("Trumpets", &[(0., 100.), (72., 108.), (88., 122.)]);
    for n in ["Flutes", "Piccolo", "Oboes", "Clarinets", "Bassoons"] {
        s.curve(n, &[(0., 75.), (22., 90.), (24., 78.), (56., 105.), (92., 118.)]);
    }

    // ================= Assemblage =================
    let cc = if render { 11 } else { 1 };
    let mut tracks = vec![Track {
        name: "WILLIZE".into(),
        evs: vec![],
        tempos: vec![(0, 600_000), (88 * PPQ, 638_298), (92 * PPQ, 750_000)],
    }];
    let mut curved: Vec<u8> = vec![];
    for p in s.parts {
        let first = p.notes.iter().map(|n| n.start).min().unwrap_or(0);
        let mut evs = vec![Ev::Cc { t: first, ch: p.ch, cc: 7, val: p.vol }];
        if p.ch != 9 {
            evs.push(Ev::Prog { t: first, ch: p.ch, prog: p.prog });
        }
        // Au rendu, un seul instrument pilote l'expression d'un canal partagé.
        if !p.curve.is_empty() && (!render || !curved.contains(&p.ch)) {
            curved.push(p.ch);
            let mut q = 0.;
            while q <= 100. {
                let c = &p.curve;
                let v = match c.iter().rposition(|d| d.0 <= q) {
                    None => c[0].1,
                    Some(i) if i + 1 == c.len() => c[i].1,
                    Some(i) => c[i].1 + (c[i + 1].1 - c[i].1) * (q - c[i].0) / (c[i + 1].0 - c[i].0),
                };
                evs.push(Ev::Cc { t: (q * PPQ as f32) as u32, ch: p.ch, cc, val: v as u8 });
                q += 0.5;
            }
        }
        evs.extend(p.notes.into_iter().map(Ev::Note));
        tracks.push(Track { name: p.name.into(), evs, tempos: vec![] });
    }
    Song { ppq: PPQ as u16, tracks }
}
