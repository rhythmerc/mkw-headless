//! The stage as the course's collision, and the contract's space as the game's.
//!
//! The race is a solo time trial on Luigi Circuit, its course swapped for the stage (the game's start stays where the
//! course has it):
//! the stage is laid so the place the player is put down (at its height: the floor, a table) is at the course's start,
//! facing the way the race starts (the game's -z), the room [`UNITS`] game units a metre (toy-sized karts).
use crate::kcl::{self, Triangle};
use engine_core::Stage;
use engine_core::glam::{Mat4, Quat, Vec3};

/// Game units a metre: the game's own (a kart about 2.5 m long, Mario about 1.5 m tall). How big that is in the room is
/// the runtime's world scale (the app's default for Mario Kart Wii: 8 game metres a real one, a toy).
pub const UNITS: f32 = 100.0;

/// Where the race (a solo time trial on Luigi Circuit) puts the kart, at the floor the stage is laid on: the kart
/// rests 26.7 units over it (measured: (-20115, 426.7, 25750) on a floor at 400).
pub const START: Vec3 = Vec3::new(-20115.0, 400.0, 25750.0);

/// A face is ground (driven on, turned to face up) when its normal is at least this much up.
const GROUND_UP: f32 = 0.45;

/// The contract's space laid onto the game's: the contract's `origin` (on its floor) at the game's `anchor`, its
/// `forward` along the game's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    origin: Vec3,
    turn: Quat,
    anchor: Vec3,
}

/// The yaw of a direction's level part (0 along +z).
fn yaw(d: Vec3) -> Option<f32> {
    let d = d.with_y(0.0).try_normalize()?;
    Some(d.x.atan2(d.z))
}

impl Space {
    /// `origin` (where the player is put down) at the race's start, facing `forward` (its level part) the way the
    /// race starts (the game's -z).
    pub fn new(origin: Vec3, forward: Vec3) -> Self {
        Self::at(origin, forward, START, Vec3::NEG_Z)
    }

    /// `origin` facing `forward` at the game's `anchor` (a floor point) facing `game_forward`: the stage laid under a
    /// kart where it is, the way it faces.
    pub fn at(origin: Vec3, forward: Vec3, anchor: Vec3, game_forward: Vec3) -> Self {
        let turn = yaw(game_forward).unwrap_or(std::f32::consts::PI) - yaw(forward).unwrap_or(std::f32::consts::PI);
        // (the point at its own height: a table's top, a ramp's, as much as the floor - `PlacePlayer`'s point is on the
        // stage's ground, wherever that is)
        Self { origin, turn: Quat::from_rotation_y(turn), anchor }
    }

    /// A contract point in the game.
    pub fn to_game(&self, p: Vec3) -> Vec3 {
        self.anchor + self.turn * (p - self.origin) * UNITS
    }

    /// A game point in the contract.
    pub fn from_game(&self, g: Vec3) -> Vec3 {
        self.origin + self.turn.inverse() * ((g - self.anchor) / UNITS)
    }

    /// A game direction in the contract.
    pub fn dir_from_game(&self, d: Vec3) -> Vec3 {
        self.turn.inverse() * d
    }

    /// A game frame (a matrix placing something, in game units) in the contract.
    pub fn frame_from_game(&self, m: Mat4) -> Mat4 {
        let to = Mat4::from_translation(self.origin) * Mat4::from_quat(self.turn.inverse()) * Mat4::from_scale(Vec3::splat(1.0 / UNITS)) * Mat4::from_translation(-self.anchor);
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
        // (put down on something 0.7 m up: that point, not the floor under it, at the start)
        let space = Space::new(Vec3::new(2.0, 0.7, -1.0), Vec3::X);
        assert!((space.to_game(Vec3::new(2.0, 0.7, -1.0)) - START).length() < 1e-3, "the place, at its height, at the start");
        assert!((space.to_game(Vec3::new(2.0, 0.0, -1.0)) - (START - Vec3::Y * 0.7 * UNITS)).length() < 1e-2, "the floor under it, under the start");
        let ahead = space.to_game(Vec3::new(3.0, 0.7, -1.0)) - START;
        assert!((ahead - Vec3::new(0.0, 0.0, -UNITS)).length() < 1e-2, "a metre ahead: UNITS along -z, {ahead}");
        let p = Vec3::new(0.5, 1.2, 3.0);
        assert!((space.from_game(space.to_game(p)) - p).length() < 1e-4);
        let m = Mat4::from_translation(space.to_game(p));
        assert!((space.frame_from_game(m).w_axis.truncate() - p).length() < 1e-4);
        // (laid under a kart somewhere else, facing +x in the game)
        let under = Space::at(Vec3::new(1.0, 0.0, 1.0), Vec3::Z, Vec3::new(500.0, 30.0, -200.0), Vec3::X);
        assert!((under.to_game(Vec3::new(1.0, 0.0, 1.0)) - Vec3::new(500.0, 30.0, -200.0)).length() < 1e-3);
        assert!((under.to_game(Vec3::new(1.0, 0.0, 2.0)) - Vec3::new(500.0 + UNITS, 30.0, -200.0)).length() < 1e-2, "forward is the game's +x");
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
