//! The kart drawn as the game draws it: its models (the kart's parts and its driver, from the player's disc) as the
//! runtime's meshes, posed each frame with the game's own matrices.
//!
//! **The look.** A kart archive (`Race/Kart/<kart>-<driver>.szs`) holds the kart's parts (`kart_model.brres`: the body,
//! the tires, ...) and the driver (`driver_model.brres`: `model`, a skeleton; `model_lod` is its far stand-in). Each
//! model's vertices are bound one node each, in that node's space (g3d's single-weight draw matrices), so a vertex
//! posed is its node's world matrix times it: the runtime's skinning with identity inverse binds, a joint a node. A
//! part drawn more than once (the front tires again at the back, larger, when the kart has no rear tires of its own)
//! has joints for each time.
//!
//! **The pose.** The game's scene keeps each drawn model as a g3d scene model (`ScnMdlSimple`) holding its `ResMdl`
//! (where the model's MDL0 is) and, one word on, its nodes' world matrices (3x4, game units) as the frame left them.
//! The pose's root is the body's (so the model is where the kart is), its joints those matrices in the body's space.
//! [`Locator`] finds them in the game's memory: the MDL0s by their bytes, then the words pointing at them. The scene
//! has each part twice: the one for this frame and a twin a couple of frames behind (the same where the kart is
//! still). [`Locator::pose`] tells them apart each frame by the body (this frame's is where the game reports the
//! kart): a part's twin is where the lagging body would carry it.
use crate::szs;
use engine_core::glam::{Mat4, Vec3, Vec4};
use engine_core::hud::Rgba8;
use engine_core::model::{AlphaMode, BodyGroup, Cast, Ellipse, GroupId, Lighting, MaterialDesc, MeshData, Point};
use engine_core::{Model, ModelSource};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// The race's kart and driver (boot_race's solo race: Mario, the Standard Kart M - the game's `mdf_kart`).
pub const ARCHIVE: &str = "files/Race/Kart/mdf_kart-mr.szs";

/// Models not drawn: the kart's blob shadow (the runtime casts its own), the driver's far stand-in.
const SKIPPED: [&str; 2] = ["shadow", "model_lod"];

/// One model drawn once: which archive file and model, its nodes, and its first joint.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub file: String,
    pub model: String,
    pub nodes: usize,
    pub joint: u16,
}

/// The kart's look and how its joints are laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct KartModel {
    pub model: Model,
    pub instances: Vec<Instance>,
    /// Each drawn model's MDL0 header (its first 0x40 bytes: where its offsets are relative to it, the same in memory).
    pub headers: Vec<(String, String, Vec<u8>)>,
    pub joints: u16,
}

/// The kart archive under the extracted disc at `disc`, read and built.
pub fn load(disc: &Path) -> Result<KartModel, String> {
    let path = disc.join(ARCHIVE);
    let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    build(&szs::read(&data)?)
}

/// The model of a kart archive's files.
pub fn build(files: &BTreeMap<String, Vec<u8>>) -> Result<KartModel, String> {
    let mut meshes = Vec::new();
    let mut textures: Vec<Rgba8> = Vec::new();
    let mut texture_ids: HashMap<String, u32> = HashMap::new();
    let mut instances = Vec::new();
    let mut headers = Vec::new();
    let mut joints: u16 = 0;
    for file in ["kart_model.brres", "driver_model.brres"] {
        let bytes = files.get(file).ok_or_else(|| format!("{file} missing"))?;
        let archive = brres::Archive::from_memory(bytes).map_err(|e| format!("{file}: {e}"))?;
        let names: Vec<&str> = archive.models.iter().map(|m| m.name.as_str()).collect();
        for (name, header) in mdl0_headers(bytes, &names) {
            if !SKIPPED.contains(&name.as_str()) {
                headers.push((file.to_string(), name, header));
            }
        }
        for model in archive.models.iter().filter(|m| !SKIPPED.contains(&m.name.as_str())) {
            // (the front tires drawn at the back too, when there are none for it)
            let times = match model.name.as_str() {
                "tire_fl" if archive.get_model("tire_rl").is_none() => 2,
                "tire_fr" if archive.get_model("tire_rr").is_none() => 2,
                _ => 1,
            };
            for _ in 0..times {
                let joint = joints;
                for mesh in model_meshes(model, &archive, bytes, joint, &mut textures, &mut texture_ids)? {
                    meshes.push(mesh);
                }
                instances.push(Instance { file: file.to_string(), model: model.name.clone(), nodes: model.bones.len(), joint });
                joints += model.bones.len() as u16;
            }
        }
    }
    let body = instances.iter().find(|i| i.file == "kart_model.brres" && i.model == "body");
    let groups = body.and_then(|b| footprint(&meshes, b.joint)).into_iter().collect();
    let model = Model { source: ModelSource::Meshes { meshes, textures, joint_count: joints }, parents: Vec::new(), groups };
    Ok(KartModel { model, instances, headers, joints })
}

