//! The kart's top speed and acceleration tuned (`settings::SPEED`, `settings::ACCELERATION`), end to end through the
//! engine contract: the player's game library (`MKW_ENGINE_LIB`) and extracted disc (`MKW_DISC_DIR`; skipped without
//! them). On a long floor, the kart put down at the start and driven, the trigger held, for six seconds a run: as the
//! game has it; at 1.5 times the top speed (it ends faster, and gets there no sooner); at twice the acceleration (it
//! is faster early on, and ends at the same top speed).
use engine_core::glam::Vec3;
use engine_core::{EngineCore, PadHeld, PlacePlayer, Stage, TouchState};
use mkw_engine::engine::settings;
use std::path::PathBuf;

/// A run's speeds, each frame (the game's metres a frame, as the stage has them).
fn run(engine: &mut Box<dyn EngineCore>, speed: f32, acceleration: f32) -> Vec<f32> {
    engine.set_setting(settings::SPEED, speed);
    engine.set_setting(settings::ACCELERATION, acceleration);
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    // (at rest at the start first)
    engine.set_input(&TouchState::default(), PadHeld::default());
    for _ in 0..90 {
        engine.step().expect("a frame");
    }
    let drive = TouchState { right_trigger: 1.0, ..Default::default() };
    engine.set_input(&drive, PadHeld::default());
    let mut last = engine.focus().expect("the kart").feet;
    let mut speeds = Vec::new();
    for _ in 0..360 {
        engine.step().expect("a frame");
        let now = engine.focus().expect("the kart").feet;
        speeds.push((now - last).length());
        last = now;
    }
    speeds
}

#[test]
fn the_karts_top_speed_and_acceleration_are_tuned() {
    let (Some(library), Some(disc)) = (std::env::var_os("MKW_ENGINE_LIB"), std::env::var_os("MKW_DISC_DIR")) else {
        eprintln!("MKW_ENGINE_LIB or MKW_DISC_DIR not set: skipped");
        return;
    };
    let data = std::env::temp_dir().join(format!("mkw-tuning-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(PathBuf::from(library), PathBuf::from(disc), data)().expect("the game");
    // (a floor 600 m along +x: no wall reached in six seconds at any speed tried)
    let m = |x: f32| x * 100.0 / mkw_engine::stage::UNITS;
    let stage = Stage {
        revision: 1,
        room_quads: vec![[Vec3::new(m(-20.0), 0.0, m(-20.0)), Vec3::new(m(-20.0), 0.0, m(20.0)), Vec3::new(m(600.0), 0.0, m(20.0)), Vec3::new(m(600.0), 0.0, m(-20.0))]],
        ..Default::default()
    };
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    engine.set_stage(&stage);
    let started = std::time::Instant::now();
    while !engine.wants_steps() {
        assert!(started.elapsed().as_secs() < 120, "the race starts");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let top = |s: &[f32]| s[s.len() - 30..].iter().sum::<f32>() / 30.0;
    // (how fast after half a second)
    let early = |s: &[f32]| s[25..35].iter().sum::<f32>() / 10.0;
    // (a first run, untimed: the race's first moments off the line are slower than any after)
    run(&mut engine, 1.0, 1.0);
    let own = run(&mut engine, 1.0, 1.0);
    let faster = run(&mut engine, 1.5, 1.0);
    let quicker = run(&mut engine, 1.0, 2.0);
    let again = run(&mut engine, 1.0, 1.0);
    for (name, s) in [("own", &own), ("speed x1.5", &faster), ("acceleration x2", &quicker), ("own again", &again)] {
        eprintln!("{name:>16}: after 0.5 s {:.4}, top {:.4} (a frame)", early(s), top(s));
    }
    assert!(top(&own) > 0.0, "it drives");
    let ratio = top(&faster) / top(&own);
    assert!((ratio - 1.5).abs() < 0.1, "x1.5 top speed: {ratio:.3} of its own");
    assert!(early(&quicker) > early(&own) * 1.3, "twice the acceleration: faster early on");
    assert!((top(&quicker) / top(&own) - 1.0).abs() < 0.05, "the same top speed with more acceleration");
    assert!((top(&again) / top(&own) - 1.0).abs() < 0.02, "back as the game has it");
    assert!((early(&again) / early(&own) - 1.0).abs() < 0.05, "and as quick off the line");
}
