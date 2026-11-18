//! What a kart archive's models hold (`cargo run --example kart_model_info -- <disc>/files/Race/Kart/ma_kart-mr.szs`).
fn main() {
    let path = std::env::args().nth(1).expect("an .szs");
    let files = mkw_engine::szs::read(&std::fs::read(&path).expect("read")).expect("an szs");
    for (name, data) in &files {
        println!("{name}: {} bytes", data.len());
        if !name.ends_with(".brres") {
            continue;
        }
        let archive = brres::Archive::from_memory(data).expect("a brres");
        for model in &archive.models {
            println!("  model {}: {} bones, {} meshes, {} materials, {} draw matrices", model.name, model.bones.len(), model.meshes.len(), model.materials.len(), model.matrices.len());
            for (i, b) in model.bones.iter().enumerate() {
                println!("    bone {i} {} parent {} t {:?} r {:?} s {:?}", b.name, b.parent, b.translate, b.rotate, b.scale);
            }
            for m in &model.meshes {
                let prims: usize = m.mprims.iter().map(|p| p.num_prims as usize).sum();
                for mp in &m.mprims {
                    println!("      mprim: {} prims, {} bytes, matrices {:?}, first {:02X?}", mp.num_prims, mp.vertex_data_buffer.len(), mp.matrices, &mp.vertex_data_buffer[..mp.vertex_data_buffer.len().min(24)]);
                }
                println!("    mesh {} pos {} uv {:?} clr {:?} mprims {} prims {} current_matrix {}", m.name, m.pos_buffer, m.uv_buffer, m.clr_buffer, m.mprims.len(), prims, m.current_matrix);
            }
            for t in &model.texcoords {
                let (lo, hi) = t.data.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), v| ([lo[0].min(v[0]), lo[1].min(v[1])], [hi[0].max(v[0]), hi[1].max(v[1])]));
                println!("    uvs {}: {} from {:.2?} to {:.2?}", t.name, t.data.len(), lo, hi);
            }
            for m in &model.materials {
                for s in &m.samplers {
                    println!("    material {} texture {} wrap {:?} {:?}", m.name, s.mTexture, s.mWrapU, s.mWrapV);
                }
                for t in &m.texMatrices {
                    println!("      tex matrix {:?}", t);
                }
            }
            for p in &model.positions {
                let (lo, hi) = p.data.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| {
                    ([lo[0].min(v[0]), lo[1].min(v[1]), lo[2].min(v[2])], [hi[0].max(v[0]), hi[1].max(v[1]), hi[2].max(v[2])])
                });
                println!("    positions {}: {} from {:.1?} to {:.1?}", p.name, p.data.len(), lo, hi);
            }
            for (i, d) in model.matrices.iter().enumerate() {
                println!("    matrix {i}: {:?}", d.weights.iter().map(|w| (w.bone_id, w.weight)).collect::<Vec<_>>());
            }
        }
        for t in &archive.textures {
            println!("  texture {} {}x{} format {} images {}", t.name, t.width, t.height, t.format, t.number_of_images);
        }
        println!("  chrs {}", archive.chrs.len());
    }
}

#[allow(dead_code)]
fn _unused() {}