/// The kart's body group: the one it casts its contact shadow as (Mario sits in it, under its shadow).
pub const KART_BODY: GroupId = 0;

/// The kart's contact shadow as a footprint of its body (`joint`, the body's: the meshes skinned to it): laid along
/// its longer level side, back to front, from the body's bounds as its meshes give them - a soft one under all of it,
/// and darker ones under its two axles, where its wheels meet the ground. In heights of the kart as drawn
/// ([`crate::engine::KART_HEIGHT`]). None when no mesh is the body's.
fn footprint(meshes: &[MeshData], joint: u16) -> Option<BodyGroup> {
    let positions = meshes.iter().filter(|m| m.joints.first().is_some_and(|j| j[0] == joint)).flat_map(|m| m.positions.iter().map(|p| Vec3::from_array(*p)));
    let (lo, hi) = positions.fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    if lo.x > hi.x {
        return None;
    }
    let size = hi - lo;
    let (axis, length, width) = if size.z >= size.x { (Vec3::Z, size.z, size.x) } else { (Vec3::X, size.x, size.z) };
    let middle = ((lo + hi) / 2.0).with_y(lo.y);
    let end = |sign: f32| Point { joint, at: middle + axis * (length / 2.0 * sign) };
    let (l, w) = (length / crate::engine::KART_HEIGHT, width / crate::engine::KART_HEIGHT);
    let ellipses = vec![
        Ellipse { along: 0.0, half_length: 0.5 * l, half_width: 0.5 * w, dark: 0.45 },
        Ellipse { along: 0.32 * l, half_length: 0.1 * l, half_width: 0.5 * w, dark: 0.5 },
        Ellipse { along: -0.32 * l, half_length: 0.1 * l, half_width: 0.5 * w, dark: 0.5 },
    ];
    Some(BodyGroup { id: KART_BODY, joints: vec![joint], cast: Cast::Footprint { back: end(-1.0), front: end(1.0), ellipses } })
}

/// Each MDL0's name (one of `names`: found from the header's name offset, whichever field it is) and its header.
fn mdl0_headers(bytes: &[u8], names: &[&str]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = bytes[i..].windows(4).position(|w| w == b"MDL0") {
        let at = i + p;
        i = at + 4;
        let Some(header) = bytes.get(at..at + 0x40) else { continue };
        // (the name's offset: a header word pointing, from the MDL0, at one of the names)
        let name = (1..32).find_map(|k| {
            let off = u32::from_be_bytes(bytes.get(at + 4 * k..at + 4 * k + 4)?.try_into().ok()?) as usize;
            let start = at.checked_add(off)?;
            let end = start + bytes.get(start..)?.iter().take(64).position(|&b| b == 0)?;
            let s = std::str::from_utf8(&bytes[start..end]).ok()?;
            names.contains(&s).then(|| s.to_string())
        });
        if let Some(name) = name {
            out.push((name, header.to_vec()));
        }
    }
    out
}

