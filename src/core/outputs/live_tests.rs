//! Opt-in integration coverage. Only use isolated OBS settings and loopback RTMP receivers.
use super::*;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

struct Receiver {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}
impl Receiver {
    fn start(port: u16) -> Self {
        let mut child = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-listen",
                "1",
                "-i",
                &format!("rtmp://127.0.0.1:{port}/live/test"),
                "-progress",
                "pipe:1",
                "-stats_period",
                "0.5",
                "-f",
                "null",
                "-",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("FFmpeg receiver");
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Self { child, lines }
    }
    async fn frames(&mut self) {
        tokio::time::timeout(Duration::from_secs(40), async {
            while let Some(line) = self.lines.next_line().await.unwrap() {
                if line
                    .strip_prefix("frame=")
                    .and_then(|n| n.parse::<u64>().ok())
                    .is_some_and(|n| n > 0)
                {
                    println!("RTMP receiver delivered {line}");
                    return;
                }
            }
            panic!("Receiver closed without delivering video frames");
        })
        .await
        .expect("No video frames within 40 seconds");
    }
    async fn closed(&mut self) {
        tokio::time::timeout(Duration::from_secs(8), self.child.wait())
            .await
            .expect("Receiver remained connected after stop")
            .unwrap();
    }
}

async fn list(e: &Rc<Engine>) -> Value {
    let r = crate::servers::request(
        e,
        &json!({"request":"StreamerzOutputs", "op":"list", "connectionId":"verification"}),
        true,
        true,
    )
    .await;
    assert_eq!(r["status"], "ok", "{r}");
    r
}
async fn command(e: &Rc<Engine>, op: &str, ids: &[Value]) -> Value {
    let r = crate::servers::request(
        e,
        &json!({"request":"StreamerzOutputs", "op":op, "connectionId":"verification", "outputs":ids}),
        true,
        true,
    )
    .await;
    assert_eq!(r["status"], "ok", "{r}");
    assert!(
        r["results"].as_array().unwrap().iter().all(|r| r["state"] != "error"),
        "{r}"
    );
    r
}
fn state<'a>(inventory: &'a Value, id: &Value) -> &'a Value {
    inventory["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| &o["id"] == id)
        .unwrap()
}

