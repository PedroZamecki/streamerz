//! The Spotify app on this computer, with no API login: Windows media sessions and, elsewhere, MPRIS
//! over D-Bus. Both tell when something changes, so nothing is looked at on a timer. It runs while the
//! local monitor is on (its triggers) or an overlay page listens to the `song` topic.
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub album: String,
    /// The cover, as an address a page can load: Spotify's own where the player gives one (MPRIS),
    /// else `/overlays/-/cover?<tag>` for the picture the player handed over (see `cover`).
    pub cover: String,
    pub duration_ms: u64,
    /// Where the song was, and when that was (ms since the epoch): a page counts on from there.
    pub position_ms: u64,
    pub at_ms: u64,
}

impl LocalTrack {
    /// Whether the player is somewhere else than it would be by playing on from `before` (a seek).
    fn jumped(&self, before: &LocalTrack) -> bool {
        let played = if before.playing {
            self.at_ms.saturating_sub(before.at_ms)
        } else {
            0
        };
        self.position_ms.abs_diff(before.position_ms + played) > 1500
    }

    /// What the pages listening to the `song` topic get.
    pub fn song(&self) -> serde_json::Value {
        serde_json::json!({"title": self.title, "artist": self.artist, "album": self.album, "cover": self.cover,
            "playing": self.playing, "durationMs": self.duration_ms, "positionMs": self.position_ms, "at": self.at_ms})
    }
}

/// Whether anything needs the player watched: the monitor's triggers, or a page showing the song.
fn wanted(engine: &crate::engine::Engine) -> bool {
    engine.config().spotify.local_monitor
        || engine.listening("Overlay", "song")
        || engine.listening("Overlay", "songrequests.queue")
}

/// One look at the player: the triggers for what changed (only with the monitor on), and the `song`
/// topic when the song, its state or its place changed.
async fn step(
    engine: &std::rc::Rc<crate::engine::Engine>,
    previous: &mut Option<LocalTrack>,
    current: Option<LocalTrack>,
) {
    let kinds = changes(previous.as_ref(), current.as_ref());
    let moved = match (previous.as_ref(), current.as_ref()) {
        (Some(before), Some(now)) => {
            now.jumped(before)
                || now.cover != before.cover
                || now.album != before.album
                || now.duration_ms != before.duration_ms
        }
        _ => false,
    };
    if engine.config().spotify.local_monitor {
        for kind in &kinds {
            crate::spotify::local_event(engine, *kind, current.as_ref().or(previous.as_ref()));
        }
    }
    if !kinds.is_empty() || moved {
        engine.overlay_publish(
            "song",
            current
                .as_ref()
                .map_or_else(|| serde_json::json!({"playing": false}), LocalTrack::song),
        );
    }
    if kinds.contains(&99809) {
        engine.song_queue_refresh().await;
    }
    *previous = current;
}

/// The cover the player handed over as a picture (Windows): its tag and bytes, for `/overlays/-/cover`.
static COVER: std::sync::Mutex<Option<(u64, Vec<u8>)>> = std::sync::Mutex::new(None);

/// The picture behind a `cover` address of ours, if the player gave one.
pub fn cover() -> Option<Vec<u8>> {
    COVER.lock().ok()?.as_ref().map(|c| c.1.clone())
}

/// Keeps a cover picture and gives the address pages load it from (new for each picture, so a browser
/// never shows the last song's).
#[cfg_attr(not(windows), allow(dead_code))]
fn keep_cover(bytes: Vec<u8>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let tag = hasher.finish();
    if let Ok(mut cover) = COVER.lock() {
        if cover.as_ref().is_none_or(|c| c.0 != tag) {
            *cover = Some((tag, bytes));
        }
    }
    format!("/overlays/-/cover?{tag:x}")
}

#[cfg(windows)]
struct Apartment;

#[cfg(windows)]
impl Apartment {
    fn new() -> Result<Self, String> {
        unsafe { windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED) }
            .map_err(|e| crate::fmt::win_error(&e))?;
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::WinRT::RoUninitialize() }
    }
}

#[cfg(windows)]
pub async fn current() -> Result<Option<LocalTrack>, String> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    // The engine runs this future on its single-thread runtime, so initialization and cleanup
    // happen on the same thread.
    let _apartment = Apartment::new()?;
    let manager = Manager::RequestAsync()
        .map_err(|e| crate::fmt::win_error(&e))?
        .await
        .map_err(|e| crate::fmt::win_error(&e))?;
    read_session(&manager).await
}

