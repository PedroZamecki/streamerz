//! Voice audio effects and pitch shifting on 16-bit PCM WAV bytes.
//! Pure Rust, zero external crates, runs only when a voice has a non-default pitch or effect.

/// Supported built-in voice effect ids.
pub const EFFECTS: &[&str] = &["", "echo", "radio", "robot"];

/// Parses a 16-bit PCM WAV into `(sample_rate, channels, samples_f32)`.
fn parse_wav(wav: &[u8]) -> Option<(u32, u16, Vec<f32>)> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let mut rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    while at + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        let end = (at + 8).checked_add(size)?;
        if end > wav.len() {
            return None;
        }
        match &wav[at..at + 4] {
            b"fmt " if end >= at + 24 => {
                let format = u16::from_le_bytes(wav[at + 8..at + 10].try_into().ok()?);
                if format != 1 {
                    return None;
                }
                channels = u16::from_le_bytes(wav[at + 10..at + 12].try_into().ok()?);
                rate = u32::from_le_bytes(wav[at + 12..at + 16].try_into().ok()?);
                bits = u16::from_le_bytes(wav[at + 22..at + 24].try_into().ok()?);
            }
            b"data" => {
                if !(1..=384_000).contains(&rate) || channels == 0 || channels > 32 || bits != 16 {
                    return None;
                }
                let raw = &wav[at + 8..end];
                if !raw.len().is_multiple_of(usize::from(channels) * 2) {
                    return None;
                }
                let mut samples = Vec::with_capacity(raw.len() / 2);
                for &[lo, hi] in raw.as_chunks::<2>().0 {
                    let s = i16::from_le_bytes([lo, hi]);
                    samples.push(s as f32 / 32768.0);
                }
                return Some((rate, channels, samples));
            }
            _ => {}
        }
        at = end.checked_add(size & 1)?;
    }
    None
}

/// Encodes mono or multi-channel `f32` samples into a 16-bit PCM WAV.
fn write_wav(rate: u32, channels: u16, samples: &[f32]) -> Result<Vec<u8>, String> {
    let mut out = crate::sound::pcm_wav_header(rate, channels, samples.len())?;
    out.extend(samples.iter().flat_map(|&s| crate::sound::pcm16(s).to_le_bytes()));
    Ok(out)
}

/// Resamples a WAV by `pitch` (e.g. `1.2` raises pitch by 20% and divides the duration by 1.2; when paired
/// with asking a neural model for `speed / pitch`, pitch shifts while speaking tempo stays constant).
pub fn apply_pitch(wav: Vec<u8>, pitch: f64) -> Vec<u8> {
    if !pitch.is_finite() || (pitch - 1.0).abs() < 0.01 {
        return wav;
    }
    let pitch = pitch.clamp(0.5, 1.5);
    let Some((rate, channels, samples)) = parse_wav(&wav) else {
        return wav;
    };
    let ch = channels as usize;
    let frames = samples.len() / ch;
    if frames < 2 {
        return wav;
    }
    let out_frames = ((frames as f64) / pitch).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_frames * ch);
    for i in 0..out_frames {
        let src = (i as f64) * pitch;
        let idx = src as usize;
        let frac = (src - idx as f64) as f32;
        for c in 0..ch {
            let a = samples[idx * ch + c];
            let b = samples[(idx + 1).min(frames - 1) * ch + c];
            out.push(a + (b - a) * frac);
        }
    }
    write_wav(rate, channels, &out).unwrap_or(wav)
}

