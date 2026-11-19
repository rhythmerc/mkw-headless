//! The game's sound in the room: the kart's and its driver's only, played from the kart.
//!
//! The game sorts its sounds onto sound players (its sound archive's, `revo_kart.brsar`): the music, the menus', the
//! course's objects and ambience, the race's countdown and fanfares, and other drivers' voices are kept silent
//! ([`MUTED`]); what's left - the engine, the tyres on the ground, drift sparks, jumps, boosts and landings, and the
//! player's driver's voice - all comes from the kart. The game's final mix of it is taken as it is made
//! ([`crate::native::AudioTap`]) and handed to the runtime as a stream, which plays it from where the kart is: in mono,
//! its left and right summed - the game pans its mix about its own camera behind the kart, which isn't where the
//! player is; the emitter at the kart places it about the player's head.
use crate::native::AudioTap;
use engine_core::audio::Stream;
use std::time::Duration;

/// The game's sound players, as its sound archive numbers them.
pub mod player {
    pub const BGM: u32 = 0;
    pub const MENU_UI: u32 = 1;
    pub const OBJECTS: u32 = 2;
    pub const ENGINE: u32 = 3;
    pub const RUN: u32 = 4;
    pub const SPARK: u32 = 5;
    pub const VEHICLE: u32 = 6;
    pub const RACE_UI: u32 = 7;
    pub const VOICE: u32 = 8;
    pub const VOICE_NPC: u32 = 9;
    pub const VOICE_DELAY: u32 = 10;
}

/// Kept silent: the music, the menus, the course (its objects and ambience), the race's own (countdown, fanfares),
/// other drivers' voices.
pub const MUTED: u32 = 1 << player::BGM | 1 << player::MENU_UI | 1 << player::OBJECTS | 1 << player::RACE_UI | 1 << player::VOICE_NPC;

/// The game's mix: 32 kHz, stereo.
const RATE: u32 = 32_000;
/// A block: a hundredth of a second.
const BLOCK_FRAMES: usize = 320;

/// The game's mix as the runtime's stream: what the game made since the last block, the rest of the block silence
/// (the game makes it as it steps; held, it makes none).
pub struct Mix {
    tap: AudioTap,
    pcm: Vec<i16>,
}

impl Mix {
    pub fn new(tap: AudioTap) -> Self {
        Self { tap, pcm: vec![0; BLOCK_FRAMES * 2] }
    }
}

impl Stream for Mix {
    fn channels(&self) -> u16 {
        1
    }

    fn rate(&self) -> u32 {
        RATE
    }

    fn block(&self) -> Duration {
        Duration::from_secs_f64(BLOCK_FRAMES as f64 / f64::from(RATE))
    }

    fn render(&mut self, paused: bool) -> Result<Vec<f32>, String> {
        let got = self.tap.take(&mut self.pcm);
        // (held: what it had made before is let go, not heard late)
        let got = if paused { 0 } else { got };
        let mut out = vec![0.0; BLOCK_FRAMES];
        for (o, lr) in out.iter_mut().zip(self.pcm[..got * 2].chunks_exact(2)) {
            *o = (f32::from(lr[0]) + f32::from(lr[1])) / 65536.0;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kart_and_its_driver_are_heard_and_nothing_else() {
        for kept in [player::ENGINE, player::RUN, player::SPARK, player::VEHICLE, player::VOICE, player::VOICE_DELAY] {
            assert_eq!(MUTED >> kept & 1, 0, "player {kept} heard");
        }
        for muted in [player::BGM, player::MENU_UI, player::OBJECTS, player::RACE_UI, player::VOICE_NPC] {
            assert_eq!(MUTED >> muted & 1, 1, "player {muted} silent");
        }
    }
}
