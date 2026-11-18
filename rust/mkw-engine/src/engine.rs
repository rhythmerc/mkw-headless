//! The game as the runtime drives it ([`engine_core::EngineCore`]): free driving on the stage. The race is a 1P race on
//! Luigi Circuit with its course's collision swapped for the stage's ([`crate::stage`]), booted once the stage is in
//! (the course's collision is read as the race loads), and stepped a frame (1/60 s) at a time once the race has
//! started. The kart is drawn as a stand-in box for now; the game's own models come later.
//!
//! For now the stage is taken once, at the start: a stage changed later, or the player put down again, waits for the
//! game to be made again (a restart of the engine).
use crate::native::{Game, Kart, Pad, button};
use crate::stage::{Space, UNITS};
use crate::kcl;
use engine_core::model::{Lighting, MaterialDesc, MeshData};
use engine_core::glam::{Mat4, Vec3, Vec4};
use engine_core::{Applied, EngineCore, EngineFactory, Focus, Model, ModelSource, PadHeld, PlacePlayer, Pose, Stage, TouchState};
use std::path::PathBuf;
use std::time::Duration;

/// Its one model, the kart.
pub const KART: engine_core::ModelId = 0;

/// The kart's height drawn (metres), and how far its position is over the ground (game units).
const KART_HEIGHT: f32 = 1.0;
const KART_RIDE: f32 = 26.7;

/// The game made on the runtime's worker: the library at `library` (built from the player's disc), the extracted disc
/// at `disc`, its saves under `data`.
pub fn factory(library: PathBuf, disc: PathBuf, data: PathBuf) -> EngineFactory {
    Box::new(move || {
        let game = Game::load(&library)?;
        Ok(Box::new(Mkwii::new(game, disc, data)) as Box<dyn EngineCore>)
    })
}

/// The running game.
pub struct Mkwii {
    game: Game,
    disc: PathBuf,
    data: PathBuf,
    /// The stage laid onto the course (from the start).
    space: Option<Space>,
    place: Option<PlacePlayer>,
    pad: Pad,
    kart: Option<Kart>,
    model: Option<Model>,
    posed: bool,
    cut: bool,
}

impl Mkwii {
    fn new(game: Game, disc: PathBuf, data: PathBuf) -> Self {
        Self { game, disc, data, space: None, place: None, pad: Pad::default(), kart: None, model: Some(stand_in()), posed: false, cut: true }
    }

    /// The kart's frame in the contract (its axes and position), from the game's 3x4 row-major matrix.
    fn kart_frame(&self, kart: &Kart) -> Option<Mat4> {
        let space = self.space?;
        let m = &kart.matrix;
        let game = Mat4::from_cols(
            Vec4::new(m[0], m[4], m[8], 0.0),
            Vec4::new(m[1], m[5], m[9], 0.0),
            Vec4::new(m[2], m[6], m[10], 0.0),
            Vec4::new(m[3], m[7], m[11], 1.0),
        );
        Some(space.frame_from_game(game))
    }
}

/// The Touch controllers as the GameCube pad: the right trigger (or A) drives, the left trigger (or B) brakes and
/// reverses, the right grip drifts (hops), the left grip uses an item, the left stick steers (and tricks).
pub fn pad(touch: &TouchState) -> Pad {
    let mut buttons = 0;
    if touch.right_trigger > 0.3 || touch.a {
        buttons |= button::A;
    }
    if touch.left_trigger > 0.3 || touch.b {
        buttons |= button::B;
    }
    if touch.right_grip > 0.5 {
        buttons |= button::R;
    }
    if touch.left_grip > 0.5 {
        buttons |= button::L;
    }
    if touch.menu {
        buttons |= button::START;
    }
    let axis = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
    Pad {
        buttons,
        stick_x: axis(touch.left_stick[0]),
        stick_y: axis(touch.left_stick[1]),
        substick_x: axis(touch.right_stick[0]),
        substick_y: axis(touch.right_stick[1]),
        trigger_l: if buttons & button::L != 0 { 255 } else { 0 },
        trigger_r: if buttons & button::R != 0 { 255 } else { 0 },
    }
}

