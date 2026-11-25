//! OBS Studio tools beyond Streamer.bot's: moving and sizing a source, volume with a fade, the monitor,
//! Studio Mode and its transitions, the virtual camera, splitting a recording, a hotkey by name,
//! profiles and scene collections, the last replay's file, a filter's or an input's settings, and a
//! source shown for a while. Streamerz-only codes 99750-99779.

use super::{b, n, new_state, s, Ctx, Engine};
use crate::model::Obj;
use serde_json::{json, Map, Value};
use std::rc::Rc;
use std::time::Duration;

/// A typed value for OBS's settings: `true`, `12`, `1.5` or the text as it is.
pub fn typed(text: &str) -> Value {
    let t = text.trim();
    if let Ok(v) = t.parse::<bool>() {
        return v.into();
    }
    if let Ok(v) = t.parse::<i64>() {
        return v.into();
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() => json!(v),
        _ => text.into(),
    }
}

/// How loud a level is as OBS's multiplier: `percent` of normal (100 is 1.0), or decibels.
pub fn volume_mul(level: f64, decibels: bool) -> f64 {
    if decibels {
        10f64.powf(level / 20.0)
    } else {
        (level / 100.0).max(0.0)
    }
}

impl Engine {
    /// One OBS request on the connection the sub-action names (empty: any), as an error text on failure.
    async fn obs_do(self: &Rc<Self>, o: &Obj, ctx: &Ctx, request: &str, data: Value) -> Result<Value, String> {
        let connection = s(o, "connectionId");
        let connection = (!connection.is_empty()).then_some(connection);
        let _ = ctx;
        self.obs_start()
            .send_request(connection, request, data)
            .await
            .map_err(|e| format!("{request}: {e}"))
    }

    /// The scene item of a source in a scene.
    async fn obs_item(self: &Rc<Self>, ctx: &Ctx, o: &Obj) -> Result<(String, i64), String> {
        let scene = self.expand(ctx, s(o, "sceneName"));
        let source = self.expand(ctx, s(o, "sourceName"));
        let got = self
            .obs_do(
                o,
                ctx,
                "GetSceneItemId",
                json!({"sceneName": scene, "sourceName": source}),
            )
            .await?;
        let id = got["sceneItemId"]
            .as_i64()
            .ok_or_else(|| format!("{source} isn't in {scene}"))?;
        Ok((scene, id))
    }

    /// A number typed in a field (variables filled in); None when it is empty.
    fn obs_number(&self, ctx: &Ctx, o: &Obj, key: &str, label: &str) -> Result<Option<f64>, String> {
        let text = self.expand(ctx, &crate::vars::text(o.get(key).unwrap_or(&Value::Null)));
        if text.trim().is_empty() {
            return Ok(None);
        }
        crate::fmt::parse_number(&text)
            .filter(|v| v.is_finite())
            .map(Some)
            .ok_or_else(|| format!("{label} isn't a number: {text}"))
    }

    /// The settings a `Name=value` list gives, typed.
    fn obs_settings(&self, ctx: &Ctx, o: &Obj) -> Value {
        let mut out = Map::new();
        if let Some(Value::Object(map)) = o.get("settings") {
            for (k, v) in map {
                let key = self.expand(ctx, k).trim().to_string();
                if key.is_empty() {
                    continue;
                }
                out.insert(key, typed(&self.expand(ctx, &crate::vars::text(v))));
            }
        }
        Value::Object(out)
    }

