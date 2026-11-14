//! Song requests for Spotify, built from the general group tools: a group with settings (who can
//! request, limits, the replies), a command and an action for each of `!sr`, `!song`, `!queue`,
//! `!remove` and `!skip`, and the Spotify logo as the group's icon. The actions use the Streamerz
//! song request sub-action (99802), which keeps the queue; see `songrequests`.
use super::{action, command, say, with, Made};
use crate::edit::{new_sub, new_trigger};
use crate::groups::{Def, Kind, Meta};
use crate::model::Obj;
use serde_json::{json, Value};

pub const GROUP: &str = "Song requests";

fn text(name: &str, label: &str, default: &str, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind: Kind::Text,
        default: json!(default),
        description: description.into(),
        ..Default::default()
    }
}

fn choice(name: &str, label: &str, options: &[&str], default: &str, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind: Kind::Choice,
        default: json!(default),
        description: description.into(),
        options: options.iter().map(|o| o.to_string()).collect(),
        ..Default::default()
    }
}

fn number(name: &str, label: &str, kind: Kind, default: Value, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind,
        default,
        description: description.into(),
        ..Default::default()
    }
}

/// The replies, with what each says by default (after Songify's).
const REPLIES: &[(&str, &str, &str)] = &[
    ("msgAdded", "Reply: song added", "%spotifyArtist% - %spotifyTitle% requested by @%user% has been added to the queue (#%spotifyPosition%, plays in %spotifyTimeToPlay%)."),
    ("msgNoQuery", "Reply: no song given", "@%user% please specify a song to add to the queue."),
    ("msgNotFound", "Reply: song not found", "No track found."),
    ("msgUnsupported", "Reply: not a track", "%spotifyError%"),
    ("msgBlockedArtist", "Reply: artist blocked", "@%user% the artist: %spotifyArtist% has been blocked by the broadcaster."),
    ("msgBlockedSong", "Reply: song blocked", "@%user% the song: %spotifyArtist% - %spotifyTitle% has been blocked by the broadcaster."),
    ("msgExplicit", "Reply: explicit song", "This song contains explicit content and is not allowed."),
    ("msgUnavailable", "Reply: song not available", "The song %spotifyArtist% - %spotifyTitle% is not available in the streamer's country."),
    ("msgTooLong", "Reply: song too long", "@%user% the song you requested exceeded the maximum song length (%spotifyMaxLength%)."),
    ("msgDuplicate", "Reply: song already queued", "@%user% this song is already in the queue."),
    ("msgMaxRequests", "Reply: too many requests", "@%user% maximum number of songs in queue reached (%spotifyMaxRequests%)."),
    ("msgNotAllowed", "Reply: not allowed", "@%user% %spotifyError%"),
    ("msgError", "Reply: Spotify failed", "@%user% there was an error with Spotify. Error message: %spotifyError%"),
    ("msgRemoved", "Reply: request taken back", "@%user% your previous request (%spotifyTitle%) will be skipped."),
    ("msgNoRemove", "Reply: nothing to take back", "@%user% you have no songs in the queue."),
    ("msgNow", "Reply: what plays", "@%user% %spotifyArtist% - %spotifyTitle%%spotifyRequestedBy%"),
    ("msgNext", "Reply: what is next", "@%user% %spotifyArtist% - %spotifyTitle%%spotifyRequestedBy%"),
    ("msgQueue", "Reply: the queue", "%spotifyList%"),
    ("msgPos", "Reply: the viewer's place", "@%user% %spotifyList%"),
    ("msgNoPos", "Reply: no songs queued", "@%user% you have no songs in the current queue."),
    ("msgEmpty", "Reply: nothing there", "%spotifyError%"),
    ("msgSkipped", "Reply: song skipped", "@%user% skipped the current song."),
    ("msgVoteSkip", "Reply: vote to skip", "@%user% voted to skip the current song. (%spotifyVotes%)"),
    ("msgVoteSkipDone", "Reply: skipped by vote", "Skipping song by vote..."),
    ("msgSkipCooldown", "Reply: skip on cooldown", "@%user% skipping is on cooldown."),
    ("msgPlay", "Reply: playback resumed", "Playback resumed."),
    ("msgPause", "Reply: playback paused", "Playback stopped."),
    ("msgVolume", "Reply: the volume", "Spotify volume at %spotifyVolume%%"),
    ("msgVolumeSet", "Reply: volume set", "Spotify volume set to %spotifyVolume%%"),
    ("msgNoVolume", "Reply: bad volume", "@%user% give a volume from 0 to 100."),
    ("msgBanned", "Reply: song blocked by !bansong", "The song %spotifyTitle% has been blocked."),
    ("msgLiked", "Reply: song liked", "The song %spotifyTitle% has been added to your liked songs."),
];

