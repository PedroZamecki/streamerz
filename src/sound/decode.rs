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
    step: u64,
    /// Decoded, interleaved samples at the file's rate, from `pos` on still to play.
    buf: Vec<f32>,
    scratch: Vec<f32>,
    /// Source position in units of 1 / RATE source frames.
    pos: u64,
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
        let duration_ms = track.num_frames.map(|n| milliseconds(u128::from(n), 1, rate));
        let id = track.id;
        Ok(Source {
            format,
            decoder,
            track: id,
            channels,
            step: u64::from(rate),
            buf: Vec::new(),
            scratch: Vec::new(),
            pos: 0,
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

    /// Decode the next packet onto the buffer, dropping the frames already played.
    fn more(&mut self) {
        let played = (self.pos / u64::from(RATE)).min(self.frames() as u64) as usize;
        if played > 0 {
            self.buf.drain(..played * self.channels);
            self.pos -= played as u64 * u64::from(RATE);
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
                    return;
                }
                Err(Error::DecodeError(_)) => continue,
                Err(_) => self.done = true,
            }
        }
    }

    /// Fill `out` (stereo, interleaved, at the mix's rate). The frames written: fewer than asked at the end.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        let wanted = out.len() / CHANNELS;
        let mut n = 0;
        while n < wanted {
            let i = (self.pos / u64::from(RATE)) as usize;
            let frac = self.pos % u64::from(RATE);
            let next = i + usize::from(frac != 0);
            if !self.done && next >= self.frames() {
                self.more();
                // Refilling also drops consumed frames, so recalculate the indices.
                continue;
            }
            if i >= self.frames() {
                break;
            }
            let t = frac as f32 / RATE as f32;
            let (l0, r0) = self.frame(i);
            let (l1, r1) = self.frame(next.min(self.frames() - 1));
            out[n * 2] = l0 + (l1 - l0) * t;
            out[n * 2 + 1] = r0 + (r1 - r0) * t;
            n += 1;
            self.pos += self.step;
        }
        n
    }
}

fn milliseconds(ticks: u128, numerator: u32, denominator: u32) -> u64 {
    let ms = ticks * 1000 * u128::from(numerator) / u128::from(denominator);
    ms.min(u128::from(u64::MAX)) as u64
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
    let mut ticks = 0u128;
    let base = format
        .tracks()
        .iter()
        .find(|t| t.id == source.track)
        .and_then(|t| t.time_base);
    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id == source.track {
            ticks += u128::from(packet.dur.get());
        }
    }
    info.duration_ms = base.map(|b| milliseconds(ticks, b.numer.get(), b.denom.get()));
    Ok(info)
}

/// Decodes any supported sound file (WAV, MP3, OGG, M4A, FLAC) into a 16-bit mono 24 kHz WAV,
/// capped at `max_secs` seconds. Returns `(wav_bytes, duration_ms)`.
pub fn decode_mono_wav(path: &Path, max_secs: u32) -> Result<(Vec<u8>, u64), String> {
    let mut source = Source::open(path)?;
    const TARGET_RATE: u32 = 24_000;
    let max_out_frames =
        (u64::from(TARGET_RATE) * u64::from(max_secs.max(1))).min(u64::from((u32::MAX - 36) / 2)) as usize;
    let mut mono = Vec::new();
    // Source::read outputs stereo interleaved at super::RATE (48 kHz) -> 2 stereo frames = 1 mono 24 kHz frame.
    let mut stereo = vec![0f32; 960 * CHANNELS];
    loop {
        let frames = source.read(&mut stereo);
        if frames == 0 {
            break;
        }
        let mut i = 0;
        while i < frames && mono.len() < max_out_frames {
            let l0 = stereo[i * 2];
            let r0 = stereo[i * 2 + 1];
            let sample = if i + 1 < frames {
                let l1 = stereo[(i + 1) * 2];
                let r1 = stereo[(i + 1) * 2 + 1];
                (l0 + r0 + l1 + r1) * 0.25
            } else {
                (l0 + r0) * 0.5
            };
            mono.push(pcm16(sample));
            i += 2;
        }
        if mono.len() >= max_out_frames {
            break;
        }
    }
    if mono.is_empty() {
        return Err(format!("{} has no audible sound", path.display()));
    }
    let duration_ms = mono.len() as u64 * 1000 / TARGET_RATE as u64;
    let mut w = pcm_wav_header(TARGET_RATE, 1, mono.len())?;
    for s in mono {
        w.extend_from_slice(&s.to_le_bytes());
    }
    Ok((w, duration_ms))
}

/// Convert a normalized sample to signed PCM, rounding and saturating at both endpoints.
pub(crate) fn pcm16(sample: f32) -> i16 {
    if sample.is_finite() {
        (sample * 32768.0).round() as i16
    } else {
        0
    }
}

