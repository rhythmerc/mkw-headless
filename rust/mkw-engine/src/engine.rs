//! The game as the runtime drives it ([`engine_core::EngineCore`]): free driving on the stage. The race is a 1P race on
//! buildAR's own empty course (`crate::course`) with its collision swapped for the stage's ([`crate::stage`]), booted once the stage is in
//! (the course's collision is read as the race loads), and stepped a frame (1/60 s) at a time once the race has
//! started. The kart is drawn with the game's own models, posed by the game's own matrices ([`crate::kart_model`]);
//! a box stands in when the disc's kart can't be read.
//!
//! A stage changed later is swapped in at the next frame; putting the player down again lays the stage again under
//! the kart, so it is where it was put (the world moves under it).
use crate::native::{Game, Kart, Pad, button};
use crate::stage::{Space, UNITS};
use crate::kcl;
use crate::kart_model::{self, KartModel, Locator};
use engine_core::model::{Lighting, MaterialDesc, MeshData};
use engine_core::glam::{Mat4, Vec3, Vec4};
use engine_core::{Applied, EngineCore, EngineFactory, Focus, Model, ModelSource, PadHeld, PlacePlayer, Pose, Stage, TouchState};
use std::path::PathBuf;
use std::time::Duration;

/// Its one model, the kart.
pub const KART: engine_core::ModelId = 0;

/// Its settings ([`EngineCore::set_setting`]).
pub mod settings {
    /// The kart's drift: 0 manual (the default), 1 automatic.
    pub const DRIFT_AUTOMATIC: &str = "drift_automatic";
}

/// The kart's height drawn, Mario on it, and how far its position is over the ground (game units).
const KART_HEIGHT: f32 = 150.0;
const KART_RIDE: f32 = 26.7;

/// The game made on the runtime's worker: the library at `library` (built from the player's disc), the extracted disc
/// at `disc`, its saves under `data`.
pub fn factory(library: PathBuf, disc: PathBuf, data: PathBuf) -> EngineFactory {
    Box::new(move || {
        let game = Game::load(&library)?;
        let look = kart_model::load(&disc).map_err(|e| eprintln!("MKWii: the kart's models: {e}; a box stands in")).ok();
        Ok(Box::new(Mkwii::new(game, disc, data, look)) as Box<dyn EngineCore>)
    })
}

/// The running game.
pub struct Mkwii {
    game: Game,
    disc: PathBuf,
    data: PathBuf,
    /// The stage laid onto the course (from the start), and the last one given.
    space: Option<Space>,
    stage: Option<Stage>,
    place: Option<PlacePlayer>,
    /// A placement asked once the stage was laid but before the kart was there: made at the first frame it is.
    place_waiting: bool,
    pad: Pad,
    kart: Option<Kart>,
    model: Option<Model>,
    /// The kart's own look (else the box), where its scene models are, and after the last frame its body's world
    /// matrix and its joints in the body's space (game units).
    look: Option<KartModel>,
    locator: Locator,
    joints: Option<(Mat4, Vec<Mat4>)>,
    /// Frames until looking for the scene models again.
    relocate_in: u32,
    posed: bool,
    cut: bool,
}

impl Mkwii {
    fn new(game: Game, disc: PathBuf, data: PathBuf, look: Option<KartModel>) -> Self {
        // (only the kart and its driver heard: `crate::sound`)
        game.mute_sound_players(crate::sound::MUTED);
        let model = Some(look.as_ref().map_or_else(stand_in, |l| l.model.clone()));
        Self {
            game,
            disc,
            data,
            space: None,
            stage: None,
            place: None,
            place_waiting: false,
            pad: Pad::default(),
            kart: None,
            model,
            look,
            locator: Locator::default(),
            joints: None,
            relocate_in: 0,
            posed: false,
            cut: true,
        }
    }

    /// The kart's joints from the game's scene: found the first time (and again, a second on, when they're lost).
    fn pose_look(&mut self, kart: &Kart) {
        let Some(look) = self.look.as_ref() else { return };
        if !self.locator.found() {
            if self.relocate_in > 0 {
                self.relocate_in -= 1;
                return;
            }
            self.locator.locate(&self.game, look);
            if !self.locator.found() {
                eprintln!("MKWii: the kart's scene models not found yet");
                self.relocate_in = 60;
                return;
            }
        }
        self.joints = self.locator.pose(&self.game, look, Vec3::from_array(kart.position));
        if self.joints.is_none() {
            self.locator = Locator::default();
        }
    }

    /// The kart's frame in the contract: its axes from the game's 3x4 row-major matrix, at its position.
    fn kart_frame(&self, kart: &Kart) -> Option<Mat4> {
        let space = self.space?;
        let m = &kart.matrix;
        let game = Mat4::from_cols(
            Vec4::new(m[0], m[4], m[8], 0.0),
            Vec4::new(m[1], m[5], m[9], 0.0),
            Vec4::new(m[2], m[6], m[10], 0.0),
            // (where the kart is: its position, as the game reports it)
            Vec4::new(kart.position[0], kart.position[1], kart.position[2], 1.0),
        );
        Some(space.frame_from_game(game))
    }
}

