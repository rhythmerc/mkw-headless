//! The kart model's textures (as built) to PNGs: `cargo run --example kart_textures -- <disc> <out dir>`.
use engine_core::ModelSource;
fn main() {
    let disc = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let out = std::path::PathBuf::from(std::env::args().nth(2).unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let kart = mkw_engine::kart_model::load(&disc).unwrap();
    let ModelSource::Meshes { meshes, textures, .. } = &kart.model.source else { panic!() };
    for (i, t) in textures.iter().enumerate() {
        let f = std::fs::File::create(out.join(format!("texture-{i}.png"))).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), t.width, t.height);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&t.pixels).unwrap();
    }
    for (i, m) in meshes.iter().enumerate() {
        let avg: Vec<f32> = (0..4).map(|c| m.colors.iter().map(|v| v[c]).sum::<f32>() / m.colors.len().max(1) as f32).collect();
        println!("mesh {i}: texture {:?}, vertex colours average {avg:.2?}", m.material.texture);
    }
}
