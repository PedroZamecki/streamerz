//! The engine, continued: the song request sub-action (99802), after Songify. A request is checked
//! against the group's rules and given to Spotify's queue at once; `songrequests` remembers who asked.
//! The operations are request, remove, current, next, queue, pos, skip, voteskip, play, pause, volume,
//! bansong and like. Each ends with `spotifyReply`: the reply to send, from the group's settings.

use super::{s, Ctx, Engine};
use crate::model::Obj;
use crate::songrequests::{clock, reject, Reason, Rejection, Request, Rules};
use crate::spotify::{Player, Track};
use serde_json::Value;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long the skip watcher waits between looks at the player when it cannot tell better.
const WATCH_MAX: Duration = Duration::from_secs(10);

/// 0 everyone, 1 subscribers, 2 VIPs, 3 moderators, 4 the broadcaster.
fn level(args: &Obj) -> u8 {
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("isBroadcaster") || args.get("role").and_then(Value::as_u64) == Some(4) {
        4
    } else if flag("isModerator") {
        3
    } else if flag("isVip") {
        2
    } else if flag("isSubscribed") {
        1
    } else {
        0
    }
}

/// The level a setting asks for, and how to name who it lets in. "nobody" asks for more than anyone has.
fn required(who: &str) -> (u8, &'static str) {
    match who.trim().to_ascii_lowercase().as_str() {
        "subscribers" => (1, "subscribers"),
        "vips" => (2, "VIPs"),
        "moderators" => (3, "moderators"),
        "broadcaster" => (4, "the broadcaster"),
        "nobody" | "none" => (5, "nobody"),
        _ => (0, "everyone"),
    }
}

