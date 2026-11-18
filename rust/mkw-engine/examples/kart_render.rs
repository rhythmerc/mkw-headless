//! The kart as the engine poses it, drawn in software to PNGs (no headset needed): at the start, then mid-turn.
//! `MKW_ENGINE_LIB=.. MKW_DISC_DIR=.. cargo run --release --example kart_render -- <out dir>`
use engine_core::glam::{Mat4, Vec3, Vec4};
use engine_core::{ModelSource, PadHeld, PlacePlayer, Pose, Stage, TouchState};
use std::path::PathBuf;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn render(model: &engine_core::Model, pose: &Pose, eye: Vec3, at: Vec3, path: &std::path::Path) {
    let (w, h) = (640usize, 480usize);
    let ModelSource::Meshes { meshes, textures, .. } = &model.source else { panic!("meshes") };
    let view = Mat4::look_at_rh(eye, at, Vec3::Y);
    let proj = Mat4::perspective_rh(0.6, w as f32 / h as f32, 0.05, 50.0);
    let mut color = vec![[0.75f32, 0.8, 0.85]; w * h];
    let mut depth = vec![f32::MAX; w * h];
    let light = Vec3::new(0.4, 1.0, 0.3).normalize();
    for mesh in meshes {
        let tex = mesh.material.texture.map(|t| &textures[t as usize]);
        let verts: Vec<(Vec4, Vec3, [f32; 2], [f32; 4])> = (0..mesh.positions.len())
            .map(|i| {
                let j = pose.root * pose.joints.get(usize::from(mesh.joints[i][0])).copied().unwrap_or(Mat4::IDENTITY);
                let p = j.transform_point3(Vec3::from_array(mesh.positions[i]));
                let n = j.transform_vector3(Vec3::from_array(mesh.normals[i])).normalize_or_zero();
                let c = mesh.colors.get(i).copied().filter(|_| mesh.material.vertex_color).unwrap_or([1.0; 4]);
                (proj * view * p.extend(1.0), n, mesh.uvs[i], c)
            })
            .collect();
        for t in mesh.indices.chunks(3) {
            let v = [verts[usize::from(t[0])], verts[usize::from(t[1])], verts[usize::from(t[2])]];
            if v.iter().any(|x| x.0.w <= 0.05) {
                continue;
            }
            let s: Vec<Vec3> = v.iter().map(|x| {
                let n = x.0.truncate() / x.0.w;
                Vec3::new((n.x * 0.5 + 0.5) * w as f32, (0.5 - n.y * 0.5) * h as f32, n.z)
            }).collect();
            let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
            // (front: counter-clockwise on screen with y up, so clockwise here with y down)
            if area >= 0.0 && !mesh.material.two_sided {
                continue;
            }
            let (x0, x1) = (s.iter().map(|p| p.x).fold(f32::MAX, f32::min).max(0.0) as usize, s.iter().map(|p| p.x).fold(f32::MIN, f32::max).min(w as f32 - 1.0) as usize);
            let (y0, y1) = (s.iter().map(|p| p.y).fold(f32::MAX, f32::min).max(0.0) as usize, s.iter().map(|p| p.y).fold(f32::MIN, f32::max).min(h as f32 - 1.0) as usize);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let e = |a: Vec3, b: Vec3| (b.x - a.x) * (py - a.y) - (px - a.x) * (b.y - a.y);
                    let (b0, b1, b2) = (e(s[1], s[2]) / area, e(s[2], s[0]) / area, e(s[0], s[1]) / area);
                    if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                        continue;
                    }
                    let z = b0 * s[0].z + b1 * s[1].z + b2 * s[2].z;
                    if z >= depth[y * w + x] {
                        continue;
                    }
                    // (perspective-correct attributes)
                    let iw = [1.0 / v[0].0.w, 1.0 / v[1].0.w, 1.0 / v[2].0.w];
                    let k = [b0 * iw[0], b1 * iw[1], b2 * iw[2]];
                    let sum = k[0] + k[1] + k[2];
                    let mix = |f: &dyn Fn(usize) -> f32| (k[0] * f(0) + k[1] * f(1) + k[2] * f(2)) / sum;
                    let (u, vv) = (mix(&|i| v[i].2[0]), mix(&|i| v[i].2[1]));
                    let mut c = [1.0f32; 4];
                    if let Some(t) = tex {
                        let tx = ((u.rem_euclid(1.0)) * t.width as f32) as usize % t.width as usize;
                        let ty = ((vv.rem_euclid(1.0)) * t.height as f32) as usize % t.height as usize;
                        let p = &t.pixels[(ty * t.width as usize + tx) * 4..][..4];
                        c = [srgb_to_linear(p[0] as f32 / 255.0), srgb_to_linear(p[1] as f32 / 255.0), srgb_to_linear(p[2] as f32 / 255.0), p[3] as f32 / 255.0];
                    }
                    if let engine_core::model::AlphaMode::Mask(cut) = mesh.material.alpha {
                        if c[3] < cut {
                            continue;
                        }
                    }
                    let base = mesh.material.base_color;
                    let n = (v[0].1 * k[0] + v[1].1 * k[1] + v[2].1 * k[2]).normalize_or_zero();
                    let lit = 0.35 + 0.65 * n.dot(light).max(0.0);
                    for ch in 0..3 {
                        let vc = mix(&|i| v[i].3[ch]);
                        c[ch] *= srgb_to_linear(base[ch]) * vc * lit;
                    }
                    depth[y * w + x] = z;
                    color[y * w + x] = [c[0], c[1], c[2]];
                }
            }
        }
    }
    let bytes: Vec<u8> = color.iter().flat_map(|c| c.map(|x| (linear_to_srgb(x) * 255.0) as u8)).collect();
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&bytes).unwrap();
    println!("{}", path.display());
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("an output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let lib = PathBuf::from(std::env::var("MKW_ENGINE_LIB").unwrap());
    let disc = PathBuf::from(std::env::var("MKW_DISC_DIR").unwrap());
    let data = std::env::temp_dir().join(format!("mkw-render-{}", std::process::id()));
    let mut engine = mkw_engine::engine::factory(lib, disc, data)().expect("the game");
    let floor = [Vec3::new(-30.0, 0.0, -30.0), Vec3::new(-30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, 30.0), Vec3::new(30.0, 0.0, -30.0)];
    engine.place_player(PlacePlayer { point: Vec3::ZERO, forward: Vec3::X });
    engine.set_stage(&Stage { revision: 1, room_quads: vec![floor], ..Default::default() });
    let model = engine.models().remove(0);
    while !engine.wants_steps() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let shoot = |engine: &mut Box<dyn engine_core::EngineCore>, name: &str| {
        let mut poses = Vec::new();
        engine.poses(&mut poses);
        let f = engine.focus().unwrap();
        let at = f.feet + Vec3::Y * 0.35;
        // (from the front-left and from behind-right)
        let side = f.forward.cross(Vec3::Y).normalize();
        render(&model, &poses[0], at + f.forward * 3.6 - side * 2.6 + Vec3::Y * 1.6, at, &out.join(format!("{name}-front.png")));
        render(&model, &poses[0], at - f.forward * 4.0 + side * 2.0 + Vec3::Y * 2.0, at, &out.join(format!("{name}-behind.png")));
    };
    for _ in 0..200 {
        engine.step().unwrap();
    }
    shoot(&mut engine, "kart-start");
    let turn = TouchState { right_trigger: 1.0, left_stick: [1.0, 0.0], ..Default::default() };
    let idle = TouchState::default();
    for frame in 0..420 {
        engine.set_input(if frame > 60 { &turn } else { &idle }, PadHeld::default());
        engine.step().unwrap();
    }
    shoot(&mut engine, "kart-turning");
    std::process::exit(0);
}
