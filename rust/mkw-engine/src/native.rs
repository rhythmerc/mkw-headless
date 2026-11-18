//! The game library (`libmkw_engine`: WiiCompiled's translation of the player's disc, headless), loaded at run time
//! from where the player keeps it, and its C API (`mkw_engine.h`).
use std::ffi::{CString, c_char, c_int};
use std::path::Path;

/// The GameCube pad in port 0 (`MkwPad`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pad {
    pub buttons: u16,
    pub stick_x: i8,
    pub stick_y: i8,
    pub substick_x: i8,
    pub substick_y: i8,
    pub trigger_l: u8,
    pub trigger_r: u8,
}

/// The pad's buttons.
pub mod button {
    pub const LEFT: u16 = 0x0001;
    pub const RIGHT: u16 = 0x0002;
    pub const DOWN: u16 = 0x0004;
    pub const UP: u16 = 0x0008;
    pub const Z: u16 = 0x0010;
    pub const R: u16 = 0x0020;
    pub const L: u16 = 0x0040;
    pub const A: u16 = 0x0100;
    pub const B: u16 = 0x0200;
    pub const X: u16 = 0x0400;
    pub const Y: u16 = 0x0800;
    pub const START: u16 = 0x1000;
}

/// The local kart (`MkwKart`): its position, its matrix (3x4 row-major: axes in the first three columns, position in
/// the last) and the race's kart updates so far.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kart {
    pub position: [f32; 3],
    pub matrix: [f32; 12],
    pub updates: u32,
}

type Boot = unsafe extern "C" fn(*const c_char, *const c_char) -> c_int;
type Racing = unsafe extern "C" fn() -> c_int;
type Step = unsafe extern "C" fn(*const Pad) -> c_int;
type GetKart = unsafe extern "C" fn(*mut Kart) -> c_int;
type SetCourseKcl = unsafe extern "C" fn(*const u8, u32);
type Read = unsafe extern "C" fn(u32, *mut u8, u32) -> c_int;
type TakeAudio = unsafe extern "C" fn(*mut i16, u32) -> u32;
type MuteSoundPlayers = unsafe extern "C" fn(u32);

/// Where the game's sound is taken from (`mkw_audio`): a function of the library, callable from any thread (the
/// library keeps the mix behind a lock), so it goes to the runtime's sound thread.
#[derive(Clone, Copy)]
pub struct AudioTap(TakeAudio);

impl AudioTap {
    /// Up to `out.len() / 2` frames of the game's mix (32 kHz, left and right interleaved): the frames given.
    pub fn take(&self, out: &mut [i16]) -> usize {
        // SAFETY: the library writes at most the frames asked for, two samples each.
        unsafe { (self.0)(out.as_mut_ptr(), (out.len() / 2) as u32) as usize }
    }
}

/// The loaded game. One a process: the game keeps its state in the library's globals, and the library stays loaded
/// when this goes (its threads still run in it).
pub struct Game {
    boot: Boot,
    racing: Racing,
    step: Step,
    kart: GetKart,
    set_course_kcl: SetCourseKcl,
    read: Read,
    take_audio: TakeAudio,
    mute_sound_players: MuteSoundPlayers,
    // (never unloaded: the game's threads run in it until the process ends)
    _library: std::mem::ManuallyDrop<libloading::Library>,
}

impl Game {
    /// The library at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: the library is the game built for this API (mkw_engine.h); its initialisers set up the runtime only.
        let library = unsafe { libloading::Library::new(path) }.map_err(|e| format!("{}: {e}", path.display()))?;
        // SAFETY: each symbol is the C function of that name in mkw_engine.h, with that signature.
        unsafe {
            let get = |name: &[u8]| -> Result<*const (), String> {
                library
                    .get::<*const ()>(name)
                    .map(|s| *s)
                    .map_err(|e| format!("{}: {e}", String::from_utf8_lossy(name)))
            };
            Ok(Self {
                boot: std::mem::transmute::<*const (), Boot>(get(b"mkw_boot\0")?),
                racing: std::mem::transmute::<*const (), Racing>(get(b"mkw_racing\0")?),
                step: std::mem::transmute::<*const (), Step>(get(b"mkw_step\0")?),
                kart: std::mem::transmute::<*const (), GetKart>(get(b"mkw_kart\0")?),
                set_course_kcl: std::mem::transmute::<*const (), SetCourseKcl>(get(b"mkw_set_course_kcl\0")?),
                read: std::mem::transmute::<*const (), Read>(get(b"mkw_read\0")?),
                take_audio: std::mem::transmute::<*const (), TakeAudio>(get(b"mkw_audio\0")?),
                mute_sound_players: std::mem::transmute::<*const (), MuteSoundPlayers>(get(b"mkw_mute_sound_players\0")?),
                _library: std::mem::ManuallyDrop::new(library),
            })
        }
    }

    /// The course's collision for the race to come (before [`Game::boot`]).
    pub fn set_course_kcl(&self, kcl: &[u8]) {
        // SAFETY: the API copies the bytes.
        unsafe { (self.set_course_kcl)(kcl.as_ptr(), kcl.len() as u32) }
    }

    /// Starts the game on its own thread from the extracted disc at `dvd_root`, its saves under `data_dir`.
    pub fn boot(&self, dvd_root: &Path, data_dir: &Path) -> Result<(), String> {
        let c = |p: &Path| CString::new(p.to_string_lossy().as_bytes()).map_err(|e| e.to_string());
        let (dvd, data) = (c(dvd_root)?, c(data_dir)?);
        // SAFETY: two NUL-terminated paths, read during the call.
        match unsafe { (self.boot)(dvd.as_ptr(), data.as_ptr()) } {
            0 => Ok(()),
            e => Err(format!("mkw_boot: {e}")),
        }
    }

    /// The race has started: steps are frames from here.
    pub fn racing(&self) -> bool {
        // SAFETY: no arguments.
        unsafe { (self.racing)() != 0 }
    }

    /// One frame with `pad`.
    pub fn step(&self, pad: &Pad) -> Result<(), String> {
        // SAFETY: the pad is read during the call.
        match unsafe { (self.step)(pad) } {
            0 => Ok(()),
            e => Err(format!("mkw_step: {e}")),
        }
    }

    /// The game's memory at guest `address` (big-endian, as the game has it), between steps: `out` filled, or false
    /// where it isn't all there.
    pub fn read(&self, address: u32, out: &mut [u8]) -> bool {
        // SAFETY: the API writes at most `out.len()` bytes.
        unsafe { (self.read)(address, out.as_mut_ptr(), out.len() as u32) == 0 }
    }

    /// Where the game's sound is taken from.
    pub fn audio(&self) -> AudioTap {
        AudioTap(self.take_audio)
    }

    /// The game's sound players kept silent, by bit (player 0's is bit 0).
    pub fn mute_sound_players(&self, mask: u32) {
        // SAFETY: a plain value.
        unsafe { (self.mute_sound_players)(mask) }
    }

    /// The local kart after the last frame.
    pub fn kart(&self) -> Option<Kart> {
        let mut kart = Kart::default();
        // SAFETY: the API writes the struct.
        (unsafe { (self.kart)(&mut kart) } == 0).then_some(kart)
    }
}
