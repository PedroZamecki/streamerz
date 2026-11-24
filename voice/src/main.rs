//! Streamerz-Voice: the neural voices of Streamerz (Piper, Kokoro and Pocket TTS, through sherpa-onnx).
//!
//! Streamerz starts it when a neural voice is first used and talks to it over stdin and stdout: one JSON
//! request a line, one JSON answer a line, and when the answer has `bytes`, that many bytes (a WAV) right
//! after it. It keeps one model loaded and quits when stdin closes (Streamerz closed it or went away).
//!
//! Requests (`op`):
//! - `hello`: `{version}`.
//! - `unpack {archive, into}`: a `.tar.bz2` into the folder `into` (its top folder dropped), whole or not
//!   at all.
//! - `load {kind, dir, lang?, mix?, threads?}`: the model, when it is not the one loaded.
//! - `speak {kind, dir, text, speaker?, speed?, lang?, mix?, reference?, threads?}`: loads, then answers
//!   `{rate, ms, bytes}` and the WAV (16-bit, mono).
//! - `unload`: drops the loaded model.
//! - `state`: `{loaded, memory}` (bytes in use by this program).
//!
//! `kind` is `piper`, `kokoro` or `pocket`. `mix` (Kokoro) is a list of `[speaker, share]` pairs whose
//! weighted average style vector is placed in slot 0 of a sidecar `voices-mixed.bin` (matching the ONNX
//! model's exact speaker count). `reference` (Pocket TTS) is the WAV of the voice to speak with.

use serde_json::{json, Value};
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig};
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};

/// Floats in one Kokoro voice (a style vector for each length up to 510, 256 wide).
const KOKORO_VOICE: usize = 510 * 256;

struct Loaded {
    key: String,
    tts: OfflineTts,
    speakers: i32,
    mixed: bool,
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut loaded: Option<Loaded> = None;
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let (answer, bytes) = match serde_json::from_str::<Value>(&line) {
            Ok(req) => match handle(&req, &mut loaded) {
                Ok(done) => done,
                Err(e) => (json!({"ok": false, "error": e}), Vec::new()),
            },
            Err(e) => (json!({"ok": false, "error": format!("not JSON: {e}")}), Vec::new()),
        };
        let mut answer = answer;
        answer["ok"] = answer.get("ok").cloned().unwrap_or(Value::Bool(true));
        if !bytes.is_empty() {
            answer["bytes"] = json!(bytes.len());
        }
        let sent = writeln!(out, "{answer}")
            .and_then(|_| out.write_all(&bytes))
            .and_then(|_| out.flush());
        if sent.is_err() {
            break;
        }
    }
}

fn handle(req: &Value, loaded: &mut Option<Loaded>) -> Result<(Value, Vec<u8>), String> {
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    match text("op") {
        "hello" => Ok((json!({"version": env!("CARGO_PKG_VERSION")}), Vec::new())),
        "unpack" => {
            unpack(Path::new(text("archive")), Path::new(text("into")))?;
            Ok((json!({}), Vec::new()))
        }
        "load" => {
            let l = load(req, loaded)?;
            Ok((json!({"rate": l.tts.sample_rate(), "speakers": l.speakers}), Vec::new()))
        }
        "speak" => {
            let started = std::time::Instant::now();
            let l = load(req, loaded)?;
            let (rate, wav) = speak(req, l)?;
            Ok((json!({"rate": rate, "ms": started.elapsed().as_millis() as u64}), wav))
        }
        "unload" => {
            *loaded = None;
            Ok((json!({}), Vec::new()))
        }
        "state" => Ok((
            json!({"loaded": loaded.as_ref().map(|l| l.key.clone()), "memory": memory()}),
            Vec::new(),
        )),
        other => Err(format!("unknown op {other:?}")),
    }
}