#[cfg(windows)]
async fn read_session(
    manager: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager,
) -> Result<Option<LocalTrack>, String> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;
    let sessions = manager.GetSessions().map_err(|e| crate::fmt::win_error(&e))?;
    for session in &sessions {
        let id = session
            .SourceAppUserModelId()
            .map_err(|e| crate::fmt::win_error(&e))?
            .to_string();
        if !is_spotify(&id) {
            continue;
        }
        let media = session
            .TryGetMediaPropertiesAsync()
            .map_err(|e| crate::fmt::win_error(&e))?
            .await
            .map_err(|e| crate::fmt::win_error(&e))?;
        // Where the song is: the player says it when it changes (a pause, a seek, a new song).
        let (mut duration_ms, mut position_ms, mut at_ms) = (0, 0, 0);
        if let Ok(timeline) = session.GetTimelineProperties() {
            let ms = |span: windows::Foundation::TimeSpan| (span.Duration / 10_000).max(0) as u64;
            let start = timeline.StartTime().map(ms).unwrap_or(0);
            duration_ms = timeline.EndTime().map(ms).unwrap_or(0).saturating_sub(start);
            position_ms = timeline.Position().map(ms).unwrap_or(0).saturating_sub(start);
            // 100 ns since 1601, as ms since 1970.
            at_ms = timeline
                .LastUpdatedTime()
                .map(|t| (t.UniversalTime / 10_000 - 11_644_473_600_000).max(0) as u64)
                .unwrap_or(0);
        }
        if at_ms == 0 {
            at_ms = crate::fmt::unix_now().as_millis() as u64;
        }
        let cover = match thumbnail(&media).await {
            Some(bytes) => keep_cover(bytes),
            None => String::new(),
        };
        return Ok(Some(LocalTrack {
            title: media.Title().map_err(|e| crate::fmt::win_error(&e))?.to_string(),
            artist: media.Artist().map_err(|e| crate::fmt::win_error(&e))?.to_string(),
            playing: session
                .GetPlaybackInfo()
                .and_then(|p| p.PlaybackStatus())
                .is_ok_and(|s| s == Status::Playing),
            album: media.AlbumTitle().map(|a| a.to_string()).unwrap_or_default(),
            cover,
            duration_ms,
            position_ms,
            at_ms,
        }));
    }
    Ok(None)
}

