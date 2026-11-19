//! Mario Kart Wii's side of the buildAR engine contract, over WiiCompiled's static recompilation of the player's own
//! disc. No Bevy, no game data: the game itself is a library built from the player's disc
//! (`libmkw_engine`, its C API `mkw_engine.h`), loaded from where they keep it.
//!
//! - [`kcl`]: course collision (KCL) written from triangles, and read back.
//! - [`native`]: the game library, loaded.
//! - [`stage`]: the stage as the course's collision, and the contract's space as the game's.
//! - [`szs`]: the game's archives (Yaz0, U8).
//! - [`course`]: buildAR's own course, empty, in place of the disc's.
//! - [`engine`]: the game as the runtime drives it ([`engine_core::EngineCore`]): free driving on the stage.
pub mod course;
pub mod engine;
pub mod kcl;
pub mod native;
pub mod stage;
pub mod szs;
pub mod sound;
pub mod kart_model;

/// The game files: the player's extracted disc (`files/` and `sys/`) with the game library built from it.
pub const DATA: engine_core::GameFiles = engine_core::GameFiles {
    drop: "MKWii",
    folder: "mkwii",
    marker: "sys/main.dol",
    beside: &[],
    how: &[
        "On a PC, once: build buildAR's Mario Kart Wii engine from your own",
        "disc (mkw-headless), then copy its folder (the extracted disc",
        "with libmkw_engine.so) to Download/MKWii and import it here.",
    ],
};

/// Parts of the player's disc the engine never reads: the cutscene videos (1.8 GB of a 2.5 GB disc; the game boots and
/// races without them) and the courses (167 MB: the race is on buildAR's own, [`course`]).
pub const UNUSED: &[&str] = &["files/thp", "files/Race/Course"];

/// The unused parts ([`UNUSED`]) removed from the player's copy of the disc at `disc`, to give their space back: the
/// bytes freed.
pub fn leave_out_unused(disc: &std::path::Path) -> std::io::Result<u64> {
    let mut freed = 0;
    for part in UNUSED {
        let at = disc.join(part);
        if !at.is_dir() {
            continue;
        }
        freed += size_of(&at);
        std::fs::remove_dir_all(&at)?;
    }
    Ok(freed)
}

fn size_of(path: &std::path::Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path).into_iter().flatten().flatten().map(|e| size_of(&e.path())).sum(),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    /// The videos go; the rest of the disc stays.
    #[test]
    fn the_unused_parts_are_left_out() {
        let disc = std::env::temp_dir().join(format!("mkw-unused-{}", std::process::id()));
        std::fs::create_dir_all(disc.join("files/thp/title")).unwrap();
        std::fs::create_dir_all(disc.join("files/Race")).unwrap();
        std::fs::write(disc.join("files/thp/title/title.thp"), vec![0u8; 1000]).unwrap();
        std::fs::write(disc.join("files/Race/Common.szs"), b"kept").unwrap();
        assert_eq!(super::leave_out_unused(&disc).unwrap(), 1000);
        assert!(!disc.join("files/thp").exists() && disc.join("files/Race/Common.szs").exists());
        assert_eq!(super::leave_out_unused(&disc).unwrap(), 0);
        let _ = std::fs::remove_dir_all(disc);
    }
}