/// A list typed in a setting: separated by commas, semicolons or lines.
fn list(text: &str) -> Vec<String> {
    text.split([',', ';', '\n'])
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn tracks_text(tracks: impl Iterator<Item = (usize, String)>) -> String {
    tracks.map(|(i, t)| format!("#{i} {t}")).collect::<Vec<_>>().join(" | ")
}

/// "3m 20s".
fn minutes_seconds(ms: u64) -> String {
    format!("{}m {}s", ms / 60_000, ms / 1_000 % 60)
}

type Done = Result<(), Rejection>;

impl Engine {
    /// A number in a field (or `%group.name%`): 0 when it is empty or not a number.
    fn song_limit(&self, ctx: &Ctx, o: &Obj, key: &str) -> u64 {
        let text = match o.get(key) {
            Some(Value::String(t)) => self.expand(ctx, t),
            Some(v) => v.to_string(),
            None => return 0,
        };
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
            .map_or(0, |n| n as u64)
    }

    fn song_flag(&self, ctx: &Ctx, o: &Obj, key: &str, default: bool) -> bool {
        let text = match o.get(key) {
            Some(Value::String(t)) => self.expand(ctx, t),
            Some(Value::Bool(v)) => return *v,
            _ => return default,
        };
        match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => true,
            "false" | "no" | "off" | "0" => false,
            _ => default,
        }
    }

    fn song_text(&self, ctx: &Ctx, o: &Obj, key: &str) -> String {
        self.expand(ctx, s(o, key))
    }

    /// The request sub-action. `operation`: request (the default), remove, current, next, queue, pos,
    /// skip, voteskip, play, pause, volume, bansong or like. The results are arguments: `spotifyQueued`
    /// (`true` or `false`), `spotifyError`, `spotifyReply` (what to say, empty for nothing), `spotifyTitle`,
    /// `spotifyArtist`, `spotifyUri`, `spotifyPosition`, `spotifyTimeToPlay`, `spotifyList`,
    /// `spotifyRequester`, `spotifyRequestedBy`, `spotifyVotes`, `spotifyVolume`, `spotifyMaxLength`,
    /// `spotifyMaxRequests` and `spotifyPlaying`.
    pub(super) async fn song_op(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) {
        for k in [
            "spotifyTitle",
            "spotifyArtist",
            "spotifyUri",
            "spotifyPosition",
            "spotifyTimeToPlay",
            "spotifyList",
            "spotifyRequester",
            "spotifyRequestedBy",
            "spotifyVotes",
            "spotifyVolume",
            "spotifyMaxLength",
            "spotifyMaxRequests",
            "spotifyError",
            "spotifyReply",
        ] {
            ctx.args.insert(k.into(), "".into());
        }
        ctx.args.insert("spotifyPlaying".into(), "false".into());
        ctx.args.remove("spotifyVoteDone");
        ctx.args.remove("spotifyVolumeSet");
        let result = self.song_run(ctx, o).await;
        self.song_events(ctx, s(o, "operation"), &result);
        self.song_queue_changed();
        let reply = match &result {
            Ok(()) => {
                // A vote that skipped the song, and a volume that was set, have replies of their own.
                let key = match (ctx.args.get("spotifyVoteDone"), ctx.args.get("spotifyVolumeSet")) {
                    (Some(Value::String(d)), _) if d == "true" => "replyDone",
                    (_, Some(Value::String(d))) if d == "true" => "replyDone",
                    _ => "reply",
                };
                let text = self.song_text(ctx, o, key);
                if text.is_empty() {
                    self.song_text(ctx, o, "reply")
                } else {
                    text
                }
            }
            Err(rej) => {
                let text = if rej.text.trim().is_empty() {
                    "Spotify request failed".to_string()
                } else {
                    rej.text.clone()
                };
                if rej.reason != Reason::Silent {
                    crate::warn(format!("Spotify: {text}"));
                    crate::spotify::event(self, 99806, &text);
                }
                ctx.args.insert("spotifyError".into(), text.clone().into());
                match rej.reason.setting() {
                    None => String::new(),
                    Some(key) => {
                        let custom = self.song_text(ctx, o, key);
                        if custom.trim().is_empty() {
                            text
                        } else {
                            custom
                        }
                    }
                }
            }
        };
        ctx.args.insert("spotifyReply".into(), reply.into());
        ctx.args
            .insert("spotifyQueued".into(), result.is_ok().to_string().into());
    }

    /// Clear Song Requests: forget what is kept (Spotify's own queue is not touched).
    pub(super) fn song_clear(&self, ctx: &mut Ctx) {
        self.songs.borrow_mut().clear();
        self.song_queue_changed();
        ctx.args.insert("spotifyOk".into(), "true".into());
    }

    /// Get Song Requests: how many are kept and who asked for what.
    pub(super) fn song_list(&self, ctx: &mut Ctx) {
        let songs = self.songs.borrow();
        let text = tracks_text(
            songs
                .pending()
                .enumerate()
                .map(|(i, r)| (i + 1, format!("{} ({})", r.track.full(), r.user))),
        );
        ctx.args
            .insert("spotifyRequestCount".into(), songs.pending().count().to_string().into());
        ctx.args.insert("spotifyList".into(), text.into());
    }

    /// Fire the triggers of what just happened (see `spotify::request_event`).
    fn song_events(self: &Rc<Self>, ctx: &Ctx, operation: &str, result: &Done) {
        let text = |k: &str| ctx.args.get(k).map(crate::vars::text).unwrap_or_default();
        let user = text("user");
        let track = (!text("spotifyUri").is_empty()).then(|| Track {
            uri: text("spotifyUri"),
            name: text("spotifyTitle"),
            artists: vec![text("spotifyArtist")],
            playing: false,
            duration_ms: 0,
            explicit: false,
            popularity: 0,
            restricted: false,
        });
        let t = track.as_ref();
        let fire = |kind, reason: &str, position| crate::spotify::request_event(self, kind, &user, t, reason, position);
        match (operation, result) {
            ("" | "request", Ok(())) => fire(99830, "", text("spotifyPosition").parse().unwrap_or(0)),
            ("" | "request", Err(rej)) if rej.reason != Reason::Silent => {
                let reason = match rej.reason {
                    Reason::NotAllowed => "notallowed",
                    Reason::NoQuery | Reason::NotFound | Reason::Unsupported => "notfound",
                    Reason::BlockedArtist | Reason::BlockedSong => "blocked",
                    Reason::Explicit => "explicit",
                    Reason::Unavailable => "unavailable",
                    Reason::TooLong => "toolong",
                    Reason::Duplicate => "duplicate",
                    Reason::MaxRequests => "limit",
                    _ => "error",
                };
                fire(99831, reason, 0);
            }
            ("remove", Ok(())) => fire(99832, "", 0),
            ("skip", Ok(())) => fire(99833, "command", 0),
            ("voteskip", Ok(())) => {
                fire(99834, "", 0);
                if text("spotifyVoteDone") == "true" {
                    fire(99833, "vote", 0);
                }
            }
            ("bansong", Ok(())) => fire(99835, "", 0),
            _ => {}
        }
    }

    fn song_fill(&self, ctx: &mut Ctx, t: &Track) {
        ctx.args.insert("spotifyTitle".into(), t.name.clone().into());
        ctx.args.insert("spotifyArtist".into(), t.artist().into());
        ctx.args.insert("spotifyUri".into(), t.uri.clone().into());
        let by = self.songs.borrow().requester(&t.uri).map(|r| r.user.clone());
        if let Some(user) = by {
            ctx.args.insert("spotifyRequester".into(), user.clone().into());
            ctx.args
                .insert("spotifyRequestedBy".into(), format!(" (requested by @{user})").into());
        }
    }

    async fn song_run(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) -> Done {
        let operation = s(o, "operation");
        let user = ctx.args.get("user").map(crate::vars::text).unwrap_or_default();
        let user_id = ctx
            .args
            .get("userId")
            .map(crate::vars::text)
            .filter(|i| !i.is_empty())
            .unwrap_or(user.clone());
        let lvl = level(&ctx.args);
        let who = self.song_text(ctx, o, "who");
        let (need, name) = required(&who);
        ctx.args.insert("spotifyWho".into(), name.into());
        // The viewer who asked for the song that plays can skip it, when the group lets them.
        if lvl < need && !(operation == "skip" && self.song_flag(ctx, o, "skipOwn", true) && self.song_is_own(&user_id))
        {
            return Err(reject(
                Reason::NotAllowed,
                format!("Sorry, only {name} or higher can do that."),
            ));
        }
        if list(&self.song_text(ctx, o, "blockedUsers"))
            .iter()
            .any(|b| b.trim_start_matches('@').eq_ignore_ascii_case(&user))
        {
            return Err(reject(Reason::Silent, ""));
        }
        let music = self.music;
        // One at a time: each looks at the queue, then changes it.
        let _turn = self.song_turn.lock().await;
        let failed = |e: String| reject(Reason::Error, crate::spotify::friendly(&e));
        match operation {
            "current" => {
                let player = music.player(self).await.map_err(failed)?;
                let t = player
                    .current
                    .clone()
                    .ok_or_else(|| reject(Reason::Other("msgEmpty"), "Nothing is playing."))?;
                self.songs.borrow_mut().sync(&player);
                *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
                self.song_fill(ctx, &t);
                ctx.args
                    .insert("spotifyPlaying".into(), player.playing.to_string().into());
                Ok(())
            }
            "next" => {
                let player = music.player(self).await.map_err(failed)?;
                self.songs.borrow_mut().sync(&player);
                *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
                let t = player
                    .queue
                    .first()
                    .ok_or_else(|| reject(Reason::Other("msgEmpty"), "There is no next song in the queue."))?;
                self.song_fill(ctx, t);
                Ok(())
            }
            "queue" => {
                let player = music.player(self).await.map_err(failed)?;
                let text = {
                    let mut songs = self.songs.borrow_mut();
                    songs.sync(&player);
                    *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
                    tracks_text(player.queue.iter().take(5).enumerate().map(|(i, t)| {
                        let by = songs
                            .requester(&t.uri)
                            .map(|r| format!(" (@{})", r.user))
                            .unwrap_or_default();
                        (i + 1, format!("{}{by}", t.full()))
                    }))
                };
                if text.is_empty() {
                    return Err(reject(Reason::Other("msgEmpty"), "The queue is empty."));
                }
                ctx.args.insert("spotifyList".into(), text.into());
                Ok(())
            }
            "pos" => {
                let player = music.player(self).await.map_err(failed)?;
                let text = {
                    let mut songs = self.songs.borrow_mut();
                    songs.sync(&player);
                    *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
                    tracks_text(player.queue.iter().enumerate().filter_map(|(i, t)| {
                        let r = songs.requester(&t.uri).filter(|r| r.user_id == user_id && !r.skip)?;
                        Some((i + 1, r.track.full()))
                    }))
                };
                if text.is_empty() {
                    return Err(reject(
                        Reason::Other("msgNoPos"),
                        "You have no songs in the current queue.",
                    ));
                }
                ctx.args.insert("spotifyList".into(), text.into());
                Ok(())
            }
            "remove" => self.song_remove(ctx, o, &user_id, lvl).await,
            "skip" => {
                let wait = Duration::from_millis(self.song_limit(ctx, o, "skipCooldownMs"));
                if self.song_skip_cooling(wait) {
                    return Err(reject(Reason::Cooldown, "Skipping is on cooldown."));
                }
                let player = music.player(self).await.map_err(failed)?;
                music.playback(self, "next").await.map_err(failed)?;
                self.song_skipped(ctx, player.current.as_ref());
                Ok(())
            }
            "voteskip" => {
                let wait = Duration::from_millis(self.song_limit(ctx, o, "skipCooldownMs"));
                if self.song_skip_cooling(wait) {
                    return Err(reject(Reason::Cooldown, "Skipping is on cooldown."));
                }
                let needed = self.song_limit(ctx, o, "votesNeeded").max(1) as usize;
                let player = music.player(self).await.map_err(failed)?;
                let uri = player.current.as_ref().map(|t| t.uri.clone()).unwrap_or_default();
                let votes = {
                    let mut v = self.song_votes.borrow_mut();
                    if v.0 != uri {
                        *v = (uri, Default::default());
                    }
                    if !v.1.insert(user_id.clone()) {
                        return Err(reject(Reason::Silent, ""));
                    }
                    v.1.len()
                };
                ctx.args
                    .insert("spotifyVotes".into(), format!("{votes}/{needed}").into());
                if votes >= needed {
                    music.playback(self, "next").await.map_err(failed)?;
                    self.song_skipped(ctx, player.current.as_ref());
                    ctx.args
                        .insert("spotifyVotes".into(), format!("{votes}/{needed}").into());
                    self.song_votes.borrow_mut().1.clear();
                    // The reply is the one for a finished vote.
                    ctx.args.insert("spotifyVoteDone".into(), "true".into());
                }
                Ok(())
            }
            "play" | "pause" => {
                music.playback(self, operation).await.map_err(failed)?;
                Ok(())
            }
            "volume" => {
                let player = music.player(self).await.map_err(failed)?;
                let text = self.song_text(ctx, o, "query");
                let text = text.trim().trim_end_matches('%').trim();
                if text.is_empty() {
                    let v = player
                        .volume
                        .ok_or_else(|| failed("Spotify did not say the volume".into()))?;
                    ctx.args.insert("spotifyVolume".into(), v.to_string().into());
                    ctx.args.insert("spotifyVolumeSet".into(), "false".into());
                } else {
                    let v: u32 = text
                        .parse()
                        .map_err(|_| reject(Reason::Other("msgNoVolume"), "Give a volume from 0 to 100."))?;
                    let v = v.min(100);
                    music.volume(self, v, &player.device_id).await.map_err(failed)?;
                    ctx.args.insert("spotifyVolume".into(), v.to_string().into());
                    ctx.args.insert("spotifyVolumeSet".into(), "true".into());
                }
                Ok(())
            }
            "bansong" => {
                let player = music.player(self).await.map_err(failed)?;
                let t = player
                    .current
                    .clone()
                    .ok_or_else(|| reject(Reason::Other("msgEmpty"), "Nothing is playing."))?;
                if ctx.group.is_empty() {
                    return Err(failed("The song requests need to be in a group to block a song".into()));
                }
                self.song_block(&ctx.group, &t.uri);
                music.playback(self, "next").await.map_err(failed)?;
                self.song_fill(ctx, &t);
                Ok(())
            }
            "like" => {
                let player = music.player(self).await.map_err(failed)?;
                let t = player
                    .current
                    .clone()
                    .ok_or_else(|| reject(Reason::Other("msgEmpty"), "Nothing is playing."))?;
                music.like(self, &t.uri).await.map_err(failed)?;
                crate::spotify::library_event(self, 99841, Some(&t), "");
                self.song_fill(ctx, &t);
                Ok(())
            }
            "" | "request" => self.song_request(ctx, o, &user, &user_id, lvl).await,
            other => Err(failed(format!("Unknown song request operation: {other}"))),
        }
    }

    /// After a Spotify sub-action worked: the summary of what it returned, the state it showed, and the
    /// triggers for what changed.
    pub(super) fn spotify_done(self: &Rc<Self>, ctx: &mut Ctx, kind: u32, value: &str, result: &Value) {
        let (count, text) = crate::spotify::api::summary(result);
        ctx.args.insert("spotifyText".into(), text.into());
        ctx.args.insert("spotifyCount".into(), count.into());
        let text = |v: &Value| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        match kind {
            // What the player shows.
            99823 => {
                let device = &result["device"];
                let shown = [
                    (99836, "volume", text(&device["volume_percent"])),
                    (99837, "shuffle", text(&result["shuffle_state"])),
                    (99838, "repeat", text(&result["repeat_state"])),
                    (99839, "device", text(&device["name"])),
                ];
                for (code, name, v) in shown {
                    ctx.args.insert(
                        format!("spotify{}{}", name[..1].to_uppercase(), &name[1..]),
                        v.clone().into(),
                    );
                    self.spotify_note(code, v);
                }
                ctx.args
                    .insert("spotifyPlaying".into(), text(&result["is_playing"]).into());
            }
            // What was just set: no need to ask Spotify again.
            99818 => self.spotify_note(99836, value.trim().to_string()),
            99843 => self.spotify_note(99836, text(result)),
            99821 => self.spotify_note(99837, value.trim().to_string()),
            99822 => self.spotify_note(99838, value.trim().to_string()),
            99862..=99864 | 99866 | 99861 => {
                let playlist = crate::spotify::api::unpack_first(value);
                crate::spotify::library_event(self, 99842, None, &playlist);
            }
            _ => {}
        }
    }

    /// Fire the trigger of a player setting when its value is not the one seen before. The first value
    /// seen is not a change.
    fn spotify_note(self: &Rc<Self>, code: u32, value: String) {
        let previous = self.spotify_seen.borrow_mut().insert(code, value.clone());
        if let Some(previous) = previous.filter(|p| *p != value) {
            crate::spotify::change_event(self, code, &value, &previous);
        }
    }

    /// Whether the song that plays now was asked for by this viewer.
    fn song_is_own(&self, user_id: &str) -> bool {
        // Only what is already known: no call to Spotify just to see.
        self.songs.borrow().pending().any(|r| {
            r.user_id == user_id && !r.skip && self.song_playing.borrow().as_deref() == Some(r.track.uri.as_str())
        })
    }

    fn song_skip_cooling(&self, wait: Duration) -> bool {
        self.song_last_skip.get().is_some_and(|at| at.elapsed() < wait)
    }

    fn song_skipped(&self, ctx: &mut Ctx, was: Option<&Track>) {
        self.song_last_skip.set(Some(Instant::now()));
        self.song_votes.borrow_mut().1.clear();
        if let Some(t) = was {
            self.song_fill(ctx, t);
        }
    }

    /// Add a song to the group's blocked songs.
    fn song_block(self: &Rc<Self>, group: &str, uri: &str) {
        self.change(|c| {
            let mut values = crate::groups::values(c, group);
            let mut blocked = list(values.get("blockedSongs").and_then(Value::as_str).unwrap_or(""));
            if !blocked.iter().any(|b| b == uri) {
                blocked.push(uri.to_string());
            }
            values.insert("blockedSongs".into(), blocked.join(", ").into());
            c.action_group_variables.insert(group.into(), Value::Object(values));
        });
    }

    async fn song_remove(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, user_id: &str, lvl: u8) -> Done {
        let music = self.music;
        let failed = |e: String| reject(Reason::Error, crate::spotify::friendly(&e));
        let player = music.player(self).await.map_err(failed)?;
        let target = self.song_text(ctx, o, "query");
        let target = target.trim();
        let taken = {
            let mut songs = self.songs.borrow_mut();
            songs.sync(&player);
            *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
            // Moderators can take back someone else's: by the number in the queue, or by name.
            if lvl >= 3 && !target.is_empty() {
                match target.parse::<usize>() {
                    Ok(n) => songs.take_back_number(n),
                    Err(_) => songs.take_back_of_name(target.trim_start_matches('@')),
                }
            } else {
                songs.take_back_last(user_id)
            }
        };
        let r = taken.ok_or_else(|| reject(Reason::Other("msgNoRemove"), "You have no songs in the queue."))?;
        self.song_fill(ctx, &r.track);
        // It plays right now: skip it.
        if player.current.as_ref().is_some_and(|c| c.uri == r.track.uri) {
            music.playback(self, "next").await.map_err(failed)?;
            self.songs.borrow_mut().take_skip(&r.track.uri);
        } else {
            self.song_watch_start();
        }
        Ok(())
    }

    async fn song_request(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, user: &str, user_id: &str, lvl: u8) -> Done {
        let music = self.music;
        let failed = |e: String| reject(Reason::Error, crate::spotify::friendly(&e));
        let query = self.song_text(ctx, o, "query");
        if query.trim().is_empty() {
            return Err(reject(Reason::NoQuery, "Please specify a song to add to the queue."));
        }
        // The limits of this viewer's role.
        let (explicit_level, _) = required(&self.song_text(ctx, o, "explicitWho"));
        let (unlimited_level, _) = required(&self.song_text(ctx, o, "unlimitedWho"));
        let mut max = self.song_limit(ctx, o, "maxPerUser");
        if lvl >= 1 {
            let subs = self.song_limit(ctx, o, "maxPerSubscriber");
            if subs > 0 {
                max = subs;
            }
        }
        if lvl >= 4 || lvl >= unlimited_level {
            max = 0;
        }
        let max_length_ms = self.song_limit(ctx, o, "maxLengthMs");
        let rules = Rules {
            max_per_user: max,
            max_length_ms,
            block_explicit: self.song_flag(ctx, o, "blockExplicit", false) && lvl < 4 && lvl < explicit_level,
            blocked_artists: list(&self.song_text(ctx, o, "blockedArtists")),
            blocked_songs: list(&self.song_text(ctx, o, "blockedSongs"))
                .iter()
                .filter_map(|u| crate::spotify::track_uri_of(u))
                .collect(),
        };
        ctx.args.insert("spotifyMaxLength".into(), clock(max_length_ms).into());
        ctx.args.insert("spotifyMaxRequests".into(), max.to_string().into());
        let track = match music.find(self, &query).await {
            Ok(t) => t,
            Err(e) if e.contains("No Spotify track found") => return Err(reject(Reason::NotFound, "No track found.")),
            Err(e) if e.contains("not supported") => return Err(reject(Reason::Unsupported, e)),
            Err(e) => return Err(failed(e)),
        };
        self.song_fill(ctx, &track);
        let player = music.player(self).await.map_err(failed)?;
        {
            let mut songs = self.songs.borrow_mut();
            songs.sync(&player);
            *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
            songs.check(&rules, user_id, &track, &player)?;
        }
        music.add(self, &track.uri, &player.device_id).await.map_err(failed)?;
        self.songs.borrow_mut().add(Request {
            user_id: user_id.into(),
            user: user.into(),
            track: track.clone(),
            skip: false,
        });
        // Where it is now, and when it plays: the queue as Spotify has it, once Spotify shows it.
        let mut after = player;
        for attempt in 0..4 {
            if attempt > 0 {
                tokio::time::sleep(self.song_pace.get().unwrap_or(Duration::from_millis(400))).await;
            }
            if let Ok(p) = music.player(self).await {
                let shown = p.queue.iter().any(|t| t.uri == track.uri);
                after = p;
                if shown {
                    break;
                }
            }
        }
        let (position, time) = place(&after, &track, self.songs.borrow().pending().count());
        ctx.args.insert("spotifyPosition".into(), position.to_string().into());
        ctx.args
            .insert("spotifyTimeToPlay".into(), minutes_seconds(time).into());
        ctx.args.insert("spotifyRequester".into(), user.to_string().into());
        ctx.args
            .insert("spotifyRequestedBy".into(), format!(" (requested by @{user})").into());
        Ok(())
    }

    /// Watch the player while a song that was taken back waits in the queue: when it starts, skip it.
    fn song_watch_start(self: &Rc<Self>) {
        if self.song_watching.replace(true) {
            return;
        }
        let me = self.clone();
        tokio::task::spawn_local(async move {
            me.song_watch().await;
            me.song_watching.set(false);
        });
    }

    async fn song_watch(self: &Rc<Self>) {
        let mut failures = 0;
        loop {
            let pause = {
                let _turn = self.song_turn.lock().await;
                if !self.songs.borrow().has_skips() {
                    return;
                }
                match self.music.player(self).await {
                    Err(e) => {
                        failures += 1;
                        crate::warn(format!("Spotify: {e}"));
                        if failures >= 5 || e.contains("disabled") || e.contains("login is required") {
                            self.songs.borrow_mut().clear();
                            self.song_queue_changed();
                            return;
                        }
                        WATCH_MAX
                    }
                    Ok(player) => {
                        failures = 0;
                        let due = {
                            let mut songs = self.songs.borrow_mut();
                            let due = player.current.as_ref().is_some_and(|t| songs.take_skip(&t.uri));
                            songs.sync(&player);
                            *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
                            due
                        };
                        self.song_queue_changed();
                        if due {
                            if let Err(e) = self.music.playback(self, "next").await {
                                crate::warn(format!("Spotify: {e}"));
                            }
                        }
                        if !self.songs.borrow().has_skips() {
                            return;
                        }
                        match (&player.current, player.playing) {
                            (Some(t), true) => {
                                Duration::from_millis(t.duration_ms.saturating_sub(player.progress_ms) + 400)
                                    .clamp(Duration::from_secs(1), WATCH_MAX)
                            }
                            _ => WATCH_MAX,
                        }
                    }
                }
            };
            tokio::time::sleep(self.song_pace.get().unwrap_or(pause)).await;
        }
    }

    /// Upcoming requests, without the song playing now or a request taken back. No cached copy.
    pub fn song_queue_value(&self) -> Value {
        let playing = self.song_playing.borrow();
        Value::Array(
            self.songs
                .borrow()
                .pending()
                .filter(|r| !r.skip && playing.as_deref() != Some(r.track.uri.as_str()))
                .map(|r| {
                    serde_json::json!({"user": r.user, "title": r.track.name,
                "artist": r.track.artist(), "uri": r.track.uri})
                })
                .collect(),
        )
    }

    fn song_queue_changed(&self) {
        self.emit("Overlay", "songrequests.queue", || self.song_queue_value());
    }

    /// The existing local media watcher calls this only on a track change. Spotify remains opt-in;
    /// no listener, no requests or a disabled integration means no API request and no timer.
    pub async fn song_queue_refresh(self: &Rc<Self>) {
        if !self.listening("Overlay", "songrequests.queue")
            || !self.config().spotify.enabled
            || self.songs.borrow().pending().next().is_none()
        {
            return;
        }
        let _turn = self.song_turn.lock().await;
        if let Ok(player) = self.music.player(self).await {
            self.songs.borrow_mut().sync(&player);
            *self.song_playing.borrow_mut() = player.current.as_ref().map(|t| t.uri.clone());
            self.song_queue_changed();
        }
    }

    /// The song requests, in play order, for the MCP and WebSocket server.
    pub fn song_requests(&self) -> Vec<Value> {
        self.songs
            .borrow()
            .pending()
            .map(|r| {
                serde_json::json!({"user": r.user, "title": r.track.name, "artist": r.track.artist(),
                    "uri": r.track.uri, "skip": r.skip})
            })
            .collect()
    }

    /// Tests: how long the skip watcher waits between looks.
    #[cfg(test)]
    pub(crate) fn set_song_pace(&self, pause: Duration) {
        self.song_pace.set(Some(pause));
    }
}

