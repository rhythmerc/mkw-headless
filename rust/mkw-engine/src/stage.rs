//! The stage as the course's collision, and the contract's space as the game's.
//!
//! The race is Luigi Circuit's, its course swapped for the stage (the game's start stays where the course has it):
//! the stage is laid so the place the player is put down is at the course's start, facing the way the race starts
//! (the game's -z), its floor (y = 0) under the start. A game unit is a hundredth of a metre (the karts' size).
use crate::kcl::{self, Triangle};
use engine_core::Stage;
use engine_core::glam::{Mat4, Quat, Vec3};

/// Game units a metre.
pub const UNITS: f32 = 100.0;

/// Where the race starts on Luigi Circuit (its first start point), at the floor the stage is laid on: the kart is
/// put a little over it and settles onto it.
pub const START: Vec3 = Vec3::new(-20915.0, 400.0, 30873.0);

/// A face is ground (driven on, turned to face up) when its normal is at least this much up.
const GROUND_UP: f32 = 0.45;

/// The contract's space laid onto the game's: the contract's `origin` at [`START`], its `forward` along the game's -z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    origin: Vec3,
    turn: Quat,
}

impl Space {
    /// `origin` (where the player is put down) at the start, facing `forward` (its level part).
    pub fn new(origin: Vec3, forward: Vec3) -> Self {
        let f = forward.with_y(0.0).try_normalize().unwrap_or(Vec3::NEG_Z);
        let yaw = f.x.atan2(f.z) - Vec3::NEG_Z.x.atan2(Vec3::NEG_Z.z);
        Self { origin: origin.with_y(0.0), turn: Quat::from_rotation_y(-yaw) }
    }

    /// A contract point in the game.
    pub fn to_game(&self, p: Vec3) -> Vec3 {
        START + self.turn * (p - self.origin) * UNITS
    }

    /// A game point in the contract.
    pub fn from_game(&self, g: Vec3) -> Vec3 {
        self.origin + self.turn.inverse() * ((g - START) / UNITS)
    }

    /// A game frame (a matrix placing something, in game units) in the contract.
    pub fn frame_from_game(&self, m: Mat4) -> Mat4 {
        let to = Mat4::from_translation(self.origin) * Mat4::from_quat(self.turn.inverse()) * Mat4::from_scale(Vec3::splat(1.0 / UNITS)) * Mat4::from_translation(-START);
        to * m
    }

    /// The stage as the course's collision (game units): ground facing up, as road; any other face both ways, as wall.
    pub fn triangles(&self, stage: &Stage) -> Vec<Triangle> {
        let mut out = Vec::new();
        let faces = stage.room().chain(stage.solids.iter().flat_map(|s| s.quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])));
        for t in faces {
            let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            if n == Vec3::ZERO {
                continue;
            }
            let game = t.map(|p| self.to_game(p));
            let flipped = [game[0], game[2], game[1]];
            if n.y.abs() >= GROUND_UP {
                out.push(Triangle { corners: if n.y > 0.0 { game } else { flipped }, attribute: kcl::ROAD });
            } else {
                out.push(Triangle { corners: game, attribute: kcl::WALL });
                out.push(Triangle { corners: flipped, attribute: kcl::WALL });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_put_down_place_is_the_start_facing_the_race() {
        let space = Space::new(Vec3::new(2.0, 0.7, -1.0), Vec3::X);
        assert!((space.to_game(Vec3::new(2.0, 0.0, -1.0)) - START).length() < 1e-3, "the place, on the floor, at the start");
        let ahead = space.to_game(Vec3::new(3.0, 0.0, -1.0)) - START;
        assert!((ahead - Vec3::new(0.0, 0.0, -UNITS)).length() < 1e-2, "a metre ahead: 100 units along -z, {ahead}");
        let p = Vec3::new(0.5, 1.2, 3.0);
        assert!((space.from_game(space.to_game(p)) - p).length() < 1e-4);
        let m = Mat4::from_translation(space.to_game(p));
        assert!((space.frame_from_game(m).w_axis.truncate() - p).length() < 1e-4);
    }

    #[test]
    fn ground_faces_up_and_walls_both_ways() {
        let space = Space::new(Vec3::ZERO, Vec3::NEG_Z);
        let stage = Stage {
            revision: 1,
            // (a floor wound facing down, and a wall)
            room_quads: vec![
                [Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 1.0), Vec3::new(-1.0, 0.0, 1.0)],
                [Vec3::new(-1.0, 0.0, -2.0), Vec3::new(1.0, 0.0, -2.0), Vec3::new(1.0, 1.0, -2.0), Vec3::new(-1.0, 1.0, -2.0)],
            ],
            ..Default::default()
        };
        let tris = space.triangles(&stage);
        assert_eq!(tris.len(), 2 + 4);
        for t in &tris[..2] {
            let n = (t.corners[1] - t.corners[0]).cross(t.corners[2] - t.corners[0]);
            assert!(n.y > 0.0 && t.attribute == kcl::ROAD);
        }
        assert!(tris[2..].iter().all(|t| t.attribute == kcl::WALL));
        assert!(kcl::write(&tris).is_ok());
    }
}