    /// Runs one OBS tool; the result goes in the arguments.
    pub(super) async fn obs_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        let text = |ctx: &Ctx, key: &str| self.expand(ctx, s(o, key)).trim().to_string();
        match kind {
            // Move, size, rotate or crop a source: what is filled in changes, the rest stays.
            99750 => {
                let (scene, id) = self.obs_item(ctx, o).await?;
                let mut transform = Map::new();
                for (key, put, label) in [
                    ("x", "positionX", "X"),
                    ("y", "positionY", "Y"),
                    ("rotation", "rotation", "Rotation"),
                ] {
                    if let Some(v) = self.obs_number(ctx, o, key, label)? {
                        transform.insert(put.into(), json!(v));
                    }
                }
                if let Some(pct) = self.obs_number(ctx, o, "scale", "Size")? {
                    transform.insert("scaleX".into(), json!(pct / 100.0));
                    transform.insert("scaleY".into(), json!(pct / 100.0));
                }
                for (key, put) in [
                    ("cropLeft", "cropLeft"),
                    ("cropRight", "cropRight"),
                    ("cropTop", "cropTop"),
                    ("cropBottom", "cropBottom"),
                ] {
                    if let Some(v) = self.obs_number(ctx, o, key, key)? {
                        transform.insert(put.into(), json!(v.clamp(0.0, 100_000.0).round() as i64));
                    }
                }
                if transform.is_empty() {
                    return Err("nothing to change: fill in a position, size, rotation or crop".into());
                }
                self.obs_do(
                    o,
                    ctx,
                    "SetSceneItemTransform",
                    json!({"sceneName": scene, "sceneItemId": id, "sceneItemTransform": transform}),
                )
                .await?;
            }
            // Volume of an input, with a fade.
            99751 => {
                let input = text(ctx, "inputName");
                let level = self.obs_number(ctx, o, "level", "Level")?.ok_or("no level given")?;
                let target = volume_mul(level, n(o, "unit") as i64 == 1).clamp(0.0, 20.0);
                let fade = self.duration_number(ctx, o, "fade").clamp(0.0, 3_600_000.0) as u64;
                if fade == 0 {
                    self.obs_do(
                        o,
                        ctx,
                        "SetInputVolume",
                        json!({"inputName": input, "inputVolumeMul": target}),
                    )
                    .await?;
                } else {
                    let now = self
                        .obs_do(o, ctx, "GetInputVolume", json!({"inputName": input}))
                        .await?;
                    let from = now["inputVolumeMul"].as_f64().unwrap_or(0.0).clamp(0.0, 20.0);
                    // Ten steps a second at most, and no more than a hundred.
                    let steps = (fade / 100).clamp(1, 100);
                    for i in 1..=steps {
                        let mul = from + (target - from) * i as f64 / steps as f64;
                        self.obs_do(
                            o,
                            ctx,
                            "SetInputVolume",
                            json!({"inputName": input, "inputVolumeMul": mul.clamp(0.0, 20.0)}),
                        )
                        .await?;
                        if i < steps {
                            tokio::time::sleep(Duration::from_millis(fade / steps)).await;
                        }
                    }
                }
            }
            99752 => {
                let monitor = [
                    "OBS_MONITORING_TYPE_NONE",
                    "OBS_MONITORING_TYPE_MONITOR_ONLY",
                    "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT",
                ][(n(o, "monitor").max(0.0) as usize).min(2)];
                self.obs_do(
                    o,
                    ctx,
                    "SetInputAudioMonitorType",
                    json!({"inputName": text(ctx, "inputName"), "monitorType": monitor}),
                )
                .await?;
            }
            // Studio Mode: on, off or toggle.
            99753 => {
                let state = n(o, "state");
                let cur = if state as i64 == 2 {
                    self.obs_do(o, ctx, "GetStudioModeEnabled", json!({})).await?["studioModeEnabled"]
                        .as_bool()
                        .unwrap_or(false)
                } else {
                    false
                };
                let on = new_state(state, cur);
                self.obs_do(o, ctx, "SetStudioModeEnabled", json!({"studioModeEnabled": on}))
                    .await?;
                ctx.args.insert("obsStudioMode".into(), on.into());
            }
            99754 => {
                self.obs_do(
                    o,
                    ctx,
                    "SetCurrentPreviewScene",
                    json!({"sceneName": text(ctx, "sceneName")}),
                )
                .await?;
            }
            99755 => {
                self.obs_do(o, ctx, "TriggerStudioModeTransition", json!({})).await?;
            }
            // The transition, and how long it takes.
            99756 => {
                let name = text(ctx, "transitionName");
                if !name.is_empty() {
                    self.obs_do(o, ctx, "SetCurrentSceneTransition", json!({"transitionName": name}))
                        .await?;
                }
                if let Some(ms) = self.obs_number(ctx, o, "duration", "Duration")? {
                    self.obs_do(
                        o,
                        ctx,
                        "SetCurrentSceneTransitionDuration",
                        json!({"transitionDuration": ms.clamp(50.0, 20000.0).round() as i64}),
                    )
                    .await?;
                }
            }
            99757 => {
                let state = (n(o, "state").max(0.0) as usize).min(2);
                let request = ["StartVirtualCam", "StopVirtualCam", "ToggleVirtualCam"][state];
                let got = self.obs_do(o, ctx, request, json!({})).await?;
                let on = match state {
                    0 => Some(true),
                    1 => Some(false),
                    _ => got["outputActive"].as_bool(),
                };
                if let Some(on) = on {
                    ctx.args.insert("obsVirtualCamera".into(), on.into());
                }
            }
            99758 => {
                self.obs_do(o, ctx, "SplitRecordFile", json!({})).await?;
            }
            99759 => {
                self.obs_do(
                    o,
                    ctx,
                    "TriggerHotkeyByName",
                    json!({"hotkeyName": text(ctx, "hotkeyName")}),
                )
                .await?;
            }
            99760 => {
                self.obs_do(
                    o,
                    ctx,
                    "SetCurrentProfile",
                    json!({"profileName": text(ctx, "profileName")}),
                )
                .await?;
            }
            99761 => {
                self.obs_do(
                    o,
                    ctx,
                    "SetCurrentSceneCollection",
                    json!({"sceneCollectionName": text(ctx, "collectionName")}),
                )
                .await?;
            }
            // Where the last saved replay went, to send it somewhere.
            99762 => {
                let got = self.obs_do(o, ctx, "GetLastReplayBufferReplay", json!({})).await?;
                ctx.args.insert("obsReplayPath".into(), got["savedReplayPath"].clone());
            }
            99763 => {
                let settings = self.obs_settings(ctx, o);
                self.obs_do(
                    o,
                    ctx,
                    "SetSourceFilterSettings",
                    json!({"sourceName": text(ctx, "sourceName"), "filterName": text(ctx, "filterName"), "filterSettings": settings, "overlay": true}),
                )
                .await?;
            }
            99764 => {
                let settings = self.obs_settings(ctx, o);
                self.obs_do(
                    o,
                    ctx,
                    "SetInputSettings",
                    json!({"inputName": text(ctx, "inputName"), "inputSettings": settings, "overlay": true}),
                )
                .await?;
            }
            // Output: start, stop or toggle one by name (a multistream or vertical canvas plugin's too).
            99766 => {
                let output = text(ctx, "outputName");
                if output.trim().is_empty() {
                    return Err("no output named".into());
                }
                let request = ["StartOutput", "StopOutput", "ToggleOutput"][(n(o, "state").max(0.0) as usize).min(2)];
                let got = self
                    .obs_do(o, ctx, request, json!({"outputName": output.trim()}))
                    .await?;
                let active = match got["outputActive"].as_bool() {
                    Some(on) => on,
                    None => self
                        .obs_do(o, ctx, "GetOutputStatus", json!({"outputName": output.trim()}))
                        .await?["outputActive"]
                        .as_bool()
                        .unwrap_or(false),
                };
                ctx.args.insert("obsOutputActive".into(), active.into());
            }
            // Lock Source: lock, unlock or toggle a source in a scene.
            99767 => {
                let (scene, id) = self.obs_item(ctx, o).await?;
                let item = json!({"sceneName": scene, "sceneItemId": id});
                let now = if n(o, "state") as u64 == 2 {
                    self.obs_do(o, ctx, "GetSceneItemLocked", item.clone()).await?["sceneItemLocked"]
                        .as_bool()
                        .unwrap_or(false)
                } else {
                    false
                };
                let locked = new_state(n(o, "state"), now);
                let mut set = item;
                set["sceneItemLocked"] = locked.into();
                self.obs_do(o, ctx, "SetSceneItemLocked", set).await?;
                ctx.args.insert("obsSourceLocked".into(), locked.into());
            }
            // Audio Sync Offset, in milliseconds (OBS takes -950 to 20000).
            99768 => {
                let offset = self.obs_number(ctx, o, "offset", "Offset")?.ok_or("no offset given")?;
                let offset = offset.round().clamp(-950.0, 20_000.0) as i64;
                self.obs_do(
                    o,
                    ctx,
                    "SetInputAudioSyncOffset",
                    json!({"inputName": text(ctx, "inputName"), "inputAudioSyncOffset": offset}),
                )
                .await?;
            }
            // Audio Balance: 0 all left, 50 the middle, 100 all right.
            99769 => {
                let balance = self.obs_number(ctx, o, "balance", "Balance")?.unwrap_or(50.0);
                let balance = (balance.clamp(0.0, 100.0) / 100.0 * 1000.0).round() / 1000.0;
                self.obs_do(
                    o,
                    ctx,
                    "SetInputAudioBalance",
                    json!({"inputName": text(ctx, "inputName"), "inputAudioBalance": balance}),
                )
                .await?;
            }
            // Shows a source for a while, then hides it: one step instead of show, wait, hide.
            99765 => {
                let (scene, id) = self.obs_item(ctx, o).await?;
                let millis = self.duration_number(ctx, o, "duration").clamp(0.0, 3_600_000.0) as u64;
                let set = |on: bool| json!({"sceneName": scene, "sceneItemId": id, "sceneItemEnabled": on});
                self.obs_do(o, ctx, "SetSceneItemEnabled", set(true)).await?;
                let wait = Duration::from_millis(millis);
                if b(o, "wait", true) {
                    tokio::time::sleep(wait).await;
                    self.obs_do(o, ctx, "SetSceneItemEnabled", set(false)).await?;
                } else {
                    // The action goes on; the source hides itself.
                    let (engine, request) = (self.clone(), (s(o, "connectionId").to_string(), set(false)));
                    tokio::task::spawn_local(async move {
                        tokio::time::sleep(wait).await;
                        let connection = (!request.0.is_empty()).then_some(request.0.as_str());
                        if let Err(e) = engine
                            .obs_start()
                            .send_request(connection, "SetSceneItemEnabled", request.1)
                            .await
                        {
                            crate::error(format!("OBS: {e}"));
                        }
                    });
                }
            }
            _ => self.unsupported(kind),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_typed_and_levels_become_obs_multipliers() {
        assert_eq!(typed("true"), json!(true));
        assert_eq!(typed(" 12 "), json!(12));
        assert_eq!(typed("1.5"), json!(1.5));
        assert_eq!(typed("#ff0000"), json!("#ff0000"));
        assert_eq!(typed("NaN"), json!("NaN"), "not a number OBS takes");
        assert!((volume_mul(100.0, false) - 1.0).abs() < 1e-9);
        assert!((volume_mul(50.0, false) - 0.5).abs() < 1e-9);
        assert!((volume_mul(0.0, true) - 1.0).abs() < 1e-9);
        assert!((volume_mul(-20.0, true) - 0.1).abs() < 1e-9);
        assert_eq!(volume_mul(-5.0, false), 0.0);
    }

    fn obj(v: Value) -> Obj {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn obs_tools_dispatch_requests_and_clamp_values() {
        use crate::engine::tests::{engine, run};
        use crate::ports::fakes::FakeObs;
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let e = engine(json!({}));
        let obs = Rc::new(FakeObs::default());
        obs.answer("GetSceneItemId", json!({"sceneItemId": 7}));
        obs.answer("GetInputVolume", json!({"inputVolumeMul": 0.0}));
        obs.answer("GetStudioModeEnabled", json!({"studioModeEnabled": true}));
        obs.answer("ToggleVirtualCam", json!({"outputActive": true}));
        obs.answer(
            "GetLastReplayBufferReplay",
            json!({"savedReplayPath": "/tmp/replay.mkv"}),
        );
        obs.answer("GetSceneItemLocked", json!({"sceneItemLocked": false}));
        for req in [
            "SetSceneItemTransform",
            "SetInputVolume",
            "SetStudioModeEnabled",
            "StartVirtualCam",
            "StopVirtualCam",
            "SetInputSettings",
            "SetSceneItemLocked",
            "SetInputAudioSyncOffset",
            "SetInputAudioBalance",
        ] {
            obs.answer(req, json!({}));
        }
        e.set_obs(obs.clone());

        run(async move {
            let mut ctx = Ctx {
                args: Obj::new(),
                group: String::new(),
            };
            // 99750: empty transform errors; filled transform rounds crop and scales percent.
            assert!(e
                .obs_tool(&mut ctx, &obj(json!({"sceneName": "S", "sourceName": "Cam"})), 99750)
                .await
                .is_err());
            e.obs_tool(
                &mut ctx,
                &obj(json!({"sceneName": "S", "sourceName": "Cam", "x": "10.5", "scale": "50", "cropLeft": "4.7"})),
                99750,
            )
            .await
            .unwrap();

            // 99751: extreme dB clamps to 20.0 before fade interpolation and skips trailing sleep.
            e.obs_tool(
                &mut ctx,
                &obj(json!({"inputName": "Mic", "level": "10000", "unit": 1, "fade": 200})),
                99751,
            )
            .await
            .unwrap();

            // 99753: explicit on avoids GetStudioModeEnabled; toggle queries it.
            e.obs_tool(&mut ctx, &obj(json!({"state": 0})), 99753).await.unwrap();
            assert_eq!(ctx.args["obsStudioMode"], json!(true));
            e.obs_tool(&mut ctx, &obj(json!({"state": 2})), 99753).await.unwrap();
            assert_eq!(ctx.args["obsStudioMode"], json!(false));

            // 99757: start, stop, toggle all populate obsVirtualCamera.
            e.obs_tool(&mut ctx, &obj(json!({"state": 0})), 99757).await.unwrap();
            assert_eq!(ctx.args["obsVirtualCamera"], json!(true));
            e.obs_tool(&mut ctx, &obj(json!({"state": 1})), 99757).await.unwrap();
            assert_eq!(ctx.args["obsVirtualCamera"], json!(false));
            e.obs_tool(&mut ctx, &obj(json!({"state": 2})), 99757).await.unwrap();
            assert_eq!(ctx.args["obsVirtualCamera"], json!(true));

            // 99762, 99764 (skips empty keys), 99767, 99768, 99769.
            e.obs_tool(&mut ctx, &obj(json!({})), 99762).await.unwrap();
            assert_eq!(ctx.args["obsReplayPath"], json!("/tmp/replay.mkv"));
            e.obs_tool(
                &mut ctx,
                &obj(json!({"inputName": "Txt", "settings": {"": "skip", "size": "24", "bold": "true"}})),
                99764,
            )
            .await
            .unwrap();
            e.obs_tool(
                &mut ctx,
                &obj(json!({"sceneName": "S", "sourceName": "Cam", "state": 2})),
                99767,
            )
            .await
            .unwrap();
            assert_eq!(ctx.args["obsSourceLocked"], json!(true));
            e.obs_tool(&mut ctx, &obj(json!({"inputName": "Mic", "offset": "-2000"})), 99768)
                .await
                .unwrap();
            e.obs_tool(&mut ctx, &obj(json!({"inputName": "Mic", "balance": "75"})), 99769)
                .await
                .unwrap();
        });

        let seen = obs.seen.borrow();
        let tr = seen
            .iter()
            .find(|(_, r, _)| r == "SetSceneItemTransform")
            .expect("transform sent");
        assert_eq!(tr.2["sceneItemTransform"]["scaleX"], json!(0.5));
        assert_eq!(tr.2["sceneItemTransform"]["cropLeft"], json!(5));

        let vols: Vec<f64> = seen
            .iter()
            .filter(|(_, r, _)| r == "SetInputVolume")
            .map(|(_, _, d)| d["inputVolumeMul"].as_f64().unwrap())
            .collect();
        assert_eq!(vols, vec![10.0, 20.0], "target clamped before fade interpolation");

        let studio_gets = seen.iter().filter(|(_, r, _)| r == "GetStudioModeEnabled").count();
        assert_eq!(studio_gets, 1, "only toggle queries studio mode");

        let settings = seen
            .iter()
            .find(|(_, r, _)| r == "SetInputSettings")
            .expect("input settings sent");
        assert!(settings.2["inputSettings"].get("").is_none());
        assert_eq!(settings.2["inputSettings"]["size"], json!(24));
        assert_eq!(settings.2["inputSettings"]["bold"], json!(true));

        let sync = seen
            .iter()
            .find(|(_, r, _)| r == "SetInputAudioSyncOffset")
            .expect("sync offset sent");
        assert_eq!(sync.2["inputAudioSyncOffset"], json!(-950));

        let bal = seen
            .iter()
            .find(|(_, r, _)| r == "SetInputAudioBalance")
            .expect("balance sent");
        assert_eq!(bal.2["inputAudioBalance"], json!(0.75));
    }
}
