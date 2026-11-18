//! The game's sound as the room gets it, recorded while the kart drives (and, `ALL_SOUND=1`, with nothing muted):
//! `cargo run --release --example kart_sound -- out.wav`. Prints its loudness each half second.
use engine_core::glam::Vec3;
use engine_core::{PadHeld, PlacePlayer, Stage, TouchState};
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("an output .wav"));
    let lib = PathBuf::from(std::env::var("MKW_ENGINE_LIB").unwrap());
    let disc = PathBuf::from(std::env::var("MKW_DISC_DIR").unwrap());
    let game = mkw_engine::native::Game::load(&lib).unwrap();
    let tap = game.audio();
    drop(game);
    let data = std::env::temp_dir().join(format!("mkw-sound-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(lib, disc, data)().expect("the game");
    if std::env::var_os("ALL_SOUND").is_some() {
        mkw_engine::native::Game::load(&PathBuf::from(std::env::var("MKW_ENGINE_LIB").unwrap())).unwrap().mute_sound_players(0);
    }
    let floor = [Vec3::new(-30.0, 0.0, -30.0), Vec3::new(-30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, -30.0)];
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    engine.set_stage(&Stage { revision: 1, room_quads: vec![floor], ..Default::default() });
    while !engine.wants_steps() {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut sink = vec![0i16; 64000];
        tap.take(&mut sink);
    }
    let (drive, idle) = (TouchState { right_trigger: 1.0, left_stick: [0.6, 0.0], ..Default::default() }, TouchState::default());
    let mut pcm: Vec<i16> = Vec::new();
    let mut block = vec![0i16; 4096];
    for frame in 0..1200 {
        engine.set_input(if frame > 240 { &drive } else { &idle }, PadHeld::default());
        engine.step().unwrap();
        let n = tap.take(&mut block);
        pcm.extend_from_slice(&block[..n * 2]);
    }
    // (loudness each half second: root mean square, in dB of full scale)
    for (i, half) in pcm.chunks(32000).enumerate() {
        let rms = (half.iter().map(|&s| (f64::from(s) / 32768.0).powi(2)).sum::<f64>() / half.len().max(1) as f64).sqrt();
        println!("{:4.1} s: {:6.1} dB", i as f64 * 0.5, 20.0 * rms.max(1e-9).log10());
    }
    let mut w = Vec::new();
    let bytes = (pcm.len() * 2) as u32;
    w.extend(b"RIFF");
    w.extend((36 + bytes).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(32000u32.to_le_bytes());
    w.extend((32000u32 * 4).to_le_bytes());
    w.extend(4u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(bytes.to_le_bytes());
    for s in &pcm {
        w.extend(s.to_le_bytes());
    }
    std::fs::write(&out, w).unwrap();
    println!("{} frames to {}", pcm.len() / 2, out.display());
    std::process::exit(0);
}