/// A g3d material as the runtime's: its first texture, its colour when the vertices don't give it, alpha tested or
/// blended as it is, both sides when it culls none.
/// (a macro: the crate doesn't name its material type)
macro_rules! material_desc {
    ($m:expr, $archive:expr, $textures:expr, $ids:expr) => {{
    let (m, archive, textures, ids): (_, &brres::Archive, &mut Vec<Rgba8>, &mut HashMap<String, u32>) = ($m, $archive, $textures, $ids);
    let texture = m.samplers.first().and_then(|s| {
        // (mirrored at its edges: baked in, the texture and its mirror image side by side - the runtime repeats)
        let (mirror_u, mirror_v) = (s.mWrapU == brres::enums::TextureWrapMode::Mirror, s.mWrapV == brres::enums::TextureWrapMode::Mirror);
        let key = format!("{}{}{}", s.mTexture, if mirror_u { "|mu" } else { "" }, if mirror_v { "|mv" } else { "" });
        if let Some(&id) = ids.get(&key) {
            return Some(id);
        }
        let t = archive.get_texture(&s.mTexture)?;
        let format = gctex::TextureFormat::from_u32(t.format)?;
        let image = mirrored(Rgba8 { width: t.width, height: t.height, pixels: gctex::decode(&t.data, t.width, t.height, format, &[], 0) }, mirror_u, mirror_v);
        let id = textures.len() as u32;
        textures.push(image);
        ids.insert(key, id);
        Some(id)
    });
    let vertex_color = m.colorChanControls.first().is_some_and(|c| c.Material == brres::enums::ColorSource::Vertex);
    let base = m.chanData.first().filter(|_| !vertex_color).map_or([1.0; 4], |c| {
        let c = &c.matColor;
        [c.r, c.g, c.b, c.a].map(|v| f32::from(v) / 255.0)
    });
    let alpha = if m.xlu {
        AlphaMode::Blend
    } else if m.alphaCompare.compLeft != brres::enums::Comparison::ALWAYS {
        AlphaMode::Mask(f32::from(m.alphaCompare.refLeft.max(1)) / 255.0)
    } else {
        AlphaMode::Opaque
    };
    MaterialDesc {
        base_color: base,
        texture,
        alpha,
        two_sided: m.cullMode == brres::enums::CullMode::None,
        vertex_color,
        lighting: Lighting::Standard,
    }
    }};
}

/// The GX vertex attributes in the crate's indexed vertices: the position/normal matrix index, position, normal, the
/// first colour and the first texture coordinates.
const PNMTXIDX: usize = 0;
const POS: usize = 9;
const NRM: usize = 10;
const CLR0: usize = 11;
const TEX0: usize = 13;

