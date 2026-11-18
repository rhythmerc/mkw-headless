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

    // (at the game's own scale, 100 units a metre: a 60 m floor, a wall 25 m ahead along +x; `m` such a metre in the
    // room at the engine's scale)
    let k = 100.0 / mkw_engine::stage::UNITS;
    let m = |x: f32| x * k;
    let stage = Stage {
        revision: 1,
        room_quads: vec![
            [Vec3::new(m(-30.0), m(0.0), m(-30.0)), Vec3::new(m(-30.0), m(0.0), m(30.0)), Vec3::new(m(30.0), m(0.0), m(30.0)), Vec3::new(m(30.0), m(0.0), m(-30.0))],
            [Vec3::new(m(25.0), m(0.0), m(-30.0)), Vec3::new(m(25.0), m(0.0), m(30.0)), Vec3::new(m(25.0), m(3.0), m(30.0)), Vec3::new(m(25.0), m(3.0), m(-30.0))],
        ],
        ..Default::default()
    };
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    assert_eq!(engine.set_stage(&stage), Applied::Done);
    let models = engine.models();
    assert_eq!(models.len(), 1, "the kart");

    let started = std::time::Instant::now();
    while !engine.wants_steps() {
        assert!(started.elapsed().as_secs() < 120, "the race starts");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let drive = TouchState { right_trigger: 1.0, ..Default::default() };
    let idle = TouchState::default();
    let mut track = Vec::new();
    // (the kart's parts, each joint's place in the body's space: at rest, and while it drives)
    let parts = |engine: &mut Box<dyn engine_core::EngineCore>| {
        let mut poses = Vec::new();
        engine.poses(&mut poses);
        poses.first().map(|p| p.joints.iter().map(|j| j.w_axis.truncate()).collect::<Vec<_>>())
    };
    let (mut at_rest, mut driving) = (None, None);
    for frame in 0..1500 {
        // (the countdown first, about four seconds: then the trigger)
        engine.set_input(if frame > 240 { &drive } else { &idle }, PadHeld::default());
        engine.step().expect("a frame");
        let focus = engine.focus().expect("the kart");
        track.push(focus.feet);
        if frame == 200 {
            at_rest = parts(&mut engine);
        }
        if frame == 650 {
            driving = parts(&mut engine);
            eprintln!("speed at frame 650: {:.3} m a frame", (track[650] - track[649]).length());
        }
        if frame % 30 == 0 && std::env::var_os("MKW_TRACE").is_some() {
            eprintln!("frame {frame}: feet {} forward {}", focus.feet, focus.forward);
        }
    }
    // (the kart drawn with the game's own models: the body at the kart, the wheels and Mario about it; while it drives,
    // each part where it was at rest about the body - this frame's parts, not their lagging twins)
    let mut poses = Vec::new();
    engine.poses(&mut poses);
    let pose = poses.first().expect("a pose");
    let feet = engine.focus().unwrap().feet;
    assert_eq!(pose.joints.len(), 20, "the kart's own look: the body, four wheels, Mario's 15 nodes");
    assert!(pose.joints.iter().all(|j| *j != engine_core::glam::Mat4::ZERO), "every part found");
    let at: Vec<_> = pose.joints.iter().map(|j| pose.root.transform_point3(j.w_axis.truncate())).collect();
    assert!(at.iter().all(|p| (*p - feet).length() < m(2.0)), "every part at the kart: {feet}");
    assert!((pose.root.w_axis.truncate() - feet).length() < m(0.5), "the root at the kart");
    let (at_rest, driving) = (at_rest.expect("a pose at rest"), driving.expect("a pose driving"));
    // (the parts' roots: the body, the wheels, Mario's skeleton's root; game units)
    for joint in [0usize, 1, 2, 3, 4, 5] {
        let moved = (driving[joint] - at_rest[joint]).length();
        eprintln!("joint {joint}: {:.1} units from where it was at rest", moved);
        assert!(moved < 6.0, "joint {joint} with the body while it drives: {moved} units off");
    }
    let start = track[200];
    let end = *track.last().unwrap();
    eprintln!("kart: from {start} to {end}; at frame 600 {}", track[600]);
    assert!(start.y.abs() < m(0.2) && start.x.abs() < m(0.5) && start.z.abs() < m(0.5), "put down where it was asked, on the floor: {start}");
    assert!(track[600].x > start.x + m(3.0), "it drives forward (+x): {}", track[600]);
    assert!(track.iter().all(|p| p.x < m(25.0)), "never through the wall");
    assert!(end.x > m(20.0) && end.y.abs() < m(0.3), "held at the wall, on the floor: {end}");

    // Put down again at the start (the stage laid again under the kart), and a wall added 10 m ahead: it drives from
    // the start again and stops at the new wall.
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    let mut stage2 = stage.clone();
    stage2.revision = 2;
    stage2.room_quads.push([Vec3::new(m(10.0), m(0.0), m(-30.0)), Vec3::new(m(10.0), m(0.0), m(30.0)), Vec3::new(m(10.0), m(3.0), m(30.0)), Vec3::new(m(10.0), m(3.0), m(-30.0))]);
    assert_eq!(engine.set_stage(&stage2), Applied::Done);
    engine.set_input(&idle, PadHeld::default());
    for _ in 0..60 {
        engine.step().expect("a frame");
    }
    let again = engine.focus().expect("the kart").feet;
    eprintln!("put down again: {again}");
    assert!(again.x.abs() < m(1.0) && again.y.abs() < m(0.3), "back at the start: {again}");
    engine.set_input(&drive, PadHeld::default());
    let mut last = again;
    for _ in 0..600 {
        engine.step().expect("a frame");
        last = engine.focus().expect("the kart").feet;
        assert!(last.x < m(10.0), "never through the new wall: {last}");
    }
    eprintln!("then: {last}");
    assert!(last.x > m(7.0) && last.y.abs() < m(0.3), "held at the new wall: {last}");
    let _ = std::fs::remove_dir_all(data);
}
