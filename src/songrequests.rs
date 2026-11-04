//! The song requests of a stream, as Songify keeps them: a request goes to Spotify's queue at once and
//! is remembered here with who asked, so the limits, `!pos`, `!remove` and "requested by" can work.
//! Spotify cannot take a song out of its queue, so taking one back marks it to be skipped when it plays.
use crate::spotify::{Player, Track};

#[derive(Clone, Debug)]
pub struct Request {
    pub user_id: String,
    pub user: String,
    pub track: Track,
    /// Taken back: skipped when it starts playing.
    pub skip: bool,
}

/// Why a request or command was not carried out; each has a reply the group can change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Nothing is said (a blocked viewer).
    Silent,
    NotAllowed,
    NoQuery,
    NotFound,
    Unsupported,
    BlockedArtist,
    BlockedSong,
    Explicit,
    Unavailable,
    TooLong,
    Duplicate,
    MaxRequests,
    Cooldown,
    /// Spotify refused.
    Error,
    /// Another reply of the group, by the name of its setting (`msgNoPos`).
    Other(&'static str),
}

impl Reason {
    /// The setting holding its reply.
    pub fn setting(&self) -> Option<&'static str> {
        Some(match self {
            Reason::Silent => return None,
            Reason::NotAllowed => "msgNotAllowed",
            Reason::NoQuery => "msgNoQuery",
            Reason::NotFound => "msgNotFound",
            Reason::Unsupported => "msgUnsupported",
            Reason::BlockedArtist => "msgBlockedArtist",
            Reason::BlockedSong => "msgBlockedSong",
            Reason::Explicit => "msgExplicit",
            Reason::Unavailable => "msgUnavailable",
            Reason::TooLong => "msgTooLong",
            Reason::Duplicate => "msgDuplicate",
            Reason::MaxRequests => "msgMaxRequests",
            Reason::Cooldown => "msgSkipCooldown",
            Reason::Error => "msgError",
            Reason::Other(s) => s,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Rejection {
    pub reason: Reason,
    /// What it says without the group's reply.
    pub text: String,
}

pub fn reject(reason: Reason, text: impl Into<String>) -> Rejection {
    Rejection {
        reason,
        text: text.into(),
    }
}

/// What a request has to meet. A limit of 0 is no limit.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    /// Songs this viewer can have in the queue (already for their role).
    pub max_per_user: u64,
    pub max_length_ms: u64,
    pub block_explicit: bool,
    pub blocked_artists: Vec<String>,
    /// Track URIs.
    pub blocked_songs: Vec<String>,
}

#[derive(Default)]
pub struct Requests {
    /// In the order they were asked: those Spotify still has queued, and the one that plays.
    list: Vec<Request>,
}

/// "3:05".
pub fn clock(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, ms / 1_000 % 60)
}

impl Requests {
    /// Forget the requests that have played: the player no longer has them queued or playing.
    pub fn sync(&mut self, player: &Player) {
        self.list.retain(|r| {
            player.queue.iter().any(|t| t.uri == r.track.uri)
                || player.current.as_ref().is_some_and(|c| c.uri == r.track.uri)
        });
    }

    pub fn pending(&self) -> impl Iterator<Item = &Request> {
        self.list.iter()
    }

    /// Whom a song was requested by, if it was.
    pub fn requester(&self, uri: &str) -> Option<&Request> {
        self.list.iter().find(|r| r.track.uri == uri)
    }

    /// How many songs this viewer has waiting (those taken back do not count).
    pub fn of(&self, user_id: &str) -> usize {
        self.list.iter().filter(|r| r.user_id == user_id && !r.skip).count()
    }