/// The model `req` names, loading it (and dropping the one before) when it is another.
fn load<'a>(req: &Value, loaded: &'a mut Option<Loaded>) -> Result<&'a Loaded, String> {
    let mix = req.get("mix").and_then(Value::as_array).filter(|m| !m.is_empty());
    let key = format!(
        "{}|{}|{}|{}",
        req["kind"].as_str().unwrap_or(""),
        req["dir"].as_str().unwrap_or(""),
        req["lang"].as_str().unwrap_or(""),
        mix.map(|m| Value::Array(m.clone()).to_string()).unwrap_or_default()
    );
    if loaded.as_ref().is_some_and(|l| l.key == key) {
        return Ok(loaded.as_ref().unwrap());
    }
    // The old model goes first: two never sit in memory together.
    *loaded = None;
    let dir = PathBuf::from(req["dir"].as_str().ok_or("no dir")?);
    let threads = req["threads"]
        .as_i64()
        .map(|n| n as i32)
        .unwrap_or_else(default_threads);
    let mut model = OfflineTtsModelConfig {
        num_threads: threads,
        ..Default::default()
    };
    let mut speakers = 0;
    let mut mixed = false;
    match req["kind"].as_str() {
        Some("piper") => {
            model.vits.model = Some(path(&find(&dir, "", ".onnx")?));
            model.vits.tokens = Some(path(&dir.join("tokens.txt")));
            model.vits.data_dir = data_dir(&dir);
            model.vits.noise_scale = 0.667;
            model.vits.noise_scale_w = 0.8;
            model.vits.length_scale = 1.0;
        }
        Some("kokoro") => {
            let voices = dir.join("voices.bin");
            let (file, count) = kokoro_voices(&voices, mix)?;
            speakers = count;
            mixed = mix.is_some();
            let lang = req["lang"].as_str().filter(|s| !s.is_empty());
            model.kokoro.model = Some(path(&find(&dir, "model", ".onnx")?));
            model.kokoro.voices = Some(path(&file));
            model.kokoro.tokens = Some(path(&dir.join("tokens.txt")));
            model.kokoro.data_dir = data_dir(&dir);
            model.kokoro.length_scale = 1.0;
            let en_lex = if lang.is_some_and(|l| l.eq_ignore_ascii_case("en-gb")) {
                "lexicon-gb-en.txt"
            } else {
                "lexicon-us-en.txt"
            };
            let lexicons: Vec<String> = [en_lex, "lexicon-zh.txt"]
                .iter()
                .map(|f| dir.join(f))
                .filter(|p| p.is_file())
                .map(|p| path(&p))
                .collect();
            if !lexicons.is_empty() {
                model.kokoro.lexicon = Some(lexicons.join(","));
            }
            if dir.join("dict").is_dir() {
                model.kokoro.dict_dir = Some(path(&dir.join("dict")));
            }
            model.kokoro.lang = lang.map(str::to_string);
        }
        Some("pocket") => {
            model.pocket.lm_flow = Some(path(&find(&dir, "lm_flow", ".onnx")?));
            model.pocket.lm_main = Some(path(&find(&dir, "lm_main", ".onnx")?));
            model.pocket.encoder = Some(path(&find(&dir, "encoder", ".onnx")?));
            model.pocket.decoder = Some(path(&find(&dir, "decoder", ".onnx")?));
            model.pocket.text_conditioner = Some(path(&find(&dir, "text_conditioner", ".onnx")?));
            model.pocket.vocab_json = Some(path(&dir.join("vocab.json")));
            model.pocket.token_scores_json = Some(path(&dir.join("token_scores.json")));
            model.pocket.voice_embedding_cache_capacity = 8;
        }
        other => return Err(format!("unknown kind {other:?}")),
    }
    let config = OfflineTtsConfig {
        model,
        max_num_sentences: 1,
        ..Default::default()
    };
    let tts = OfflineTts::create(&config).ok_or_else(|| format!("{}: the voice didn't load", dir.display()))?;
    if speakers == 0 {
        speakers = tts.num_speakers();
    }
    Ok(loaded.insert(Loaded {
        key,
        tts,
        speakers,
        mixed,
    }))
}

