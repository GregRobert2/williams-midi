mod arrange;
mod corpus;
mod harmony;
mod input;
mod melody;
mod midi;
mod train;

use arrange::{Arranger, Mood, Opts, Score, PALETTE};
use corpus::Inst;
use harmony::Key;
use midi::{Ev, Note, Song, Track};
use std::process::Command;

const USAGE: &str = "willize — moteur d'orchestration appris sur John Williams

  willize train [--corpus DIR] [--report FICHIER.md]
      apprend sur tous les MIDI du corpus (sauf piano) et écrit le rapport

  willize arrange ENTRÉE.mid [options]
      --track NOM        pistes à utiliser (répétable ; défaut : toutes)
      --mood M           majestic | lyrical | action | mysterious (défaut majestic)
      --seconds S        durée visée (défaut 60)
      --bpm B            tempo (défaut : celui du fichier)
      --key K            tonalité, ex. Em, D, F#m (défaut : détectée)
      --hr X             rythme harmonique (1 = Williams moyen, 0.7 = plus lent)
      --carrier I        porteur du 1er exposé : horn, violin, trumpet, oboe…
      --modulate         module le 2e exposé (intervalle appris)
      --add-theme        compose un nouveau thème par-dessus le matériau
      --out FICHIER.mid  (défaut : <entrée>_willize.mid)
      --mp3 FICHIER.mp3  rendu audio GM de contrôle
      --corpus DIR       corpus Williams (défaut : dossier du dépôt)
";

fn default_corpus() -> String {
    format!("{}/../..", env!("CARGO_MANIFEST_DIR"))
}

fn parse_key(s: &str) -> Key {
    let (n, minor) = match s.strip_suffix('m') {
        Some(x) => (x, true),
        None => (s, false),
    };
    let tonic = train::PCN.iter().position(|p| p.eq_ignore_ascii_case(n)).or(match n {
        "Db" => Some(1),
        "D#" => Some(3),
        "Gb" => Some(6),
        "G#" => Some(8),
        "A#" => Some(10),
        _ => None,
    });
    Key { tonic: tonic.expect("tonalité inconnue") as u8, maj: !minor }
}

fn parse_inst(s: &str) -> Inst {
    match s.to_lowercase().as_str() {
        "horn" | "horns" | "cor" | "cors" => Inst::Horn,
        "violin" | "violins" | "violon" | "violons" => Inst::Violin,
        "viola" | "alto" | "altos" => Inst::Viola,
        "cello" | "cellos" | "violoncelle" => Inst::Cello,
        "trumpet" | "trompette" | "trompettes" => Inst::Trumpet,
        "trombone" | "trombones" => Inst::Trombone,
        "flute" | "flûte" => Inst::Flute,
        "oboe" | "hautbois" => Inst::Oboe,
        "clarinet" | "clarinette" => Inst::Clarinet,
        _ => panic!("instrument inconnu : {}", s),
    }
}

fn to_song(sc: &Score, render: Option<usize>) -> Song {
    const PPQ: u32 = 480;
    let tk = |b: f32| (b * PPQ as f32).round() as u32;
    let mut cond = Track { name: "WILLIZE".into(), ..Default::default() };
    for (t, bpm) in &sc.tempos {
        cond.tempos.push((tk(*t), (60e6 / bpm) as u32));
    }
    let mut last = (0, 0);
    for b in &sc.bars {
        if (b.num, b.den) != last {
            cond.timesigs.push((tk(b.t), b.num, b.den));
            last = (b.num, b.den);
        }
    }
    let mut tracks = vec![cond];
    let mut chn = 0u8;
    for (i, dk) in PALETTE.iter().enumerate() {
        if render.is_some_and(|r| r != i) {
            continue;
        }
        let ch = if dk.perc.is_some() {
            9
        } else if render.is_some() {
            0
        } else {
            let c = chn;
            chn = (chn + 1) % 16;
            if chn == 9 {
                chn = 10;
            }
            c
        };
        let mut evs = vec![];
        if dk.perc.is_none() {
            evs.push(Ev::Prog { t: 0, ch, prog: dk.prog });
        }
        let cc = if render.is_some() { 11 } else { 1 };
        if !matches!(dk.inst, Inst::Perc | Inst::Timpani | Inst::Harp | Inst::Glock | Inst::Celesta | Inst::Xylo) {
            for (t, v) in &sc.cc[i] {
                evs.push(Ev::Cc { t: tk(*t), ch, cc, val: v.clamp(1., 127.) as u8 });
            }
        }
        for n in &sc.parts[i] {
            evs.push(Ev::Note(Note { start: tk(n.t), len: tk(n.l).max(20), key: n.k, vel: n.v, ch }));
        }
        tracks.push(Track { name: dk.name.into(), evs, ..Default::default() });
    }
    Song { ppq: PPQ as u16, tracks }
}

