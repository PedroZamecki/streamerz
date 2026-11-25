//! The sounds of one device, added up into one stream. Volumes move in steps per frame (a fade), so a
//! change never clicks; a sound faded out to stop ends when its volume reaches nothing.

use super::decode::Source;
use super::{CHANNELS, RATE};
use tokio::sync::oneshot;

/// Why a sound ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Finished,
    Stopped,
    Failed,
}

/// How a sound ended, and what went wrong when it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub why: End,
    pub error: Option<String>,
}

impl End {
    pub fn word(self) -> &'static str {
        match self {
            End::Finished => "finished",
            End::Stopped => "stopped",
            End::Failed => "failed",
        }
    }
}

/// A volume that moves to its target, a step per frame.
#[derive(Debug, Clone, Copy)]
pub struct Ramp {
    pub now: f32,
    pub target: f32,
    step: f32,
}

impl Ramp {
    pub fn new(v: f32) -> Self {
        Ramp {
            now: v,
            target: v,
            step: 0.0,
        }
    }

    /// Go to `target` over `ms` (at once for 0).
    pub fn to(&mut self, target: f32, ms: u64) {
        self.target = target;
        let frames = ms * RATE as u64 / 1000;
        if frames == 0 {
            self.now = target;
            self.step = 0.0;
        } else {
            self.step = (target - self.now).abs() / frames as f32;
        }
    }

    fn next(&mut self) -> f32 {
        if self.now < self.target {
            self.now = (self.now + self.step).min(self.target);
        } else if self.now > self.target {
            self.now = (self.now - self.step).max(self.target);
        }
        self.now
    }

    fn moving(&self) -> bool {
        self.now != self.target
    }
}

pub struct Voice {
    pub id: u64,
    pub name: String,
    pub file: String,
    pub source: Source,
    pub volume: Ramp,
    pub paused: bool,
    /// Ends when its volume reaches 0 (a stop with a fade).
    pub stopping: bool,
    pub played: u64,
    pub done: Option<oneshot::Sender<Ended>>,
}

impl Voice {
    pub fn position_ms(&self) -> u64 {
        self.played * 1000 / RATE as u64
    }
}

pub struct Mixer {
    pub voices: Vec<Voice>,
    pub master: Ramp,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(master: f32) -> Self {
        Mixer {
            voices: Vec::new(),
            master: Ramp::new(master),
            scratch: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    /// End a sound now and say why.
    fn finish(mut voice: Voice, why: End) {
        if let Some(done) = voice.done.take() {
            let _ = done.send(Ended { why, error: None });
        }
        super::changed();
    }

    /// End every sound because the device failed.
    pub fn fail_all(&mut self, error: &str) {
        for mut voice in self.voices.drain(..) {
            if let Some(done) = voice.done.take() {
                let _ = done.send(Ended {
                    why: End::Failed,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    /// Stop the sounds `which` picks, at once or over `fade_ms`. How many.
    pub fn stop(&mut self, which: impl Fn(&Voice) -> bool, fade_ms: u64) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < self.voices.len() {
            if which(&self.voices[i]) {
                n += 1;
                if fade_ms == 0 || self.voices[i].paused {
                    Self::finish(self.voices.remove(i), End::Stopped);
                    continue;
                }
                self.voices[i].stopping = true;
                self.voices[i].volume.to(0.0, fade_ms);
            }
            i += 1;
        }
        n
    }

    /// Mix the next frames into `out` (stereo, interleaved), scaled by the master volume. Sounds that
    /// ended are taken out and told.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / CHANNELS;
        self.scratch.resize(out.len(), 0.0);
        let mut i = 0;
        while i < self.voices.len() {
            let voice = &mut self.voices[i];
            if voice.paused {
                i += 1;
                continue;
            }
            let got = voice.source.read(&mut self.scratch);
            for f in 0..got {
                let v = voice.volume.next();
                out[f * 2] += self.scratch[f * 2] * v;
                out[f * 2 + 1] += self.scratch[f * 2 + 1] * v;
            }
            voice.played += got as u64;
            let faded = voice.stopping && !voice.volume.moving() && voice.volume.now <= 0.0;
            if got < frames || faded {
                let why = if faded { End::Stopped } else { End::Finished };
                Self::finish(self.voices.remove(i), why);
                continue;
            }
            i += 1;
        }
        for f in 0..frames {
            let m = self.master.next();
            out[f * 2] = (out[f * 2] * m).clamp(-1.0, 1.0);
            out[f * 2 + 1] = (out[f * 2 + 1] * m).clamp(-1.0, 1.0);
        }
    }
}
