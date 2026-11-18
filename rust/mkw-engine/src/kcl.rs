//! Course collision as Mario Kart Wii reads it (KCL, big-endian): each triangle stored as a prism (a corner, its face
//! normal, its three edge normals and its height), and a grid of cells listing the prisms near each one.
//!
//! The layout (a 0x3C header, then the corners, the normals, the prisms and the grid):
//! - header: offsets to the corners, the normals, the prisms (0x10 before the first: prisms count from 1) and the
//!   grid; the prisms' thickness; the area's lowest corner; its x, y and z masks (`!(size - 1)`, a size a power of
//!   two); the cell size's shift; the shifts that make a cell's index; the largest sphere it is asked about.
//! - a prism: its height (f32), then u16s: its corner, its face normal, its three edge normals, its attribute.
//! - the grid: one u32 per cell, x fastest, then y, then z. A leaf (top bit set) points, from the grid's start, at
//!   the u16 before its list of prisms (the game steps forward before reading); a list ends with 0.
//!
//! A triangle A, B, C (counter-clockwise seen from the side its normal points to) is kept as: face normal
//! `N = (B - A) x (C - A)`, edge normals pointing out of the triangle (`(A - C) x N`, `(B - A) x N`,
//! `(C - B) x N`), and height `(B - A) . E3`, the distance from A to the far edge. The game rebuilds B and C from
//! those (see [`Prism::corners`]).
//!
//! Here the grid is one level: every cell is a leaf, listing each triangle whose box (grown by the sphere radius)
//! meets the cell's.
use glam::Vec3;

/// A triangle and its attribute (its kind in the low five bits: [`ROAD`], [`WALL`], ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub corners: [Vec3; 3],
    pub attribute: u16,
}

/// Road (drivable), the plain kind.
pub const ROAD: u16 = 0x00;
/// A wall: karts bounce off it.
pub const WALL: u16 = 0x0C;

/// The prisms' thickness, and the largest sphere the game asks about (its courses' usual values).
pub const THICKNESS: f32 = 300.0;
pub const SPHERE_RADIUS: f32 = 250.0;

/// The cell size: 2^this units.
const CELL_SHIFT: u32 = 10;

const HEADER: usize = 0x3C;

/// A triangle as a prism (as read back).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prism {
    pub height: f32,
    pub corner: Vec3,
    pub face: Vec3,
    pub edges: [Vec3; 3],
    pub attribute: u16,
}

impl Prism {
    /// Its triangle's corners, rebuilt as the game rebuilds them.
    pub fn corners(&self) -> [Vec3; 3] {
        let cross_a = self.edges[0].cross(self.face);
        let cross_b = self.edges[1].cross(self.face);
        let b = self.corner + cross_b * (self.height / cross_b.dot(self.edges[2]));
        let c = self.corner + cross_a * (self.height / cross_a.dot(self.edges[2]));
        [self.corner, b, c]
    }
}

/// Why triangles couldn't be written.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// None to write.
    Empty,
    /// More than a KCL can index (u16).
    TooMany(usize),
}