/// A model's meshes, its nodes from joint `joint` on.
fn model_meshes(
    model: &brres::Model,
    archive: &brres::Archive,
    bytes: &[u8],
    joint: u16,
    textures: &mut Vec<Rgba8>,
    texture_ids: &mut HashMap<String, u32>,
) -> Result<Vec<MeshData>, String> {
    // (each polygon's material: from the nodes' draw calls)
    let mut material_of: HashMap<u32, u32> = HashMap::new();
    for bone in &model.bones {
        for call in &bone.draw_calls {
            material_of.insert(call.poly, call.material);
        }
    }
    let bone_of = |draw_matrix: usize| -> u16 {
        model
            .matrices
            .get(draw_matrix)
            .and_then(|m| m.weights.iter().max_by(|a, b| a.weight.total_cmp(&b.weight)))
            .map_or(0, |w| w.bone_id as u16)
    };
    let mut out = Vec::new();
    for (poly, mesh) in model.meshes.iter().enumerate() {
        let positions = model.positions.iter().find(|b| b.name == mesh.pos_buffer).ok_or_else(|| format!("{}: no positions", mesh.name))?;
        let normals = model.normals.iter().find(|b| b.name == mesh.nrm_buffer);
        let colors = mesh.clr_buffer.first().and_then(|n| model.colors.iter().find(|b| !n.is_empty() && &b.name == n));
        let uvs = mesh.uv_buffer.first().filter(|n| !n.is_empty()).and_then(|n| {
            model.texcoords.iter().find(|b| &b.name == n).or_else(|| n.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()).and_then(|i| model.texcoords.get(i)))
        });
        let material = material_of.get(&(poly as u32)).and_then(|&m| model.materials.get(m as usize));
        // (the colours as the file has them: the crate reads a colour a byte)
        let colors = colors.and_then(|c| raw_colors(bytes, &c.name));
        // (greys only: not colour but the console's lighting baked in - the material colour its lights brighten back
        // towards white. The runtime lights the model itself, so they're left out: a white texel is white, the
        // room's reference white under its light)
        let baked_light = colors.as_ref().is_some_and(|c| c.iter().all(|v| v[0] == v[1] && v[1] == v[2]));
        let mut data = MeshData::default();
        let mut seen: HashMap<[u16; 5], u16> = HashMap::new();
        let mut corners: Vec<u16> = Vec::new();
        for mprim in &mesh.mprims {
            let buf = &mprim.vertex_data_buffer;
            let mut cursor = 0;
            for _ in 0..mprim.num_prims {
                let kind = *buf.get(cursor).ok_or("primitives truncated")?;
                let count = buf.get(cursor + 1..cursor + 4).ok_or("primitives truncated")?;
                let count = (usize::from(count[0]) << 16) | (usize::from(count[1]) << 8) | usize::from(count[2]);
                cursor += 4;
                let mut prim = Vec::with_capacity(count);
                for _ in 0..count {
                    let raw = buf.get(cursor..cursor + 52).ok_or("primitives truncated")?;
                    cursor += 52;
                    let at = |a: usize| u16::from_le_bytes([raw[a * 2], raw[a * 2 + 1]]);
                    // (the vertex's draw matrix: its own index into the primitive's list, else the mesh's one)
                    let draw = if mesh.current_matrix >= 0 {
                        mesh.current_matrix as usize
                    } else {
                        // (GX's own value: a matrix's row in the matrix memory, three a matrix)
                        let i = usize::from(at(PNMTXIDX)) / 3;
                        mprim.matrices.get(i).copied().unwrap_or(0).max(0) as usize
                    };
                    let key = [draw as u16, at(POS), at(NRM), at(CLR0), at(TEX0)];
                    let index = match seen.get(&key) {
                        Some(&i) => i,
                        None => {
                            let i = u16::try_from(data.positions.len()).map_err(|_| format!("{}: too many vertices", mesh.name))?;
                            data.positions.push(*positions.data.get(usize::from(key[1])).ok_or("position index")?);
                            data.normals.push(normals.and_then(|n| n.data.get(usize::from(key[2])).copied()).unwrap_or([0.0, 1.0, 0.0]));
                            data.uvs.push(uvs.and_then(|u| u.data.get(usize::from(key[4])).copied()).unwrap_or([0.0, 0.0]));
                            if let Some(c) = colors.as_ref() {
                                let c = c.get(usize::from(key[3])).copied().unwrap_or([255; 4]);
                                // (their alpha: how strongly the game's shading adds its highlights, not coverage - opaque)
                                data.colors.push([srgb(c[0]), srgb(c[1]), srgb(c[2]), 1.0]);
                            }
                            data.joints.push([joint + bone_of(draw), 0, 0, 0]);
                            data.weights.push([1.0, 0.0, 0.0, 0.0]);
                            seen.insert(key, i);
                            i
                        }
                    };
                    prim.push(index);
                }
                triangles(kind, &prim, &mut corners);
            }
        }
        // (the texture's coordinates as its material's texture matrix makes them - scaled, moved - and halved where
        // its texture is baked mirrored)
        if let Some(m) = material {
            let (scale, offset) = m.texMatrices.first().map_or(((1.0, 1.0), (0.0, 0.0)), |t| (t.scale, t.translate));
            let mirror = m.samplers.first().map_or((false, false), |s| {
                (s.mWrapU == brres::enums::TextureWrapMode::Mirror, s.mWrapV == brres::enums::TextureWrapMode::Mirror)
            });
            for uv in &mut data.uvs {
                let u = uv[0] * scale.0 + offset.0;
                let v = uv[1] * scale.1 + offset.1;
                *uv = [if mirror.0 { u / 2.0 } else { u }, if mirror.1 { v / 2.0 } else { v }];
            }
        }
        // (GX's front face is the other way round from the contract's: turned by whichever way agrees with the
        // vertices' normals, most of the time)
        let agree: f32 = corners
            .chunks(3)
            .map(|t| {
                let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from_array(data.positions[usize::from(i)])).collect();
                let n = t.iter().map(|&i| Vec3::from_array(data.normals[usize::from(i)])).sum::<Vec3>();
                (p[1] - p[0]).cross(p[2] - p[0]).dot(n).signum()
            })
            .sum();
        for t in corners.chunks_mut(3) {
            if agree < 0.0 {
                t.swap(1, 2);
            }
        }
        data.indices = corners;
        data.material = match material {
            Some(m) => material_desc!(m, archive, textures, texture_ids),
            None => MaterialDesc::default(),
        };
        if data.colors.is_empty() || baked_light {
            data.material.vertex_color = false;
        }
        if !data.indices.is_empty() {
            out.push(data);
        }
    }
    Ok(out)
}