fn speak(req: &Value, l: &Loaded) -> Result<(i32, Vec<u8>), String> {
    let text = req["text"].as_str().unwrap_or("").trim();
    if text.is_empty() {
        return Err("nothing to say".into());
    }
    let sid = if l.mixed {
        0
    } else {
        req["speaker"]
            .as_i64()
            .unwrap_or(0)
            .clamp(0, (l.speakers - 1).max(0) as i64) as i32
    };
    let mut config = GenerationConfig {
        sid,
        speed: req["speed"].as_f64().filter(|s| *s > 0.0).unwrap_or(1.0) as f32,
        ..Default::default()
    };
    if let Some(reference) = req["reference"].as_str().filter(|s| !s.is_empty()) {
        let wave = sherpa_onnx::Wave::read(reference).ok_or_else(|| format!("{reference}: not a WAV"))?;
        config.reference_audio = Some(wave.samples().to_vec());
        config.reference_sample_rate = wave.sample_rate();
    }
    let audio = l
        .tts
        .generate_with_config::<fn(&[f32], f32) -> bool>(text, &config, None)
        .ok_or("the voice said nothing")?;
    Ok((audio.sample_rate(), wav(audio.samples(), audio.sample_rate())))
}

/// Kokoro's voices file. When `mix` is given, writes a same-sized `voices-mixed.bin` beside it with
/// slot 0 replaced by the weighted average of the selected speakers' style vectors (sherpa-onnx checks
/// that the table has the exact speaker count from the ONNX model's metadata).
fn kokoro_voices(voices: &Path, mix: Option<&Vec<Value>>) -> Result<(PathBuf, i32), String> {
    let len = std::fs::metadata(voices)
        .map_err(|e| format!("{}: {e}", voices.display()))?
        .len() as usize;
    let count = (len / (KOKORO_VOICE * 4)) as i32;
    let Some(mix) = mix.filter(|m| !m.is_empty()) else {
        return Ok((voices.to_path_buf(), count));
    };
    let mut bytes = std::fs::read(voices).map_err(|e| format!("{}: {e}", voices.display()))?;
    let parts: Vec<(usize, f32)> = mix
        .iter()
        .filter_map(|p| Some((p.get(0)?.as_u64()? as usize, p.get(1)?.as_f64()? as f32)))
        .filter(|(sid, share)| *sid < count as usize && share.is_finite() && *share > 0.0)
        .collect();
    mix_slot_zero(&mut bytes, &parts)?;
    let file = voices.with_file_name("voices-mixed.bin");
    std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok((file, count))
}

/// Replaces speaker 0 in a raw Kokoro `voices.bin` buffer with the weighted average of `parts`.
fn mix_slot_zero(bytes: &mut [u8], parts: &[(usize, f32)]) -> Result<(), String> {
    let stride = KOKORO_VOICE * 4;
    if bytes.len() < stride {
        return Err("voices.bin is too small".into());
    }
    let total: f32 = parts.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        return Err("a mix needs a voice with a share".into());
    }
    let mut mixed = vec![0u8; stride];
    for i in 0..KOKORO_VOICE {
        let mut sum = 0.0f32;
        for &(sid, share) in parts {
            let at = sid * stride + i * 4;
            let val = f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            sum += val * share / total;
        }
        mixed[i * 4..i * 4 + 4].copy_from_slice(&sum.to_le_bytes());
    }
    bytes[..stride].copy_from_slice(&mixed);
    Ok(())
}

