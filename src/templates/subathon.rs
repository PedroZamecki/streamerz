//! A subathon timer, built only from the general group tools: a group with typed settings, three
//! actions using the countdown sub-action, and the overlay as a file of the group.
use super::{action, with, Made};
use crate::edit::new_sub;
use crate::groups::{text_file, Def, Kind, Meta};
use serde_json::{json, Value};

pub const GROUP: &str = "Subathon";

fn def(name: &str, label: &str, kind: Kind, default: Value, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind,
        default,
        description: description.into(),
        ..Default::default()
    }
}

pub fn made() -> Made {
    let clock = crate::engine::uuid();
    let count = |operation: &str, more: Value| {
        with(
            new_sub(99803),
            with_map(
                json!({"clockId": clock, "operation": operation, "maxMs": "%group.max%"}),
                more,
            ),
        )
    };
    let mut reset = action(
        "Subathon: reset",
        &[],
        vec![count("reset", json!({"startMs": "%group.start%"}))],
    );
    let mut subs = action(
        "Subathon: subscriptions",
        &[103, 104, 105],
        vec![count("add", json!({"perUnitMs": "%group.subscription%", "units": "1"}))],
    );
    let mut donations = action(
        "Subathon: donations",
        &[201],
        vec![count(
            "add",
            json!({"perUnitMs": "%group.donation%", "units": "%donationAmount%", "expectedCurrency": "%group.currency%"}),
        )],
    );
    for a in [&mut reset, &mut subs, &mut donations] {
        a.group = GROUP.into();
    }
    let overlay = include_str!("../../assets/overlays/subathon.html").replace("__COUNTDOWN_ID__", &clock);
    let meta = Meta {
        description: "A countdown that grows with subscriptions and donations. Add subathon.html as a browser source."
            .into(),
        variables: vec![
            def(
                "start",
                "Starting time",
                Kind::Duration,
                json!(3_600_000),
                "Time on the clock when it starts or is reset",
            ),
            def(
                "max",
                "Maximum time",
                Kind::Duration,
                json!(86_400_000),
                "The clock never goes above this",
            ),
            def(
                "subscription",
                "Time per subscription",
                Kind::Duration,
                json!(300_000),
                "Added for each subscription or gift",
            ),
            def(
                "donation",
                "Time per donation unit",
                Kind::Duration,
                json!(60_000),
                "Added for each unit of the donation currency",
            ),
            def(
                "currency",
                "Donation currency",
                Kind::Text,
                json!("USD"),
                "Only donations in this currency count",
            ),
        ],
        files: vec![text_file("subathon.html", &overlay)],
        ..Default::default()
    };
    Made {
        actions: vec![reset, subs, donations],
        groups: vec![(GROUP.into(), meta)],
        ..Default::default()
    }
}

fn with_map(mut a: Value, b: Value) -> Value {
    if let (Some(x), Value::Object(y)) = (a.as_object_mut(), b) {
        x.extend(y);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups;
    use crate::templates::clock;

    fn installed() -> (crate::Config, String) {
        let mut c = crate::Config::default();
        let made = made();
        let clock_id = made.actions[0].sub_actions[0]["clockId"].as_str().unwrap().to_string();
        made.add_to(&mut c).unwrap();
        (c, clock_id)
    }

    #[test]
    fn installing_declares_the_settings_and_starts_the_clock() {
        let (c, id) = installed();
        let v = groups::values(&c, GROUP);
        assert_eq!(
            (v["start"].clone(), v["currency"].clone()),
            (json!(3_600_000), json!("USD"))
        );
        let s = clock::get(&c, &id, clock::now_ms()).unwrap();
        assert!(s.running && s.max_ms == 86_400_000 && s.remaining_ms > 3_599_000);
        let m = groups::meta(&c, GROUP);
        let overlay = String::from_utf8(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &m.files[0].content).unwrap(),
        )
        .unwrap();
        assert!(overlay.contains(&id) && !overlay.contains("__COUNTDOWN_ID__"));
        // Installing again keeps the clock that is already running.
        assert_eq!(c.actions.actions.len(), 3);
    }

    #[test]
    fn export_and_import_carry_settings_files_and_actions() {
        let (mut c, _) = installed();
        let mut v = groups::values(&c, GROUP);
        v.insert("max".into(), json!(7_200_000));
        groups::set_values(&mut c, GROUP, v).unwrap();
        let ids: Vec<String> = c.actions.actions.iter().map(|a| a.id.clone()).collect();
        let x = crate::edit::export(&c, &ids, ["Subathon", "", "", ""]);
        let x: crate::model::Obj = crate::model::read_export(&crate::model::write_export(&x)).unwrap();
        let mut d = crate::Config::default();
        crate::edit::import(&mut d, &x).unwrap();
        assert_eq!(groups::values(&d, GROUP)["max"], 7_200_000);
        assert_eq!(groups::meta(&d, GROUP), groups::meta(&c, GROUP));
        assert_eq!(d.actions.actions.len(), 3);
        let id = d.actions.actions[0].sub_actions[0]["clockId"].as_str().unwrap();
        assert_eq!(clock::get(&d, id, clock::now_ms()).unwrap().max_ms, 7_200_000);
    }

    #[test]
    fn subscriptions_and_matching_donations_add_time() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let (mut config, id) = installed();
            let mut v = groups::values(&config, GROUP);
            for (k, n) in [
                ("start", 10_000),
                ("max", 20_000),
                ("subscription", 1_000),
                ("donation", 1_000),
            ] {
                v.insert(k.into(), json!(n));
            }
            groups::set_values(&mut config, GROUP, v).unwrap();
            clock::reset(&mut config, &id, 10_000, 20_000, clock::now_ms());
            let engine = crate::engine::Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let args = serde_json::Map::new();
            assert_eq!(engine.fire(103, "Subscription", "Twitch", "twitch", &args, |_| true), 1);
            assert_eq!(engine.fire(105, "Gift", "Twitch", "twitch", &args, |_| true), 1);
            let donation = |currency: &str| {
                json!({"donationAmount": 2.5, "donationCurrency": currency})
                    .as_object()
                    .unwrap()
                    .clone()
            };
            assert_eq!(
                engine.fire(201, "Donation", "Streamlabs", "streamlabs", &donation("EUR"), |_| true),
                1
            );
            assert_eq!(
                engine.fire(201, "Donation", "Streamlabs", "streamlabs", &donation("USD"), |_| true),
                1
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let left = clock::get(&engine.config(), &id, clock::now_ms()).unwrap().remaining_ms;
            assert!((14_000..=14_500).contains(&left), "remaining: {left}");
            let reset = engine.config().actions.actions[0].id.clone();
            assert!(engine.queue(&reset, serde_json::Map::new()).is_some());
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let left = clock::get(&engine.config(), &id, clock::now_ms()).unwrap().remaining_ms;
            assert!((9_800..=10_000).contains(&left), "after reset: {left}");
        });
    }
}
