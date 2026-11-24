//! System voices: text to speech with what the system has, nothing to download. On Linux espeak-ng
//! (`espeak-ng --stdout`, on the host in a Flatpak), on Windows the system's speech synthesis (WinRT).
//! Either gives a WAV, which Streamerz's own player plays, so a voice goes to any output.
//!
//! Nothing runs until something speaks: espeak-ng is started for each speech and ends with it.

use serde::{Deserialize, Serialize};

/// A voice of the system.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Voice {
    /// What a sub-action keeps: espeak-ng's voice name, Windows's voice name.
    pub id: String,
    /// What is shown.
    pub name: String,
    /// The language, as the system says it (`pt-br`, `en-US`).
    pub language: String,
    /// `male`, `female` or empty.
    pub gender: String,
}

/// How the voices speak by default (`voices` in the settings file).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// The voice Speak uses when it names none (empty: the system's).
    pub voice: String,
    /// 0.5 to 3, 1 as the voice speaks.
    pub speed: f64,
    /// 0.5 to 1.5, 1 as the voice speaks.
    pub pitch: f64,
    /// 0 to 100.
    pub volume: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            voice: String::new(),
            speed: 1.0,
            pitch: 1.0,
            volume: 100.0,
        }
    }
}

/// The program that speaks on Linux. Tests put a stand-in of their own.
#[cfg(not(windows))]
fn program() -> String {
    #[cfg(test)]
    if let Some(p) = TEST_PROGRAM.with_borrow(|p| p.clone()) {
        return p;
    }
    "espeak-ng".into()
}

#[cfg(all(test, not(windows)))]
thread_local! {
    pub static TEST_PROGRAM: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[cfg(not(windows))]
fn command(args: &[&str]) -> std::process::Command {
    let mut cmd = std::process::Command::new(program());
    cmd.args(args);
    crate::sandbox::on_host(cmd)
}

/// The system's voices, or why there are none (espeak-ng isn't installed).
pub fn voices() -> Result<Vec<Voice>, String> {
    if OFFLINE.get() {
        return Ok(vec![Voice {
            id: "en".into(),
            name: "English".into(),
            language: "en".into(),
            gender: String::new(),
        }]);
    }
    #[cfg(windows)]
    return windows_voices();
    #[cfg(not(windows))]
    {
        let out = command(&["--voices"]).output().map_err(|_| missing())?;
        if !out.status.success() {
            return Err(missing());
        }
        Ok(parse_espeak_voices(&String::from_utf8_lossy(&out.stdout)))
    }
}

/// What to do when espeak-ng isn't there: the command for this system.
#[cfg(not(windows))]
pub fn missing() -> String {
    crate::i18n::trf("voices.espeak_missing", &[("command", &install_command())])
}

/// The command that installs espeak-ng here (by the distribution's family).
#[cfg(not(windows))]
pub fn install_command() -> String {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let family = release
        .lines()
        .filter(|l| l.starts_with("ID=") || l.starts_with("ID_LIKE="))
        .map(|l| l.split_once('=').map_or("", |(_, v)| v).trim_matches('"').to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let has = |name: &str| family.split_whitespace().any(|w| w == name);
    if has("arch") {
        "sudo pacman -S espeak-ng".into()
    } else if has("fedora") || has("rhel") {
        "sudo dnf install espeak-ng".into()
    } else if has("opensuse") || has("suse") {
        "sudo zypper install espeak-ng".into()
    } else {
        "sudo apt install espeak-ng".into()
    }
}

/// `espeak-ng --voices`: `Pty Language Age/Gender VoiceName File Other Languages`.
#[cfg(any(not(windows), test))]
fn parse_espeak_voices(text: &str) -> Vec<Voice> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            let (language, gender, name, file) = (cols.get(1)?, cols.get(2)?, cols.get(3)?, cols.get(4)?);
            let gender = match gender.rsplit('/').next() {
                Some("M") => "male",
                Some("F") => "female",
                _ => "",
            };
            Some(Voice {
                // The file names the voice uniquely (`gmw/en-US`); its last part is what -v takes.
                id: file.rsplit('/').next().unwrap_or(file).to_string(),
                name: name.replace('_', " "),
                language: language.to_string(),
                gender: gender.into(),
            })
        })
        .collect()
}

