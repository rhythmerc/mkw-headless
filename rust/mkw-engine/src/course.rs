//! buildAR's own course: an empty one, made here, in place of the disc's (so the race needs none of the disc's course
//! files). What the race needs of a course archive (found by leaving each file and each KMP section out in turn, the
//! free-drive test driving on each): a start point and the stage's settings in its KMP, and a collision (the stage's,
//! swapped in for the archive's as the race loads: `crate::stage`). No models (the course, its sky, its map: nothing
//! draws them), and no routes: no checkpoints (so no laps, no wrong way), no CPU or item routes, no cameras, no
//! respawn points.
//!
//! It is given to the game as a disc-shaped overlay ([`overlay`]): the course the race loads, replaced by this one.
use crate::kcl;
use engine_core::glam::Vec3;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The course the race loads (its course id 0x08: Luigi Circuit's slot), its path on the disc.
pub const REPLACES: &str = "Race/Course/beginner_course.szs";

/// Where the kart starts, at the floor the stage is laid on (game units): the course's middle.
pub const START: Vec3 = Vec3::ZERO;
/// The start point's height over that floor: the kart is put there and falls to the floor as the race loads.
const START_DROP: f32 = 70.0;

/// The KMP's sections, in the order the game reads them.
const SECTIONS: [&[u8; 4]; 15] =
    [b"KTPT", b"ENPT", b"ENPH", b"ITPT", b"ITPH", b"CKPT", b"CKPH", b"GOBJ", b"POTI", b"AREA", b"CAME", b"JGPT", b"CNPT", b"MSPT", b"STGI"];
/// The KMP version the game's courses carry.
const KMP_VERSION: u32 = 2520;

/// The course's KMP: one start point (facing the game's -z, as the race starts), the stage's settings, every other
/// section empty.
pub fn kmp() -> Vec<u8> {
    let mut sections: Vec<Vec<u8>> = Vec::new();
    for magic in SECTIONS {
        let mut s = magic.to_vec();
        match magic {
            b"KTPT" => {
                // (one: its position, its rotation (degrees: turned 180 about y, facing -z), the player index -1:
                // any)
                s.extend(1u16.to_be_bytes());
                s.extend(0u16.to_be_bytes());
                for v in [START.x, START.y + START_DROP, START.z, 0.0, 180.0, 0.0] {
                    s.extend(v.to_be_bytes());
                }
                s.extend((-1i16).to_be_bytes());
                s.extend(0u16.to_be_bytes());
            }
            b"STGI" => {
                // (one: laps 3, the pole on the left, normal spacing, no lens flare; its colour white at 0x4B; the
                // speed factor unchanged)
                s.extend(1u16.to_be_bytes());
                s.extend(0u16.to_be_bytes());
                s.extend([3, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0x4B, 0, 0, 0]);
            }
            _ => {
                s.extend(0u16.to_be_bytes());
                s.extend(0u16.to_be_bytes());
            }
        }
        sections.push(s);
    }
    let header = 16 + 4 * sections.len();
    let size = header + sections.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend(b"RKMD");
    out.extend((size as u32).to_be_bytes());
    out.extend((sections.len() as u16).to_be_bytes());
    out.extend((header as u16).to_be_bytes());
    out.extend(KMP_VERSION.to_be_bytes());
    let mut at = 0u32;
    for s in &sections {
        out.extend(at.to_be_bytes());
        at += s.len() as u32;
    }
    for s in sections {
        out.extend(s);
    }
    out
}

/// A collision for the archive until the stage's is swapped in: a floor 20 m across about the start.
fn placeholder_kcl() -> Vec<u8> {
    let h = 1000.0;
    let corners = [Vec3::new(-h, 0.0, -h), Vec3::new(-h, 0.0, h), Vec3::new(h, 0.0, h), Vec3::new(h, 0.0, -h)].map(|c| c + START);
    let floor = [
        kcl::Triangle { corners: [corners[0], corners[1], corners[2]], attribute: kcl::ROAD },
        kcl::Triangle { corners: [corners[0], corners[2], corners[3]], attribute: kcl::ROAD },
    ];
    kcl::write(&floor).expect("a floor's collision")
}

/// The course's archive (`.szs`): its KMP and its placeholder collision.
pub fn archive() -> Vec<u8> {
    let mut files = BTreeMap::new();
    files.insert("course.kmp".to_string(), kmp());
    files.insert("course.kcl".to_string(), placeholder_kcl());
    crate::szs::write(&files)
}

/// The overlay the game is given, under `data` (its root: disc-shaped, the course where the disc has the one it
/// replaces), written: its root.
pub fn overlay(data: &Path) -> std::io::Result<PathBuf> {
    let root = data.join("WiiCompiled").join("overlay");
    let file = root.join(REPLACES);
    std::fs::create_dir_all(file.parent().unwrap())?;
    let archive = archive();
    // (written again only when it changed)
    if std::fs::read(&file).ok().as_deref() != Some(&archive[..]) {
        std::fs::write(&file, archive)?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be16(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }
    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// Its KMP as the game reads it: the header's size and sections, each section where its offset says, one start
    /// point and the stage's settings, the rest empty.
    #[test]
    fn the_kmp_has_a_start_and_nothing_else() {
        let k = kmp();
        assert_eq!(&k[..4], b"RKMD");
        assert_eq!(be32(&k, 4) as usize, k.len());
        let (count, header) = (be16(&k, 8) as usize, be16(&k, 10) as usize);
        assert_eq!((count, header, be32(&k, 12)), (15, 0x4C, KMP_VERSION));
        for (i, magic) in SECTIONS.iter().enumerate() {
            let at = header + be32(&k, 16 + 4 * i) as usize;
            assert_eq!(&k[at..at + 4], *magic);
            let entries = be16(&k, at + 4);
            let wanted = if matches!(*magic, b"KTPT" | b"STGI") { 1 } else { 0 };
            assert_eq!(entries, wanted, "{}", String::from_utf8_lossy(*magic));
        }
        // (the start: at the start, a little over the floor, turned 180 about y)
        let ktpt = header + 8;
        let f = |at: usize| f32::from_be_bytes(k[at..at + 4].try_into().unwrap());
        assert_eq!((f(ktpt), f(ktpt + 4), f(ktpt + 8), f(ktpt + 16)), (START.x, START.y + START_DROP, START.z, 180.0));
    }

    /// The archive reads back as its two files, the collision a KCL the reader takes.
    #[test]
    fn the_archive_holds_the_kmp_and_a_collision() {
        let files = crate::szs::read(&archive()).unwrap();
        assert_eq!(files.keys().cloned().collect::<Vec<_>>(), ["course.kcl", "course.kmp"]);
        assert_eq!(files["course.kmp"], kmp());
        assert!(!kcl::Kcl::new(&files["course.kcl"]).near(START + Vec3::Y).is_empty(), "the floor under the start");
    }
}
