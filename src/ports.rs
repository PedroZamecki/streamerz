//! Ports: what the engine needs from a connector, with no word about how it is done. Production
//! adapters (`obs::ObsHandle`, `spotify::Spotify`) implement them; tests use the in-memory fakes
//! in `fakes`, so an action can be tried with no OBS, Spotify account or network.

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A scene and source switcher such as OBS: requests in the obs-websocket shape, by connection.
pub trait ObsPort {
    fn send_request<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        request_type: &'a str,
        request_data: Value,
    ) -> Fut<'a, Result<Value, String>>;
    fn send_batch<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        halt_on_failure: bool,
        requests: Vec<Value>,
    ) -> Fut<'a, Result<Vec<Value>, String>>;
    fn is_connected(&self, connection_id: Option<&str>) -> bool;
    fn connect(&self, connection_id: Option<&str>);
    fn disconnect(&self, connection_id: Option<&str>);
    fn reload(&self);
}

/// A music player the streamer controls: what plays, the queue and the transport.
pub trait MusicPort {
    /// Execute a player operation and return its JSON result, without exposing transport details.
    fn player_action<'a>(
        &'a self,
        _engine: &'a Rc<crate::engine::Engine>,
        _kind: u32,
        _value: &'a str,
    ) -> Fut<'a, Result<Value, String>> {
        Box::pin(async { Err("Spotify player action is unavailable".into()) })
    }
    fn current<'a>(
        &'a self,
        engine: &'a Rc<crate::engine::Engine>,
    ) -> Fut<'a, Result<Option<crate::spotify::Track>, String>>;
    /// The track a track link, URI or search phrase means.
    fn find<'a>(
        &'a self,
        engine: &'a Rc<crate::engine::Engine>,
        query: &'a str,
    ) -> Fut<'a, Result<crate::spotify::Track, String>>;
    /// Adds a track (by URI) to the end of the player's queue, on `device` when one is known.
    fn add<'a>(
        &'a self,
        engine: &'a Rc<crate::engine::Engine>,
        uri: &'a str,
        device: &'a str,
    ) -> Fut<'a, Result<(), String>>;
    /// Sets the volume, 0 to 100.
    fn volume<'a>(
        &'a self,
        engine: &'a Rc<crate::engine::Engine>,
        percent: u32,
        device: &'a str,
    ) -> Fut<'a, Result<(), String>>;
    /// Saves a track (by URI) to the streamer's liked songs.
    fn like<'a>(&'a self, engine: &'a Rc<crate::engine::Engine>, uri: &'a str) -> Fut<'a, Result<(), String>>;
    /// What plays and what is queued after it.
    fn player<'a>(&'a self, engine: &'a Rc<crate::engine::Engine>) -> Fut<'a, Result<crate::spotify::Player, String>>;
    /// Queues a track link, URI or search phrase; the track's URI.
    fn queue<'a>(&'a self, engine: &'a Rc<crate::engine::Engine>, query: &'a str) -> Fut<'a, Result<String, String>> {
        Box::pin(async move {
            let track = self.find(engine, query).await?;
            self.add(engine, &track.uri, "").await?;
            Ok(track.uri)
        })
    }
    /// `play`, `pause`, `next` or `previous`.
    fn playback<'a>(&'a self, engine: &'a Rc<crate::engine::Engine>, command: &'a str) -> Fut<'a, Result<(), String>>;
}

#[cfg(test)]
pub mod fakes {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Answers OBS requests from a table and keeps every request it got.
    #[derive(Default)]
    pub struct FakeObs {
        pub answers: RefCell<HashMap<String, Value>>,
        pub seen: RefCell<Vec<(Option<String>, String, Value)>>,
        pub connected: std::cell::Cell<bool>,
    }

    impl FakeObs {
        pub fn answer(&self, request_type: &str, value: Value) {
            self.answers.borrow_mut().insert(request_type.into(), value);
        }
        /// The requests of one type, in order.
        pub fn requests(&self, request_type: &str) -> Vec<Value> {
            self.seen
                .borrow()
                .iter()
                .filter(|r| r.1 == request_type)
                .map(|r| r.2.clone())
                .collect()
        }
    }