#[test]
#[ignore = "needs isolated OBS and FFmpeg; dev/test-obs-outputs.py prepares them"]
fn stream_lifecycle_through_backend() {
    crate::engine::tests::run(async {
        let port: u16 = std::env::var("STREAMERZ_TEST_OBS_PORT").unwrap().parse().unwrap();
        let source = std::env::var("STREAMERZ_TEST_OBS_SOURCE").unwrap_or("obs".into());
        let target_port = match source.as_str() {
            SUITE => Some(19451),
            VERTICAL => Some(19452),
            MULTI => Some(19453),
            RTMP => Some(19454),
            "obs" => None,
            _ => panic!("Unknown test source"),
        };
        let target_port = std::env::var("STREAMERZ_TEST_OBS_TARGET_PORT")
            .ok()
            .map(|p| p.parse::<u16>().unwrap())
            .or(target_port);
        assert!(
            target_port.is_none_or(|p| (19451..=19454).contains(&p)),
            "Only the test receiver ports are allowed"
        );
        let selected_name = std::env::var("STREAMERZ_TEST_OBS_OUTPUT_NAME").ok();
        let config: crate::Config = serde_json::from_value(json!({"obs":{"connections":[{"id":"verification", "name":"Verification", "host":"127.0.0.1", "port":port, "autoConnect":true,"reconnect":false}]}})).unwrap();
        let e = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut subs = crate::engine::events::Subs::new();
        crate::engine::events::update(&mut subs, &json!({"ObsStudio":["Connected"]}), true);
        e.listen(123, &subs, &tx);
        let obs = e.obs_start();
        tokio::time::timeout(Duration::from_secs(15), rx.recv())
            .await
            .unwrap()
            .unwrap();
        e.listen(123, &Default::default(), &tx);
        let service = ask(obs.as_ref(), "verification", "GetStreamServiceSettings", json!({}))
            .await
            .unwrap();
        assert_eq!(
            service["streamServiceSettings"]["server"], "rtmp://127.0.0.1:19450/live",
            "Refusing a non-test destination"
        );
        let mut rtmp_id = None;
        if source == RTMP {
            vendor(
                obs.as_ref(),
                "verification",
                RTMP,
                "add_target",
                json!({"name":"YouTube test", "protocol":"RTMP"}),
            )
            .await
            .unwrap();
            let targets = vendor(obs.as_ref(), "verification", RTMP, "list_targets", json!({}))
                .await
                .unwrap();
            let id = targets["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == "YouTube test")
                .unwrap()["id"]
                .clone();
            vendor(
                obs.as_ref(),
                "verification",
                RTMP,
                "update_service_param",
                json!({"id":id,"key":"server","value":"rtmp://127.0.0.1:19454/live"}),
            )
            .await
            .unwrap();
            vendor(
                obs.as_ref(),
                "verification",
                RTMP,
                "update_stream_key",
                json!({"id":id,"streamKey":"test"}),
            )
            .await
            .unwrap();
            rtmp_id = Some(id);
        }
        let initial = list(&e).await;
        let mut canvas_ids = HashSet::new();
        for canvas in initial["canvases"].as_array().unwrap() {
            let id = canvas["canvasUuid"]
                .as_str()
                .or_else(|| canvas["uuid"].as_str())
                .unwrap();
            assert!(canvas_ids.insert(id), "Duplicate canvas: {initial}");
        }
        let outputs = initial["outputs"].as_array().unwrap();
        assert!(!outputs.iter().any(|o| o["name"] == "Recording filter test"));
        let main = outputs.iter().find(|o| o["source"] == "obs").unwrap()["id"].clone();
        assert_eq!(state(&initial, &main)["active"], false);
        let selected = outputs
            .iter()
            .find(|o| o["source"] == source && selected_name.as_ref().is_none_or(|name| o["name"] == *name))
            .unwrap_or_else(|| panic!("Missing {source}: {initial}"))["id"]
            .clone();
        assert_eq!(state(&initial, &selected)["controllable"], true);
        // A stale or foreign selection must fail before it can start the otherwise valid main output.
        let invalid = crate::servers::request(&e, &json!({"request":"StreamerzOutputs","op":"start","connectionId":"verification","outputs":[main,"foreign-id"]}), true, true).await;
        assert_eq!(invalid["status"], "error");
        assert_eq!(state(&list(&e).await, &main)["active"], false);
        if std::env::var("STREAMERZ_TEST_OBS_NO_MAIN").as_deref() == Ok("1") {
            assert_eq!(source, MULTI, "this fixture deliberately shares the main encoder");
            let result = command(&e, "start", std::slice::from_ref(&selected)).await;
            assert_eq!(
                result["results"][0]["state"], "requested",
                "vendor acceptance is not a live confirmation"
            );
            let inventory = list(&e).await;
            assert_eq!(
                state(&inventory, &main)["active"],
                false,
                "an unselected main must never be started"
            );
            assert_ne!(state(&inventory, &selected)["active"], true);
            obs.disconnect(Some("verification"));
            println!("PASS shared encoder without main: request remains unconfirmed; main stays stopped");
            return;
        }
        let mut main_receiver = Receiver::start(19450);
        let mut target_receiver = target_port.map(Receiver::start);
        // FFmpeg provides no ready event for its RTMP listener. This delay is test setup only.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let ids = if target_port.is_some() {
            vec![selected.clone(), main.clone()]
        } else {
            vec![main.clone()]
        };
        let started = command(&e, "start", &ids).await;
        assert_eq!(started["results"][0]["id"], main);
        assert_eq!(started["results"][0]["state"], "confirmed");
        main_receiver.frames().await;
        if let Some(receiver) = &mut target_receiver {
            receiver.frames().await;
        }
        let active = list(&e).await;
        assert!(
            !active["outputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["name"] == "multi-output"),
            "vendor targets already identify Multi-RTMP runtime outputs"
        );
        assert_eq!(state(&active, &main)["active"], true, "{active}");
        assert_eq!(state(&active, &selected)["active"], true, "{active}");
        let repeated = command(&e, "start", &ids).await;
        assert!(
            repeated["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["state"] == "unchanged"),
            "{repeated}"
        );
        if let Some(receiver) = &mut target_receiver {
            command(&e, "stop", std::slice::from_ref(&selected)).await;
            receiver.closed().await;
            let stopped = list(&e).await;
            assert_eq!(state(&stopped, &selected)["active"], false, "{stopped}");
            assert_eq!(state(&stopped, &main)["active"], true, "unselected main must stay live");
            assert_eq!(
                command(&e, "stop", std::slice::from_ref(&selected)).await["results"][0]["state"],
                "unchanged"
            );
        }
        if std::env::var("STREAMERZ_TEST_OBS_RECONNECT").as_deref() == Ok("1") {
            let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subs = crate::engine::events::Subs::new();
            crate::engine::events::update(&mut subs, &json!({"ObsStudio":["Event"]}), true);
            e.listen(124, &subs, &event_tx);
            main_receiver.child.kill().await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(event) = event_rx.recv().await {
                    let event: Value = serde_json::from_str(&event).unwrap();
                    if event["data"]["connectionId"] == "verification"
                        && event["data"]["outputState"] == "OBS_WEBSOCKET_OUTPUT_RECONNECTING"
                    {
                        return;
                    }
                }
                panic!("OBS event channel closed");
            })
            .await
            .expect("OBS did not report reconnection");
            e.listen(124, &Default::default(), &event_tx);
            main_receiver = Receiver::start(19450);
            tokio::time::sleep(Duration::from_millis(500)).await;
            let reconnected = command(&e, "start", std::slice::from_ref(&main)).await;
            assert!(
                matches!(
                    reconnected["results"][0]["state"].as_str(),
                    Some("confirmed" | "unchanged")
                ),
                "{reconnected}"
            );
            main_receiver.frames().await;
            let inventory = list(&e).await;
            assert_eq!(state(&inventory, &main)["active"], true);
            assert_eq!(state(&inventory, &main)["reconnecting"], false);
            println!("PASS main reconnection: receiver replaced, state confirmed, no duplicate start");
        }
        command(&e, "stop", std::slice::from_ref(&main)).await;
        main_receiver.closed().await;
        assert_eq!(state(&list(&e).await, &main)["active"], false);
        if let Some(id) = rtmp_id {
            vendor(obs.as_ref(), "verification", RTMP, "delete_target", json!({"id":id}))
                .await
                .unwrap();
        }
        obs.disconnect(Some("verification"));
        println!("PASS {source}: selected start, frames, active state, idempotence, selected stop, main kept live, stopped state");
    });
}