    /// Why this request cannot be taken, in Songify's order.
    pub fn check(&self, rules: &Rules, user_id: &str, track: &Track, player: &Player) -> Result<(), Rejection> {
        if rules.blocked_songs.contains(&track.uri) {
            return Err(reject(
                Reason::BlockedSong,
                "That song has been blocked by the broadcaster.",
            ));
        }
        if rules.block_explicit && track.explicit {
            return Err(reject(
                Reason::Explicit,
                "This song contains explicit content and is not allowed.",
            ));
        }
        if track.restricted {
            return Err(reject(
                Reason::Unavailable,
                "That song is not available in the streamer's country.",
            ));
        }
        let blocked = |name: &str| rules.blocked_artists.iter().any(|b| b.eq_ignore_ascii_case(name));
        if track.artists.iter().any(|a| blocked(a)) {
            return Err(reject(
                Reason::BlockedArtist,
                "That artist has been blocked by the broadcaster.",
            ));
        }
        if rules.max_length_ms > 0 && track.duration_ms >= rules.max_length_ms {
            return Err(reject(
                Reason::TooLong,
                format!("That song is longer than the {} allowed.", clock(rules.max_length_ms)),
            ));
        }
        if player.current.as_ref().is_some_and(|c| c.uri == track.uri)
            || player.queue.iter().any(|t| t.uri == track.uri)
            || self.list.iter().any(|r| r.track.uri == track.uri)
        {
            return Err(reject(Reason::Duplicate, "That song is already in the queue."));
        }
        if rules.max_per_user > 0 && self.of(user_id) as u64 >= rules.max_per_user {
            return Err(reject(
                Reason::MaxRequests,
                format!(
                    "The maximum number of songs in the queue is reached ({}).",
                    rules.max_per_user
                ),
            ));
        }
        Ok(())
    }

    pub fn add(&mut self, r: Request) {
        self.list.push(r);
    }

    /// Take back the viewer's last request: it is skipped when it plays. `None` when there is none.
    pub fn take_back_last(&mut self, user_id: &str) -> Option<Request> {
        let r = self.list.iter_mut().rev().find(|r| r.user_id == user_id && !r.skip)?;
        r.skip = true;
        Some(r.clone())
    }

    /// Take back the last request of the viewer called `name`.
    pub fn take_back_of_name(&mut self, name: &str) -> Option<Request> {
        let r = self
            .list
            .iter_mut()
            .rev()
            .find(|r| r.user.eq_ignore_ascii_case(name) && !r.skip)?;
        r.skip = true;
        Some(r.clone())
    }

    /// Take back request number `n` (1 is the first).
    pub fn take_back_number(&mut self, n: usize) -> Option<Request> {
        let r = self.list.get_mut(n.checked_sub(1)?).filter(|r| !r.skip)?;
        r.skip = true;
        Some(r.clone())
    }

    pub fn has_skips(&self) -> bool {
        self.list.iter().any(|r| r.skip)
    }

