//! The examples that come with Streamerz: overlays made with the same tools a user has, kept as plain
//! folders in `assets/examples` (a `template.json` and its `files`, what a `.streamerz` file holds
//! when unpacked). They are there to be added, read and changed.

use crate::package::{self, Package};

pub struct Example {
    /// Its folder in `assets/examples`.
    pub slug: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}

macro_rules! example {
    // With actions: the folder also has an `actions.json` (the group's actions and commands).
    ($slug:literal, actions $(, $file:literal)*) => {
        Example {
            slug: $slug,
            files: &[
                ("template.json", include_bytes!(concat!("../assets/examples/", $slug, "/template.json"))),
                $((concat!("files/", $file), include_bytes!(concat!("../assets/examples/", $slug, "/files/", $file))),)*
                ("actions.json", include_bytes!(concat!("../assets/examples/", $slug, "/actions.json"))),
            ],
        }
    };
    ($slug:literal $(, $file:literal)*) => {
        Example {
            slug: $slug,
            files: &[
                ("template.json", include_bytes!(concat!("../assets/examples/", $slug, "/template.json"))),
                $((concat!("files/", $file), include_bytes!(concat!("../assets/examples/", $slug, "/files/", $file))),)*
            ],
        }
    };
}

/// In the order they are listed: the simplest to read first.
pub const ALL: &[Example] = &[
    example!("clock", "index.html"),
    example!("subathon", actions, "index.html"),
    example!("viewers", "index.html"),
    example!("ad", "index.html"),
    example!("now-playing", "index.html"),
    example!("chat", "index.html"),
    example!("danmaku", "index.html"),
    example!("alerts", "index.html"),
    example!("emote-wall", "index.html"),
    example!("top-gifters", actions, "index.html"),
    example!("credits", "index.html"),
    example!("song-queue", "index.html"),
    example!("tiktok-gifts", "index.html"),
    example!("poll", actions, "index.html", "dock.html"),
    example!("wheel", actions, "index.html"),
    example!("chat-dock", "index.html"),
    example!("goal", "index.html"),
    example!("starting-soon", "index.html", "img/background.svg"),
    example!("control-dock", actions, "dock.html", "banner.html"),
];

/// An example as a template, ready to describe or install.
pub fn package(slug: &str) -> Result<Package, String> {
    let example = ALL
        .iter()
        .find(|e| e.slug == slug)
        .ok_or_else(|| format!("no example named {slug}"))?;
    let entries: Vec<(String, Vec<u8>)> = example
        .files
        .iter()
        .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
        .collect();
    package::unpacked(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups;

    #[test]
    fn gifters_count_without_a_page_and_keep_periods_and_users_separate() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let mut config = crate::Config::default();
            crate::package::install(&mut config, &package("top-gifters").unwrap()).unwrap();
            let engine = crate::engine::Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let fire = |kind, id: &str, name: &str, gifts, bomb| {
                let args = serde_json::json!({"user": name, "userId": id, "gifts": gifts, "fromGiftBomb": bomb});
                engine.fire(kind, "Gift", "Twitch", "twitch", args.as_object().unwrap(), |_| true)
            };
            fire(106, "1", "Same name", 5, false);
            fire(105, "1", "Same name", 1, true);
            fire(105, "1", "Same name", 1, false);
            fire(105, "2", "Same name", 1, false);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let values = groups::values(&engine.config(), "Top gifters");
            assert_eq!(values["score_1_twitch_1"], 6);
            assert_eq!(values["score_1_twitch_2"], 1);
            assert_eq!(values["name_1_twitch_1"], "Same name");
            engine.change(|c| {
                let mut values = groups::values(c, "Top gifters");
                values.insert("period".into(), 2.into());
                groups::set_values(c, "Top gifters", values).unwrap();
            });
            fire(106, "1", "Renamed", 3, false);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let saved = serde_json::to_vec(&*engine.config()).unwrap();
            let restored: crate::Config = serde_json::from_slice(&saved).unwrap();
            let values = groups::values(&restored, "Top gifters");
            assert_eq!(values["score_1_twitch_1"], 6);
            assert_eq!(values["score_2_twitch_1"], 3);
            assert_eq!(values["name_2_twitch_1"], "Renamed");
        });
    }

    #[test]
    fn every_example_installs_and_passes_the_share_checks() {
        let mut ids = std::collections::HashSet::new();
        for example in ALL {
            let p = package(example.slug).unwrap_or_else(|e| panic!("{}: {e}", example.slug));
            let t = &p.meta.template;
            assert!(ids.insert(t.id.clone()), "{}: its id is taken", example.slug);
            assert_eq!(t.id, format!("streamerz.example.{}", example.slug));
            assert!(!t.version.is_empty() && !t.author.is_empty() && !p.meta.description.is_empty());
            assert!(!t.pages.is_empty(), "{}", example.slug);
            // Every setting is used by a page or an action, so none is there for nothing.
            let html: String = example
                .files
                .iter()
                .filter(|(name, _)| name.ends_with(".html") || *name == "actions.json")
                .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
                .collect();
            for v in &p.meta.variables {
                assert!(
                    html.contains(&format!(".{}", v.name))
                        || html.contains(&format!("\"{}\"", v.name))
                        || html.contains(&format!("%group.{}%", v.name)),
                    "{}: {} is not used",
                    example.slug,
                    v.name
                );
            }
            let mut c = crate::Config::default();
            let (group, unsupported) = package::install(&mut c, &p).unwrap();
            assert!(unsupported.is_empty());
            // The ones with actions come connected to them; the rest make no group of actions.
            let with_actions = example.files.iter().any(|f| f.0 == "actions.json");
            assert_eq!(c.actions.actions.iter().any(|a| a.group == group), with_actions);
            assert_eq!(groups::listed(&c, &group), with_actions, "{}", example.slug);
            // A command comes with the action it starts.
            let (commands, _) = groups::members(&c, &group);
            for id in &commands {
                assert!(
                    c.actions.actions.iter().any(|a| a.group == group
                        && a.triggers
                            .iter()
                            .any(|t| t.get("commandId").and_then(|v| v.as_str()) == Some(id))),
                    "{}: nothing starts with {id}",
                    example.slug
                );
            }
            assert_eq!(groups::values(&c, &group).len(), p.meta.variables.len());
            // What it uses and what it asks for agree, its links lead somewhere.
            assert_eq!(package::checks(&c, &group), Vec::<String>::new(), "{}", example.slug);
            assert_eq!(package::describe(&c, &p)["installed"]["state"], "same");
            let _ = groups::set_meta(&mut c, &group, groups::Meta::default());
        }
        assert!(package("nope").is_err());
    }
}

#[cfg(test)]
mod subathon_tests {
    use super::*;
    use crate::{groups, templates::clock};
    use serde_json::json;
    const GROUP: &str = "Subathon";
    fn installed() -> (crate::Config, String) {
        let mut c = crate::Config::default();
        let package = package("subathon").unwrap();
        crate::package::install(&mut c, &package).unwrap();
        let clock_id = groups::values(&c, GROUP)["clockId"].as_str().unwrap().to_string();
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
        let overlay = String::from_utf8(groups::read_file(&c, GROUP, "index.html").unwrap()).unwrap();
        assert!(overlay.contains("streamerz.countdown(id"));
        // Installing again keeps the clock that is already running.
        assert_eq!(c.actions.actions.len(), 4);
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
        assert_eq!(groups::meta_with_files(&d, GROUP), groups::meta_with_files(&c, GROUP));
        assert_eq!(d.actions.actions.len(), 4);
        let id = "streamerz.example.subathon";
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
