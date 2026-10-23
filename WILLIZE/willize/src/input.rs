//! Analyse du MIDI de l'utilisateur : chaque piste est classée en thème,
//! ostinato, accords ou basse ; signature, tempo et tonalité sont repris.

use crate::corpus::N;
use crate::midi::{self, Ev};
use crate::train::{bars_of, Bar};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Theme,
    Ostinato,
    Chords,
    Bass,
}

pub struct Material {
    pub name: String,
    pub kind: Kind,
    pub notes: Vec<N>,
}

pub struct Input {
    pub items: Vec<Material>,
    pub bars: Vec<Bar>,
    pub bpm: f32,
    pub sigs: Vec<(f32, u8, u8)>,
}

fn classify(ns: &[N], bars: &[Bar]) -> Kind {
    let mut onsets: Vec<f32> = ns.iter().map(|n| (n.t * 12.).round() / 12.).collect();
    onsets.dedup();
    let poly = ns.len() as f32 / onsets.len().max(1) as f32;
    let avg = ns.iter().map(|n| n.l).sum::<f32>() / ns.len() as f32;
    let mut ks: Vec<u8> = ns.iter().map(|n| n.k).collect();
    ks.sort();
    let med = ks[ks.len() / 2];
    if poly >= 2.2 {
        return Kind::Chords;
    }
    // répétition mesure à mesure (hauteurs + rythme)
    let sig = |b: &Bar| -> Vec<(i32, u8)> {
        ns.iter().filter(|n| n.t >= b.t - 0.01 && n.t < b.t + b.len - 0.01).map(|n| (((n.t - b.t) * 4.).round() as i32, n.k)).collect()
    };
    let sigs: Vec<Vec<(i32, u8)>> = bars.iter().map(sig).filter(|s| !s.is_empty()).collect();
    let rep = sigs.windows(2).filter(|w| w[0] == w[1]).count() + sigs.windows(3).filter(|w| w[0] == w[2]).count();
    let rep = rep as f32 / (2 * sigs.len().saturating_sub(1)).max(1) as f32;
    if rep >= 0.4 && avg <= 1.0 && sigs.len() >= 2 {
        Kind::Ostinato
    } else if med < 48 {
        Kind::Bass
    } else {
        Kind::Theme
    }
}

pub fn read(path: &str, pick: &[String]) -> Input {
    let song = midi::read(&std::fs::read(path).expect("fichier MIDI illisible"));
    let ppq = song.ppq as f32;
    let mut sigs: Vec<(f32, u8, u8)> = song.tracks.iter().flat_map(|t| t.timesigs.iter().map(|&(t, n, d)| (t as f32 / ppq, n, d))).collect();
    sigs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if sigs.is_empty() || sigs[0].0 > 0.01 {
        sigs.insert(0, (0., 4, 4));
    }
    let bpm = song.tracks.iter().flat_map(|t| t.tempos.first()).next().map(|&(_, us)| 60e6 / us as f32).unwrap_or(120.);
    let mut tracks: Vec<(String, Vec<N>)> = vec![];
    for tr in &song.tracks {
        let ns: Vec<N> = tr
            .evs
            .iter()
            .filter_map(|e| match e {
                Ev::Note(n) if n.ch != 9 => Some(N { t: n.start as f32 / ppq, l: n.len as f32 / ppq, k: n.key, v: n.vel }),
                _ => None,
            })
            .collect();
        if !ns.is_empty() && (pick.is_empty() || pick.iter().any(|p| tr.name.to_lowercase().contains(&p.to_lowercase()))) {
            tracks.push((tr.name.trim().to_string(), ns));
        }
    }
    assert!(!tracks.is_empty(), "aucune piste retenue dans {}", path);
    // fenêtre : de la première mesure contenant du matériau à la dernière note
    let first = tracks.iter().flat_map(|t| t.1.iter().map(|n| n.t)).fold(f32::MAX, f32::min);
    let last = tracks.iter().flat_map(|t| t.1.iter().map(|n| n.t + n.l)).fold(0f32, f32::max);
    let all = bars_of(&sigs, last + 0.01);
    let b0 = all.iter().rposition(|b| b.t <= first + 0.01).unwrap_or(0);
    let off = all[b0].t;
    let bars: Vec<Bar> = all[b0..].iter().map(|b| Bar { t: b.t - off, len: b.len, num: b.num, den: b.den }).collect();
    let sigs2: Vec<(f32, u8, u8)> = {
        let mut v = vec![];
        for b in &bars {
            if v.last().is_none_or(|x: &(f32, u8, u8)| (x.1, x.2) != (b.num, b.den)) {
                v.push((b.t, b.num, b.den));
            }
        }
        v
    };
    let mut items = vec![];
    for (name, mut ns) in tracks {
        for n in ns.iter_mut() {
            n.t -= off;
        }
        ns.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        let kind = classify(&ns, &bars);
        items.push(Material { name, kind, notes: ns });
    }
    Input { items, bars, bpm, sigs: sigs2 }
}

/// Ligne monophonique (note la plus aiguë à chaque attaque), legato nettoyé.
pub fn mono(ns: &[N]) -> Vec<N> {
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
    for i in 0..v.len().saturating_sub(1) {
        let nt = v[i + 1].t;
        if v[i].t + v[i].l > nt {
            v[i].l = nt - v[i].t;
        }
        // une note qui s'arrête juste avant la suivante est tenue jusqu'à elle
        if nt - (v[i].t + v[i].l) < 0.3 {
            v[i].l = nt - v[i].t;
        }
    }
    v
}
