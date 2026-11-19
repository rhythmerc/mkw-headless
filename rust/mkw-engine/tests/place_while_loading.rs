//! The player put down while the race loads (the stage laid, no kart yet): the kart is there once the race runs, not at
//! the race's start where the stage was first laid. The app shows the kart only once the player has put it down, so
//! the first placement often comes while the race still loads. The player's game library (`MKW_ENGINE_LIB`) and
//! extracted disc (`MKW_DISC_DIR`; skipped without them). Its own file: the game is one a process.
use engine_core::glam::Vec3;
use engine_core::{Applied, PadHeld, PlacePlayer, Stage, TouchState};
use std::path::PathBuf;

#[test]
fn a_kart_put_down_while_the_race_loads_is_there_once_it_runs() {
    let (Some(library), Some(disc)) = (std::env::var_os("MKW_ENGINE_LIB"), std::env::var_os("MKW_DISC_DIR")) else {
        eprintln!("MKW_ENGINE_LIB or MKW_DISC_DIR not set: skipped");
        return;
    };
    let data = std::env::temp_dir().join(format!("mkw-place-loading-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(PathBuf::from(library), PathBuf::from(disc), data.clone())().expect("the game");
    let m = |x: f32| x * 100.0 / mkw_engine::stage::UNITS;
    let stage = Stage {
        revision: 1,
        room_quads: vec![[Vec3::new(m(-30.0), 0.0, m(-30.0)), Vec3::new(m(-30.0), 0.0, m(30.0)), Vec3::new(m(30.0), 0.0, m(30.0)), Vec3::new(m(30.0), 0.0, m(-30.0))]],
        ..Default::default()
    };
    // (no placement yet: the stage laid with the race's start at the origin, and the race loading)
    assert_eq!(engine.set_stage(&stage), Applied::Done);
    assert!(!engine.wants_steps(), "still loading");
    let spot = Vec3::new(m(8.0), 0.0, m(-6.0));
    engine.place_player(PlacePlayer { point: spot, forward: Vec3::X });
    let started = std::time::Instant::now();
    while !engine.wants_steps() {
        assert!(started.elapsed().as_secs() < 120, "the race starts");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    engine.set_input(&TouchState::default(), PadHeld::default());
    for _ in 0..60 {
        engine.step().expect("a frame");
    }
    let focus = engine.focus().expect("the kart");
    eprintln!("put down at {spot} while loading; after a second of the race: {} facing {}", focus.feet, focus.forward);
    assert!((focus.feet - spot).length() < m(1.0), "where it was put down: {}", focus.feet);
    assert!(focus.forward.dot(Vec3::X) > 0.9, "facing the way it was put down: {}", focus.forward);
    let _ = std::fs::remove_dir_all(data);
}