/// A primitive's triangles (as GX winds them): its kind as the crate numbers them (quads, quads again, triangles, a
/// strip, a fan, lines, a line strip, points), or as GX's opcode.
fn triangles(kind: u8, v: &[u16], out: &mut Vec<u16>) {
    let kind = if kind < 8 { 0x80 + kind * 8 } else { kind };
    match kind & 0xF8 {
        // (quads)
        0x80 | 0x88 => {
            for q in v.chunks_exact(4) {
                out.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
        }
        0x90 => out.extend(&v[..v.len() / 3 * 3]),
        // (a strip: every other one the other way round)
        0x98 => {
            for i in 2..v.len() {
                if i % 2 == 0 {
                    out.extend([v[i - 2], v[i - 1], v[i]]);
                } else {
                    out.extend([v[i - 1], v[i - 2], v[i]]);
                }
            }
        }
        0xA0 => {
            for i in 2..v.len() {
                out.extend([v[0], v[i - 1], v[i]]);
            }
        }
        // (lines and points: not drawn)
        _ => {}
    }
}

/// A model's vertex colours named `name`, read from its file's bytes: the colour array whose header names it (its size,
/// its offsets back to the model, to its data and to its name, its index, RGB or RGBA, its format, its stride and its
/// count), decoded from whichever of GX's colour formats it is.
fn raw_colors(bytes: &[u8], name: &str) -> Option<Vec<[u8; 4]>> {
    let be32 = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
    (0..bytes.len().saturating_sub(0x20)).step_by(4).find_map(|at| {
        let (size, back, data, name_at) = (be32(at)? as i32, be32(at + 4)? as i32, be32(at + 8)? as i32, be32(at + 12)? as i32);
        let (format, stride) = (be32(at + 0x18)?, usize::from(bytes[at + 0x1C]));
        let count = usize::from(u16::from_be_bytes([bytes[at + 0x1E], bytes[at + 0x1F]]));
        if back >= 0 || data <= 0 || data >= size || format > 5 || !(2..=4).contains(&stride) || size < (count * stride) as i32 {
            return None;
        }
        let start = at.checked_add_signed(name_at as isize)?;
        let end = start + bytes.get(start..)?.iter().take(128).position(|&b| b == 0)?;
        if &bytes[start..end] != name.as_bytes() {
            return None;
        }
        let data = bytes.get(at + data as usize..at + data as usize + count * stride)?;
        Some(
            data.chunks_exact(stride)
                .map(|c| {
                    let x4 = |v: u16| (v * 17) as u8;
                    let x6 = |v: u32| ((v * 255 + 31) / 63) as u8;
                    let x5 = |v: u16| ((u32::from(v) * 255 + 15) / 31) as u8;
                    match format {
                        // (RGB565)
                        0 => {
                            let v = u16::from_be_bytes([c[0], c[1]]);
                            [x5(v >> 11), ((u32::from(v >> 5 & 63) * 255 + 31) / 63) as u8, x5(v & 31), 255]
                        }
                        // (RGB8, RGBX8)
                        1 | 2 => [c[0], c[1], c[2], 255],
                        // (RGBA4)
                        3 => {
                            let v = u16::from_be_bytes([c[0], c[1]]);
                            [x4(v >> 12), x4(v >> 8 & 15), x4(v >> 4 & 15), x4(v & 15)]
                        }
                        // (RGBA6)
                        4 => {
                            let v = u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]);
                            [x6(v >> 18), x6(v >> 12 & 63), x6(v >> 6 & 63), x6(v & 63)]
                        }
                        _ => [c[0], c[1], c[2], c[3]],
                    }
                })
                .collect(),
        )
    })
}

/// An image with its mirror image after it, across (`u`) and down (`v`): repeating it is mirroring the original.
fn mirrored(image: Rgba8, u: bool, v: bool) -> Rgba8 {
    let (w, h) = (image.width as usize, image.height as usize);
    let (nw, nh) = (if u { 2 * w } else { w }, if v { 2 * h } else { h });
    let mut pixels = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let sy = if y < h { y } else { 2 * h - 1 - y };
        for x in 0..nw {
            let sx = if x < w { x } else { 2 * w - 1 - x };
            pixels[(y * nw + x) * 4..][..4].copy_from_slice(&image.pixels[(sy * w + sx) * 4..][..4]);
        }
    }
    Rgba8 { width: nw as u32, height: nh as u32, pixels }
}

/// An 8-bit sRGB channel, linear.
fn srgb(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// The game's memory, read between steps (`Game::read`).
pub trait Memory {
    fn read(&self, address: u32, out: &mut [u8]) -> bool;
}

impl Memory for crate::native::Game {
    fn read(&self, address: u32, out: &mut [u8]) -> bool {
        crate::native::Game::read(self, address, out)
    }
}

/// The game's main memory (MEM1) and its second (MEM2), as the game addresses them.
const RANGES: [(u32, u32); 2] = [(0x8000_0000, 24 << 20), (0x9000_0000, 64 << 20)];

/// A scene model found: which drawn model it is (an index into [`KartModel::headers`]) and where its world matrices
/// are pointed to from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Found {
    header: usize,
    /// The word holding the address of its world matrices.
    matrices_at: u32,
}