fn meta() -> Meta {
    let said = "Can use %user%, %spotifyTitle%, %spotifyArtist%, %spotifyPosition%, %spotifyTimeToPlay%, %spotifyList%, %spotifyRequestedBy%, %spotifyVotes%, %spotifyVolume% and %spotifyError%.";
    let mut variables = vec![
        choice(
            "who",
            "Who can request",
            &["everyone", "subscribers", "vips", "moderators"],
            "everyone",
            "The lowest role that can use !sr",
        ),
        number(
            "maxPerUser",
            "Songs per viewer",
            Kind::Integer,
            json!(3),
            "Songs one viewer can have in the queue (0 is no limit)",
        ),
        number(
            "maxPerSubscriber",
            "Songs per subscriber",
            Kind::Integer,
            json!(3),
            "For subscribers and above (0 is the same as everyone)",
        ),
        choice(
            "unlimitedWho",
            "No limit for",
            &["nobody", "vips", "moderators"],
            "nobody",
            "Roles that can request any number (the broadcaster always can)",
        ),
        number(
            "maxLength",
            "Longest song",
            Kind::Duration,
            json!(600_000),
            "Songs this long or longer are refused (0:00 is no limit)",
        ),
        number(
            "blockExplicit",
            "Block explicit songs",
            Kind::Boolean,
            json!(false),
            "Songs Spotify marks as explicit",
        ),
        choice(
            "explicitWho",
            "Explicit songs allowed for",
            &["nobody", "subscribers", "vips", "moderators"],
            "nobody",
            "Roles that can still request them (the broadcaster always can)",
        ),
        text(
            "blockedUsers",
            "Blocked viewers",
            "",
            "Names that cannot request, separated by commas. They get no answer.",
        ),
        text(
            "blockedArtists",
            "Blocked artists",
            "",
            "Artist names, separated by commas",
        ),
        text(
            "blockedSongs",
            "Blocked songs",
            "",
            "Spotify links or URIs, separated by commas. !bansong adds the song that plays.",
        ),
        choice(
            "skipWho",
            "Who can skip",
            &["moderators", "vips", "subscribers", "broadcaster"],
            "moderators",
            "The lowest role that can use !skip",
        ),
        number(
            "skipOwn",
            "A viewer can skip their own song",
            Kind::Boolean,
            json!(true),
            "While the song they requested plays",
        ),
        number(
            "voteSkipCount",
            "Votes to skip",
            Kind::Integer,
            json!(5),
            "How many !voteskip skip the song",
        ),
        number(
            "skipCooldown",
            "Wait between skips",
            Kind::Duration,
            json!(10_000),
            "After a skip or a vote that skips",
        ),
        choice(
            "controlWho",
            "Who controls playback",
            &["moderators", "vips", "broadcaster"],
            "moderators",
            "!play, !pause, !vol, !bansong and !songlike",
        ),
    ];
    variables.extend(
        REPLIES
            .iter()
            .map(|(name, label, default)| text(name, label, default, said)),
    );
    Meta {
        description: "Viewers ask for songs with !sr and the song goes to your Spotify queue at once, as in Songify. \
                      Spotify Premium and a connected Spotify account are needed."
            .into(),
        variables,
        ..Default::default()
    }
}

/// One platform's check of where the command came from, then `inner`.
fn on_platform(platform: &str, inner: Vec<Obj>) -> Obj {
    with(
        new_sub(120),
        json!({"input": "%commandSource%", "operation": 7, "value": platform,
            "subActions": [with(new_sub(99901), json!({"subActions": inner})), new_sub(99902)]}),
    )
}

/// The reply on the platform the command came from, unless there is nothing to say.
fn replies() -> Vec<Obj> {
    ["twitch", "youtube", "kick"]
        .iter()
        .map(|p| {
            let said = with(
                new_sub(120),
                json!({"input": "%spotifyReply%", "operation": 8, "value": "",
                    "subActions": [with(new_sub(99901), json!({"subActions": [say(p, "%spotifyReply%")]})), new_sub(99902)]}),
            );
            on_platform(p, vec![said])
        })
        .collect()
}