/// Rendu GM : chaque pupitre rendu séparément (canal et expression propres), puis mixage.
fn render(sc: &Score, mp3: &str) {
    let dir = std::env::temp_dir().join(format!("willize_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sf = "/usr/share/sounds/sf2/FluidR3_GM.sf2";
    let gain = |name: &str| -> f32 {
        match name {
            "Horns" => 1.3,
            "Trumpets" | "Trombones" => 0.9,
            "Violins 1" | "Violins 2" => 1.1,
            "Harp" => 1.1,
            "Glockenspiel" | "Celesta" | "Xylophone" => 0.6,
            "Piatti" => 0.7,
            _ => 1.0,
        }
    };
    let mut wavs = vec![];
    for (i, dk) in PALETTE.iter().enumerate() {
        if sc.parts[i].is_empty() {
            continue;
        }
        let mid = dir.join(format!("{}.mid", i));
        let wav = dir.join(format!("{}.wav", i));
        std::fs::write(&mid, midi::write(&to_song(sc, Some(i)))).unwrap();
        let ok = Command::new("fluidsynth")
            .args(["-ni", "-g", "0.5", "-R", "1", "-r", "44100", "-F"])
            .arg(&wav)
            .arg(sf)
            .arg(&mid)
            .output()
            .is_ok_and(|o| o.status.success());
        if ok {
            wavs.push((wav, gain(dk.name)));
        }
    }
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error"]);
    for (w, _) in &wavs {
        cmd.arg("-i").arg(w);
    }
    let mut f = String::new();
    for (j, (_, g)) in wavs.iter().enumerate() {
        f += &format!("[{}]volume={}[a{}];", j, g, j);
    }
    for j in 0..wavs.len() {
        f += &format!("[a{}]", j);
    }
    f += &format!("amix=inputs={}:normalize=0,silenceremove=stop_periods=1:stop_duration=1.5:stop_threshold=-60dB,loudnorm=I=-16:TP=-1", wavs.len());
    cmd.args(["-filter_complex", &f, "-b:a", "192k", mp3]);
    let st = cmd.status().expect("ffmpeg absent");
    assert!(st.success(), "mixage ffmpeg échoué");
    let _ = std::fs::remove_dir_all(&dir);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let flag = |n: &str| a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned();
    let has = |n: &str| a.iter().any(|x| x == n);
    let corpus_dir = flag("--corpus").unwrap_or_else(default_corpus);
    match a.get(1).map(String::as_str) {
        Some("train") => {
            let c = corpus::load_all(&corpus_dir);
            let m = train::train(&c);
            let out = flag("--report").unwrap_or_else(|| "ANALYSE_WILLIAMS.md".into());
            std::fs::write(&out, train::report(&m)).unwrap();
            println!("{} pièces apprises → {}", m.pieces, out);
        }
        Some("arrange") => {
            let inp_path = a.get(2).expect(USAGE);
            let picks: Vec<String> = a.windows(2).filter(|w| w[0] == "--track").map(|w| w[1].clone()).collect();
            let inp = input::read(inp_path, &picks);
            let c = corpus::load_all(&corpus_dir);
            let m = train::train(&c);
            let mood = match flag("--mood").as_deref().unwrap_or("majestic") {
                "lyrical" | "lyrique" => Mood::Lyrical,
                "action" => Mood::Action,
                "mysterious" | "mystérieux" => Mood::Mysterious,
                _ => Mood::Majestic,
            };
            let opts = Opts {
                mood,
                seconds: flag("--seconds").map(|s| s.parse().unwrap()).unwrap_or(60.),
                bpm: flag("--bpm").map(|s| s.parse().unwrap()),
                key: flag("--key").map(|s| parse_key(&s)),
                hr: flag("--hr").map(|s| s.parse().unwrap()).unwrap_or(1.),
                modulate: has("--modulate"),
                add_theme: has("--add-theme"),
                carrier: flag("--carrier").map(|s| parse_inst(&s)),
            };
            let sc = Arranger::new(&m, opts).run(&inp);
            let out = flag("--out").unwrap_or_else(|| inp_path.trim_end_matches(".mid").trim_end_matches(".midi").to_string() + "_willize.mid");
            std::fs::write(&out, midi::write(&to_song(&sc, None))).unwrap();
            for l in &sc.plan {
                println!("{}", l);
            }
            println!("→ {}", out);
            if let Some(mp3) = flag("--mp3") {
                render(&sc, &mp3);
                println!("→ {}", mp3);
            }
        }
        Some("survey") => {
            let c = corpus::load_all(&corpus_dir);
            for p in &c {
                println!("## {}", p.file);
                for pt in &p.parts {
                    println!("   {:?} p{} {} n={}", pt.inst, pt.prog, pt.name, pt.notes.len());
                }
            }
        }
        Some("dump") => {
            let s = midi::read(&std::fs::read(&a[2]).unwrap());
            println!("ppq {}", s.ppq);
            for (i, t) in s.tracks.iter().enumerate() {
                println!("== {} '{}' evs={} sigs={:?} tempos={:?}", i, t.name, t.evs.len(), t.timesigs, t.tempos);
                if a.len() > 3 && t.name.contains(&a[3]) {
                    for e in &t.evs {
                        if let Ev::Note(n) = e {
                            println!("{:>7} {:>5} k{} v{} ch{}", n.start, n.len, n.key, n.vel, n.ch);
                        }
                    }
                }
            }
        }
        _ => print!("{}", USAGE),
    }
}
