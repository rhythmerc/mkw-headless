//! The game's own driving, kept with the stage laid where the player puts it and the kart tuned slow, end to end
//! through the engine contract: the player's game library (`MKW_ENGINE_LIB`) and extracted disc (`MKW_DISC_DIR`;
//! skipped without them). One race, put down three times:
//! - on a platform 1.5 m up, driven off it: it lands on the floor and drives on (the floor is under where it was put,
//!   and the game sends a kart under its height 0 out of bounds - the stage is laid high: `course::START`);
//! - at the least throttle, the drift held while it turns: it keeps going (not taken for standing still and turned on
//!   the spot), and with the throttle opened in the drift it speeds up;
//! - at 30% top speed into a wall: it bumps back off it (the speed lost that bumps it is a share of the top speed).
use engine_core::glam::Vec3;
use engine_core::{EngineCore, PadHeld, PlacePlayer, Stage, TouchState};
use mkw_engine::engine::settings;
use std::path::PathBuf;

/// Room metres in the stage's (contract) units.
fn m(x: f32) -> f32 {
    x * 100.0 / mkw_engine::stage::UNITS
}

fn floor(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> [Vec3; 4] {
    [Vec3::new(m(x0), m(y), m(z0)), Vec3::new(m(x0), m(y), m(z1)), Vec3::new(m(x1), m(y), m(z1)), Vec3::new(m(x1), m(y), m(z0))]
}

/// A wall across x at `x`, 3 m high.
fn wall(x: f32, z0: f32, z1: f32) -> [Vec3; 4] {
    [Vec3::new(m(x), 0.0, m(z0)), Vec3::new(m(x), 0.0, m(z1)), Vec3::new(m(x), m(3.0), m(z1)), Vec3::new(m(x), m(3.0), m(z0))]
}

/// `frames` frames with `touch`: where the kart is after each (room metres).
fn drive(engine: &mut Box<dyn EngineCore>, touch: &TouchState, frames: usize) -> Vec<Vec3> {
    engine.set_input(touch, PadHeld::default());
    (0..frames)
        .map(|_| {
            engine.step().expect("a frame");
            engine.focus().expect("the kart").feet / m(1.0)
        })
        .collect()
}

/// Put down at `at` (room metres) facing +x, at rest a second.
fn put(engine: &mut Box<dyn EngineCore>, at: Vec3) {
    let before = engine.focus().map(|f| f.feet / m(1.0));
    engine.place_player(PlacePlayer { point: at * m(1.0), forward: Vec3::X });
    let after = drive(engine, &TouchState::default(), 60);
    if std::env::var_os("MKW_TRACE").is_some() {
        eprintln!("put at {at} (from {before:?}): {} after a frame, {} after 10, {} after a second", after[0], after[9], after[59]);
    }
}

#[test]
fn the_kart_drives_as_the_game_does_when_put_high_and_tuned_slow() {
    let (Some(library), Some(disc)) = (std::env::var_os("MKW_ENGINE_LIB"), std::env::var_os("MKW_DISC_DIR")) else {
        eprintln!("MKW_ENGINE_LIB or MKW_DISC_DIR not set: skipped");
        return;
    };
    let data = std::env::temp_dir().join(format!("mkw-tuned-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(PathBuf::from(library), PathBuf::from(disc), data)().expect("the game");
    // (a floor 300 m square; a platform 1.5 m up west of its middle; a wall across it at x 40)
    let stage = Stage {
        revision: 1,
        room_quads: vec![floor(-150.0, -150.0, 150.0, 150.0, 0.0), floor(-50.0, -4.0, -42.0, 4.0, 1.5), wall(40.0, -150.0, 150.0)],
        ..Default::default()
    };
    engine.place_player(PlacePlayer { point: Vec3::new(m(-46.0), m(1.5), 0.0), forward: Vec3::X });
    engine.set_stage(&stage);
    let started = std::time::Instant::now();
    while !engine.wants_steps() {
        assert!(started.elapsed().as_secs() < 120, "the race starts");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let full = TouchState { right_trigger: 1.0, ..Default::default() };

    // Off the platform onto the floor: never taken back (a jump), on the floor and on its way.
    put(&mut engine, Vec3::new(-46.0, 1.5, 0.0));
    let track = drive(&mut engine, &full, 300);
    let jump = track.windows(2).map(|w| (w[1] - w[0]).length()).fold(0.0, f32::max);
    let end = *track.last().unwrap();
    eprintln!("off the platform: to {end}, the most in a frame {jump:.2} m");
    assert!(jump < 1.5, "taken back out of bounds: {jump:.2} m in a frame");
    assert!(end.y.abs() < 0.2 && end.x > -20.0, "on the floor, driven on: {end}");

    // The least throttle, then the drift held turning; then the throttle opened in the drift.
    put(&mut engine, Vec3::new(-60.0, 0.0, -60.0));
    let least = TouchState { right_trigger: 0.06, ..Default::default() };
    drive(&mut engine, &least, 120);
    let drift = |trigger: f32| TouchState { right_trigger: trigger, right_grip: 1.0, left_stick: [1.0, 0.0], ..Default::default() };
    let slow = drive(&mut engine, &drift(0.06), 120);
    let opened = drive(&mut engine, &drift(1.0), 120);
    let pace = |t: &[Vec3]| t.windows(2).map(|w| (w[1] - w[0]).length()).sum::<f32>() / (t.len() - 1) as f32;
    let (slow_pace, opened_pace) = (pace(&slow[60..]), pace(&opened[60..]));
    eprintln!("drifting at the least throttle: {slow_pace:.3} m a frame; opened: {opened_pace:.3}");
    assert!(slow_pace > 0.05, "drifting slow, it keeps going: {slow_pace:.3} m a frame");
    assert!(opened_pace > slow_pace * 3.0, "the throttle opened in the drift, it speeds up: {opened_pace:.3}");
    // (to a stop: a kart put down keeps its speed - the stage is laid under it where it is)
    drive(&mut engine, &TouchState { left_trigger: 1.0, ..Default::default() }, 60);
    drive(&mut engine, &TouchState::default(), 240);

    // At 30% top speed into the wall: bumped back off it.
    engine.set_setting(settings::SPEED, 0.3);
    put(&mut engine, Vec3::new(25.0, 0.0, 0.0));
    let track = drive(&mut engine, &full, 300);
    let back = track.windows(2).map(|w| w[1].x - w[0].x).fold(0.0, f32::min);
    eprintln!("into the wall at 30%: the most back in a frame {back:.3} m");
    if std::env::var_os("MKW_TRACE").is_some() {
        for (i, w) in track.windows(2).enumerate().step_by(5).take(30) {
            eprintln!("  wall frame {i}: x {:.2} y {:.2} z {:.2} dx {:.3}", w[1].x, w[1].y, w[1].z, w[1].x - w[0].x);
        }
    }
    assert!(track.iter().all(|p| p.x < 40.0), "not through the wall");
    assert!(back < -0.02, "bumped back off the wall: {back:.3} m a frame");
}
