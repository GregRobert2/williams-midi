mod compose;
mod midi;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("gen") => {
            std::fs::write(&a[2], midi::write(&compose::build(false))).unwrap();
            std::fs::write(&a[3], midi::write(&compose::build(true))).unwrap();
        }
        Some("dump") => {
            let s = midi::read(&std::fs::read(&a[2]).unwrap());
            println!("ppq {}", s.ppq);
            for (i, t) in s.tracks.iter().enumerate() {
                println!("== {} '{}' evs={} tempos={:?}", i, t.name, t.evs.len(), t.tempos);
                if a.len() > 3 && t.name.contains(&a[3]) {
                    for e in &t.evs {
                        if let midi::Ev::Note(n) = e {
                            println!("{:>6} {:>5} k{} v{}", n.start, n.len, n.key, n.vel);
                        }
                    }
                }
            }
        }
        _ => eprintln!("usage: willize gen <out.mid> <render.mid> | dump <in.mid> [piste]"),
    }
}