thread_local! {
    /// Offline scenarios speak with a beep of a tenth of a second a word instead of a voice (on the
    /// scenario's thread, where its engine runs).
    static OFFLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn offline() {
    OFFLINE.set(true);
}

/// Speech as a WAV: `speed` and `pitch` around 1, as the voice speaks.
pub async fn speak(text: &str, voice: &str, speed: f64, pitch: f64) -> Result<Vec<u8>, String> {
    if OFFLINE.get() {
        let words = text.split_whitespace().count().max(1) as u32;
        let _ = (voice, speed, pitch);
        return Ok(crate::sound::sine_wav(16_000, 1, 440.0, 1_600 * words));
    }
    let speed = if speed.is_finite() { speed.clamp(0.5, 3.0) } else { 1.0 };
    let pitch = if pitch.is_finite() { pitch.clamp(0.5, 1.5) } else { 1.0 };
    #[cfg(windows)]
    return windows_speak(text, voice, speed, pitch).await;
    #[cfg(not(windows))]
    {
        use tokio::io::AsyncWriteExt;
        // espeak-ng: 175 words a minute and pitch 50 are its own.
        let wpm = ((175.0 * speed).round() as u32).to_string();
        let pitch = (((pitch - 0.5) * 99.0).round() as u32).min(99).to_string();
        let mut args = vec!["--stdout", "-s", &wpm, "-p", &pitch];
        if !voice.trim().is_empty() {
            args.extend(["-v", voice.trim()]);
        }
        // The text goes in on stdin: a message starting with `-` is never read as an option.
        let mut child = tokio::process::Command::from(command(&args))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| missing())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await.map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
        if !out.status.success() || out.stdout.len() < 44 {
            let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if said.is_empty() {
                "espeak-ng gave no sound".into()
            } else {
                said
            });
        }
        let mut wav = out.stdout;
        true_sizes(&mut wav);
        Ok(wav)
    }
}

#[cfg(windows)]
fn windows_voices() -> Result<Vec<Voice>, String> {
    use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceGender};
    let all = SpeechSynthesizer::AllVoices().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for v in all {
        let name = v.DisplayName().map(|s| s.to_string()).unwrap_or_default();
        out.push(Voice {
            id: name.clone(),
            name,
            language: v.Language().map(|s| s.to_string()).unwrap_or_default(),
            gender: match v.Gender() {
                Ok(VoiceGender::Male) => "male",
                Ok(VoiceGender::Female) => "female",
                _ => "",
            }
            .into(),
        });
    }
    Ok(out)
}

#[cfg(windows)]
async fn windows_speak(text: &str, voice: &str, speed: f64, pitch: f64) -> Result<Vec<u8>, String> {
    use windows::core::HSTRING;
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    use windows::Storage::Streams::DataReader;
    let err = |e: windows::core::Error| e.to_string();
    let synth = SpeechSynthesizer::new().map_err(err)?;
    if !voice.trim().is_empty() {
        for v in SpeechSynthesizer::AllVoices().map_err(err)? {
            if v.DisplayName()
                .map(|n| n.to_string().eq_ignore_ascii_case(voice.trim()))
                .unwrap_or(false)
            {
                synth.SetVoice(&v).map_err(err)?;
                break;
            }
        }
    }
    let options = synth.Options().map_err(err)?;
    options.SetSpeakingRate(speed).map_err(err)?;
    options.SetAudioPitch(pitch).map_err(err)?;
    let stream = synth
        .SynthesizeTextToStreamAsync(&HSTRING::from(text))
        .map_err(err)?
        .await
        .map_err(err)?;
    let size = stream.Size().map_err(err)? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(err)?).map_err(err)?;
    reader.LoadAsync(size).map_err(err)?.await.map_err(err)?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).map_err(err)?;
    true_sizes(&mut bytes);
    Ok(bytes)
}

