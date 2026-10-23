//! Lecture/écriture MIDI minimale, sans dépendance.

#[derive(Clone, Debug)]
pub struct Note {
    pub start: u32,
    pub len: u32,
    pub key: u8,
    pub vel: u8,
    pub ch: u8,
}

#[derive(Clone, Debug)]
pub enum Ev {
    Note(Note),
    Cc { t: u32, ch: u8, cc: u8, val: u8 },
    Prog { t: u32, ch: u8, prog: u8 },
}

#[derive(Default, Debug)]
pub struct Track {
    pub name: String,
    pub evs: Vec<Ev>,
    pub tempos: Vec<(u32, u32)>,
    pub timesigs: Vec<(u32, u8, u8)>,
}

pub struct Song {
    pub ppq: u16,
    pub tracks: Vec<Track>,
}

fn vlq(d: &[u8], p: &mut usize) -> u32 {
    let mut v = 0u32;
    loop {
        let b = d[*p];
        *p += 1;
        v = (v << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            return v;
        }
    }
}

pub fn read(d: &[u8]) -> Song {
    let ppq = u16::from_be_bytes([d[12], d[13]]);
    let mut p = 14;
    let mut tracks = vec![];
    while p + 8 <= d.len() {
        let len = (u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize).min(d.len() - p - 8);
        let body = &d[p + 8..p + 8 + len];
        p += 8 + len;
        let mut tr = Track::default();
        let (mut q, mut t, mut rs) = (0usize, 0u32, 0u8);
        let mut open: Vec<(u8, u8, u32, u8)> = vec![];
        while q < body.len() {
            t += vlq(body, &mut q);
            let mut st = body[q];
            if st & 0x80 != 0 {
                q += 1;
            } else {
                st = rs;
            }
            match st {
                0xff => {
                    let ty = body[q];
                    q += 1;
                    let l = vlq(body, &mut q) as usize;
                    let m = &body[q..q + l];
                    if ty == 3 {
                        tr.name = String::from_utf8_lossy(m).into_owned();
                    }
                    if ty == 0x58 && l >= 2 {
                        tr.timesigs.push((t, m[0], 1u8 << m[1].min(6)));
                    }
                    if ty == 0x51 {
                        tr.tempos.push((t, (m[0] as u32) << 16 | (m[1] as u32) << 8 | m[2] as u32));
                    }
                    q += l;
                }
                0xf0 | 0xf7 => {
                    let l = vlq(body, &mut q) as usize;
                    q += l;
                }
                _ => {
                    rs = st;
                    let ch = st & 15;
                    let a = body[q];
                    let two = !matches!(st & 0xf0, 0xc0 | 0xd0);
                    let b = if two { body[q + 1] } else { 0 };
                    q += if two { 2 } else { 1 };
                    match st & 0xf0 {
                        0x90 if b > 0 => open.push((ch, a, t, b)),
                        0x80 | 0x90 => {
                            if let Some(i) = open.iter().position(|o| o.0 == ch && o.1 == a) {
                                let o = open.remove(i);
                                tr.evs.push(Ev::Note(Note { start: o.2, len: t - o.2, key: a, vel: o.3, ch }));
                            }
                        }
                        0xb0 => tr.evs.push(Ev::Cc { t, ch, cc: a, val: b }),
                        0xc0 => tr.evs.push(Ev::Prog { t, ch, prog: a }),
                        _ => {}
                    }
                }
            }
        }
        tracks.push(tr);
    }
    Song { ppq, tracks }
}

fn wvlq(o: &mut Vec<u8>, mut v: u32) {
    let mut buf = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        buf.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    buf.reverse();
    o.extend(buf);
}

pub fn write(s: &Song) -> Vec<u8> {
    let mut o = b"MThd".to_vec();
    o.extend(6u32.to_be_bytes());
    o.extend(1u16.to_be_bytes());
    o.extend((s.tracks.len() as u16).to_be_bytes());
    o.extend(s.ppq.to_be_bytes());
    for tr in &s.tracks {
        // (temps, priorité, octets) : note-off avant note-on au même tick
        let mut raw: Vec<(u32, u8, Vec<u8>)> = vec![];
        let mut nm = vec![0xff, 3];
        wvlq(&mut nm, tr.name.len() as u32);
        nm.extend(tr.name.as_bytes());
        raw.push((0, 0, nm));
        for &(t, n, d) in &tr.timesigs {
            raw.push((t, 0, vec![0xff, 0x58, 4, n, d.trailing_zeros() as u8, 24, 8]));
        }
        for &(t, us) in &tr.tempos {
            raw.push((t, 0, vec![0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8]));
        }
        for e in &tr.evs {
            match e {
                Ev::Note(n) => {
                    raw.push((n.start, 3, vec![0x90 | n.ch, n.key, n.vel.max(1)]));
                    raw.push((n.start + n.len, 2, vec![0x80 | n.ch, n.key, 0]));
                }
                Ev::Cc { t, ch, cc, val } => raw.push((*t, 1, vec![0xb0 | ch, *cc, *val])),
                Ev::Prog { t, ch, prog } => raw.push((*t, 0, vec![0xc0 | ch, *prog])),
            }
        }
        raw.sort_by_key(|r| (r.0, r.1));
        let mut b = vec![];
        let mut last = 0;
        for (t, _, bytes) in raw {
            wvlq(&mut b, t - last);
            last = t;
            b.extend(bytes);
        }
        b.extend([0, 0xff, 0x2f, 0]);
        o.extend(b"MTrk");
        o.extend((b.len() as u32).to_be_bytes());
        o.extend(b);
    }
    o
}