/// A 16-bit PCM header with space reserved for the caller's interleaved samples.
pub(crate) fn pcm_wav_header(rate: u32, channels: u16, samples: usize) -> Result<Vec<u8>, String> {
    let error = "invalid PCM WAV size or format";
    let align = channels.checked_mul(2).filter(|a| *a > 0).ok_or(error)?;
    let byte_rate = rate.checked_mul(u32::from(align)).filter(|r| *r > 0).ok_or(error)?;
    let data = u32::try_from(samples)
        .ok()
        .and_then(|n| n.checked_mul(2))
        .ok_or(error)?;
    let riff = data.checked_add(36).ok_or(error)?;
    let capacity = (riff as usize).checked_add(8).ok_or(error)?;
    if !data.is_multiple_of(u32::from(align)) {
        return Err(error.into());
    }
    let mut w = Vec::new();
    w.try_reserve_exact(capacity).map_err(|e| e.to_string())?;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&riff.to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&align.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    Ok(w)
}

/// A WAV file of PCM 16 bits: `frames` of a sine at `hz` (tests, and the voice of offline scenarios).
pub fn sine_wav(rate: u32, channels: u16, hz: f32, frames: u32) -> Vec<u8> {
    let samples = frames as usize * usize::from(channels);
    let mut w = pcm_wav_header(rate, channels, samples).expect("valid offline sound");
    for i in 0..frames {
        let v = (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin();
        let s = pcm16(v * 0.5);
        for _ in 0..channels {
            w.extend_from_slice(&s.to_le_bytes());
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_conversion_clips_rounds_and_handles_nonfinite_samples() {
        assert_eq!(pcm16(-2.0), i16::MIN);
        assert_eq!(pcm16(2.0), i16::MAX);
        assert_eq!(pcm16(0.5), 16_384);
        assert_eq!(pcm16(-0.5), -16_384);
        assert_eq!(pcm16(1.0 / 40_000.0), 1);
        assert_eq!(pcm16(f32::NAN), 0);
    }

    #[test]
    fn wav_header_describes_interleaved_pcm_and_rejects_invalid_sizes() {
        let wav = sine_wav(24_000, 2, 6_000.0, 2);
        assert_eq!(wav.len(), 52);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[4..8], &44u32.to_le_bytes());
        assert_eq!(&wav[24..28], &24_000u32.to_le_bytes());
        assert_eq!(&wav[28..32], &96_000u32.to_le_bytes());
        assert_eq!(&wav[32..34], &4u16.to_le_bytes());
        assert_eq!(&wav[40..44], &8u32.to_le_bytes());
        assert_eq!(&wav[44..], &[0, 0, 0, 0, 0, 64, 0, 64]);
        assert!(pcm_wav_header(24_000, 2, 3).is_err());
        assert!(pcm_wav_header(u32::MAX, 2, 4).is_err());
        assert!(pcm_wav_header(24_000, 1, usize::MAX).is_err());
        assert_eq!(milliseconds(u128::from(u64::MAX), 1, 1000), u64::MAX);
    }

    #[test]
    fn resampling_preserves_frame_count_across_packets_and_at_eof() {
        for (rate, count, expected) in [
            (24_000, 1, 2),
            (48_000, 1, 1),
            (44_100, 441, 480),
            (192_000, 131_073, 32_769),
        ] {
            let path = std::env::temp_dir().join(format!("streamerz-resample-{}-{rate}.wav", crate::engine::uuid()));
            let mut wav = pcm_wav_header(rate, 1, count).unwrap();
            wav.extend((0..count).flat_map(|i| ((i % 16_381) as i16).to_le_bytes()));
            std::fs::write(&path, wav).unwrap();
            let mut source = Source::open(&path).unwrap();
            let mut frames = 0;
            let mut out = [0.0; 62];
            loop {
                let n = source.read(&mut out);
                if n == 0 {
                    break;
                }
                for pair in out[..n * 2].as_chunks::<2>().0 {
                    let src = frames as f64 * f64::from(rate) / f64::from(RATE);
                    let i = src as usize;
                    let a = (i % 16_381) as f64;
                    let b = ((i + 1).min(count - 1) % 16_381) as f64;
                    let expected = (a + (b - a) * src.fract()) / 32768.0;
                    assert!((f64::from(pair[0]) - expected).abs() < 1e-6, "frame {frames} at {rate}");
                    assert_eq!(pair[0], pair[1]);
                    frames += 1;
                }
            }
            assert_eq!(frames, expected, "rate {rate}");
            drop(source);
            std::fs::remove_file(path).unwrap();
        }
    }
}