/// A WAV written as it was made says it is endless (espeak-ng writes the header first): the sizes it
/// really has go in, so its length is right.
fn true_sizes(wav: &mut [u8]) {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return;
    }
    let riff = (wav.len() - 8) as u32;
    wav[4..8].copy_from_slice(&riff.to_le_bytes());
    let mut at = 12;
    while at + 8 <= wav.len() {
        let size = u32::from_le_bytes([wav[at + 4], wav[at + 5], wav[at + 6], wav[at + 7]]) as usize;
        if &wav[at..at + 4] == b"data" {
            let real = (wav.len() - at - 8) as u32;
            wav[at + 4..at + 8].copy_from_slice(&real.to_le_bytes());
            return;
        }
        at += 8 + size + (size & 1);
    }
}

/// Speech written to a file of its own, for the player (removed when it has played).
pub fn to_file(wav: &[u8]) -> Result<std::path::PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("streamerz-speech-{}-{n}.wav", std::process::id()));
    std::fs::write(&path, wav).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espeak_voices_are_read_with_their_language_and_gender() {
        let text = "Pty Language       Age/Gender VoiceName          File                 Other Languages\n \
             5  af              --/M      Afrikaans          gmw/af\n \
             5  pt-BR           --/F      Portuguese_(Brazil) roa/pt-BR          (pt 6)\n";
        let voices = parse_espeak_voices(text);
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[1].id, "pt-BR");
        assert_eq!(voices[1].name, "Portuguese (Brazil)");
        assert_eq!(voices[1].language, "pt-BR");
        assert_eq!(voices[1].gender, "female");
        assert_eq!(voices[0].gender, "male");
    }

    #[cfg(not(windows))]
    #[test]
    fn speech_comes_from_espeak_with_the_text_on_stdin() {
        let dir = std::env::temp_dir().join(format!("streamerz-voice-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A stand-in espeak-ng: writes what it was asked into a file and a tiny WAV out.
        let fake = dir.join("espeak-ng");
        let seen = dir.join("seen");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\necho \"$@\" > {0}\ncat >> {0}\nprintf 'RIFF$\\000\\000\\000WAVEfmt \\020\\000\\000\\000\\001\\000\\001\\000\\200\\076\\000\\000\\000\\175\\000\\000\\002\\000\\020\\000data\\000\\000\\000\\000'\n",
                seen.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        TEST_PROGRAM.with_borrow_mut(|p| *p = Some(fake.display().to_string()));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let wav = runtime.block_on(speak("-hi there", "pt-BR", 2.0, 1.5));
        TEST_PROGRAM.with_borrow_mut(|p| *p = None);
        let wav = wav.unwrap();
        assert!(wav.starts_with(b"RIFF") && wav.len() >= 44);
        let seen = std::fs::read_to_string(&seen).unwrap();
        assert!(seen.starts_with("--stdout -s 350 -p 99 -v pt-BR"), "{seen}");
        assert!(seen.ends_with("-hi there"), "the text came on stdin: {seen}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_streamed_wav_gets_its_true_length() {
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(0xffff_ffffu32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend([1, 0, 1, 0, 0x22, 0x56, 0, 0, 0x44, 0xac, 0, 0, 2, 0, 16, 0]);
        wav.extend(b"data");
        wav.extend(0xffff_ffffu32.to_le_bytes());
        wav.extend([0u8; 100]);
        true_sizes(&mut wav);
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 136);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 100);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_missing_espeak_says_how_to_install_it() {
        TEST_PROGRAM.with_borrow_mut(|p| *p = Some("/nowhere/espeak-ng".into()));
        let got = voices();
        TEST_PROGRAM.with_borrow_mut(|p| *p = None);
        let error = got.unwrap_err();
        assert!(error.contains("espeak-ng") && error.contains("sudo"), "{error}");
    }
}