    impl ObsPort for FakeObs {
        fn send_request<'a>(
            &'a self,
            connection_id: Option<&'a str>,
            request_type: &'a str,
            request_data: Value,
        ) -> Fut<'a, Result<Value, String>> {
            self.seen
                .borrow_mut()
                .push((connection_id.map(String::from), request_type.into(), request_data));
            let answer = self.answers.borrow().get(request_type).cloned();
            Box::pin(async move { answer.ok_or_else(|| format!("no answer for {request_type}")) })
        }
        fn send_batch<'a>(
            &'a self,
            _: Option<&'a str>,
            _: bool,
            requests: Vec<Value>,
        ) -> Fut<'a, Result<Vec<Value>, String>> {
            Box::pin(async move { Ok(requests) })
        }
        fn is_connected(&self, _: Option<&str>) -> bool {
            self.connected.get()
        }
        fn connect(&self, _: Option<&str>) {
            self.connected.set(true);
        }
        fn disconnect(&self, _: Option<&str>) {
            self.connected.set(false);
        }
        fn reload(&self) {}
    }

    /// A player in memory: a current track, a queue and the last transport command. A query is a track
    /// named by it; "missing" is not found.
    #[derive(Default)]
    pub struct FakeMusic {
        pub track: RefCell<Option<crate::spotify::Track>>,
        pub queued: RefCell<Vec<String>>,
        pub commands: RefCell<Vec<String>>,
        /// Where the playing track is, in milliseconds; and whether it plays.
        pub progress: std::cell::Cell<u64>,
        pub paused: std::cell::Cell<bool>,
        pub explicit: RefCell<Vec<String>>,
        /// Queries whose track Spotify says is not playable in the streamer's country.
        pub restricted: RefCell<Vec<String>>,
        pub volume: std::cell::Cell<u32>,
        pub liked: RefCell<Vec<String>>,
        /// Adding to the queue fails (no active device).
        pub fail_add: std::cell::Cell<bool>,
    }

    impl FakeMusic {
        pub fn track(name: &str) -> crate::spotify::Track {
            crate::spotify::Track {
                uri: format!("spotify:track:{name}"),
                name: name.into(),
                artists: vec!["Fake Artist".into()],
                playing: false,
                duration_ms: 180_000,
                explicit: false,
                popularity: 50,
                restricted: false,
            }
        }
    }

    impl MusicPort for FakeMusic {
        fn player_action<'a>(
            &'a self,
            _: &'a Rc<crate::engine::Engine>,
            kind: u32,
            value: &'a str,
        ) -> Fut<'a, Result<Value, String>> {
            Box::pin(async move {
                self.commands.borrow_mut().push(format!("{kind}:{value}"));
                Ok(serde_json::json!({"kind": kind, "value": value}))
            })
        }
        fn current<'a>(
            &'a self,
            _: &'a Rc<crate::engine::Engine>,
        ) -> Fut<'a, Result<Option<crate::spotify::Track>, String>> {
            Box::pin(async move { Ok(self.track.borrow().clone()) })
        }
        fn find<'a>(
            &'a self,
            _: &'a Rc<crate::engine::Engine>,
            query: &'a str,
        ) -> Fut<'a, Result<crate::spotify::Track, String>> {
            Box::pin(async move {
                if query == "missing" {
                    return Err("No Spotify track found".into());
                }
                let mut t = FakeMusic::track(query);
                t.explicit = self.explicit.borrow().iter().any(|e| e == query);
                t.restricted = self.restricted.borrow().iter().any(|e| e == query);
                if let Some(minutes) = query.strip_prefix("long") {
                    t.duration_ms = 60_000 * minutes.parse::<u64>().unwrap_or(30);
                }
                Ok(t)
            })
        }
        fn add<'a>(
            &'a self,
            _: &'a Rc<crate::engine::Engine>,
            uri: &'a str,
            _: &'a str,
        ) -> Fut<'a, Result<(), String>> {
            Box::pin(async move {
                if self.fail_add.get() {
                    return Err("Spotify has no active device: start playing something in Spotify first".into());
                }
                self.queued
                    .borrow_mut()
                    .push(uri.trim_start_matches("spotify:track:").into());
                Ok(())
            })
        }
        fn volume<'a>(
            &'a self,
            _: &'a Rc<crate::engine::Engine>,
            percent: u32,
            _: &'a str,
        ) -> Fut<'a, Result<(), String>> {
            Box::pin(async move {
                self.volume.set(percent);
                Ok(())
            })
        }
        fn like<'a>(&'a self, _: &'a Rc<crate::engine::Engine>, uri: &'a str) -> Fut<'a, Result<(), String>> {
            Box::pin(async move {
                self.liked.borrow_mut().push(uri.into());
                Ok(())
            })
        }
        fn player<'a>(&'a self, _: &'a Rc<crate::engine::Engine>) -> Fut<'a, Result<crate::spotify::Player, String>> {
            Box::pin(async move {
                Ok(crate::spotify::Player {
                    current: self.track.borrow().clone(),
                    playing: !self.paused.get(),
                    progress_ms: self.progress.get(),
                    queue: self.queued.borrow().iter().map(|n| FakeMusic::track(n)).collect(),
                    device_id: "fake-device".into(),
                    volume: Some(self.volume.get()),
                })
            })
        }
        fn playback<'a>(&'a self, _: &'a Rc<crate::engine::Engine>, command: &'a str) -> Fut<'a, Result<(), String>> {
            Box::pin(async move {
                if !["play", "pause", "next", "previous"].contains(&command) {
                    return Err("Unknown Spotify playback command".into());
                }
                self.commands.borrow_mut().push(command.into());
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::{FakeMusic, FakeObs};
    use super::*;
    use crate::engine::tests::{action, engine, run, settle};
    use serde_json::json;

    #[test]
    fn obs_sub_action_runs_against_a_fake_port() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let obs = Rc::new(FakeObs::default());
        obs.answer("GetSceneItemId", json!({"sceneItemId": 7}));
        obs.answer("GetSceneItemEnabled", json!({"sceneItemEnabled": true}));
        obs.answer("SetSceneItemEnabled", json!({}));
        run(async {
            let e = engine(json!({"actions": [action(
                "a1",
                "toggle",
                json!([{"type": 30, "connectionId": "main", "sceneName": "Game", "sourceName": "Cam", "state": 2}]),
            )]}));
            e.set_obs(obs.clone());
            e.queue("a1", Default::default()).unwrap();
            settle().await;
        });
        let calls = obs.requests("SetSceneItemEnabled");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["sceneItemId"], 7);
        assert_eq!(calls[0]["sceneItemEnabled"], false);
        assert_eq!(obs.seen.borrow()[0].0.as_deref(), Some("main"));
    }

    #[test]
    fn websocket_music_requests_go_through_the_port() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let music: &'static FakeMusic = Box::leak(Box::default());
        run(async {
            let e = crate::engine::Engine::with_music(
                std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0),
                |_| {},
                music,
            );
            let ask = |v: serde_json::Value| {
                let e = e.clone();
                async move { crate::servers::spotify_request(&e, &v).await }
            };
            assert_eq!(
                ask(json!({"request": "StreamerzSpotifyQueue", "query": "song"}))
                    .await
                    .unwrap()["uri"],
                "spotify:track:song"
            );
            ask(json!({"request": "StreamerzSpotifyPlayback", "command": "next"}))
                .await
                .unwrap();
            assert_eq!(
                ask(json!({"request": "StreamerzSpotifyFind", "query": "song"}))
                    .await
                    .unwrap()["track"]["name"],
                "song"
            );
            let player = ask(json!({"request": "StreamerzSpotifyPlayer"})).await.unwrap();
            assert_eq!(player["queue"][0]["name"], "song");
            assert_eq!(player["requests"], json!([]));
            assert!(ask(json!({"request": "StreamerzSpotifyPlayback", "command": "x"}))
                .await
                .is_err());
            assert_eq!(
                ask(json!({"request": "StreamerzSpotifyCurrent"})).await.unwrap()["track"],
                json!(null)
            );
        });
        assert_eq!(*music.queued.borrow(), ["song"]);
        assert_eq!(*music.commands.borrow(), ["next"]);
    }

    #[test]
    fn connectors_and_raw_obs_requests_use_the_ports() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let obs = Rc::new(FakeObs::default());
        obs.answer("GetSceneList", json!({"scenes": []}));
        run(async {
            let e = engine(json!({"actions": []}));
            let none = crate::servers::obs_request(&e, &json!({"requestType": "GetSceneList"})).await;
            assert_eq!(none.unwrap_err(), "No OBS connection is set up");
            e.set_obs(obs.clone());
            let r = crate::servers::obs_request(&e, &json!({"requestType": "GetSceneList", "connectionId": "main"}))
                .await
                .unwrap();
            assert_eq!(r["responseData"]["scenes"], json!([]));
            assert!(crate::servers::obs_request(&e, &json!({})).await.is_err());
            let c = crate::servers::connectors(&e);
            assert_eq!(c["obs"]["started"], true);
            assert_eq!(c["spotify"]["enabled"], false);
            assert!(!c.to_string().contains("token"));
        });
        assert_eq!(obs.seen.borrow()[0].0.as_deref(), Some("main"));
    }
}
