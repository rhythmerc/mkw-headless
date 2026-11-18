//! Mario Kart Wii's side of the buildAR engine contract, over WiiCompiled's static recompilation of the player's own
//! disc. No Bevy, no game data: the game itself is a library built from the player's disc
//! (`libmkw_engine`, its C API `mkw_engine.h`), loaded from where they keep it.
//!
//! - [`kcl`]: course collision (KCL) written from triangles, and read back.
//! - [`native`]: the game library, loaded.
//! - [`stage`]: the stage as the course's collision, and the contract's space as the game's.
//! - [`szs`]: the game's archives (Yaz0, U8).
//! - [`engine`]: the game as the runtime drives it ([`engine_core::EngineCore`]): free driving on the stage.
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