/// The KCL for `triangles` (degenerate ones are left out).
pub fn write(triangles: &[Triangle]) -> Result<Vec<u8>, Error> {
    let mut corners: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut prisms: Vec<(f32, [u16; 5], u16)> = Vec::new();
    let mut kept: Vec<Triangle> = Vec::new();
    for t in triangles {
        let [a, b, c] = t.corners;
        let n = (b - a).cross(c - a);
        if n.length_squared() < 1e-12 {
            continue;
        }
        let n = n.normalize();
        let e1 = (a - c).cross(n).normalize();
        let e2 = (b - a).cross(n).normalize();
        let e3 = (c - b).cross(n).normalize();
        let height = (b - a).dot(e3);
        let index = |v: &mut Vec<Vec3>, x: Vec3| {
            v.push(x);
            (v.len() - 1) as u16
        };
        if corners.len() >= u16::MAX as usize || normals.len() + 4 >= u16::MAX as usize {
            return Err(Error::TooMany(triangles.len()));
        }
        let corner = index(&mut corners, a);
        let face = index(&mut normals, n);
        let edges = [index(&mut normals, e1), index(&mut normals, e2), index(&mut normals, e3)];
        prisms.push((height, [corner, face, edges[0], edges[1], edges[2]], t.attribute));
        kept.push(*t);
    }
    if kept.is_empty() {
        return Err(Error::Empty);
    }
    if kept.len() >= u16::MAX as usize {
        return Err(Error::TooMany(kept.len()));
    }

    // The area: every corner, with room for the sphere, its sides powers of two of whole cells.
    let margin = Vec3::splat(SPHERE_RADIUS + 1.0);
    let (lo, hi) = kept.iter().flat_map(|t| t.corners).fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    let lo = (lo - margin).floor();
    let hi = hi + margin;
    let shift = |extent: f32| -> u32 {
        let mut s = CELL_SHIFT;
        while ((1u64 << s) as f32) < extent {
            s += 1;
        }
        s
    };
    let size_shift = [shift(hi.x - lo.x), shift(hi.y - lo.y), shift(hi.z - lo.z)];
    let cells_shift = size_shift.map(|s| s - CELL_SHIFT);
    let cells = cells_shift.map(|s| 1usize << s);
    let cell = (1u32 << CELL_SHIFT) as f32;

    // Each cell's list: the triangles whose box, grown by the sphere, meets it.
    let boxes: Vec<(Vec3, Vec3)> = kept
        .iter()
        .map(|t| {
            let (l, h) = t.corners.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), &p| (l.min(p), h.max(p)));
            (l - margin, h + margin)
        })
        .collect();
    let mut lists: Vec<Vec<u16>> = Vec::with_capacity(cells[0] * cells[1] * cells[2]);
    for z in 0..cells[2] {
        for y in 0..cells[1] {
            for x in 0..cells[0] {
                let min = lo + Vec3::new(x as f32, y as f32, z as f32) * cell;
                let max = min + Vec3::splat(cell);
                let list: Vec<u16> = boxes
                    .iter()
                    .enumerate()
                    .filter(|(_, (l, h))| l.cmple(max).all() && h.cmpge(min).all())
                    .map(|(i, _)| (i + 1) as u16)
                    .collect();
                lists.push(list);
            }
        }
    }

    // The file.
    let mut out = vec![0u8; HEADER];
    let pos_offset = out.len();
    for p in &corners {
        put_vec(&mut out, *p);
    }
    let nrm_offset = out.len();
    for n in &normals {
        put_vec(&mut out, *n);
    }
    // (prisms count from 1: the offset is 0x10 before the first)
    let prism_offset = out.len() - 0x10;
    for (height, indices, attribute) in &prisms {
        out.extend(height.to_be_bytes());
        for i in indices {
            out.extend(i.to_be_bytes());
        }
        out.extend(attribute.to_be_bytes());
    }
    let grid_offset = out.len();
    let grid_len = lists.len() * 4;
    out.resize(grid_offset + grid_len, 0);
    // (a list is reached at the u16 before it: each starts after a 0, the last list's end or this one)
    out.extend(0u16.to_be_bytes());
    let empty = out.len() - grid_offset - 2;
    out.extend(0u16.to_be_bytes());
    for (i, list) in lists.iter().enumerate() {
        let at = if list.is_empty() {
            empty
        } else {
            let before = out.len() - grid_offset - 2;
            for &p in list {
                out.extend(p.to_be_bytes());
            }
            out.extend(0u16.to_be_bytes());
            before
        };
        let entry = 0x8000_0000u32 | at as u32;
        out[grid_offset + i * 4..grid_offset + i * 4 + 4].copy_from_slice(&entry.to_be_bytes());
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }

    let mut header = Vec::with_capacity(HEADER);
    for v in [pos_offset as u32, nrm_offset as u32, prism_offset as u32, grid_offset as u32] {
        header.extend(v.to_be_bytes());
    }
    header.extend(THICKNESS.to_be_bytes());
    put_vec(&mut header, lo);
    for s in size_shift {
        header.extend((!((1u32 << s) - 1)).to_be_bytes());
    }
    header.extend(CELL_SHIFT.to_be_bytes());
    header.extend(cells_shift[0].to_be_bytes());
    header.extend((cells_shift[0] + cells_shift[1]).to_be_bytes());
    header.extend(SPHERE_RADIUS.to_be_bytes());
    out[..HEADER].copy_from_slice(&header);
    Ok(out)
}

fn put_vec(out: &mut Vec<u8>, v: Vec3) {
    for x in v.to_array() {
        out.extend(x.to_be_bytes());
    }
}

/// A KCL read back: its prisms (from 1), and the prisms listed in the cell holding a point.
pub struct Kcl<'a> {
    data: &'a [u8],
}