/// Where the kart's scene models are in the game's memory.
#[derive(Clone, Debug, Default)]
pub struct Locator {
    found: Vec<Found>,
}

/// A 3x4 row-major big-endian matrix as the game keeps it, as a matrix.
fn matrix(b: &[u8]) -> Mat4 {
    let f = |i: usize| f32::from_bits(u32::from_be_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()));
    Mat4::from_cols(Vec4::new(f(0), f(4), f(8), 0.0), Vec4::new(f(1), f(5), f(9), 0.0), Vec4::new(f(2), f(6), f(10), 0.0), Vec4::new(f(3), f(7), f(11), 1.0))
}

/// A world matrix as a node's: finite, each axis a sensible length.
fn plausible(m: &Mat4) -> bool {
    m.is_finite() && [m.x_axis, m.y_axis, m.z_axis].iter().all(|a| (0.3..4.0).contains(&a.truncate().length()))
}

impl Locator {
    /// Whether the kart's scene models have been found.
    pub fn found(&self) -> bool {
        !self.found.is_empty()
    }

    /// Finds the kart's scene models (once the race is on): the drawn models' MDL0s by their headers, then each word
    /// pointing at one that is followed by a pointer to plausible world matrices.
    pub fn locate(&mut self, memory: &dyn Memory, kart: &KartModel) {
        let mut mem = Vec::new();
        for &(at, size) in &RANGES {
            let mut v = vec![0u8; size as usize];
            for (i, chunk) in v.chunks_mut(1 << 20).enumerate() {
                memory.read(at + (i as u32) * (1 << 20), chunk);
            }
            mem.push((at, v));
        }
        // (each header's MDL0s: the 4-byte magic first, then the whole header)
        let mut mdl0: HashMap<u32, usize> = HashMap::new();
        for (at, v) in &mem {
            let mut i = 0;
            while let Some(p) = v[i..].windows(4).position(|w| w == b"MDL0") {
                let here = i + p;
                i = here + 4;
                if let Some(h) = kart.headers.iter().position(|(_, _, h)| v.get(here..here + h.len()) == Some(h.as_slice())) {
                    mdl0.insert(at + here as u32, h);
                }
            }
        }
        let read = |a: u32, n: usize| -> Option<&[u8]> {
            mem.iter().find_map(|(at, v)| a.checked_sub(*at).and_then(|o| v.get(o as usize..o as usize + n)))
        };
        self.found.clear();
        for (at, v) in &mem {
            for (k, w) in v.chunks_exact(4).enumerate() {
                let word = u32::from_be_bytes(w.try_into().unwrap());
                let Some(&header) = mdl0.get(&word) else { continue };
                let here = at + (k as u32) * 4;
                let nodes = kart.instances.iter().find(|i| (&i.file, &i.model) == (&kart.headers[header].0, &kart.headers[header].1)).map_or(1, |i| i.nodes);
                let ok = read(here + 4, 4)
                    .map(|p| u32::from_be_bytes(p.try_into().unwrap()))
                    .and_then(|p| read(p, nodes * 48))
                    .is_some_and(|b| b.chunks_exact(48).all(|m| plausible(&matrix(m))));
                if ok {
                    self.found.push(Found { header, matrices_at: here + 4 });
                }
            }
        }
    }