pub fn made() -> Made {
    let mut made = Made::default();
    // The replies to what goes wrong are the same for every command.
    let mut common = json!({"blockedUsers": "%group.blockedUsers%"});
    for (name, _, _) in REPLIES {
        if ![
            "msgAdded",
            "msgRemoved",
            "msgNow",
            "msgNext",
            "msgQueue",
            "msgPos",
            "msgSkipped",
            "msgVoteSkip",
            "msgVoteSkipDone",
            "msgPlay",
            "msgPause",
            "msgVolume",
            "msgVolumeSet",
            "msgBanned",
            "msgLiked",
        ]
        .contains(name)
        {
            common[*name] = format!("%group.{name}%").into();
        }
    }
    let request = json!({
        "operation": "request", "query": "%rawInput%", "who": "%group.who%",
        "maxPerUser": "%group.maxPerUser%", "maxPerSubscriber": "%group.maxPerSubscriber%",
        "unlimitedWho": "%group.unlimitedWho%", "maxLengthMs": "%group.maxLength%",
        "blockExplicit": "%group.blockExplicit%", "explicitWho": "%group.explicitWho%",
        "blockedArtists": "%group.blockedArtists%", "blockedSongs": "%group.blockedSongs%",
        "reply": "%group.msgAdded%",
    });
    let control = |op: &str, reply: &str| json!({"operation": op, "who": "%group.controlWho%", "reply": format!("%group.{reply}%")});
    // (name, aliases, cooldown in seconds, what the sub-action is told)
    let all: [(&str, &str, u64, Value); 13] = [
        ("request", "!sr\n!ssr", 5, request),
        (
            "current song",
            "!song",
            10,
            json!({"operation": "current", "reply": "%group.msgNow%"}),
        ),
        (
            "next song",
            "!next",
            10,
            json!({"operation": "next", "reply": "%group.msgNext%"}),
        ),
        (
            "queue",
            "!queue",
            10,
            json!({"operation": "queue", "reply": "%group.msgQueue%"}),
        ),
        (
            "position",
            "!pos",
            10,
            json!({"operation": "pos", "reply": "%group.msgPos%"}),
        ),
        (
            "take back",
            "!remove",
            5,
            json!({"operation": "remove", "query": "%rawInput%", "reply": "%group.msgRemoved%"}),
        ),
        (
            "skip",
            "!skip",
            0,
            json!({"operation": "skip", "who": "%group.skipWho%", "skipOwn": "%group.skipOwn%",
            "skipCooldownMs": "%group.skipCooldown%", "reply": "%group.msgSkipped%"}),
        ),
        (
            "vote skip",
            "!voteskip",
            0,
            json!({"operation": "voteskip", "votesNeeded": "%group.voteSkipCount%",
            "skipCooldownMs": "%group.skipCooldown%", "reply": "%group.msgVoteSkip%", "replyDone": "%group.msgVoteSkipDone%"}),
        ),
        ("play", "!play", 5, control("play", "msgPlay")),
        ("pause", "!pause", 5, control("pause", "msgPause")),
        (
            "volume",
            "!vol",
            5,
            with_reply_done(control("volume", "msgVolume"), "%group.msgVolumeSet%"),
        ),
        ("block song", "!bansong", 5, control("bansong", "msgBanned")),
        ("like song", "!songlike", 5, control("like", "msgLiked")),
    ];
    for (what, alias, cooldown, mut fields) in all {
        if what == "volume" {
            fields["query"] = "%rawInput%".into();
        }
        if let (Some(f), Some(c)) = (fields.as_object_mut(), common.as_object()) {
            for (k, v) in c {
                f.entry(k.clone()).or_insert(v.clone());
            }
        }
        let mut c = command(&format!("Song requests: {what}"), alias, "", cooldown);
        // TikTok cannot send a message back.
        c.commands[0].insert("sources".into(), (1 | 1024 | 2_097_152).into());
        c.commands[0].insert("group".into(), GROUP.into());
        let id = c.commands[0]["id"].as_str().unwrap_or_default().to_string();
        let mut subs = vec![with(new_sub(99802), fields)];
        subs.extend(replies());
        let mut a = action(&format!("Song requests: {what}"), &[], subs);
        a.group = GROUP.into();
        a.triggers = vec![with(new_trigger(401), json!({"commandId": id}))];
        made.actions.push(a);
        made.commands.extend(c.commands);
    }
    made.groups.push((GROUP.into(), meta()));
    made.icons
        .push((GROUP.into(), include_bytes!("../../assets/icons/spotify.png").to_vec()));
    made
}

