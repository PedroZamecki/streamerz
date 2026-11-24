//! Core > Sounds: Play Sound, Play Sound From Folder and the two stops (Streamer.bot's), with
//! Streamerz's own volume, pause, resume, master volume and sound info, and the Sound Started and
//! Sound Ended triggers. The playing itself is `crate::sound`; nothing is open until a sound plays.

use super::{b, n, s, Ctx, Engine};
use crate::model::Obj;
use crate::sound::{self, resolve};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub const T_SOUND_STARTED: u32 = 99300;
pub const T_SOUND_ENDED: u32 = 99301;
pub const T_SOUND_DEVICE_CHANGED: u32 = 99302;

/// The sub-actions this module runs.
pub fn handles(kind: u64) -> bool {
    matches!(kind, 1 | 7 | 30001 | 30002 | 99300..=99306)
}

thread_local! {
    /// The overlays sounds were played on (Play Sound In Overlay), for the stops to reach them.
    static OVERLAY_SOUNDS: RefCell<std::collections::HashSet<String>> = RefCell::new(Default::default());
    /// The last sound each folder gave, so the next pick is another.
    static LAST: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// A sound's name for a sub-action: the one given, all of them for "".
fn which(name: &str) -> Option<&str> {
    let name = name.trim();
    (!name.is_empty()).then_some(name)
}

fn number(o: &Obj, key: &str, default: f64) -> f64 {
    if o.contains_key(key) {
        n(o, key)
    } else {
        default
    }
}

fn fade(o: &Obj) -> u64 {
    number(o, "fadeMs", 0.0).clamp(0.0, 60_000.0) as u64
}

impl Engine {
    pub(super) async fn sound_sub(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        match kind {
            1 => {
                let file = self.expand(ctx, s(o, "soundFile"));
                if file.trim().is_empty() {
                    crate::warn("Play Sound: no sound file".to_string());
                    return;
                }
                self.play_sound(ctx, o, resolve(&file)).await;
            }
            7 => {
                let folder = resolve(&self.expand(ctx, s(o, "folder")));
                let key = folder.to_string_lossy().into_owned();
                let avoid = if b(o, "avoidRepeats", true) {
                    LAST.with_borrow(|l| l.get(&key).cloned())
                } else {
                    None
                };
                let picked = match sound::pick_from_folder(&folder, b(o, "recursive", false), avoid.as_deref()) {
                    Ok(p) => p,
                    Err(e) => {
                        crate::warn(format!("Play Sound From Folder: {e}"));
                        return;
                    }
                };
                let full = picked.to_string_lossy().into_owned();
                LAST.with_borrow_mut(|l| {
                    if l.len() > 64 {
                        l.clear();
                    }
                    l.insert(key, full.clone());
                });
                let file_name = picked.file_name().map(|f| f.to_string_lossy().into_owned());
                ctx.args.insert("randomSoundFile".into(), full.into());
                ctx.args
                    .insert("randomSoundFileName".into(), file_name.unwrap_or_default().into());
                self.play_sound(ctx, o, picked).await;
            }
            30001 => {
                sound::stop(None, 0);
                self.overlay_sounds_stop(None);
            }
            30002 => {
                let name = self.expand(ctx, s(o, "soundName"));
                if let Some(name) = which(&name) {
                    sound::stop(Some(name), 0);
                    self.overlay_sounds_stop(Some(name));
                }
            }
            // Play Sound In Overlay: the overlay's open pages play it (the browser source's audio, so
            // OBS mixes it with that source).
            99305 => {
                let overlay = self.expand(ctx, s(o, "overlay")).trim().to_string();
                let file = self.expand(ctx, s(o, "soundFile")).trim().to_string();
                let name = self.expand(ctx, s(o, "soundName")).trim().to_string();
                let volume = sound::volume(number(o, "volume", 100.0));
                match self.overlay_sound(&overlay, &file, &name, volume) {
                    Ok(pages) => {
                        ctx.args.insert("overlayPages".into(), pages.into());
                    }
                    Err(e) => crate::warn(format!("Play Sound In Overlay: {e}")),
                }
            }
            // Set Sound Device: what Default means for Streamerz from now on (kept in the Output settings).
            99306 => {
                let device = self.expand(ctx, s(o, "device")).trim().to_string();
                let device = if device.eq_ignore_ascii_case("Default") {
                    String::new()
                } else {
                    device
                };
                self.change(|c| c.sounds.device = device.clone());
                sound::configure(&self.config().sounds);
            }
            99300 => {
                let name = self.expand(ctx, s(o, "soundName"));
                let volume = sound::volume(number(o, "volume", 100.0));
                sound::set_volume(which(&name), volume, fade(o));
            }
            99301 | 99302 => {
                let name = self.expand(ctx, s(o, "soundName"));
                sound::pause(which(&name), kind == 99301);
            }
            99303 => {
                let v = sound::volume(number(o, "volume", 100.0));
                sound::set_master(v, fade(o));
                // Kept, as the Output settings keep it.
                self.change(|c| c.sounds.master = (f64::from(v) * 100.0).round());
            }
            99304 => {
                let file = resolve(&self.expand(ctx, s(o, "soundFile")));
                match sound::info(&file) {
                    Ok(info) => {
                        if let Some(ms) = info.duration_ms {
                            ctx.args
                                .insert("soundLength".into(), crate::vars::number(ms as f64 / 1000.0));
                            ctx.args.insert("soundLengthMs".into(), ms.into());
                        }
                        ctx.args.insert("soundFormat".into(), info.format.into());
                        ctx.args.insert("soundChannels".into(), info.channels.into());
                        ctx.args.insert("soundRate".into(), info.rate.into());
                    }
                    Err(e) => crate::warn(format!("Get Sound Info: {e}")),
                }
            }
            _ => {}
        }
    }

    /// Whether an action listens for Sound Device Changed (then, and only then, outputs are followed).
    pub fn wants_device_events(&self) -> bool {
        self.config().actions.actions.iter().any(|a| {
            a.enabled
                && a.triggers.iter().any(|t| {
                    t.get("type").and_then(serde_json::Value::as_u64) == Some(T_SOUND_DEVICE_CHANGED as u64)
                        && b(t, "enabled", true)
                })
        })
    }

    /// Sound Device Changed for each output that came or went since `known`, which becomes the list now.
    pub fn sound_devices_changed(self: &Rc<Self>, known: &mut Vec<(String, String)>) {
        let now = sound::outputs();
        crate::catalog::set("sound.devices", now.clone());
        let came = now
            .iter()
            .filter(|d| !known.iter().any(|k| k.0 == d.0))
            .map(|d| (d, true));
        let went = known
            .iter()
            .filter(|k| !now.iter().any(|d| d.0 == k.0))
            .map(|d| (d, false));
        let changes: Vec<((String, String), bool)> = came.chain(went).map(|(d, on)| (d.clone(), on)).collect();
        for ((device, shown), available) in changes {
            let mut args = Obj::new();
            args.insert("device".into(), device.clone().into());
            args.insert("deviceName".into(), shown.into());
            args.insert("available".into(), available.into());
            self.fire(
                T_SOUND_DEVICE_CHANGED,
                "Sound Device Changed",
                "Core/Sounds",
                "sounds",
                &args,
                |t| {
                    let want = s(t, "device").trim();
                    want.is_empty() || want.eq_ignore_ascii_case(&device)
                },
            );
        }
        *known = now;
    }

    /// Plays a sound on the open pages of an overlay: one of its files, or a web address. How many
    /// pages listen.
    pub fn overlay_sound(&self, overlay: &str, file: &str, name: &str, volume: f32) -> Result<usize, String> {
        let config = self.config();
        if !crate::groups::exists(&config, overlay) {
            return Err(format!("there is no overlay {overlay}"));
        }
        let url = if file.starts_with("https://") || file.starts_with("http://") {
            file.to_string()
        } else if crate::groups::meta(&config, overlay)
            .files
            .iter()
            .any(|f| f.name == file)
        {
            // Served by Streamerz: the page loads it from where it is served.
            crate::overlays::url(&config, overlay, file)
        } else {
            return Err(format!("{file} is not one of {overlay}'s files nor a web address"));
        };
        drop(config);
        let pages = crate::overlays::open_pages(overlay);
        OVERLAY_SOUNDS.with_borrow_mut(|g| g.insert(overlay.to_string()));
        let data = serde_json::json!({"url": url, "volume": volume, "name": name});
        self.emit("Sound", overlay, || data.clone());
        Ok(pages)
    }

    /// Stops the sounds overlays play (of a name, or all).
    fn overlay_sounds_stop(&self, name: Option<&str>) {
        let groups: Vec<String> = OVERLAY_SOUNDS.with_borrow(|g| g.iter().cloned().collect());
        for group in groups {
            self.emit("SoundStop", &group, || serde_json::json!({"name": name.unwrap_or("")}));
        }
    }

    /// Play one file as a Play Sound sub-action asks (its device, volume, name and whether to wait).
    async fn play_sound(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, file: PathBuf) {
        let name = if b(o, "useFileNameAsName", false) {
            file.file_stem()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            self.expand(ctx, s(o, "soundName")).trim().to_string()
        };
        let device = self.expand(ctx, s(o, "device"));
        let volume = sound::volume(number(o, "volume", 100.0));
        let started = match sound::play(&file, &name, &device, volume) {
            Ok(started) => started,
            Err(e) => {
                crate::warn(format!("Play Sound: {e}"));
                return;
            }
        };
        if let Some(ms) = started.duration_ms {
            ctx.args
                .insert("soundLength".into(), crate::vars::number(ms as f64 / 1000.0));
            ctx.args.insert("soundLengthMs".into(), ms.into());
        }
        // An action run by a sound's own trigger doesn't fire them again (no endless loop).
        let quiet = matches!(
            ctx.args.get("__source").and_then(|v| v.as_str()),
            Some("SoundStarted" | "SoundEnded")
        );
        let mut args = Obj::new();
        args.insert("soundName".into(), name.into());
        args.insert("soundFile".into(), file.to_string_lossy().into_owned().into());
        args.insert("soundDevice".into(), sound::device_key(&device).into());
        if let Some(ms) = started.duration_ms {
            args.insert("soundLengthMs".into(), ms.into());
        }
        if let Some(action) = ctx.args.get("actionName") {
            args.insert("actionName".into(), action.clone());
        }
        if !quiet {
            self.fire(T_SOUND_STARTED, "Sound Started", "Core/Sounds", "sounds", &args, |_| {
                true
            });
        }
        let me = self.clone();
        let ended = async move {
            let ended = started.ended.await.unwrap_or(sound::Ended {
                why: sound::End::Failed,
                error: Some("the sound was lost".into()),
            });
            if let Some(error) = &ended.error {
                crate::warn(format!("Play Sound: {error}"));
            }
            if !quiet {
                args.insert("reason".into(), ended.why.word().into());
                me.fire(T_SOUND_ENDED, "Sound Ended", "Core/Sounds", "sounds", &args, |_| true);
            }
        };
        if b(o, "finishBeforeContinuing", true) {
            ended.await;
        } else {
            tokio::task::spawn_local(ended);
        }
    }
}

/// Follows the sound outputs while an action listens for Sound Device Changed (started and stopped
/// by `servers::run` as the actions change).
pub async fn follow_devices(e: Rc<Engine>) {
    let mut known = sound::outputs();
    let mut events = match sound::DeviceEvents::start() {
        Ok(events) => events,
        Err(error) => {
            crate::warn(format!("Sound Device Changed: {error}"));
            return;
        }
    };
    while events.next().await {
        e.sound_devices_changed(&mut known);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{action, engine, log, run};
    use serde_json::json;

    #[test]
    fn play_sound_waits_for_its_end_and_fires_its_triggers() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _turn = crate::sound::TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("streamerz-engine-sound-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("more")).unwrap();
        let file = dir.join("ding.wav");
        std::fs::write(&file, crate::sound::sine_wav(48_000, 1, 880.0, 4_800)).unwrap();
        std::fs::write(
            dir.join("more/dong.wav"),
            crate::sound::sine_wav(48_000, 1, 440.0, 2_400),
        )
        .unwrap();
        let path = file.to_string_lossy().into_owned();
        let folder = dir.to_string_lossy().into_owned();
        let out = run(async move {
            let mut on_start = action("s", "On start", json!([log("T started %soundName% %soundLengthMs%")]));
            on_start["triggers"] = json!([{"id": "t1", "type": 99300, "enabled": true}]);
            let mut on_end = action(
                "e",
                "On end",
                json!([
                    log("T ended %soundName% %reason%"),
                    // Played by a sound trigger's action: no trigger again.
                    {"type": 1, "soundFile": path, "soundName": "echo", "finishBeforeContinuing": true}
                ]),
            );
            on_end["triggers"] = json!([{"id": "t2", "type": 99301, "enabled": true}]);
            let e = engine(json!({"actions": [
                action("p", "Play", json!([
                    {"type": 1, "soundFile": path, "soundName": "ding", "volume": 80, "finishBeforeContinuing": true},
                    log("T after %soundLength%"),
                    {"type": 7, "folder": folder, "recursive": true, "finishBeforeContinuing": false,
                     "useFileNameAsName": true},
                    log("T picked %randomSoundFileName%"),
                    {"type": 99304, "soundFile": path},
                    log("T info %soundLengthMs% %soundChannels% %soundRate% %soundFormat%"),
                ])),
                on_start,
                on_end,
            ]}));
            e.queue("p", Obj::new());
            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        });
        let has = |line: &str| out.iter().any(|l| l == line);
        assert!(has("T started ding 100"), "{out:?}");
        assert!(has("T after 0.1"), "{out:?}");
        assert!(has("T ended ding finished"), "{out:?}");
        assert!(has("T info 100 1 48000 wav"), "{out:?}");
        assert!(
            out.iter().any(|l| l == "T picked ding.wav" || l == "T picked dong.wav"),
            "{out:?}"
        );
        // started before after, after before the end of the next sound
        let at = |p: &str| out.iter().position(|l| l.starts_with(p)).unwrap();
        assert!(at("T started ding") < at("T after"));
        assert!(!out.iter().any(|l| l.contains("echo")), "{out:?}");
    }

    #[test]
    fn paths_and_names_read_as_written() {
        assert!(resolve("/abs/a.mp3").is_absolute());
        assert_eq!(resolve("sounds/a.mp3").file_name().unwrap(), "a.mp3");
        assert!(resolve("sounds/a.mp3").is_absolute() || crate::config_path().parent().is_none());
        assert_eq!(which("  "), None);
        assert_eq!(which(" Music "), Some("Music"));
        let mut o = Obj::new();
        assert_eq!(fade(&o), 0);
        o.insert("fadeMs".into(), "250".into());
        assert_eq!(fade(&o), 250);
        o.insert("fadeMs".into(), (-5).into());
        assert_eq!(fade(&o), 0);
        assert!(handles(1) && handles(7) && handles(30002) && handles(99304) && !handles(99307));
    }
}