    /// The kart this frame, the game reporting it at `kart_position`: its body's world matrix (game units) and each
    /// joint in the body's space; a part not found folded away (zero). None when the scene models aren't where they
    /// were found any more.
    pub fn pose(&self, memory: &dyn Memory, kart: &KartModel, kart_position: Vec3) -> Option<(Mat4, Vec<Mat4>)> {
        // (each scene model's node matrices now)
        let mut now: Vec<(Found, Vec<Mat4>)> = Vec::new();
        for f in &self.found {
            let mut p = [0u8; 4];
            if !memory.read(f.matrices_at, &mut p) {
                continue;
            }
            let (header, nodes) = (&kart.headers[f.header], kart.instances.iter().find(|i| (&i.file, &i.model) == (&kart.headers[f.header].0, &kart.headers[f.header].1))?.nodes);
            let _ = header;
            let mut b = vec![0u8; nodes * 48];
            if !memory.read(u32::from_be_bytes(p), &mut b) {
                continue;
            }
            let ms: Vec<Mat4> = b.chunks_exact(48).map(matrix).collect();
            if ms.iter().all(plausible) {
                now.push((*f, ms));
            }
        }
        // (this frame's body: where the kart is; its twin, the other)
        let body = kart.headers.iter().position(|(f, m, _)| f == "kart_model.brres" && m == "body")?;
        let mut bodies: Vec<&(Found, Vec<Mat4>)> = now.iter().filter(|(f, _)| f.header == body).collect();
        if bodies.is_empty() {
            return None;
        }
        bodies.sort_by(|a, b| {
            let d = |x: &&(Found, Vec<Mat4>)| (x.1[0].w_axis.truncate() - kart_position).length();
            d(a).total_cmp(&d(b)).then(a.0.matrices_at.cmp(&b.0.matrices_at))
        });
        let current = bodies[0].1[0];
        let lagging = bodies.get(1).map_or(current, |b| b.1[0]);
        let (into_current, into_lagging) = (current.inverse(), lagging.inverse());
        let mut joints = vec![Mat4::ZERO; usize::from(kart.joints)];
        for (h, (file, model, _)) in kart.headers.iter().enumerate() {
            let slots: Vec<&Instance> = kart.instances.iter().filter(|i| (&i.file, &i.model) == (file, model)).collect();
            let mut cands: Vec<&(Found, Vec<Mat4>)> = now.iter().filter(|(f, _)| f.header == h).collect();
            cands.sort_by_key(|c| c.0.matrices_at);
            // (twins paired: the pair whose offsets match best first; in each, this frame's one is where the current
            // body carries it as the lagging body carries the other; a tie, the lower)
            let mut chosen: Vec<&(Found, Vec<Mat4>)> = Vec::new();
            if h == body {
                chosen.push(bodies[0]);
            } else if cands.len() > slots.len() {
                let rel = |m: &Mat4, into: &Mat4| (*into * *m).w_axis.truncate();
                let mut free: Vec<bool> = vec![true; cands.len()];
                while chosen.len() < slots.len() {
                    let mut best: Option<(f32, usize, usize)> = None;
                    for a in 0..cands.len() {
                        for b in 0..cands.len() {
                            if a == b || !free[a] || !free[b] {
                                continue;
                            }
                            let cost = (rel(&cands[a].1[0], &into_current) - rel(&cands[b].1[0], &into_lagging)).length();
                            if best.is_none_or(|(c, _, _)| cost < c - 1e-3) {
                                best = Some((cost, a, b));
                            }
                        }
                    }
                    let Some((_, a, b)) = best else { break };
                    free[a] = false;
                    free[b] = false;
                    chosen.push(cands[a]);
                }
                chosen.sort_by_key(|c| c.0.matrices_at);
            } else {
                chosen = cands;
            }
            for (slot, c) in slots.iter().zip(&chosen) {
                for (n, m) in c.1.iter().enumerate().take(slot.nodes) {
                    joints[usize::from(slot.joint) + n] = into_current * *m;
                }
            }
        }
        Some((current, joints))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_become_triangles() {
        let mut out = Vec::new();
        triangles(0x98, &[0, 1, 2, 3], &mut out);
        assert_eq!(out, [0, 1, 2, 2, 1, 3]);
        out.clear();
        triangles(3, &[0, 1, 2, 3], &mut out);
        assert_eq!(out, [0, 1, 2, 2, 1, 3], "the crate's strip");
        out.clear();
        triangles(0xA0, &[0, 1, 2, 3], &mut out);
        assert_eq!(out, [0, 1, 2, 0, 2, 3]);
        out.clear();
        triangles(0x80, &[0, 1, 2, 3], &mut out);
        assert_eq!(out, [0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn a_mirrored_texture_repeats_as_the_original_mirrors() {
        let image = Rgba8 { width: 2, height: 1, pixels: vec![1, 0, 0, 0, 2, 0, 0, 0] };
        let m = mirrored(image, true, false);
        assert_eq!((m.width, m.height), (4, 1));
        assert_eq!(m.pixels.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [1, 2, 2, 1]);
    }

    #[test]
    fn a_game_matrix_reads_as_its_axes_and_place() {
        let mut b = Vec::new();
        for v in [0.0f32, 0.0, 1.0, 10.0, 0.0, 1.0, 0.0, 20.0, -1.0, 0.0, 0.0, 30.0] {
            b.extend(v.to_be_bytes());
        }
        let m = matrix(&b);
        assert_eq!(m.transform_point3(Vec3::ZERO), Vec3::new(10.0, 20.0, 30.0));
        assert_eq!(m.transform_vector3(Vec3::X), Vec3::new(0.0, 0.0, -1.0));
        assert!(plausible(&m));
    }

    /// With the player's disc (`MKW_DISC_DIR`): the kart and Mario built, and Mario's vertices, posed by his
    /// skeleton at rest, stand as tall as his far stand-in.
    #[test]
    fn the_kart_and_driver_build_from_the_disc() {
        let Some(disc) = std::env::var_os("MKW_DISC_DIR") else { return };
        let files = szs::read(&std::fs::read(Path::new(&disc).join(ARCHIVE)).unwrap()).unwrap();
        let kart = build(&files).expect("the kart");
        let names: Vec<_> = kart.instances.iter().map(|i| i.model.as_str()).collect();
        assert_eq!(names, ["body", "tire_fl", "tire_fl", "tire_fr", "tire_fr", "model"]);
        // (its shadow: the body's footprint, along its length, about as long as the kart and as wide)
        let [BodyGroup { id: KART_BODY, joints, cast: Cast::Footprint { back, front, ellipses } }] = &kart.model.groups[..] else {
            panic!("the body's footprint: {:?}", kart.model.groups)
        };
        assert_eq!((joints.as_slice(), back.joint, front.joint), (&[0u16][..], 0, 0));
        let long = back.at.distance(front.at);
        eprintln!("the body's footprint: from {} to {} ({long:.0} long), ellipses {ellipses:?}", back.at, front.at);
        assert!((100.0..300.0).contains(&long), "a kart's length: {long}");
        assert!(ellipses[0].half_width > 0.2 && ellipses[0].half_width < ellipses[0].half_length);
        assert_eq!(kart.joints, 1 + 4 + 15);
        assert_eq!(kart.headers.len(), 4, "body, the two tires and Mario: {:?}", kart.headers.iter().map(|h| &h.1).collect::<Vec<_>>());
        let ModelSource::Meshes { meshes, textures, .. } = &kart.model.source else { panic!() };
        assert!(meshes.iter().all(|m| m.material.texture.is_some()), "every mesh textured");
        assert!(textures.iter().all(|t| t.pixels.len() == (t.width * t.height * 4) as usize));
        // (Mario at rest: each node's world matrix from its scale, rotation (degrees, x then y then z) and place)
        let driver = brres::Archive::from_memory(&files["driver_model.brres"]).unwrap();
        let skeleton = &driver.get_model("model").unwrap().bones;
        let mut world: Vec<Mat4> = Vec::new();
        for b in skeleton {
            let r = b.rotate.map(f32::to_radians);
            let local = Mat4::from_translation(Vec3::from_array(b.translate))
                * Mat4::from_rotation_z(r[2])
                * Mat4::from_rotation_y(r[1])
                * Mat4::from_rotation_x(r[0])
                * Mat4::from_scale(Vec3::from_array(b.scale));
            world.push(if b.parent >= 0 { world[b.parent as usize] * local } else { local });
        }
        let first = kart.instances.iter().find(|i| i.model == "model").unwrap().joint;
        let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
        for m in meshes.iter().filter(|m| m.joints.iter().all(|j| j[0] >= first)) {
            for (p, j) in m.positions.iter().zip(&m.joints) {
                let q = world[usize::from(j[0] - first)].transform_point3(Vec3::from_array(*p));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        eprintln!("Mario at rest: from {lo} to {hi}");
        assert!(hi.y - lo.y > 90.0 && hi.y - lo.y < 160.0 && lo.y > -20.0, "Mario stands up: {lo} to {hi}");
        // (the vertex colours as the file has them: Mario's first array's first colours are greys)
        let mario = &files["driver_model.brres"];
        let colours = raw_colors(mario, "mario_body__mario_all_tx").expect("Mario's colours");
        assert_eq!(colours.len(), 139);
        assert_eq!(colours[..2], [[0x49; 4], [0xC5; 4]]);
        // (greys, the console's baked lighting: not multiplied in - Mario's white is white)
        assert!(meshes.iter().all(|m| !m.material.vertex_color), "baked lighting left out");
        // (and wound with the normals: most triangles face the way their corners' normals do)
        for m in meshes {
            let agree = m.indices.chunks(3).filter(|t| {
                let p = t.iter().map(|&i| Vec3::from_array(m.positions[usize::from(i)])).collect::<Vec<_>>();
                let n: Vec3 = t.iter().map(|&i| Vec3::from_array(m.normals[usize::from(i)])).sum();
                (p[1] - p[0]).cross(p[2] - p[0]).dot(n) >= 0.0
            });
            assert!(agree.count() * 10 >= m.indices.len() / 3 * 7, "a mesh wound against its normals");
        }
    }
}