/// The cover the player shows, as the bytes of its picture (a PNG or a JPEG, at most 4 MB).
#[cfg(windows)]
async fn thumbnail(
    media: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Vec<u8>> {
    use windows::Storage::Streams::DataReader;
    let stream = media.Thumbnail().ok()?.OpenReadAsync().ok()?.await.ok()?;
    let size = u32::try_from(stream.Size().ok()?)
        .ok()
        .filter(|s| (1..=4 << 20).contains(s))?;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let read = reader.LoadAsync(size).ok()?.await.ok()?;
    let mut bytes = vec![0u8; read as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}

/// Subscriptions are removed when monitoring is disabled or the task is cancelled.
#[cfg(windows)]
struct Subscriptions {
    manager: windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager,
    manager_token: i64,
    sessions: Vec<(
        windows::Media::Control::GlobalSystemMediaTransportControlsSession,
        i64,
        i64,
        Option<i64>,
    )>,
}

#[cfg(windows)]
impl Drop for Subscriptions {
    fn drop(&mut self) {
        for (session, media, playback, timeline) in self.sessions.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(media);
            let _ = session.RemovePlaybackInfoChanged(playback);
            if let Some(timeline) = timeline {
                let _ = session.RemoveTimelinePropertiesChanged(timeline);
            }
        }
        let _ = self.manager.RemoveSessionsChanged(self.manager_token);
    }
}

#[cfg(windows)]
impl Subscriptions {
    fn refresh(&mut self, tx: &tokio::sync::mpsc::UnboundedSender<bool>) -> Result<(), String> {
        use windows::Foundation::TypedEventHandler;
        for (session, media, playback, timeline) in self.sessions.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(media);
            let _ = session.RemovePlaybackInfoChanged(playback);
            if let Some(timeline) = timeline {
                let _ = session.RemoveTimelinePropertiesChanged(timeline);
            }
        }
        let sessions = self.manager.GetSessions().map_err(|e| crate::fmt::win_error(&e))?;
        for session in &sessions {
            let id = session
                .SourceAppUserModelId()
                .map_err(|e| crate::fmt::win_error(&e))?
                .to_string();
            if !is_spotify(&id) {
                continue;
            }
            let send = tx.clone();
            let media = session
                .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = send.send(false);
                    Ok(())
                }))
                .map_err(|e| crate::fmt::win_error(&e))?;
            let send = tx.clone();
            let playback = match session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = send.send(false);
                Ok(())
            })) {
                Ok(token) => token,
                Err(error) => {
                    let _ = session.RemoveMediaPropertiesChanged(media);
                    return Err(error.to_string());
                }
            };
            // A seek: the song's place changed and nothing else did.
            let send = tx.clone();
            let timeline = session
                .TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = send.send(false);
                    Ok(())
                }))
                .ok();
            self.sessions.push((session, media, playback, timeline));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub async fn monitor(engine: std::rc::Rc<crate::engine::Engine>) -> Result<(), String> {
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    let _apartment = Apartment::new()?;
    let manager = Manager::RequestAsync()
        .map_err(|e| crate::fmt::win_error(&e))?
        .await
        .map_err(|e| crate::fmt::win_error(&e))?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = tx.clone();
    let manager_token = manager
        .SessionsChanged(&TypedEventHandler::new(move |_, _| {
            let _ = sender.send(true);
            Ok(())
        }))
        .map_err(|e| crate::fmt::win_error(&e))?;
    let mut subscriptions = Subscriptions {
        manager,
        manager_token,
        sessions: Vec::new(),
    };
    subscriptions.refresh(&tx)?;
    let mut previous = None;
    loop {
        step(&engine, &mut previous, read_session(&subscriptions.manager).await?).await;
        let Some(mut sessions_changed) = rx.recv().await else {
            break;
        };
        // A new song comes as several events at once: one look after the last of them.
        while let Ok(more) = rx.try_recv() {
            sessions_changed |= more;
        }
        if !wanted(&engine) {
            break;
        }
        if sessions_changed {
            subscriptions.refresh(&tx)?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
const NAME: &str = "org.mpris.MediaPlayer2.spotify";
#[cfg(not(windows))]
const PATH: &str = "/org/mpris/MediaPlayer2";

/// What the Spotify app on this desktop says through MPRIS: None when it is not running.
#[cfg(not(windows))]
pub async fn current() -> Result<Option<LocalTrack>, String> {
    let c = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    read(&c).await
}

#[cfg(not(windows))]
async fn read(c: &zbus::Connection) -> Result<Option<LocalTrack>, String> {
    use std::collections::HashMap;
    use zbus::zvariant::OwnedValue;
    let err = |e: zbus::Error| e.to_string();
    let bus = zbus::fdo::DBusProxy::new(c).await.map_err(|e| e.to_string())?;
    let name = zbus::names::BusName::try_from(NAME).map_err(|e| e.to_string())?;
    // Ask first: a proxy to a name nobody owns may start something.
    if !bus.name_has_owner(name).await.map_err(|e| e.to_string())? {
        return Ok(None);
    }
    // Asked each time and never remembered: the player doesn't say when the song's place moves on.
    let p: zbus::Proxy = zbus::proxy::Builder::new(c)
        .destination(NAME)
        .map_err(err)?
        .path(PATH)
        .map_err(err)?
        .interface("org.mpris.MediaPlayer2.Player")
        .map_err(err)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(err)?;
    let status: String = p.get_property("PlaybackStatus").await.map_err(err)?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.map_err(err)?;
    let position: i64 = p.get_property("Position").await.unwrap_or(0);
    let text = |k: &str| {
        meta.get(k)
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
    };
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .unwrap_or_default()
        .join(", ");
    // Microseconds: an unsigned number from Spotify, a signed one by the book.
    let length = meta.get("mpris:length").map_or(0, |v| {
        u64::try_from(v.clone())
            .ok()
            .or_else(|| i64::try_from(v.clone()).ok().map(|n| n.max(0) as u64))
            .unwrap_or(0)
    });
    Ok(Some(LocalTrack {
        title: text("xesam:title"),
        artist,
        playing: status == "Playing",
        album: text("xesam:album"),
        cover: Some(text("mpris:artUrl"))
            .filter(|url| url.starts_with("https://"))
            .unwrap_or_default(),
        duration_ms: length / 1000,
        position_ms: (position.max(0) / 1000) as u64,
        at_ms: crate::fmt::unix_now().as_millis() as u64,
    }))
}

/// Looks when the player says something changed (a song, a pause, a seek) or comes and goes on the
/// bus: no timer.
#[cfg(not(windows))]
pub async fn monitor(engine: std::rc::Rc<crate::engine::Engine>) -> Result<(), String> {
    use futures_util::{FutureExt, StreamExt};
    use zbus::message::Type::Signal;
    let err = |e: zbus::Error| e.to_string();
    let c = zbus::Connection::session().await.map_err(err)?;
    let player = zbus::MatchRule::builder()
        .msg_type(Signal)
        .sender(NAME)
        .map_err(err)?
        .path(PATH)
        .map_err(err)?
        .build();
    let owner = zbus::MatchRule::builder()
        .msg_type(Signal)
        .sender("org.freedesktop.DBus")
        .map_err(err)?
        .interface("org.freedesktop.DBus")
        .map_err(err)?
        .member("NameOwnerChanged")
        .map_err(err)?
        .arg(0, NAME)
        .map_err(err)?
        .build();
    let mut player = zbus::MessageStream::for_match_rule(player, &c, Some(16))
        .await
        .map_err(err)?;
    let mut owner = zbus::MessageStream::for_match_rule(owner, &c, Some(4))
        .await
        .map_err(err)?;
    let mut previous = None;
    loop {
        step(&engine, &mut previous, read(&c).await?).await;
        tokio::select! {
            signal = player.next() => if signal.is_none() { break },
            signal = owner.next() => if signal.is_none() { break },
        }
        // A new song comes as several signals at once: one look after the last of them.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        while player.next().now_or_never().flatten().is_some() {}
        while owner.next().now_or_never().flatten().is_some() {}
        if !wanted(&engine) {
            break;
        }
    }
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn is_spotify(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    id == "spotify.exe" || id.starts_with("spotifyab.spotifymusic_") || id.ends_with("\\spotify.exe")
}

fn changes(before: Option<&LocalTrack>, after: Option<&LocalTrack>) -> Vec<u32> {
    let mut events = Vec::new();
    match (before, after) {
        (None, Some(_)) => events.push(99807),
        (Some(_), None) => events.push(99808),
        _ => {}
    }
    if let Some(track) = after {
        if before.is_none_or(|old| old.title != track.title || old.artist != track.artist) {
            events.push(99809);
        }
        if before.is_none_or(|old| old.playing != track.playing) {
            events.push(if track.playing { 99825 } else { 99826 });
        }
    }
    events
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_spotify_sessions_without_matching_other_players() {
        assert!(super::is_spotify("Spotify.exe"));
        assert!(super::is_spotify("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!super::is_spotify("NotSpotify.exe"));
    }

    #[test]
    fn media_changes_are_reported_once() {
        let a = super::LocalTrack {
            title: "One".into(),
            artist: "Artist".into(),
            playing: true,
            ..Default::default()
        };
        let mut b = a.clone();
        assert_eq!(super::changes(None, Some(&a)), [99807, 99809, 99825]);
        assert!(super::changes(Some(&a), Some(&a)).is_empty());
        b.playing = false;
        assert_eq!(super::changes(Some(&a), Some(&b)), [99826]);
        b.title = "Two".into();
        assert_eq!(super::changes(Some(&a), Some(&b)), [99809, 99826]);
        assert_eq!(super::changes(Some(&b), None), [99808]);
    }

    #[test]
    fn a_seek_is_told_apart_from_playing_on() {
        let at = |position_ms, at_ms, playing| super::LocalTrack {
            position_ms,
            at_ms,
            playing,
            ..Default::default()
        };
        // Ten seconds later, ten seconds further: it only played on.
        assert!(!at(40_000, 10_000, true).jumped(&at(30_000, 0, true)));
        assert!(at(90_000, 10_000, true).jumped(&at(30_000, 0, true)));
        // Paused, it stays where it was.
        assert!(!at(30_000, 10_000, false).jumped(&at(30_000, 0, false)));
        assert!(at(5_000, 10_000, false).jumped(&at(30_000, 0, false)));
        let song = at(1_000, 5, true).song();
        assert_eq!(
            (song["positionMs"].as_u64(), song["at"].as_u64()),
            (Some(1_000), Some(5))
        );
    }
}
