//! A sound file read as it plays: one packet decoded at a time, turned into stereo frames at the
//! mix's rate (a linear resampler), so a long file never sits whole in memory.

use super::{CHANNELS, RATE};
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// The extensions the player opens (what Streamer.bot lists, and FLAC).
pub const EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "oga", "m4a", "aac", "mp4", "flac"];

/// What a file holds, read without decoding it.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub duration_ms: Option<u64>,
    pub rate: u32,
    pub channels: usize,
    pub format: String,
}

pub struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    channels: usize,
    /// Source frames per mix frame.
    step: f64,
    /// Decoded, interleaved samples at the file's rate, from `pos` on still to play.
    buf: Vec<f32>,
    scratch: Vec<f32>,
    pos: f64,
    done: bool,
    pub info: Info,
}

fn open_reader(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    symphonia::default::get_probe()
        .probe(&hint, stream, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("{} is not a sound Streamerz can play ({e})", path.display()))
}

fn format_name(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

impl Source {
    pub fn open(path: &Path) -> Result<Self, String> {
        let format = open_reader(path)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| format!("{} has no sound in it", path.display()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| format!("{} uses a sound format Streamerz can't play", path.display()))?
            .clone();
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let rate = params.sample_rate.unwrap_or(RATE).max(1);
        let channels = params.channels.as_ref().map_or(2, |c| c.count()).max(1);
        let duration_ms = track.num_frames.map(|n| n * 1000 / rate as u64);
        let id = track.id;
        Ok(Source {
            format,
            decoder,
            track: id,
            channels,
            step: rate as f64 / RATE as f64,
            buf: Vec::new(),
            scratch: Vec::new(),
            pos: 0.0,
            done: false,
            info: Info {
                duration_ms,
                rate,
                channels,
                format: format_name(path),
            },
        })
    }

    fn frames(&self) -> usize {
        self.buf.len() / self.channels
    }

    fn frame(&self, i: usize) -> (f32, f32) {
        let at = i * self.channels;
        if self.channels == 1 {
            (self.buf[at], self.buf[at])
        } else {
            (self.buf[at], self.buf[at + 1])
        }
    }

    /// Decode the next packet onto the buffer, dropping the frames already played. False at the end.
    fn more(&mut self) -> bool {
        let played = self.pos.floor() as usize;
        if played > 0 {
            self.buf.drain(..(played * self.channels).min(self.buf.len()));
            self.pos -= played as f64;
        }
        while !self.done {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) | Err(_) => {
                    self.done = true;
                    break;
                }
            };
            if packet.track_id != self.track {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    let channels = audio.spec().channels().count().max(1);
                    if channels != self.channels {
                        // A stream that changes its layout midway: keep the layout it started with.
                        continue;
                    }
                    self.scratch.resize(audio.samples_interleaved(), 0.0);
                    audio.copy_to_slice_interleaved(&mut self.scratch);
                    self.buf.extend_from_slice(&self.scratch);
                    return true;
                }
                Err(Error::DecodeError(_)) => continue,
                Err(_) => self.done = true,
            }
        }
        false
    }

    /// Fill `out` (stereo, interleaved, at the mix's rate). The frames written: fewer than asked at the end.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        let wanted = out.len() / CHANNELS;
        let mut n = 0;
        while n < wanted {
            let i = self.pos as usize;
            if i + 1 >= self.frames() {
                if self.more() {
                    continue;
                }
                break;
            }
            let t = (self.pos - i as f64) as f32;
            let (l0, r0) = self.frame(i);
            let (l1, r1) = self.frame(i + 1);
            out[n * 2] = l0 + (l1 - l0) * t;
            out[n * 2 + 1] = r0 + (r1 - r0) * t;
            n += 1;
            self.pos += self.step;
        }
        n
    }
}

/// A file's length, rate and channels. The length comes from the container, else from adding up
/// its packets (read, not decoded).
pub fn info(path: &Path) -> Result<Info, String> {
    let source = Source::open(path)?;
    if source.info.duration_ms.is_some() {
        return Ok(source.info);
    }
    let mut info = source.info.clone();
    let mut format = source.format;
    let mut ticks = 0u64;
    let base = format
        .tracks()
        .iter()
        .find(|t| t.id == source.track)
        .and_then(|t| t.time_base);
    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id == source.track {
            ticks += packet.dur.get();
        }
    }
    info.duration_ms = base.map(|b| ticks * 1000 * b.numer.get() as u64 / b.denom.get() as u64);
    Ok(info)
}

/// A WAV file of PCM 16 bits, for tests: `frames` of a sine at `hz`.
#[cfg(test)]
pub fn sine_wav(rate: u32, channels: u16, hz: f32, frames: u32) -> Vec<u8> {
    let data = frames * channels as u32 * 2;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let v = (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin();
        let s = (v * 0.5 * i16::MAX as f32) as i16;
        for _ in 0..channels {
            w.extend_from_slice(&s.to_le_bytes());
        }
    }
    w
}
