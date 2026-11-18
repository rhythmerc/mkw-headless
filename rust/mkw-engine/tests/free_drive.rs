//! The game driving on a stage, end to end through the engine contract: the player's game library
//! (`MKW_ENGINE_LIB`) and extracted disc (`MKW_DISC_DIR`; skipped without them). A floor, a wall across the way ahead,
//! the player put down facing +x; the right trigger held: the kart drives along +x on the floor and stops at the wall.
use engine_core::glam::Vec3;
use engine_core::{Applied, PadHeld, PlacePlayer, Stage, TouchState};
use std::path::PathBuf;

#[test]
fn the_kart_drives_on_the_stage_and_stops_at_its_wall() {
    let (Some(library), Some(disc)) = (std::env::var_os("MKW_ENGINE_LIB"), std::env::var_os("MKW_DISC_DIR")) else {
        eprintln!("MKW_ENGINE_LIB or MKW_DISC_DIR not set: skipped");
        return;
    };
    let data = std::env::temp_dir().join(format!("mkw-free-drive-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(PathBuf::from(library), PathBuf::from(disc), data.clone())().expect("the game");

    // (metres: a 60 m floor; a wall 25 m ahead along +x, across it)
    let stage = Stage {
        revision: 1,
        room_quads: vec![
            [Vec3::new(-30.0, 0.0, -30.0), Vec3::new(-30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, -30.0)],
            [Vec3::new(25.0, 0.0, -30.0), Vec3::new(25.0, 0.0, 30.0), Vec3::new(25.0, 3.0, 30.0), Vec3::new(25.0, 3.0, -30.0)],
        ],
        ..Default::default()
    };
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    assert_eq!(engine.set_stage(&stage), Applied::Done);
    let models = engine.models();
    assert_eq!(models.len(), 1, "the kart's stand-in");

    let started = std::time::Instant::now();
    while !engine.wants_steps() {
        assert!(started.elapsed().as_secs() < 120, "the race starts");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let drive = TouchState { right_trigger: 1.0, ..Default::default() };
    let idle = TouchState::default();
    let mut track = Vec::new();
    for frame in 0..1500 {
        // (the countdown first, about four seconds: then the trigger)
        engine.set_input(if frame > 240 { &drive } else { &idle }, PadHeld::default());
        engine.step().expect("a frame");
        let focus = engine.focus().expect("the kart");
        track.push(focus.feet);
        if frame % 30 == 0 && std::env::var_os("MKW_TRACE").is_some() {
            eprintln!("frame {frame}: feet {} forward {}", focus.feet, focus.forward);
        }
    }
    let start = track[200];
    let end = *track.last().unwrap();
    eprintln!("kart: from {start} to {end}; at frame 600 {}", track[600]);
    assert!(start.y.abs() < 0.2 && start.x.abs() < 0.5 && start.z.abs() < 0.5, "put down where it was asked, on the floor: {start}");
    assert!(track[600].x > start.x + 3.0, "it drives forward (+x): {}", track[600]);
    assert!(track.iter().all(|p| p.x < 25.0), "never through the wall");
    assert!(end.x > 20.0 && end.y.abs() < 0.3, "held at the wall, on the floor: {end}");

    // Put down again at the start (the stage laid again under the kart), and a wall added 10 m ahead: it drives from
    // the start again and stops at the new wall.
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    let mut stage2 = stage.clone();
    stage2.revision = 2;
    stage2.room_quads.push([Vec3::new(10.0, 0.0, -30.0), Vec3::new(10.0, 0.0, 30.0), Vec3::new(10.0, 3.0, 30.0), Vec3::new(10.0, 3.0, -30.0)]);
    assert_eq!(engine.set_stage(&stage2), Applied::Done);
    engine.set_input(&idle, PadHeld::default());
    for _ in 0..60 {
        engine.step().expect("a frame");
    }
    let again = engine.focus().expect("the kart").feet;
    eprintln!("put down again: {again}");
    assert!(again.x.abs() < 1.0 && again.y.abs() < 0.3, "back at the start: {again}");
    engine.set_input(&drive, PadHeld::default());
    let mut last = again;
    for _ in 0..600 {
        engine.step().expect("a frame");
        last = engine.focus().expect("the kart").feet;
        assert!(last.x < 10.0, "never through the new wall: {last}");
    }
    eprintln!("then: {last}");
    assert!(last.x > 7.0 && last.y.abs() < 0.3, "held at the new wall: {last}");
    let _ = std::fs::remove_dir_all(data);
}