    /// Whether the song that plays now was taken back; it is forgotten, as it is about to be skipped.
    pub fn take_skip(&mut self, uri: &str) -> bool {
        match self.list.iter().position(|r| r.track.uri == uri && r.skip) {
            Some(i) => {
                self.list.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> Track {
        Track {
            uri: format!("spotify:track:{name}"),
            name: name.into(),
            artists: vec!["Artist".into()],
            playing: false,
            duration_ms: 180_000,
            explicit: false,
            popularity: 0,
            restricted: false,
        }
    }

    fn request(user: &str, song: &str) -> Request {
        Request {
            user_id: user.into(),
            user: user.into(),
            track: track(song),
            skip: false,
        }
    }

    fn rules() -> Rules {
        Rules {
            max_per_user: 2,
            max_length_ms: 300_000,
            ..Default::default()
        }
    }

    fn player(queue: &[&str]) -> Player {
        Player {
            queue: queue.iter().map(|n| track(n)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn sync_forgets_what_has_played() {
        let mut q = Requests::default();
        q.add(request("ana", "a"));
        q.add(request("bia", "b"));
        q.sync(&player(&["x", "a", "b"]));
        assert_eq!(q.pending().count(), 2);
        // a starts playing: still counted, as Songify does until it is over.
        let mut p = player(&["x", "b"]);
        p.current = Some(track("a"));
        q.sync(&p);
        assert_eq!(q.pending().count(), 2);
        assert_eq!(q.of("ana"), 1);
        q.sync(&player(&["b"]));
        assert_eq!(q.pending().count(), 1);
        assert_eq!(q.requester("spotify:track:b").unwrap().user, "bia");
        assert!(q.requester("spotify:track:a").is_none());
    }

    #[test]
    fn rules_say_why_in_songifys_order() {
        let mut q = Requests::default();
        let p = Player {
            current: Some(track("playing")),
            queue: vec![track("queued")],
            ..Default::default()
        };
        let ask = |q: &Requests, rules: &Rules, t: &Track| q.check(rules, "ana", t, &p).map_err(|e| e.reason);
        assert_eq!(ask(&q, &rules(), &track("new")), Ok(()));
        let mut loud = track("loud");
        loud.explicit = true;
        assert_eq!(
            ask(
                &q,
                &Rules {
                    block_explicit: true,
                    ..rules()
                },
                &loud
            ),
            Err(Reason::Explicit)
        );
        assert_eq!(ask(&q, &rules(), &loud), Ok(()));
        let mut away = track("away");
        away.restricted = true;
        assert_eq!(ask(&q, &rules(), &away), Err(Reason::Unavailable));
        let mut long = track("long");
        long.duration_ms = 300_000;
        assert_eq!(
            ask(&q, &rules(), &long),
            Err(Reason::TooLong),
            "Songify refuses a song as long as the limit"
        );
        long.duration_ms = 299_999;
        assert_eq!(ask(&q, &rules(), &long), Ok(()));
        let blocked = Rules {
            blocked_artists: vec!["ARTIST".into()],
            ..rules()
        };
        assert_eq!(ask(&q, &blocked, &track("new")), Err(Reason::BlockedArtist));
        let song = Rules {
            blocked_songs: vec!["spotify:track:new".into()],
            ..rules()
        };
        assert_eq!(ask(&q, &song, &track("new")), Err(Reason::BlockedSong));
        assert_eq!(ask(&q, &rules(), &track("playing")), Err(Reason::Duplicate));
        assert_eq!(ask(&q, &rules(), &track("queued")), Err(Reason::Duplicate));
        q.add(request("ana", "a"));
        assert_eq!(ask(&q, &rules(), &track("a")), Err(Reason::Duplicate));
        q.add(request("ana", "b"));
        assert_eq!(ask(&q, &rules(), &track("c")), Err(Reason::MaxRequests));
        assert!(q.check(&rules(), "bia", &track("c"), &p).is_ok());
        assert!(q
            .check(
                &Rules {
                    max_per_user: 0,
                    ..rules()
                },
                "ana",
                &track("c"),
                &p
            )
            .is_ok());
    }

    #[test]
    fn taking_back_marks_the_song_to_skip_and_frees_the_place() {
        let mut q = Requests::default();
        assert!(q.take_back_last("ana").is_none());
        q.add(request("ana", "a"));
        q.add(request("bia", "b"));
        q.add(request("ana", "c"));
        assert_eq!(q.take_back_last("ana").unwrap().track.name, "c");
        assert_eq!(q.of("ana"), 1, "a taken back song no longer counts");
        assert_eq!(q.take_back_last("ana").unwrap().track.name, "a");
        assert!(q.take_back_last("ana").is_none());
        assert!(q.has_skips());
        assert_eq!(q.take_back_of_name("BIA").unwrap().track.name, "b");
        assert!(q.take_back_number(1).is_none(), "already taken back");
        assert!(q.take_skip("spotify:track:a"));
        assert!(!q.take_skip("spotify:track:a"));
        assert_eq!(q.pending().count(), 2);
        q.clear();
        assert!(q.pending().next().is_none() && !q.has_skips());
    }

    #[test]
    fn time_shows_as_minutes_and_seconds() {
        assert_eq!(clock(185_000), "3:05");
        assert_eq!(clock(0), "0:00");
    }
}
