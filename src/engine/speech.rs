//! Speaking: text to speech played by Streamerz's player (system voices for now). Each speech is a WAV
//! of its own, removed once it has played.
use super::Engine;
use crate::sound;
use std::rc::Rc;

impl Engine {
    /// Says `text` now with `voice` (empty: the default), on `device` ("" Default, "Monitor").
    /// Runs on its own; a failure goes to the log.
    pub fn say_now(self: &Rc<Self>, text: &str, voice: &str, speed: f64, pitch: f64, volume: f64, device: &str) {
        let (text, device) = (text.to_string(), device.to_string());
        let voice = if voice.is_empty() {
            self.config().voices.voice.clone()
        } else {
            voice.to_string()
        };
        tokio::task::spawn_local(async move {
            if let Err(e) = say(&text, &voice, speed, pitch, volume, &device).await {
                crate::warn(format!("Speak: {e}"));
            }
        });
    }
}

/// Speaks and waits for the end; the speech's length in ms when it played.
pub async fn say(
    text: &str,
    voice: &str,
    speed: f64,
    pitch: f64,
    volume: f64,
    device: &str,
) -> Result<Option<u64>, String> {
    let wav = crate::voice::speak(text, voice, speed, pitch).await?;
    let file = crate::voice::to_file(&wav)?;
    let played = sound::play(&file, "speech", device, sound::volume(volume));
    let result = match played {
        Ok(started) => {
            let _ = started.ended.await;
            Ok(started.duration_ms)
        }
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&file);
    result
}
