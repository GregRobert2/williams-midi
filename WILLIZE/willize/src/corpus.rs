//! Chargement du corpus Williams : chaque fichier → parties instrumentales
//! classées par famille, notes en temps (noires).

use crate::midi::{self, Ev};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Inst {
    Piccolo, Flute, Oboe, EnglishHorn, Clarinet, Bassoon,
    Horn, Trumpet, Trombone, Tuba,
    Violin, Viola, Cello, Bass, Strings,
    Timpani, Harp, Glock, Celesta, Xylo, Marimba, Perc,
    Choir, Piano, Synth, Other,
}

pub const ORCH: [Inst; 20] = [
    Inst::Piccolo, Inst::Flute, Inst::Oboe, Inst::Clarinet, Inst::Bassoon,
    Inst::Horn, Inst::Trumpet, Inst::Trombone, Inst::Tuba,
    Inst::Timpani, Inst::Harp, Inst::Glock, Inst::Celesta, Inst::Xylo, Inst::Perc,
    Inst::Violin, Inst::Viola, Inst::Cello, Inst::Bass, Inst::Strings,
];

#[derive(Clone, Copy, Debug)]
pub struct N {
    pub t: f32,
    pub l: f32,
    pub k: u8,
    pub v: u8,
}

pub struct Part {
    pub inst: Inst,
    pub name: String,
    pub prog: u8,
    pub notes: Vec<N>,
}

pub struct Piece {
    pub file: String,
    pub parts: Vec<Part>,
    pub len: f32,
    /// temps (noires) par mesure (première signature)
    pub bar: f32,
    /// changements de signature : (temps, numérateur, dénominateur)
    pub sigs: Vec<(f32, u8, u8)>,
    /// tempo : (temps, bpm)
    pub tempos: Vec<(f32, f32)>,
}

fn by_name(n: &str) -> Option<Inst> {
    let n = n.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    Some(if has(&["piccolo", "picc"]) {
        Inst::Piccolo
    } else if has(&["flute", "flauti", "flöte", "flauto"]) {
        Inst::Flute
    } else if has(&["english", "cor ang", "eng. h", "eng h"]) {
        Inst::EnglishHorn
    } else if has(&["oboe", "oboi"]) {
        Inst::Oboe
    } else if has(&["clar", "klar"]) {
        Inst::Clarinet
    } else if has(&["bassoon", "fagot", "fagg"]) {
        Inst::Bassoon
    } else if has(&["horn", "corno", "cors ", "f hn", "hrn"]) && !has(&["english"]) {
        Inst::Horn
    } else if has(&["trump", "tromp", "tpt", "trp"]) {
        Inst::Trumpet
    } else if has(&["tromb", "tbn", "trb", "posaun"]) {
        Inst::Trombone
    } else if has(&["tuba"]) {
        Inst::Tuba
    } else if has(&["timp", "kettle"]) {
        Inst::Timpani
    } else if has(&["harp", "arpa"]) {
        Inst::Harp
    } else if has(&["glock", "bells"]) {
        Inst::Glock
    } else if has(&["celest"]) {
        Inst::Celesta
    } else if has(&["xylo"]) {
        Inst::Xylo
    } else if has(&["marimba", "vibra"]) {
        Inst::Marimba
    } else if has(&["viola", "alto"]) {
        Inst::Viola
    } else if has(&["violin", "vln", "vl.", "violon"]) {
        Inst::Violin
    } else if has(&["cello", "celli", "vc", "violoncel"]) {
        Inst::Cello
    } else if has(&["contrab", "double b", "d.b", "basses", "cb", "string bass", "contrebass"]) {
        Inst::Bass
    } else if has(&["choir", "chorus", "voice", "voices", "aah", "ooh"]) {
        Inst::Choir
    } else if has(&["piano"]) {
        Inst::Piano
    } else {
        return None;
    })
}

fn by_prog(p: u8) -> Inst {
    match p {
        0..=7 => Inst::Piano,
        8 => Inst::Celesta,
        9 | 14 => Inst::Glock,
        10..=12 => Inst::Marimba,
        13 => Inst::Xylo,
        40 => Inst::Violin,
        41 => Inst::Viola,
        42 => Inst::Cello,
        43 => Inst::Bass,
        44..=45 | 48..=51 => Inst::Strings,
        46 => Inst::Harp,
        47 => Inst::Timpani,
        52..=54 | 91 => Inst::Choir,
        56 | 59 => Inst::Trumpet,
        57 => Inst::Trombone,
        58 => Inst::Tuba,
        60 | 61 => Inst::Horn,
        68 => Inst::Oboe,
        69 => Inst::EnglishHorn,
        70 => Inst::Bassoon,
        71 => Inst::Clarinet,
        72 => Inst::Piccolo,
        73..=75 => Inst::Flute,
        62..=67 | 76..=79 => Inst::Other,
        80..=103 => Inst::Synth,
        _ => Inst::Other,
    }
}