/// A stand-in for the kart: a box the kart's size (game units, its forward +z), its bottom on the ground.
fn stand_in() -> Model {
    let (w, h, l) = (60.0, 45.0, 85.0);
    let lo = Vec3::new(-w, -KART_RIDE, -l);
    let hi = Vec3::new(w, h - KART_RIDE, l);
    let mut mesh = MeshData::default();
    // (each face its own four corners, wound counter-clockwise from outside)
    let faces: [(Vec3, [Vec3; 4]); 6] = [
        (Vec3::Y, [Vec3::new(lo.x, hi.y, lo.z), Vec3::new(lo.x, hi.y, hi.z), Vec3::new(hi.x, hi.y, hi.z), Vec3::new(hi.x, hi.y, lo.z)]),
        (Vec3::NEG_Y, [Vec3::new(lo.x, lo.y, lo.z), Vec3::new(hi.x, lo.y, lo.z), Vec3::new(hi.x, lo.y, hi.z), Vec3::new(lo.x, lo.y, hi.z)]),
        (Vec3::X, [Vec3::new(hi.x, lo.y, lo.z), Vec3::new(hi.x, hi.y, lo.z), Vec3::new(hi.x, hi.y, hi.z), Vec3::new(hi.x, lo.y, hi.z)]),
        (Vec3::NEG_X, [Vec3::new(lo.x, lo.y, lo.z), Vec3::new(lo.x, lo.y, hi.z), Vec3::new(lo.x, hi.y, hi.z), Vec3::new(lo.x, hi.y, lo.z)]),
        (Vec3::Z, [Vec3::new(lo.x, lo.y, hi.z), Vec3::new(hi.x, lo.y, hi.z), Vec3::new(hi.x, hi.y, hi.z), Vec3::new(lo.x, hi.y, hi.z)]),
        (Vec3::NEG_Z, [Vec3::new(lo.x, lo.y, lo.z), Vec3::new(lo.x, hi.y, lo.z), Vec3::new(hi.x, hi.y, lo.z), Vec3::new(hi.x, lo.y, lo.z)]),
    ];
    for (normal, corners) in faces {
        let base = mesh.positions.len() as u16;
        for c in corners {
            mesh.positions.push(c.to_array());
            mesh.normals.push(normal.to_array());
            mesh.uvs.push([0.0, 0.0]);
            // (the front a lighter red, so it reads which way the kart faces)
            let shade = if normal == Vec3::Z { [1.0, 0.55, 0.5, 1.0] } else { [1.0, 1.0, 1.0, 1.0] };
            mesh.colors.push(shade);
            mesh.joints.push([0; 4]);
            mesh.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh.material = MaterialDesc { base_color: [0.85, 0.12, 0.1, 1.0], vertex_color: true, lighting: Lighting::default(), ..MaterialDesc::default() };
    Model { source: ModelSource::Meshes { meshes: vec![mesh], textures: Vec::new(), joint_count: 1 }, parents: vec![None], groups: Vec::new() }
}

impl EngineCore for Mkwii {
    fn period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / 60.0)
    }

    fn set_input(&mut self, touch: &TouchState, held: PadHeld) {
        self.pad = if held.held { Pad::default() } else { pad(touch) };
    }

    /// The first stage, laid where the player is put down (else its origin), as the course's collision; then the race
    /// boots. Later stages wait for the next time the game is made.
    fn set_stage(&mut self, stage: &Stage) -> Applied {
        if self.space.is_some() {
            return Applied::Done;
        }
        if stage.revision == 0 {
            return Applied::Deferred;
        }
        let space = self.place.map_or(Space::new(Vec3::ZERO, Vec3::NEG_Z), |p| Space::new(p.point, p.forward));
        match kcl::write(&space.triangles(stage)) {
            Ok(kcl) => self.game.set_course_kcl(&kcl),
            Err(e) => eprintln!("MKWii: the stage's collision: {e:?}"),
        }
        if let Err(e) = self.game.boot(&self.disc, &self.data) {
            eprintln!("MKWii: {e}");
        }
        self.space = Some(space);
        Applied::Done
    }

    fn place_player(&mut self, place: PlacePlayer) {
        if self.space.is_none() {
            self.place = Some(place);
        }
    }

    /// Once the race has started (loading runs on the game's own thread).
    fn wants_steps(&self) -> bool {
        self.space.is_some() && self.game.racing()
    }

    fn step(&mut self) -> Result<(), String> {
        self.game.step(&self.pad)?;
        if let Some(kart) = self.game.kart() {
            self.kart = Some(kart);
            self.posed = true;
        }
        Ok(())
    }

    fn focus(&self) -> Option<Focus> {
        let kart = self.kart.as_ref()?;
        let frame = self.kart_frame(kart)?;
        let up = frame.y_axis.truncate().normalize_or(Vec3::Y);
        Some(Focus {
            feet: frame.w_axis.truncate() - up * (KART_RIDE / UNITS),
            up,
            forward: frame.z_axis.truncate().normalize_or(Vec3::NEG_Z),
            height: KART_HEIGHT,
        })
    }

    fn models(&mut self) -> Vec<Model> {
        self.model.take().into_iter().collect()
    }

    fn poses(&mut self, out: &mut Vec<Pose>) {
        if !std::mem::take(&mut self.posed) {
            return;
        }
        let Some(root) = self.kart.as_ref().and_then(|k| self.kart_frame(k)) else { return };
        out.push(Pose { model: KART, discontinuity: std::mem::take(&mut self.cut), root, joints: vec![Mat4::IDENTITY], ..Pose::default() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_touch_controllers_drive() {
        let touch = TouchState { right_trigger: 1.0, left_stick: [-1.0, 0.25], right_grip: 1.0, ..Default::default() };
        let p = pad(&touch);
        assert_eq!(p.buttons, button::A | button::R);
        assert_eq!((p.stick_x, p.stick_y, p.trigger_r), (-127, 32, 255));
        assert_eq!(pad(&TouchState::default()), Pad::default());
    }

    #[test]
    fn the_stand_in_is_a_closed_box_on_the_ground() {
        let model = stand_in();
        let ModelSource::Meshes { meshes, .. } = &model.source else { panic!("meshes") };
        assert_eq!((meshes[0].positions.len(), meshes[0].indices.len()), (24, 36));
        let lowest = meshes[0].positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((lowest + KART_RIDE).abs() < 1e-3);
    }
}