impl<'a> Kcl<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn u32_at(&self, at: usize) -> u32 {
        u32::from_be_bytes(self.data[at..at + 4].try_into().unwrap())
    }

    fn u16_at(&self, at: usize) -> u16 {
        u16::from_be_bytes(self.data[at..at + 2].try_into().unwrap())
    }

    fn f32_at(&self, at: usize) -> f32 {
        f32::from_bits(self.u32_at(at))
    }

    fn vec_at(&self, at: usize) -> Vec3 {
        Vec3::new(self.f32_at(at), self.f32_at(at + 4), self.f32_at(at + 8))
    }

    /// Prism `index` (from 1).
    pub fn prism(&self, index: u16) -> Prism {
        let at = self.u32_at(8) as usize + index as usize * 0x10;
        let pos = self.u32_at(0) as usize;
        let nrm = self.u32_at(4) as usize;
        let n = |k: usize| self.vec_at(nrm + self.u16_at(at + 4 + k * 2) as usize * 12);
        Prism {
            height: self.f32_at(at),
            corner: self.vec_at(pos + self.u16_at(at + 4) as usize * 12),
            face: n(1),
            edges: [n(2), n(3), n(4)],
            attribute: self.u16_at(at + 14),
        }
    }

    /// The prisms listed in the cell holding `p` (none outside the area), walked as the game walks the grid.
    pub fn near(&self, p: Vec3) -> Vec<u16> {
        let lo = self.vec_at(0x14);
        let d = p - lo;
        if d.min_element() < 0.0 {
            return Vec::new();
        }
        let (x, y, z) = (d.x as u32, d.y as u32, d.z as u32);
        if x & self.u32_at(0x20) != 0 || y & self.u32_at(0x24) != 0 || z & self.u32_at(0x28) != 0 {
            return Vec::new();
        }
        let mut shift = self.u32_at(0x2C);
        let mut node = self.u32_at(0x0C) as usize;
        let index = ((z >> shift) << self.u32_at(0x34)) | ((y >> shift) << self.u32_at(0x30)) | (x >> shift);
        let mut offset = self.u32_at(node + index as usize * 4);
        while offset & 0x8000_0000 == 0 {
            node += offset as usize;
            shift -= 1;
            let child = (((z >> shift) & 1) << 2) | (((y >> shift) & 1) << 1) | ((x >> shift) & 1);
            offset = self.u32_at(node + child as usize * 4);
        }
        let mut at = node + (offset & 0x7FFF_FFFF) as usize;
        let mut out = Vec::new();
        loop {
            at += 2;
            match self.u16_at(at) {
                0 => return out,
                p => out.push(p),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, attribute: u16) -> [Triangle; 2] {
        [Triangle { corners: [a, b, c], attribute }, Triangle { corners: [a, c, d], attribute }]
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 0.05
    }

    /// A floor, a ramp and a wall: each triangle comes back where it went, facing the same way, with its kind.
    #[test]
    fn triangles_come_back_from_their_prisms() {
        let y = Vec3::Y;
        let mut tris = Vec::new();
        // (floors and ramps wound counter-clockwise seen from above: normals up)
        tris.extend(quad(Vec3::new(-5000.0, 0.0, -5000.0), Vec3::new(-5000.0, 0.0, 5000.0), Vec3::new(5000.0, 0.0, 5000.0), Vec3::new(5000.0, 0.0, -5000.0), ROAD));
        tris.extend(quad(Vec3::new(-500.0, 0.0, -1000.0), Vec3::new(-500.0, 0.0, 0.0), Vec3::new(500.0, 0.0, 0.0), Vec3::new(500.0, 300.0, -1000.0), ROAD));
        tris.extend(quad(Vec3::new(-2000.0, 0.0, -3000.0), Vec3::new(2000.0, 0.0, -3000.0), Vec3::new(2000.0, 1000.0, -3000.0), Vec3::new(-2000.0, 1000.0, -3000.0), WALL));
        let data = write(&tris).unwrap();
        let kcl = Kcl::new(&data);
        for (i, t) in tris.iter().enumerate() {
            let prism = kcl.prism(i as u16 + 1);
            let back = prism.corners();
            for k in 0..3 {
                assert!(close(back[k], t.corners[k]), "triangle {i} corner {k}: {} vs {}", back[k], t.corners[k]);
            }
            assert_eq!(prism.attribute, t.attribute);
            assert!(prism.height > 0.0);
        }
        assert!(kcl.prism(1).face.dot(y) > 0.99, "a floor faces up");
        // (each edge normal points away from the corner across from it)
        let p = kcl.prism(1);
        let [a, b, c] = tris[0].corners;
        assert!(p.edges[0].dot(b - a) < 0.0 && p.edges[1].dot(c - a) < 0.0 && p.edges[2].dot(a - b) < 0.0);
    }

    /// The grid finds what is near a point, and nothing outside the area.
    #[test]
    fn the_grid_lists_what_is_near() {
        let floor = quad(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 4000.0), Vec3::new(4000.0, 0.0, 4000.0), Vec3::new(4000.0, 0.0, 0.0), ROAD);
        let wall = quad(Vec3::new(10000.0, 0.0, 0.0), Vec3::new(10000.0, 0.0, 500.0), Vec3::new(10000.0, 500.0, 500.0), Vec3::new(10000.0, 500.0, 0.0), WALL);
        let tris: Vec<Triangle> = floor.into_iter().chain(wall).collect();
        let data = write(&tris).unwrap();
        let kcl = Kcl::new(&data);
        let mut on_floor = kcl.near(Vec3::new(2000.0, 50.0, 2000.0));
        on_floor.sort();
        assert_eq!(on_floor, [1, 2]);
        let by_wall = kcl.near(Vec3::new(9900.0, 100.0, 200.0));
        assert!(by_wall.contains(&3) && by_wall.contains(&4) && !by_wall.contains(&1), "{by_wall:?}");
        assert!(kcl.near(Vec3::new(6500.0, 100.0, 2000.0)).is_empty(), "between them, nothing");
        assert!(kcl.near(Vec3::new(-5000.0, 0.0, 0.0)).is_empty(), "outside the area");
        assert_eq!(data.len() % 4, 0);
    }

    #[test]
    fn nothing_to_write() {
        assert_eq!(write(&[]), Err(Error::Empty));
        let flat = Triangle { corners: [Vec3::ZERO, Vec3::X, Vec3::X * 2.0], attribute: ROAD };
        assert_eq!(write(&[flat]), Err(Error::Empty));
    }
}