pub fn load(path: &str) -> Option<Piece> {
    let data = std::fs::read(path).ok()?;
    let song = std::panic::catch_unwind(|| midi::read(&data)).ok()?;
    let ppq = song.ppq as f32;
    // programmes par canal, triés dans le temps (toutes pistes confondues)
    let mut progs: Vec<Vec<(u32, u8)>> = vec![vec![]; 16];
    for tr in &song.tracks {
        for e in &tr.evs {
            if let Ev::Prog { t, ch, prog } = e {
                progs[*ch as usize].push((*t, *prog));
            }
        }
    }
    for p in progs.iter_mut() {
        p.sort();
    }
    let mut parts: Vec<Part> = vec![];
    let mut len = 0f32;
    for (ti, tr) in song.tracks.iter().enumerate() {
        for e in &tr.evs {
            let Ev::Note(n) = e else { continue };
            let prog = progs[n.ch as usize].iter().rev().find(|p| p.0 <= n.start).map(|p| p.1).unwrap_or(0);
            let inst = if n.ch == 9 {
                Inst::Perc
            } else {
                by_name(&tr.name).unwrap_or_else(|| by_prog(prog))
            };
            let key = format!("{}|{}|{}", ti, n.ch, prog);
            let i = match parts.iter().position(|p| p.name == key) {
                Some(i) => i,
                None => {
                    parts.push(Part { inst, name: key, prog, notes: vec![] });
                    parts.len() - 1
                }
            };
            let t = n.start as f32 / ppq;
            let l = n.len as f32 / ppq;
            len = len.max(t + l);
            parts[i].notes.push(N { t, l, k: n.key, v: n.vel });
        }
    }
    for (p, tr) in parts.iter_mut().map(|p| {
        let ti: usize = p.name.split('|').next().unwrap().parse().unwrap();
        (p, ti)
    }) {
        p.name = format!("{} [{}]", song.tracks[tr].name.trim(), p.name);
        p.notes.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    }
    parts.retain(|p| p.notes.len() > 4);
    // Cordes génériques : pupitre déduit de la tessiture médiane
    for p in parts.iter_mut().filter(|p| p.inst == Inst::Strings) {
        let mut ks: Vec<u8> = p.notes.iter().map(|n| n.k).collect();
        ks.sort();
        let m = ks[ks.len() / 2];
        p.inst = match m {
            69.. => Inst::Violin,
            58..=68 => Inst::Viola,
            45..=57 => Inst::Cello,
            _ => Inst::Bass,
        };
    }
    for p in parts.iter_mut() {
        p.inst = match p.inst {
            Inst::EnglishHorn => Inst::Oboe,
            Inst::Marimba => Inst::Xylo,
            i => i,
        };
    }
    let bar = song
        .tracks
        .iter()
        .flat_map(|t| t.timesigs.first())
        .next()
        .map(|&(_, n, d)| n as f32 * 4. / d as f32)
        .unwrap_or(4.);
    let mut sigs: Vec<(f32, u8, u8)> = song
        .tracks
        .iter()
        .flat_map(|t| t.timesigs.iter().map(|&(t, n, d)| (t as f32 / ppq, n, d)))
        .collect();
    sigs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    sigs.dedup_by(|b, a| (b.0 - a.0).abs() < 0.01 || (b.1, b.2) == (a.1, a.2));
    if sigs.is_empty() || sigs[0].0 > 0.01 {
        sigs.insert(0, (0., 4, 4));
    }
    let mut tempos: Vec<(f32, f32)> = song
        .tracks
        .iter()
        .flat_map(|t| t.tempos.iter().map(|&(t, us)| (t as f32 / ppq, 60e6 / us.max(1) as f32)))
        .collect();
    tempos.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if tempos.is_empty() {
        tempos.push((0., 120.));
    }
    Some(Piece { file: path.rsplit('/').next().unwrap().to_string(), parts, len, bar, sigs, tempos })
}

pub fn load_all(dir: &str) -> Vec<Piece> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|p| p.ends_with(".mid") && !p.to_lowercase().contains("piano"))
        .collect();
    v.sort();
    std::panic::set_hook(Box::new(|_| {}));
    let r = v.iter().filter_map(|p| load(p)).collect();
    let _ = std::panic::take_hook();
    r
}