/// Applies a named effect (`echo`, `radio`, `robot`) to a 16-bit PCM WAV. Unknown or empty effect
/// returns the WAV unchanged.
pub fn apply_effect(wav: Vec<u8>, effect: &str) -> Vec<u8> {
    let effect = effect.trim().to_ascii_lowercase();
    if !matches!(effect.as_str(), "echo" | "radio" | "robot") {
        return wav;
    }
    let Some((rate, channels, samples)) = parse_wav(&wav) else {
        return wav;
    };
    if samples.is_empty() {
        return wav;
    }
    let ch = channels as usize;
    let frames = samples.len() / ch;
    let out = match effect.as_str() {
        "echo" => {
            let delay = ((rate as f32) * 0.18).round().max(1.0) as usize;
            let tail = ((rate as f32) * 0.45).round() as usize;
            let total = frames + tail;
            let mut out = vec![0.0f32; total * ch];
            for i in 0..total {
                for c in 0..ch {
                    let dry = if i < frames { samples[i * ch + c] } else { 0.0 };
                    let wet = if i >= delay {
                        out[(i - delay) * ch + c] * 0.42
                    } else {
                        0.0
                    };
                    out[i * ch + c] = (dry + wet).clamp(-1.0, 1.0);
                }
            }
            out
        }
        "radio" => {
            // Simple 1-pole high-pass (~320 Hz) + low-pass (~3200 Hz) + gentle drive.
            let dt = 1.0 / (rate as f32);
            let rc_hp = 1.0 / (std::f32::consts::TAU * 320.0);
            let alpha_hp = rc_hp / (rc_hp + dt);
            let rc_lp = 1.0 / (std::f32::consts::TAU * 3200.0);
            let alpha_lp = dt / (rc_lp + dt);
            let mut out = vec![0.0f32; samples.len()];
            let mut prev_in = vec![0.0f32; ch];
            let mut prev_hp = vec![0.0f32; ch];
            let mut prev_lp = vec![0.0f32; ch];
            for i in 0..frames {
                for c in 0..ch {
                    let x = samples[i * ch + c];
                    let hp = alpha_hp * (prev_hp[c] + x - prev_in[c]);
                    prev_in[c] = x;
                    prev_hp[c] = hp;
                    let lp = prev_lp[c] + alpha_lp * (hp - prev_lp[c]);
                    prev_lp[c] = lp;
                    // Soft saturation gives classic comms-radio warmth.
                    let driven = (lp * 2.4).tanh() * 0.85;
                    out[i * ch + c] = driven.clamp(-1.0, 1.0);
                }
            }
            out
        }
        "robot" => {
            // Ring modulation at 85 Hz blended with a short 11 ms metallic comb resonator.
            let delay = ((rate as f32) * 0.011).round().max(1.0) as usize;
            let mut out = vec![0.0f32; samples.len()];
            for i in 0..frames {
                let carrier = (i as f32 / rate as f32 * 85.0 * std::f32::consts::TAU).sin();
                for c in 0..ch {
                    let x = samples[i * ch + c];
                    let ringed = x * (0.45 + 0.55 * carrier);
                    let fb = if i >= delay {
                        out[(i - delay) * ch + c] * 0.52
                    } else {
                        0.0
                    };
                    let y = (ringed * 0.72 + fb).clamp(-1.0, 1.0);
                    out[i * ch + c] = y;
                }
            }
            out
        }
        _ => return wav,
    };
    write_wav(rate, channels, &out).unwrap_or(wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_resamples_and_keeps_valid_wav() {
        let wav = crate::sound::sine_wav(24_000, 1, 440.0, 2400);
        let higher = apply_pitch(wav.clone(), 1.25);
        let (rate, ch, samples) = parse_wav(&higher).unwrap();
        assert_eq!((rate, ch), (24_000, 1));
        assert_eq!(samples.len(), 1920);
        assert_eq!(apply_pitch(wav.clone(), 1.0), wav);
    }

    #[test]
    fn pitch_holds_the_last_frame_without_extrapolating() {
        let wav = write_wav(24_000, 2, &[0.0, 0.5, 0.5, 0.0]).unwrap();
        let shifted = apply_pitch(wav, 0.5);
        let (_, _, samples) = parse_wav(&shifted).unwrap();
        assert_eq!(samples, [0.0, 0.5, 0.25, 0.25, 0.5, 0.0, 0.5, 0.0]);
    }

    #[test]
    fn effects_leave_truncated_or_incomplete_audio_unchanged() {
        let mut wav = write_wav(24_000, 2, &[0.0, 0.5]).unwrap();
        wav.pop();
        assert_eq!(apply_effect(wav.clone(), "echo"), wav);
        wav[40..44].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(apply_effect(wav.clone(), "robot"), wav);
    }

    #[test]
    fn effects_produce_valid_audio_for_echo_radio_and_robot() {
        let wav = crate::sound::sine_wav(24_000, 1, 440.0, 4800);
        let echo = apply_effect(wav.clone(), "echo");
        let (_, _, es) = parse_wav(&echo).unwrap();
        assert!(es.len() > 4800, "echo appends tail");

        for fx in ["radio", "robot"] {
            let out = apply_effect(wav.clone(), fx);
            let (rate, ch, samples) = parse_wav(&out).unwrap();
            assert_eq!((rate, ch, samples.len()), (24_000, 1, 4800));
            assert!(samples.iter().any(|s| s.abs() > 0.01), "{fx} stays audible");
            assert_ne!(out, wav, "{fx} transforms the audio");
        }
        assert_eq!(apply_effect(wav.clone(), ""), wav);
    }
}