/// Where a track is in the player's queue (1 is next), and how long until it plays. When Spotify does not
/// show it yet, it is last of the requests known.
fn place(player: &Player, t: &Track, requests: usize) -> (usize, u64) {
    let shown = player.queue.iter().position(|q| q.uri == t.uri);
    let index = shown.unwrap_or(requests.saturating_sub(1).min(player.queue.len()));
    let left = player
        .current
        .as_ref()
        .map_or(0, |c| c.duration_ms.saturating_sub(player.progress_ms));
    let before: u64 = player.queue.iter().take(index).map(|q| q.duration_ms).sum();
    (index + 1, left + before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::FakeMusic;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::watch;

    fn engine(music: &'static FakeMusic) -> Rc<Engine> {
        Engine::with_music(Arc::new(watch::channel(crate::Config::default()).0), |_| {}, music)
    }

    fn ctx(user: &str, args: Value) -> Ctx {
        let mut a = json!({"user": user, "userId": user}).as_object().unwrap().clone();
        a.extend(args.as_object().cloned().unwrap_or_default());
        Ctx {
            args: a,
            group: String::new(),
        }
    }

    fn op(fields: Value) -> Obj {
        let mut o = json!({"type": 99802}).as_object().unwrap().clone();
        o.extend(fields.as_object().unwrap().clone());
        o
    }

    async fn go(e: &Rc<Engine>, c: &mut Ctx, o: Value) -> String {
        e.one(c, &op(o)).await;
        c.args["spotifyQueued"].as_str().unwrap().to_string()
    }

    fn arg(c: &Ctx, k: &str) -> String {
        c.args[k].as_str().unwrap().to_string()
    }

    fn leak() -> &'static FakeMusic {
        Box::leak(Box::default())
    }

    #[test]
    fn queue_topic_tracks_requests_removal_playback_and_clear() {
        run(async {
            let music = leak();
            let e = engine(music);
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subs = super::super::events::Subs::new();
            subs.entry("Overlay".into())
                .or_default()
                .insert("songrequests.queue".into());
            e.listen(44, &subs, &tx);
            assert_eq!(e.overlay_value("songrequests.queue"), Some(json!([])));
            let mut a = ctx("ana", json!({}));
            assert_eq!(go(&e, &mut a, json!({"query": "one"})).await, "true");
            assert_eq!(e.overlay_value("songrequests.queue").unwrap()[0]["user"], "ana");
            let event: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(event["event"]["type"], "songrequests.queue");
            assert_eq!(event["data"][0]["title"], "one");
            assert_eq!(go(&e, &mut a, json!({"query": "two"})).await, "true");
            // Disabled Spotify does not touch the player, even with a page and pending requests.
            *music.track.borrow_mut() = Some(FakeMusic::track("one"));
            e.song_queue_refresh().await;
            assert_eq!(e.song_queue_value().as_array().unwrap().len(), 2);
            e.change(|c| c.spotify.enabled = true);
            e.song_queue_refresh().await;
            assert_eq!(e.song_queue_value().as_array().unwrap().len(), 1);
            assert_eq!(e.song_queue_value()[0]["title"], "two");
            // A page closing stops refreshes; reopening gets the current snapshot.
            e.listen(44, &Default::default(), &tx);
            *music.track.borrow_mut() = Some(FakeMusic::track("two"));
            e.song_queue_refresh().await;
            assert_eq!(e.song_queue_value()[0]["title"], "two");
            e.listen(44, &subs, &tx);
            e.song_queue_refresh().await;
            assert_eq!(e.song_queue_value()[0]["title"], "one");
            // Taking a pending request back excludes it immediately, before the skip watcher runs.
            e.songs.borrow_mut().take_back_number(1);
            e.song_queue_changed();
            assert_eq!(e.song_queue_value(), json!([]));
            e.song_clear(&mut a);
            assert_eq!(e.overlay_value("songrequests.queue"), Some(json!([])));
        });
    }

    #[test]
    fn player_action_expands_values_and_returns_port_result() {
        run(async {
            let music = leak();
            let e = engine(music);
            let mut context = ctx("viewer", json!({"position": 42000}));
            let action = json!({"type": 99817, "value": "%position%"});
            e.one(&mut context, action.as_object().unwrap()).await;
            assert_eq!(music.commands.borrow().as_slice(), ["99817:42000"]);
            assert_eq!(context.args["spotifyResult"]["value"], "42000");
            assert_eq!(context.args["spotifyError"], "");
        });
    }

    fn run<F: std::future::Future<Output = ()>>(f: F) {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, f);
    }

    #[test]
    fn a_request_goes_to_spotify_at_once_with_its_place_and_when_it_plays() {
        let music = leak();
        let mut playing = FakeMusic::track("playing");
        playing.duration_ms = 200_000;
        *music.track.borrow_mut() = Some(playing);
        music.progress.set(170_000);
        run(async {
            let e = engine(music);
            let mut a = ctx("ana", json!({"rawInput": "firework"}));
            let fields = json!({"query": "%rawInput%", "reply": "%spotifyArtist% - %spotifyTitle% for @%user%: #%spotifyPosition% in %spotifyTimeToPlay%"});
            assert_eq!(go(&e, &mut a, fields.clone()).await, "true");
            assert_eq!(*music.queued.borrow(), ["firework"], "with Spotify at once");
            assert_eq!(arg(&a, "spotifyReply"), "Fake Artist - firework for @ana: #1 in 0m 30s");
            let mut b = ctx("bia", json!({"rawInput": "roar"}));
            assert_eq!(go(&e, &mut b, fields).await, "true");
            assert_eq!(arg(&b, "spotifyReply"), "Fake Artist - roar for @bia: #2 in 3m 30s");
            assert_eq!(*music.queued.borrow(), ["firework", "roar"]);
            assert_eq!(e.song_requests().len(), 2);
        });
    }

    #[test]
    fn what_goes_wrong_is_said_with_the_groups_reply_or_the_default() {
        let music = leak();
        run(async {
            let e = engine(music);
            let mut a = ctx("ana", json!({"rawInput": "missing"}));
            let f = json!({"query": "%rawInput%", "reply": "ok", "msgNotFound": "nothing like %rawInput% for @%user%"});
            assert_eq!(go(&e, &mut a, f).await, "false");
            assert_eq!(arg(&a, "spotifyReply"), "nothing like missing for @ana");
            assert_eq!(arg(&a, "spotifyError"), "No track found.");
            let mut b = ctx("bia", json!({"rawInput": "missing"}));
            go(&e, &mut b, json!({"query": "%rawInput%"})).await;
            assert_eq!(arg(&b, "spotifyReply"), "No track found.", "no setting: what it says");
            assert_eq!(arg(&b, "spotifyTitle"), "");
            let mut c = ctx("cy", json!({}));
            go(&e, &mut c, json!({"query": "  "})).await;
            assert_eq!(arg(&c, "spotifyReply"), "Please specify a song to add to the queue.");
            // Spotify refuses: its own reason, and the group's reply for it.
            music.fail_add.set(true);
            let mut d = ctx("dee", json!({}));
            let f = json!({"query": "one", "msgError": "@%user% Spotify: %spotifyError%"});
            assert_eq!(go(&e, &mut d, f).await, "false");
            assert!(arg(&d, "spotifyReply").starts_with("@dee Spotify: Spotify has no active device"));
            assert!(
                e.song_requests().is_empty(),
                "a request Spotify did not take is not remembered"
            );
        });
    }

    #[test]
    fn the_groups_rules_are_enforced_as_songify_does() {
        let music = leak();
        music.explicit.borrow_mut().push("dirty".into());
        music.restricted.borrow_mut().push("abroad".into());
        run(async {
            let e = engine(music);
            let rules = |q: &str| {
                json!({"query": q, "maxPerUser": "2", "maxPerSubscriber": "3", "maxLengthMs": "300000", "blockExplicit": "True",
                    "explicitWho": "moderators", "unlimitedWho": "nobody", "blockedArtists": "Nobody Known",
                    "blockedUsers": "@troll, spam", "reply": "ok"})
            };
            let mut a = ctx("ana", json!({}));
            for (query, why) in [
                ("dirty", "explicit"),
                ("long9", "longer than the 5:00"),
                ("abroad", "not available"),
            ] {
                assert_eq!(go(&e, &mut a, rules(query)).await, "false", "{query}");
                assert!(
                    arg(&a, "spotifyReply").contains(why),
                    "{query}: {}",
                    arg(&a, "spotifyReply")
                );
            }
            // A moderator can request explicit songs.
            let mut m = ctx("mo", json!({"isModerator": true}));
            assert_eq!(go(&e, &mut m, rules("dirty")).await, "true");
            // A blocked artist, and a blocked song (typed as a link).
            let blocked = json!({"query": "x", "blockedArtists": "fake artist"});
            assert_eq!(go(&e, &mut a, blocked).await, "false");
            assert!(arg(&a, "spotifyReply").contains("blocked"));
            let song = json!({"query": "0123456789ABCDEFGHIJKL", "blockedSongs": "https://open.spotify.com/track/0123456789ABCDEFGHIJKL"});
            assert_eq!(go(&e, &mut a, song).await, "false");
            // Duplicates, then the limit of two for a viewer.
            assert_eq!(go(&e, &mut a, rules("one")).await, "true");
            assert_eq!(go(&e, &mut a, rules("one")).await, "false");
            assert!(arg(&a, "spotifyReply").contains("already in the queue"));
            assert_eq!(go(&e, &mut a, rules("two")).await, "true");
            assert_eq!(go(&e, &mut a, rules("three")).await, "false");
            assert!(
                arg(&a, "spotifyReply").contains("maximum number of songs in queue reached (2)")
                    || arg(&a, "spotifyReply").contains("(2)")
            );
            // A subscriber is allowed three, a moderator is not limited when the group says so.
            let mut s = ctx("sue", json!({"isSubscribed": true}));
            for q in ["s1", "s2", "s3"] {
                assert_eq!(go(&e, &mut s, rules(q)).await, "true", "{q}");
            }
            assert_eq!(go(&e, &mut s, rules("s4")).await, "false");
            let mut mo = ctx("mo", json!({"isModerator": true}));
            let free = json!({"query": "m2", "maxPerUser": "1", "unlimitedWho": "moderators"});
            assert_eq!(go(&e, &mut mo, free).await, "true");
            // A blocked viewer gets no answer at all.
            let mut t = ctx("troll", json!({}));
            assert_eq!(go(&e, &mut t, rules("fine")).await, "false");
            assert_eq!(arg(&t, "spotifyReply"), "");
        });
    }

    #[test]
    fn who_can_request_and_skip_and_a_viewer_skips_their_own_song() {
        let music = leak();
        run(async {
            let e = engine(music);
            let mut viewer = ctx("ana", json!({}));
            let subs = json!({"query": "one", "who": "subscribers"});
            assert_eq!(go(&e, &mut viewer, subs.clone()).await, "false");
            assert_eq!(
                arg(&viewer, "spotifyReply"),
                "Sorry, only subscribers or higher can do that."
            );
            let mut sub = ctx("bia", json!({"isSubscribed": true}));
            assert_eq!(go(&e, &mut sub, subs).await, "true");
            let skip = json!({"operation": "skip", "who": "moderators", "skipOwn": "true", "reply": "skipped"});
            assert_eq!(go(&e, &mut viewer, skip.clone()).await, "false");
            assert!(music.commands.borrow().is_empty());
            // bia's song starts: she can skip it, ana still cannot.
            music.queued.borrow_mut().clear();
            *music.track.borrow_mut() = Some(FakeMusic::track("one"));
            *e.song_playing.borrow_mut() = Some("spotify:track:one".into());
            assert_eq!(go(&e, &mut viewer, skip.clone()).await, "false");
            assert_eq!(go(&e, &mut sub, skip.clone()).await, "true");
            assert_eq!(*music.commands.borrow(), ["next"]);
            let mut modr = ctx("mo", json!({"isModerator": true}));
            let slow = json!({"operation": "skip", "who": "moderators", "skipCooldownMs": "60000", "reply": "skipped"});
            assert_eq!(go(&e, &mut modr, slow).await, "false", "the cooldown after a skip");
            assert_eq!(*music.commands.borrow(), ["next"]);
            assert_eq!(go(&e, &mut modr, skip).await, "true");
        });
    }

    #[test]
    fn a_taken_back_request_is_skipped_when_it_plays() {
        let music = leak();
        run(async {
            let e = engine(music);
            e.set_song_pace(Duration::from_millis(20));
            let mut a = ctx("ana", json!({}));
            go(&e, &mut a, json!({"query": "one"})).await;
            let mut b = ctx("bia", json!({}));
            go(&e, &mut b, json!({"query": "two"})).await;
            let rm = json!({"operation": "remove", "query": "", "reply": "%spotifyTitle% will be skipped", "msgNoRemove": "none"});
            assert_eq!(go(&e, &mut a, rm.clone()).await, "true");
            assert_eq!(arg(&a, "spotifyReply"), "one will be skipped");
            assert_eq!(go(&e, &mut a, rm.clone()).await, "false");
            assert_eq!(arg(&a, "spotifyReply"), "none");
            assert!(
                music.commands.borrow().is_empty(),
                "not playing yet: nothing is skipped"
            );
            assert_eq!(*music.queued.borrow(), ["one", "two"], "Spotify cannot give it back");
            // It plays: the watcher skips it.
            music.queued.borrow_mut().remove(0);
            *music.track.borrow_mut() = Some(FakeMusic::track("one"));
            tokio::time::sleep(Duration::from_millis(120)).await;
            assert_eq!(*music.commands.borrow(), ["next"]);
            assert!(!e.song_watching.get(), "nothing left to watch");
            // A viewer can ask again now.
            assert_eq!(
                go(&e, &mut a, json!({"query": "three", "maxPerUser": "1"})).await,
                "true"
            );
            // Taking back the song that plays skips it at once.
            music.commands.borrow_mut().clear();
            music.queued.borrow_mut().clear();
            *music.track.borrow_mut() = Some(FakeMusic::track("three"));
            assert_eq!(go(&e, &mut a, rm).await, "true");
            assert_eq!(*music.commands.borrow(), ["next"]);
        });
    }

    #[test]
    fn moderators_take_back_other_requests_by_name_or_number() {
        let music = leak();
        run(async {
            let e = engine(music);
            let (mut a, mut b) = (ctx("ana", json!({})), ctx("bia", json!({})));
            go(&e, &mut a, json!({"query": "one"})).await;
            go(&e, &mut b, json!({"query": "two"})).await;
            let mut modr = ctx("mo", json!({"isModerator": true}));
            let by_name = json!({"operation": "remove", "query": "@Ana", "reply": "%spotifyTitle%"});
            assert_eq!(go(&e, &mut modr, by_name).await, "true");
            assert_eq!(arg(&modr, "spotifyReply"), "one");
            let mut viewer = ctx("cy", json!({}));
            let denied = json!({"operation": "remove", "query": "1", "reply": "x", "msgNoRemove": "nothing of yours"});
            assert_eq!(
                go(&e, &mut viewer, denied).await,
                "false",
                "a viewer only takes back their own"
            );
            let by_number = json!({"operation": "remove", "query": "2", "reply": "%spotifyTitle%"});
            assert_eq!(go(&e, &mut modr, by_number).await, "true");
            assert_eq!(arg(&modr, "spotifyReply"), "two");
        });
    }

    #[test]
    fn a_vote_skip_needs_enough_different_viewers() {
        let music = leak();
        *music.track.borrow_mut() = Some(FakeMusic::track("playing"));
        run(async {
            let e = engine(music);
            let vote = json!({"operation": "voteskip", "votesNeeded": "2", "reply": "voted (%spotifyVotes%)", "replyDone": "skipping"});
            let mut a = ctx("ana", json!({}));
            assert_eq!(go(&e, &mut a, vote.clone()).await, "true");
            assert_eq!(arg(&a, "spotifyReply"), "voted (1/2)");
            assert_eq!(
                go(&e, &mut a, vote.clone()).await,
                "false",
                "the same viewer cannot vote twice"
            );
            assert_eq!(arg(&a, "spotifyReply"), "");
            let mut b = ctx("bia", json!({}));
            assert_eq!(go(&e, &mut b, vote).await, "true");
            assert_eq!(arg(&b, "spotifyReply"), "skipping");
            assert_eq!(*music.commands.borrow(), ["next"]);
        });
    }

    #[test]
    fn the_other_commands_report_what_is_there() {
        let music = leak();
        run(async {
            let e = engine(music);
            let mut c = ctx("ana", json!({}));
            let empty = json!({"operation": "current", "reply": "x", "msgEmpty": "empty: %spotifyError%"});
            assert_eq!(go(&e, &mut c, empty).await, "false");
            assert_eq!(arg(&c, "spotifyReply"), "empty: Nothing is playing.");
            *music.track.borrow_mut() = Some(FakeMusic::track("playing"));
            let now = json!({"operation": "current", "reply": "%spotifyArtist% - %spotifyTitle%%spotifyRequestedBy% %spotifyPlaying%"});
            assert_eq!(go(&e, &mut c, now).await, "true");
            assert_eq!(arg(&c, "spotifyReply"), "Fake Artist - playing true");
            let queue = json!({"operation": "queue", "reply": "%spotifyList%", "msgEmpty": "nothing queued"});
            assert_eq!(go(&e, &mut c, queue.clone()).await, "false");
            assert_eq!(arg(&c, "spotifyReply"), "nothing queued");
            let next = json!({"operation": "next", "reply": "next: %spotifyTitle%", "msgEmpty": "no next"});
            assert_eq!(go(&e, &mut c, next.clone()).await, "false");
            let mut b = ctx("bia", json!({}));
            go(&e, &mut b, json!({"query": "one"})).await;
            go(&e, &mut c, json!({"query": "two"})).await;
            go(&e, &mut b, json!({"query": "three"})).await;
            assert_eq!(go(&e, &mut c, queue).await, "true");
            assert_eq!(
                arg(&c, "spotifyReply"),
                "#1 Fake Artist - one (@bia) | #2 Fake Artist - two (@ana) | #3 Fake Artist - three (@bia)"
            );
            assert_eq!(go(&e, &mut c, next).await, "true");
            assert_eq!(arg(&c, "spotifyReply"), "next: one");
            let now = json!({"operation": "current", "reply": "%spotifyTitle%%spotifyRequestedBy%"});
            *music.track.borrow_mut() = Some(FakeMusic::track("two"));
            assert_eq!(go(&e, &mut c, now).await, "true");
            assert_eq!(arg(&c, "spotifyReply"), "two (requested by @ana)");
            let pos = json!({"operation": "pos", "reply": "@%user% %spotifyList%", "msgNoPos": "none for @%user%"});
            assert_eq!(go(&e, &mut b, pos.clone()).await, "true");
            assert_eq!(
                arg(&b, "spotifyReply"),
                "@bia #1 Fake Artist - one | #3 Fake Artist - three"
            );
            let mut nobody = ctx("cy", json!({}));
            assert_eq!(go(&e, &mut nobody, pos).await, "false");
            assert_eq!(arg(&nobody, "spotifyReply"), "none for @cy");
        });
    }

    #[test]
    fn playback_volume_ban_and_like() {
        let music = leak();
        *music.track.borrow_mut() = Some(FakeMusic::track("playing"));
        run(async {
            let e = engine(music);
            e.change(|c| {
                c.action_group_variables
                    .insert("Song requests".into(), json!({"blockedSongs": "spotify:track:old"}));
            });
            let mut modr = ctx("mo", json!({"isModerator": true}));
            modr.group = "Song requests".into();
            let mut viewer = ctx("ana", json!({}));
            let control = |op: &str| json!({"operation": op, "who": "moderators", "reply": op});
            assert_eq!(go(&e, &mut viewer, control("pause")).await, "false");
            assert_eq!(go(&e, &mut modr, control("pause")).await, "true");
            assert_eq!(go(&e, &mut modr, control("play")).await, "true");
            assert_eq!(*music.commands.borrow(), ["pause", "play"]);
            // The volume: set when a number is given, told when not.
            music.volume.set(40);
            let vol = json!({"operation": "volume", "query": "%rawInput%", "reply": "at %spotifyVolume%%", "replyDone": "set to %spotifyVolume%%"});
            modr.args.insert("rawInput".into(), "".into());
            assert_eq!(go(&e, &mut modr, vol.clone()).await, "true");
            assert_eq!(arg(&modr, "spotifyReply"), "at 40%");
            modr.args.insert("rawInput".into(), "65%".into());
            assert_eq!(go(&e, &mut modr, vol.clone()).await, "true");
            assert_eq!(
                (arg(&modr, "spotifyReply"), music.volume.get()),
                ("set to 65%".into(), 65)
            );
            modr.args.insert("rawInput".into(), "300".into());
            go(&e, &mut modr, vol.clone()).await;
            assert_eq!(music.volume.get(), 100, "more than 100 is 100");
            modr.args.insert("rawInput".into(), "loud".into());
            assert_eq!(go(&e, &mut modr, vol).await, "false");
            // Like saves the song that plays; ban blocks it for good and skips it.
            assert_eq!(go(&e, &mut modr, control("like")).await, "true");
            assert_eq!(*music.liked.borrow(), ["spotify:track:playing"]);
            music.commands.borrow_mut().clear();
            assert_eq!(go(&e, &mut modr, control("bansong")).await, "true");
            assert_eq!(*music.commands.borrow(), ["next"]);
            let blocked = e.config().action_group_variables["Song requests"]["blockedSongs"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(blocked, "spotify:track:old, spotify:track:playing");
        });
    }

    #[test]
    fn requests_fire_their_triggers_and_a_missing_login_says_what_it_limits() {
        let music = leak();
        run(async {
            let mut config = crate::Config::default();
            for (i, code) in [99830u64, 99831, 99832, 99833, 99835].iter().enumerate() {
                let mut a = crate::edit::new_action(&format!("on {code}"), "");
                a.id = format!("a{i}");
                a.triggers = vec![json!({"type": code, "id": "t", "enabled": true})
                    .as_object()
                    .unwrap()
                    .clone()];
                config.actions.actions.push(a);
            }
            let e = Engine::with_music(Arc::new(watch::channel(config).0), |_| {}, music);
            let mut a = ctx("ana", json!({}));
            go(&e, &mut a, json!({"query": "one"})).await;
            go(&e, &mut a, json!({"query": "missing"})).await;
            go(&e, &mut a, json!({"operation": "remove"})).await;
            *music.track.borrow_mut() = Some(FakeMusic::track("playing"));
            let mut m = ctx("mo", json!({"isModerator": true}));
            m.group = "g".into();
            go(&e, &mut m, json!({"operation": "skip", "who": "moderators"})).await;
            go(&e, &mut m, json!({"operation": "bansong"})).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(e.history().len(), 5, "requested, refused, taken back, skipped, blocked");
        });
        assert!(crate::spotify::friendly("Spotify is disabled").contains("Integrations"));
        assert_eq!(crate::spotify::friendly("other"), "other");
    }

    #[test]
    fn the_wider_api_carries_two_values_and_says_what_changed() {
        let music = leak();
        run(async {
            let mut config = crate::Config::default();
            for (i, code) in [99836u64, 99842].iter().enumerate() {
                let mut a = crate::edit::new_action(&format!("on {code}"), "");
                a.id = format!("a{i}");
                a.triggers = vec![json!({"type": code, "id": "t", "enabled": true})
                    .as_object()
                    .unwrap()
                    .clone()];
                config.actions.actions.push(a);
            }
            let e = Engine::with_music(Arc::new(watch::channel(config).0), |_| {}, music);
            let mut c = ctx("ana", json!({}));
            let run_one = |kind: u32, value: &str, extra: &str| {
                json!({"type": kind, "value": value, "extra": extra})
                    .as_object()
                    .unwrap()
                    .clone()
            };
            // The first volume seen is not a change; the next different one is.
            e.one(&mut c, &run_one(99818, "40", "")).await;
            e.one(&mut c, &run_one(99818, "40", "")).await;
            e.one(&mut c, &run_one(99818, "65", "")).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(e.history().len(), 1);
            // Two values reach the port as one text, and a playlist action says the playlist changed.
            e.one(&mut c, &run_one(99863, "playlist", "fireworks")).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(music.commands.borrow().last().unwrap(), "99863:playlist\nfireworks");
            assert_eq!(e.history().len(), 2);
            assert_eq!(arg(&c, "spotifyError"), "");
            assert_eq!(c.args["spotifyCount"], 0);
        });
    }
}
