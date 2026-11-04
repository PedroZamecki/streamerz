//! The Spotify app on this computer, with no API login: Windows media sessions (events) and, elsewhere,
//! MPRIS over D-Bus (looked at every few seconds while monitoring is on).
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    pub title: String,
    pub artist: String,
    pub playing: bool,
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
        return Ok(Some(LocalTrack {
            title: media.Title().map_err(|e| crate::fmt::win_error(&e))?.to_string(),
            artist: media.Artist().map_err(|e| crate::fmt::win_error(&e))?.to_string(),
            playing: session
                .GetPlaybackInfo()
                .and_then(|p| p.PlaybackStatus())
                .is_ok_and(|s| s == Status::Playing),
        }));
    }
    Ok(None)
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
    )>,
}

#[cfg(windows)]
impl Drop for Subscriptions {
    fn drop(&mut self) {
        for (session, media, playback) in self.sessions.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(media);
            let _ = session.RemovePlaybackInfoChanged(playback);
        }
        let _ = self.manager.RemoveSessionsChanged(self.manager_token);
    }
}

#[cfg(windows)]
impl Subscriptions {
    fn refresh(&mut self, tx: &tokio::sync::mpsc::UnboundedSender<bool>) -> Result<(), String> {
        use windows::Foundation::TypedEventHandler;
        for (session, media, playback) in self.sessions.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(media);
            let _ = session.RemovePlaybackInfoChanged(playback);
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
            self.sessions.push((session, media, playback));
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
        let current = read_session(&subscriptions.manager).await?;
        for kind in changes(previous.as_ref(), current.as_ref()) {
            crate::spotify::local_event(&engine, kind, current.as_ref().or(previous.as_ref()));
        }
        previous = current;
        let Some(sessions_changed) = rx.recv().await else { break };
        if sessions_changed {
            subscriptions.refresh(&tx)?;
        }
    }
    Ok(())
}

/// What the Spotify app on this desktop says through MPRIS: None when it is not running.
#[cfg(not(windows))]
pub async fn current() -> Result<Option<LocalTrack>, String> {
    use std::collections::HashMap;
    use zbus::zvariant::OwnedValue;
    const NAME: &str = "org.mpris.MediaPlayer2.spotify";
    let err = |e: zbus::Error| e.to_string();
    let c = zbus::Connection::session().await.map_err(err)?;
    let bus = zbus::fdo::DBusProxy::new(&c).await.map_err(|e| e.to_string())?;
    let name = zbus::names::BusName::try_from(NAME).map_err(|e| e.to_string())?;
    // Ask first: a proxy to a name nobody owns may start something.
    if !bus.name_has_owner(name).await.map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let p = zbus::Proxy::new(&c, NAME, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player")
        .await
        .map_err(err)?;
    let status: String = p.get_property("PlaybackStatus").await.map_err(err)?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.map_err(err)?;
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
    Ok(Some(LocalTrack {
        title: text("xesam:title"),
        artist,
        playing: status == "Playing",
    }))
}

/// Without Windows' media events: look every few seconds, only while monitoring is on.
#[cfg(not(windows))]
pub async fn monitor(engine: std::rc::Rc<crate::engine::Engine>) -> Result<(), String> {
    let mut previous = None;
    loop {
        let current = current().await?;
        for kind in changes(previous.as_ref(), current.as_ref()) {
            crate::spotify::local_event(&engine, kind, current.as_ref().or(previous.as_ref()));
        }
        previous = current;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
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
}
