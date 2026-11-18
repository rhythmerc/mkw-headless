//! A test course over Luigi Circuit's start (game units): a floor under the start, a ramp ahead on the route's first
//! stretch (toward -z) and a wall across it further on. `cargo run --example gate_course -- out.kcl`.
use glam::Vec3;
use mkw_engine::kcl::{self, Triangle, ROAD, WALL};

fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, attribute: u16) -> [Triangle; 2] {
    [Triangle { corners: [a, b, c], attribute }, Triangle { corners: [a, c, d], attribute }]
}

fn main() {
    let out = std::env::args().nth(1).expect("an output path");
    let floor = 400.0;
    let mut tris = Vec::new();
    // (floors and ramps counter-clockwise from above: normals up; the wall's toward +z, the way the kart comes)
    tris.extend(quad(
        Vec3::new(-30000.0, floor, 10000.0),
        Vec3::new(-30000.0, floor, 40000.0),
        Vec3::new(-11000.0, floor, 40000.0),
        Vec3::new(-11000.0, floor, 10000.0),
        ROAD,
    ));
    tris.extend(quad(
        Vec3::new(-21900.0, floor + 600.0, 26000.0),
        Vec3::new(-21900.0, floor, 28000.0),
        Vec3::new(-19900.0, floor, 28000.0),
        Vec3::new(-19900.0, floor + 600.0, 26000.0),
        ROAD,
    ));
    tris.extend(quad(
        Vec3::new(-30000.0, floor, 20000.0),
        Vec3::new(-11000.0, floor, 20000.0),
        Vec3::new(-11000.0, floor + 3000.0, 20000.0),
        Vec3::new(-30000.0, floor + 3000.0, 20000.0),
        WALL,
    ));
    let data = kcl::write(&tris).expect("a KCL");
    std::fs::write(&out, &data).expect("written");
    for (i, t) in tris.iter().enumerate() {
        let n = (t.corners[1] - t.corners[0]).cross(t.corners[2] - t.corners[0]).normalize();
        println!("triangle {}: normal {n:.2}", i + 1);
    }
    println!("{} bytes to {out}", data.len());
}