/// A 16-bit mono WAV of `samples`.
fn wav(samples: &[f32], rate: i32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(rate as u32).to_le_bytes());
    out.extend_from_slice(&(rate as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

fn beside(path: &Path, ext: &str) -> PathBuf {
    let mut n = path.as_os_str().to_owned();
    n.push(".");
    n.push(ext);
    PathBuf::from(n)
}

/// `archive` (`.tar.bz2`) unpacked into `into`, without its top folder: first into `<into>.part`, then
/// renamed, so a cut leaves nothing half there. Only files and folders, and none outside `into`.
fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let part = beside(into, "part");
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let file = std::fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(file)));
    let entries = tar.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry.path().map_err(|e| e.to_string())?.into_owned();
        let mut parts = name.components();
        parts.next();
        let rel: PathBuf = parts.as_path().to_path_buf();
        if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            continue;
        }
        let to = part.join(&rel);
        match entry.header().entry_type() {
            tar::EntryType::Directory => std::fs::create_dir_all(&to).map_err(|e| e.to_string())?,
            tar::EntryType::Regular => {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut f = std::fs::File::create(&to).map_err(|e| format!("{}: {e}", to.display()))?;
                std::io::copy(&mut entry, &mut f).map_err(|e| format!("{}: {e}", to.display()))?;
            }
            _ => {}
        }
    }
    let _ = std::fs::remove_dir_all(into);
    std::fs::rename(&part, into).map_err(|e| format!("{}: {e}", into.display()))
}

/// The file in `dir` named `prefix…suffix`, the int8 one first when there are both.
fn find(dir: &Path, prefix: &str, suffix: &str) -> Result<PathBuf, String> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(prefix) && name.ends_with(suffix)
        })
        .collect();
    found.sort_by_key(|p| !p.to_string_lossy().contains("int8"));
    found
        .into_iter()
        .next()
        .ok_or_else(|| format!("{}: no {prefix}*{suffix}", dir.display()))
}

fn data_dir(dir: &Path) -> Option<String> {
    let d = dir.join("espeak-ng-data");
    d.is_dir().then(|| path(&d))
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Half the cores, from 1 to 4: fast enough to speak sooner than it's heard, and the rest stays free.
fn default_threads() -> i32 {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    (cores / 2).clamp(1, 4) as i32
}

/// The memory this program uses (its resident set), in bytes.
#[cfg(target_os = "linux")]
fn memory() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map(|pages| pages * 4096)
        .unwrap_or(0)
}

#[cfg(windows)]
fn memory() -> u64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_non_paged: usize,
        quota_non_paged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: the counters live on this stack and their size is given.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok != 0 {
        c.working_set as u64
    } else {
        0
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn memory() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_matches_samples_and_rate() {
        let data = wav(&[0.0, 1.0, -1.0, 2.0], 24_000);
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes(data[46..48].try_into().unwrap()), 32767);
        assert_eq!(i16::from_le_bytes(data[48..50].try_into().unwrap()), -32767);
        assert_eq!(i16::from_le_bytes(data[50..52].try_into().unwrap()), 32767);
    }

    #[test]
    fn kokoro_mix_writes_weighted_average_into_slot_zero() {
        let stride = KOKORO_VOICE * 4;
        let mut buf = vec![0u8; stride * 2];
        for i in 0..KOKORO_VOICE {
            buf[i * 4..i * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
            buf[stride + i * 4..stride + i * 4 + 4].copy_from_slice(&5.0f32.to_le_bytes());
        }
        mix_slot_zero(&mut buf, &[(0, 0.75), (1, 0.25)]).unwrap();
        let first = f32::from_le_bytes(buf[0..4].try_into().unwrap());
        assert!((first - 2.0).abs() < 1e-5, "expected 2.0, got {first}");
        let unchanged = f32::from_le_bytes(buf[stride..stride + 4].try_into().unwrap());
        assert!((unchanged - 5.0).abs() < 1e-5);
    }

    #[test]
    fn beside_keeps_dots_in_folder_name() {
        assert_eq!(
            beside(Path::new("/tmp/kokoro-v1.0"), "part"),
            PathBuf::from("/tmp/kokoro-v1.0.part")
        );
    }
}
