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
    start: f32,
    frames: u64,
    remaining: u64,
}

impl Ramp {
    pub fn new(v: f32) -> Self {
        Ramp {
            now: v,
            target: v,
            start: v,
            frames: 0,
            remaining: 0,
        }
    }

    /// Go to `target` over `ms` (at once for 0).
    pub fn to(&mut self, target: f32, ms: u64) {
        self.target = target;
        // RATE is a whole number of frames per millisecond.
        self.frames = ms.saturating_mul(u64::from(RATE / 1000));
        self.remaining = if self.now == target { 0 } else { self.frames };
        self.start = self.now;
        if self.remaining == 0 {
            self.now = target;
        }
    }

    fn next(&mut self) -> f32 {
        if self.remaining != 0 {
            self.remaining -= 1;
            let elapsed = (self.frames - self.remaining) as f32 / self.frames as f32;
            self.now = self.start + (self.target - self.start) * elapsed;
            if self.remaining == 0 {
                self.now = self.target;
            }
        }
        self.now
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

pub type Completion = (oneshot::Sender<Ended>, End);

pub struct Mixer {
    pub voices: Vec<Voice>,
    pub master: Ramp,
    scratch: Vec<f32>,
    pub output_open: bool,
}

impl Mixer {
    pub fn new(master: f32) -> Self {
        Mixer {
            voices: Vec::new(),
            master: Ramp::new(master),
            scratch: Vec::new(),
            output_open: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn has_active(&self) -> bool {
        self.voices.iter().any(|v| !v.paused)
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
        super::changed();
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

    pub fn pause(&mut self, which: impl Fn(&Voice) -> bool, paused: bool) -> usize {
        let mut count = 0;
        self.voices.retain_mut(|voice| {
            if !which(voice) {
                return true;
            }
            count += 1;
            voice.paused = paused;
            if paused && voice.stopping {
                if let Some(done) = voice.done.take() {
                    let _ = done.send(Ended {
                        why: End::Stopped,
                        error: None,
                    });
                }
                return false;
            }
            true
        });
        count
    }

    /// Mix stereo frames and return the number to write. Report completions after writing them.
    pub fn render(&mut self, out: &mut [f32], ended: &mut Vec<Completion>) -> usize {
        out.fill(0.0);
        let frames = out.len() / CHANNELS;
        self.scratch.resize(out.len(), 0.0);
        let mut written = 0;
        let mut i = 0;
        while i < self.voices.len() {
            let voice = &mut self.voices[i];
            if voice.paused {
                i += 1;
                continue;
            }
            let wanted = if voice.stopping {
                frames.min(voice.volume.remaining.min(usize::MAX as u64) as usize)
            } else {
                frames
            };
            let got = voice.source.read(&mut self.scratch[..wanted * CHANNELS]);
            written = written.max(got);
            for f in 0..got {
                let v = voice.volume.next();
                for c in 0..CHANNELS {
                    let at = f * CHANNELS + c;
                    let sample = self.scratch[at];
                    if sample.is_finite() {
                        out[at] += sample * v;
                    }
                }
            }
            voice.played += got as u64;
            let faded = voice.stopping && voice.volume.remaining == 0;
            if got < wanted || faded {
                let why = if faded { End::Stopped } else { End::Finished };
                if let Some(done) = self.voices.remove(i).done {
                    ended.push((done, why));
                }
                super::changed();
                continue;
            }
            i += 1;
        }
        for frame in out[..written * CHANNELS].as_chunks_mut::<CHANNELS>().0 {
            let m = self.master.next();
            for sample in frame {
                *sample = (*sample * m).clamp(-1.0, 1.0);
            }
        }
        written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_its_target_on_the_requested_frame() {
        let mut ramp = Ramp::new(1.0);
        ramp.to(0.0, 100);
        for _ in 1..4_800 {
            assert!(ramp.next() > 0.0);
        }
        assert_eq!(ramp.next(), 0.0);
        ramp.to(1.0, u64::MAX);
        assert_eq!(ramp.remaining, u64::MAX);
        ramp.to(0.2, 0);
        assert_eq!(ramp.next(), 0.2);
    }
}