fn with_reply_done(mut fields: Value, done: &str) -> Value {
    fields["replyDone"] = done.into();
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::FakeMusic;
    use std::time::Duration;

    /// A config with the template added, without the cooldowns between one viewer's commands.
    fn installed() -> crate::Config {
        let mut c = crate::Config::default();
        made().add_to(&mut c).unwrap();
        c.commands.iter_mut().for_each(|x| {
            x.insert("userCooldown".into(), 0.into());
            x.insert("globalCooldown".into(), 0.into());
        });
        c
    }

    /// What the chat saw after these messages: (platform, user, role, text).
    fn chat(c: crate::Config, music: &'static FakeMusic, messages: &[(&str, &str, &str, &str)]) -> Vec<String> {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e =
                crate::engine::Engine::with_music(std::sync::Arc::new(tokio::sync::watch::channel(c).0), |_| {}, music);
            e.set_song_pace(Duration::from_millis(20));
            e.dev_mode();
            let mut out = Vec::new();
            for (platform, user, role, text) in messages {
                let args = json!({"args": {
                    "isSubscribed": *role == "subscriber", "isModerator": *role == "moderator",
                }});
                e.inject_chat(platform, user, "", role, text, &args).unwrap();
                tokio::time::sleep(Duration::from_millis(60)).await;
                out.extend(
                    e.dev_take()
                        .unwrap()
                        .iter()
                        .map(|m| m["text"].as_str().unwrap_or("").to_string()),
                );
            }
            out
        })
    }

    fn leak() -> &'static FakeMusic {
        Box::leak(Box::default())
    }

    #[test]
    fn the_template_is_a_group_with_settings_commands_and_the_spotify_icon() {
        let c = installed();
        let meta = crate::groups::meta(&c, GROUP);
        meta.validate().unwrap();
        assert!(meta.variables.iter().any(|d| d.name == "who" && d.kind == Kind::Choice));
        for name in REPLIES.iter().map(|r| r.0) {
            assert!(meta.variables.iter().any(|d| d.name == name), "{name}");
        }
        assert_eq!(c.commands.len(), 13);
        assert_eq!(c.actions.actions.len(), 13);
        assert!(c.action_group_icons.contains_key(GROUP));
        let values = crate::groups::values(&c, GROUP);
        assert_eq!(
            (values["maxPerUser"].clone(), values["who"].clone()),
            (json!(3), json!("everyone"))
        );
        for a in &c.actions.actions {
            let command = a.triggers[0]["commandId"].as_str().unwrap();
            assert!(c.commands.iter().any(|x| x["id"] == command && x["group"] == GROUP));
        }
        // It survives an export with its commands, settings and icon.
        let ids: Vec<String> = c.actions.actions.iter().map(|a| a.id.clone()).collect();
        let x = crate::edit::export(&c, &ids, ["", "", "", ""]);
        assert_eq!(x["data"]["commands"].as_array().unwrap().len(), 13);
        assert!(x["streamerz"]["groupIcons"][GROUP].is_object());
        assert_eq!(x["data"]["actions"][0]["subActions"][0]["type"], 99802);
        // Adding it again keeps what was set.
        let mut c = c;
        let mut v = crate::groups::values(&c, GROUP);
        v.insert("maxPerUser".into(), json!(1));
        crate::groups::set_values(&mut c, GROUP, v).unwrap();
        made().add_to(&mut c).unwrap();
        assert_eq!(crate::groups::values(&c, GROUP)["maxPerUser"], 1);
    }

    #[test]
    fn a_request_is_answered_with_the_song_its_place_and_time_on_every_platform() {
        for platform in ["twitch", "youtube", "kick"] {
            let music = leak();
            let said = chat(
                installed(),
                music,
                &[
                    (platform, "ana", "viewer", "!sr firework"),
                    (platform, "bia", "viewer", "!ssr roar"),
                ],
            );
            assert_eq!(
                said,
                [
                    "Fake Artist - firework requested by @ana has been added to the queue (#1, plays in 0m 0s).",
                    "Fake Artist - roar requested by @bia has been added to the queue (#2, plays in 3m 0s)."
                ],
                "{platform}"
            );
            assert_eq!(*music.queued.borrow(), ["firework", "roar"], "with Spotify at once");
        }
    }

    #[test]
    fn failures_say_why_and_every_reply_is_the_groups_to_change() {
        let said = chat(
            installed(),
            leak(),
            &[
                ("twitch", "ana", "viewer", "!sr missing"),
                ("twitch", "ana", "viewer", "!sr"),
            ],
        );
        assert_eq!(
            said,
            ["No track found.", "@ana please specify a song to add to the queue."]
        );
        let mut c = installed();
        let mut v = crate::groups::values(&c, GROUP);
        v.insert("msgAdded".into(), json!("%user% wants %spotifyTitle%!"));
        v.insert("msgNotFound".into(), json!("nothing for @%user%"));
        crate::groups::set_values(&mut c, GROUP, v).unwrap();
        assert_eq!(
            chat(
                c,
                leak(),
                &[
                    ("kick", "ana", "viewer", "!sr roar"),
                    ("kick", "bia", "viewer", "!sr missing")
                ]
            ),
            ["ana wants roar!", "nothing for @bia"]
        );
    }

    #[test]
    fn the_group_decides_who_can_request_and_how_many() {
        let mut c = installed();
        let mut v = crate::groups::values(&c, GROUP);
        v.insert("who".into(), json!("subscribers"));
        v.insert("maxPerUser".into(), json!(1));
        v.insert("maxPerSubscriber".into(), json!(2));
        crate::groups::set_values(&mut c, GROUP, v).unwrap();
        let said = chat(
            c,
            leak(),
            &[
                ("twitch", "ana", "viewer", "!sr one"),
                ("twitch", "bia", "subscriber", "!sr one"),
                ("twitch", "bia", "subscriber", "!sr two"),
                ("twitch", "bia", "subscriber", "!sr three"),
            ],
        );
        assert_eq!(
            said,
            [
                "@ana Sorry, only subscribers or higher can do that.",
                "Fake Artist - one requested by @bia has been added to the queue (#1, plays in 0m 0s).",
                "Fake Artist - two requested by @bia has been added to the queue (#2, plays in 3m 0s).",
                "@bia maximum number of songs in queue reached (2).",
            ]
        );
    }

    #[test]
    fn the_other_commands_work_from_chat() {
        let music = leak();
        *music.track.borrow_mut() = Some(FakeMusic::track("playing"));
        let said = chat(
            installed(),
            music,
            &[
                ("twitch", "ana", "viewer", "!sr one"),
                ("twitch", "bia", "subscriber", "!sr two"),
                ("twitch", "ana", "viewer", "!song"),
                ("twitch", "ana", "viewer", "!next"),
                ("twitch", "ana", "viewer", "!queue"),
                ("twitch", "bia", "subscriber", "!pos"),
                ("twitch", "ana", "viewer", "!remove"),
                ("twitch", "ana", "viewer", "!remove"),
                ("twitch", "ana", "viewer", "!skip"),
                ("twitch", "mo", "moderator", "!skip"),
                ("twitch", "mo", "moderator", "!pause"),
                ("twitch", "ana", "viewer", "!pause"),
                ("twitch", "mo", "moderator", "!vol 55"),
                ("twitch", "mo", "moderator", "!vol"),
                ("twitch", "mo", "moderator", "!songlike"),
                ("twitch", "mo", "moderator", "!bansong"),
            ],
        );
        assert_eq!(
            said,
            [
                "Fake Artist - one requested by @ana has been added to the queue (#1, plays in 3m 0s).",
                "Fake Artist - two requested by @bia has been added to the queue (#2, plays in 6m 0s).",
                "@ana Fake Artist - playing",
                "@ana Fake Artist - one (requested by @ana)",
                "#1 Fake Artist - one (@ana) | #2 Fake Artist - two (@bia)",
                "@bia #2 Fake Artist - two",
                "@ana your previous request (one) will be skipped.",
                "@ana you have no songs in the queue.",
                "@ana Sorry, only moderators or higher can do that.",
                "@mo skipped the current song.",
                "Playback stopped.",
                "@ana Sorry, only moderators or higher can do that.",
                "Spotify volume set to 55%",
                "Spotify volume at 55%",
                "The song playing has been added to your liked songs.",
                "The song playing has been blocked.",
            ]
        );
        assert_eq!(*music.commands.borrow(), ["next", "pause", "next"]);
        assert_eq!(*music.liked.borrow(), ["spotify:track:playing"]);
    }
}