impl Mkwii {
    /// The stage laid again under `kart`, so that it is where `place` puts it, facing that way.
    fn put_down(&mut self, place: PlacePlayer, kart: Kart) {
        let m = &kart.matrix;
        let floor = Vec3::from_array(kart.position) - Vec3::Y * KART_RIDE;
        let forward = Vec3::new(m[2], m[6], m[10]);
        self.space = Some(Space::at(place.point, place.forward, floor, forward));
        self.cut = true;
        self.lay();
    }

    /// The stage as the course's collision, laid in the space it is in now.
    fn lay(&mut self) {
        let (Some(space), Some(stage)) = (self.space, self.stage.as_ref()) else { return };
        match kcl::write(&space.triangles(stage)) {
            Ok(kcl) => self.game.set_course_kcl(&kcl),
            Err(e) => eprintln!("MKWii: the stage's collision: {e:?}"),
        }
    }
}

/// The Touch controllers as the GameCube pad: the right trigger (or A) drives, the left trigger (or B) brakes and
/// reverses, the right grip drifts (hops), the left stick steers (and tricks).
///
/// Not mapped: the item (L; the time trial's three mushrooms, used whenever the left grip was held - the controller
/// held is often gripped - and taken for boosts out of nowhere) and Start (the menu button opened the app's menu and
/// paused the game too, which then waited for A to go on).
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
    let axis = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
    Pad {
        buttons,
        stick_x: axis(touch.left_stick[0]),
        stick_y: axis(touch.left_stick[1]),
        substick_x: axis(touch.right_stick[0]),
        substick_y: axis(touch.right_stick[1]),
        trigger_l: 0,
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

    fn set_setting(&mut self, key: &str, value: f32) {
        if key == settings::DRIFT_AUTOMATIC {
            self.game.set_drift_automatic(value > 0.5);
        }
    }

    fn set_input(&mut self, touch: &TouchState, held: PadHeld) {
        self.pad = if held.held { Pad::default() } else { pad(touch) };
    }

    /// The first stage, laid where the player is put down (else its origin), as the course's collision; then the race
    /// boots. A later one is the course's collision from the next frame, laid the same way.
    fn set_stage(&mut self, stage: &Stage) -> Applied {
        if stage.revision == 0 {
            return Applied::Deferred;
        }
        if self.stage.as_ref().is_some_and(|s| s.revision == stage.revision) {
            return Applied::Done;
        }
        self.stage = Some(stage.clone());
        if self.space.is_some() {
            self.lay();
            return Applied::Done;
        }
        let space = self.place.map_or(Space::new(Vec3::ZERO, Vec3::NEG_Z), |p| Space::new(p.point, p.forward));
        self.space = Some(space);
        self.lay();
        // (buildAR's own course in place of the disc's)
        if let Err(e) = crate::course::overlay(&self.data) {
            eprintln!("MKWii: buildAR's course not written ({e}): the race has no course to load");
        }
        if let Err(e) = self.game.boot(&self.disc, &self.data) {
            eprintln!("MKWii: {e}");
        }
        Applied::Done
    }

    /// Before the race: where the stage is laid. During it: the stage laid again under the kart, so that it is
    /// where it was put down, facing that way (the world moves, not the kart).
    fn place_player(&mut self, place: PlacePlayer) {
        self.place = Some(place);
        let Some(kart) = self.kart else {
            // (the race still loading: laid under the kart once it is there)
            self.place_waiting = self.space.is_some();
            return;
        };
        self.put_down(place, kart);
    }

    /// Once the race has started (loading runs on the game's own thread).
    fn wants_steps(&self) -> bool {
        self.space.is_some() && self.game.racing()
    }

    fn step(&mut self) -> Result<(), String> {
        self.game.step(&self.pad)?;
        if let Some(kart) = self.game.kart() {
            if std::mem::take(&mut self.place_waiting)
                && let Some(place) = self.place
            {
                self.put_down(place, kart);
            }
            self.pose_look(&kart);
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
            height: KART_HEIGHT / UNITS,
        })
    }

    /// The game's sound (the kart's and its driver's), played from the kart.
    fn stream(&mut self) -> Option<engine_core::audio::StreamFactory> {
        let tap = self.game.audio();
        Some(Box::new(move || Ok(Box::new(crate::sound::Mix::new(tap)) as Box<dyn engine_core::audio::Stream>)))
    }

    fn models(&mut self) -> Vec<Model> {
        self.model.take().into_iter().collect()
    }

    fn poses(&mut self, out: &mut Vec<Pose>) {
        if !std::mem::take(&mut self.posed) {
            return;
        }
        let (root, joints) = match (&self.look, &self.joints, self.space) {
            // (the game's matrices, in its space: carried into the contract's)
            (Some(_), Some((body, joints)), Some(space)) => (space.frame_from_game(*body), joints.clone()),
            (Some(_), _, _) => return,
            (None, _, _) => {
                let Some(root) = self.kart.as_ref().and_then(|k| self.kart_frame(k)) else { return };
                (root, vec![Mat4::IDENTITY])
            }
        };
        out.push(Pose { model: KART, discontinuity: std::mem::take(&mut self.cut), root, joints, ..Pose::default() });
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
        // (the left grip held and the menu button pressed: no item, no pause)
        let p = pad(&TouchState { left_grip: 1.0, menu: true, ..Default::default() });
        assert_eq!(p, Pad::default());
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
