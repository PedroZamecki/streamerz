//! On-demand Spotify authorization and Web API operations. No background task is started.

use crate::engine::Engine;
use crate::ports::{Fut, MusicPort};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::rc::Rc;

pub mod api;
pub mod local;

/// An error as the streamer should read it: when there is no login, say what that limits.
pub fn friendly(error: &str) -> String {
    if error == "Spotify is disabled" || error == "Spotify login is required" {
        "This needs the Spotify login (Integrations). Without it Streamerz only sees the song that plays on this computer."
            .into()
    } else {
        error.to_string()
    }
}

/// Notify actions and WebSocket listeners of an authorization or request event.
pub fn event(engine: &Rc<Engine>, kind: u32, error: &str) {
    let (name, event) = match kind {
        99804 => ("Spotify Connected", "Connected"),
        99805 => ("Spotify Disconnected", "Disconnected"),
        99806 => ("Spotify Error", "Error"),
        99840 => ("Spotify Login Expired", "LoginExpired"),
        _ => return,
    };
    let mut args = crate::model::Obj::new();
    if !error.is_empty() {
        args.insert("spotifyError".into(), error.into());
    }
    engine.fire(kind, name, "Integrations/Spotify", "spotify", &args, |_| true);
    engine.emit("Spotify", event, || serde_json::json!(args));
}

/// A song request happened (99830 to 99835): who asked, for what, and why not when it was refused.
pub fn request_event(engine: &Rc<Engine>, kind: u32, user: &str, track: Option<&Track>, reason: &str, position: usize) {
    let (name, event) = match kind {
        99830 => ("Spotify Song Requested", "SongRequested"),
        99831 => ("Spotify Song Request Refused", "SongRequestRefused"),
        99832 => ("Spotify Song Request Taken Back", "SongRequestTakenBack"),
        99833 => ("Spotify Song Skipped", "SongSkipped"),
        99834 => ("Spotify Vote Skip", "VoteSkip"),
        99835 => ("Spotify Song Blocked", "SongBlocked"),
        _ => return,
    };
    let mut args = crate::model::Obj::new();
    args.insert("spotifyRequester".into(), user.into());
    if let Some(t) = track {
        args.insert("spotifyTitle".into(), t.name.clone().into());
        args.insert("spotifyArtist".into(), t.artist().into());
        args.insert("spotifyUri".into(), t.uri.clone().into());
        args.insert("spotifyDurationMs".into(), t.duration_ms.into());
    }
    if !reason.is_empty() {
        args.insert("spotifyReason".into(), reason.into());
    }
    if position > 0 {
        args.insert("spotifyPosition".into(), position.into());
    }
    engine.fire(kind, name, "Integrations/Spotify", "spotify", &args, |_| true);
    engine.emit("Spotify", event, || serde_json::json!(args));
}

/// A setting of the player changed (volume, shuffle, repeat or device): the new value and the old one.
pub fn change_event(engine: &Rc<Engine>, kind: u32, value: &str, previous: &str) {
    let (name, event) = match kind {
        99836 => ("Spotify Volume Changed", "VolumeChanged"),
        99837 => ("Spotify Shuffle Changed", "ShuffleChanged"),
        99838 => ("Spotify Repeat Changed", "RepeatChanged"),
        99839 => ("Spotify Device Changed", "DeviceChanged"),
        _ => return,
    };
    let mut args = crate::model::Obj::new();
    args.insert("spotifyValue".into(), value.into());
    args.insert("spotifyPrevious".into(), previous.into());
    engine.fire(kind, name, "Integrations/Spotify", "spotify", &args, |_| true);
    engine.emit("Spotify", event, || serde_json::json!(args));
}

/// The library or a playlist changed (99841 and 99842): the song and the playlist, when there are any.
pub fn library_event(engine: &Rc<Engine>, kind: u32, track: Option<&Track>, playlist: &str) {
    let (name, event) = match kind {
        99841 => ("Spotify Song Liked", "SongLiked"),
        99842 => ("Spotify Playlist Changed", "PlaylistChanged"),
        _ => return,
    };
    let mut args = crate::model::Obj::new();
    if let Some(t) = track {
        args.insert("spotifyTitle".into(), t.name.clone().into());
        args.insert("spotifyArtist".into(), t.artist().into());
        args.insert("spotifyUri".into(), t.uri.clone().into());
    }
    if !playlist.is_empty() {
        args.insert("spotifyPlaylist".into(), playlist.into());
    }
    engine.fire(kind, name, "Integrations/Spotify", "spotify", &args, |_| true);
    engine.emit("Spotify", event, || serde_json::json!(args));
}

pub fn local_event(engine: &Rc<Engine>, kind: u32, track: Option<&local::LocalTrack>) {
    let (name, event) = match kind {
        99807 => ("Spotify Local Connected", "LocalConnected"),
        99808 => ("Spotify Local Disconnected", "LocalDisconnected"),
        99809 => ("Spotify Track Changed", "TrackChanged"),
        99825 => ("Spotify Playback Started", "PlaybackStarted"),
        99826 => ("Spotify Playback Paused", "PlaybackPaused"),
        _ => return,
    };
    let mut args = crate::model::Obj::new();
    if let Some(track) = track {
        args.insert("spotifyTitle".into(), track.title.clone().into());
        args.insert("spotifyArtist".into(), track.artist.clone().into());
        args.insert("spotifyPlaying".into(), track.playing.into());
    }
    engine.fire(kind, name, "Integrations/Spotify", "spotify", &args, |_| true);
    engine.emit("Spotify", event, || serde_json::json!(args));
}

/// The Spotify Web API behind `MusicPort`: authorization on demand, one request per call.
pub struct Spotify;

impl MusicPort for Spotify {
    fn player_action<'a>(
        &'a self,
        engine: &'a Rc<Engine>,
        kind: u32,
        value: &'a str,
    ) -> Fut<'a, Result<Value, String>> {
        Box::pin(async move { player_action(&saved_token(engine).await?, kind, value).await })
    }
    fn current<'a>(&'a self, engine: &'a Rc<Engine>) -> Fut<'a, Result<Option<Track>, String>> {
        Box::pin(async move {
            if !engine.config().spotify.enabled {
                return Ok(local::current().await?.map(Track::from));
            }
            currently_playing(&saved_token(engine).await?).await
        })
    }

    fn find<'a>(&'a self, engine: &'a Rc<Engine>, query: &'a str) -> Fut<'a, Result<Track, String>> {
        Box::pin(async move { find(&saved_token(engine).await?, query).await })
    }

    fn add<'a>(&'a self, engine: &'a Rc<Engine>, uri: &'a str, device: &'a str) -> Fut<'a, Result<(), String>> {
        Box::pin(async move { add_uri(&saved_token(engine).await?, uri, device).await })
    }

    fn volume<'a>(&'a self, engine: &'a Rc<Engine>, percent: u32, device: &'a str) -> Fut<'a, Result<(), String>> {
        Box::pin(async move { set_volume(&saved_token(engine).await?, percent, device).await })
    }

    fn like<'a>(&'a self, engine: &'a Rc<Engine>, uri: &'a str) -> Fut<'a, Result<(), String>> {
        Box::pin(async move { like(&saved_token(engine).await?, uri).await })
    }

    fn player<'a>(&'a self, engine: &'a Rc<Engine>) -> Fut<'a, Result<Player, String>> {
        Box::pin(async move { player(&saved_token(engine).await?).await })
    }

    fn playback<'a>(&'a self, engine: &'a Rc<Engine>, command: &'a str) -> Fut<'a, Result<(), String>> {
        Box::pin(async move {
            let token = saved_token(engine).await?;
            // Starting playback when no device is active needs one named.
            let device = if command == "play" {
                player(&token).await?.device_id
            } else {
                String::new()
            };
            playback_on(API, &token, command, &device).await
        })
    }
}

pub static SPOTIFY: Spotify = Spotify;

const API: &str = "https://api.spotify.com/v1";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const SCOPE: &str =
    "user-read-currently-playing user-read-playback-state user-modify-playback-state user-library-modify user-read-private user-read-recently-played user-library-read user-follow-read user-follow-modify user-top-read playlist-read-private playlist-modify-public playlist-modify-private";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpotifyConfig {
    pub enabled: bool,
    /// Monitor the Windows media session, independently of the API login.
    pub local_monitor: bool,
    pub client_id: String,
    pub token: Token,
}

/// Obtain a usable saved token, refreshing it only on demand.
pub async fn saved_token(engine: &std::rc::Rc<crate::engine::Engine>) -> Result<String, String> {
    saved_token_at(engine, TOKEN).await
}

async fn saved_token_at(engine: &std::rc::Rc<crate::engine::Engine>, token_url: &str) -> Result<String, String> {
    static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = REFRESH.lock().await;
    let config = engine.config().spotify.clone();
    if !config.enabled {
        return Err("Spotify is disabled".into());
    }
    if config.token.access_token.is_empty() {
        return Err("Spotify login is required".into());
    }
    if !crate::oauth::stale(config.token.expires_at) {
        return Ok(config.token.access_token);
    }
    let token = match refresh_at(token_url, &config.client_id, &config.token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") || e.contains("revoked") {
                // Clear the revoked token so subsequent calls do not loop refreshing it.
                engine.change(|c| {
                    if c.spotify.client_id == config.client_id
                        && c.spotify.token.refresh_token == config.token.refresh_token
                    {
                        c.spotify.token = Token::default();
                    }
                });
                event(engine, 99840, &e);
            }
            return Err(e);
        }
    };
    let access = token.access_token.clone();
    engine.change(|c| {
        if c.spotify.enabled
            && c.spotify.client_id == config.client_id
            && c.spotify.token.refresh_token == config.token.refresh_token
        {
            c.spotify.token = token;
        }
    });
    // A user can disconnect while refresh is in flight; do not use the old login then.
    let current = engine.config();
    if !current.spotify.enabled || current.spotify.token.access_token != access {
        return Err("Spotify login changed".into());
    }
    Ok(access)
}

impl From<crate::oauth::Token> for Token {
    fn from(t: crate::oauth::Token) -> Self {
        Self {
            access_token: t.access,
            refresh_token: t.refresh,
            expires_at: t.expires_at,
        }
    }
}

/// The redirect URI to register in the Spotify dashboard. Its dashboard rejects a loopback URI
/// without a port, so the port is fixed (like Kick).
pub const REDIRECT: &str = "http://127.0.0.1:45564/callback";

pub async fn login(client_id: &str) -> Result<Token, String> {
    let id = client_id.trim();
    if id.is_empty() {
        return Err("Set your Spotify client ID first".into());
    }
    let login = crate::oauth::Login::bind(45564, "/callback").await?;
    let encode = crate::commands::url_encode;
    crate::oauth::open_browser(&format!(
        "https://accounts.spotify.com/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge_method=S256&code_challenge={}&state={}",
        encode(id), encode(&login.redirect), encode(SCOPE), login.challenge, login.state
    ));
    let redirect = login.redirect.clone();
    let verifier = login.verifier.clone();
    let code = login.code().await?;
    crate::oauth::token(
        TOKEN,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", id),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ],
    )
    .await
    .map(Token::from)
}

/// Spotify may omit a replacement refresh token, in which case the existing one is retained.
pub async fn refresh(client_id: &str, previous: &Token) -> Result<Token, String> {
    refresh_at(TOKEN, client_id, previous).await
}

async fn refresh_at(token_url: &str, client_id: &str, previous: &Token) -> Result<Token, String> {
    if client_id.trim().is_empty() || previous.refresh_token.is_empty() {
        return Err("Spotify login is required".into());
    }
    let t = crate::oauth::token(
        token_url,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &previous.refresh_token),
            ("client_id", client_id.trim()),
        ],
    )
    .await?;
    let mut token = Token::from(t);
    if token.refresh_token.is_empty() {
        token.refresh_token.clone_from(&previous.refresh_token);
    }
    Ok(token)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Track {
    pub uri: String,
    pub name: String,
    pub artists: Vec<String>,
    pub playing: bool,
    pub duration_ms: u64,
    pub explicit: bool,
    pub popularity: u32,
    /// Spotify says it cannot be played in the streamer's country.
    pub restricted: bool,
}

impl From<local::LocalTrack> for Track {
    fn from(track: local::LocalTrack) -> Self {
        let artists = if track.artist.is_empty() {
            Vec::new()
        } else {
            vec![track.artist]
        };
        Self {
            uri: String::new(),
            name: track.title,
            artists,
            playing: track.playing,
            duration_ms: track.duration_ms,
            explicit: false,
            popularity: 0,
            restricted: false,
        }
    }
}

impl Track {
    /// "Artist, Artist" (the first four).
    pub fn artist(&self) -> String {
        self.artists.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
    }

    /// "Artist - Title".
    pub fn full(&self) -> String {
        format!("{} - {}", self.artist(), self.name)
    }
}

fn track(value: &Value, playing: bool) -> Option<Track> {
    let uri = value["uri"].as_str()?;
    track_uri(uri)?;
    Some(Track {
        uri: uri.into(),
        name: value["name"].as_str()?.into(),
        artists: value["artists"]
            .as_array()
            .map(|a| a.iter().filter_map(|a| a["name"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        playing,
        duration_ms: value["duration_ms"].as_u64().unwrap_or(0),
        explicit: value["explicit"].as_bool().unwrap_or(false),
        popularity: value["popularity"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
        // With market=from_token Spotify says whether the track plays here.
        restricted: value["is_playable"].as_bool() == Some(false)
            || value["restrictions"]["reason"].as_str() == Some("market"),
    })
}

/// A letter without its accent.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c => c,
    }
}

/// What a viewer typed, as words: lowercase letters and digits only, accents dropped. Punctuation such
/// as the dash in "song - artist" must not reach Spotify's search, where `-` excludes what follows.
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(fold)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Words that mark a different recording of a song; a request that does not use them wants the original.
const VERSIONS: &[&str] = &[
    "karaoke",
    "instrumental",
    "tribute",
    "cover",
    "remix",
    "live",
    "acoustic",
    "nightcore",
    "sped",
    "slowed",
    "reverb",
    "8d",
    "piano",
    "lullaby",
    "version",
    "demo",
];

fn same_word(a: &str, b: &str) -> bool {
    a == b || (a.len().min(b.len()) >= 4 && (a.starts_with(b) || b.starts_with(a)))
}

/// How much of `wanted` is said by `have` (0 to 1).
fn coverage(wanted: &[String], have: &[String]) -> f64 {
    if wanted.is_empty() {
        return 0.0;
    }
    wanted.iter().filter(|w| have.iter().any(|h| same_word(h, w))).count() as f64 / wanted.len() as f64
}

/// The ways a request can be read as a title and an artist: "title - artist", "artist - title",
/// "title by artist", the first word as the artist, the last word as the artist.
fn readings(query: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let mut out = Vec::new();
    let lower = query.to_lowercase();
    let dash: Vec<&str> = query.split(" - ").map(str::trim).filter(|p| !p.is_empty()).collect();
    if dash.len() >= 2 {
        let (first, rest) = (words(dash[0]), words(&dash[1..].join(" ")));
        out.push((first.clone(), rest.clone()));
        out.push((rest, first));
    }
    if let Some(i) = lower.rfind(" by ") {
        out.push((words(&lower[..i]), words(&lower[i + 4..])));
    }
    let all = words(query);
    if all.len() >= 2 {
        out.push((all[1..].to_vec(), all[..1].to_vec()));
        out.push((all[..all.len() - 1].to_vec(), all[all.len() - 1..].to_vec()));
    }
    out
}

/// How well a track answers a request: the share of the requested words in its title and artists, a
/// good deal when the request reads as its title and its artist, a little when it names an artist of
/// it, less for other recordings of it, and its popularity (when Spotify gives one) to break ties.
pub fn score(query: &str, t: &Track) -> f64 {
    let q = words(query);
    if q.is_empty() {
        return 0.0;
    }
    let parsed = readings(query);
    score_with(&q, &parsed, t)
}

fn score_with(q: &[String], parsed: &[(Vec<String>, Vec<String>)], t: &Track) -> f64 {
    let title = words(&t.name);
    let artists: Vec<String> = t.artists.iter().flat_map(|a| words(a)).collect();
    let all: Vec<String> = title.iter().chain(&artists).cloned().collect();
    let found = coverage(q, &all);
    let said = coverage(&title, q);
    // "firework katy perry": an artist whose whole name was asked for is who they meant.
    let artist = t.artists.iter().any(|a| {
        let w = words(a);
        !w.is_empty() && coverage(&w, q) == 1.0
    });
    // Both halves of "title - artist" (either way round) found: it is that song.
    let read = parsed
        .iter()
        .map(|(title_part, artist_part)| {
            if title_part.is_empty() || artist_part.is_empty() {
                0.0
            } else {
                (coverage(title_part, &title) + coverage(&title, title_part)) / 2.0
                    * (coverage(artist_part, &artists) + coverage(&artists, artist_part).min(1.0))
                    / 2.0
            }
        })
        .fold(0.0, f64::max);
    let other = all
        .iter()
        .filter(|w| VERSIONS.contains(&w.as_str()) && !q.iter().any(|x| x == *w))
        .count() as f64;
    found + 0.3 * said + 0.6 * read + if artist { 0.35 } else { 0.0 } - 0.5 * other.min(2.0)
        + f64::from(t.popularity) / 1000.0
}

/// The best of the tracks Spotify found for `query`, in Spotify's own order of relevance: when two
/// answer equally well, the one Spotify put first.
pub fn best(query: &str, tracks: Vec<Track>) -> Option<Track> {
    let wanted = words(query);
    if wanted.is_empty() {
        return None;
    }
    let parsed = readings(query);
    let n = tracks.len().max(1) as f64;
    let mut top: Option<(f64, Track)> = None;
    for (i, t) in tracks.into_iter().enumerate() {
        // Spotify always finds something; if it says less than half of what was asked, it is not it.
        let said: Vec<String> = words(&t.name)
            .into_iter()
            .chain(t.artists.iter().flat_map(|a| words(a)))
            .collect();
        if coverage(&wanted, &said) < 0.5 {
            continue;
        }
        let s = score_with(&wanted, &parsed, &t) + 0.1 * (1.0 - i as f64 / n);
        if top.as_ref().is_none_or(|(best_s, _)| s > *best_s) {
            top = Some((s, t));
        }
    }
    top.map(|(_, t)| t)
}

/// What a request is: a track, a link to something that is not one, or words to search.
#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Track(String),
    /// "episode", "artist", "album", "playlist", "audiobook" or "shortened".
    Unsupported(&'static str),
    Search(String),
}

/// Whether `id` is a Spotify base-62 id (22 ASCII letters or digits).
pub(crate) fn is_id(id: &str) -> bool {
    id.len() == 22 && id.bytes().all(|c| c.is_ascii_alphanumeric())
}

/// Strip a leading ASCII prefix, ignoring case.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// The path after `open.spotify.com/` (with `https://`, `http://`, or no scheme).
fn open_spotify_path(input: &str) -> Option<&str> {
    let rest = strip_prefix_ci(input, "https://")
        .or_else(|| strip_prefix_ci(input, "http://"))
        .unwrap_or(input);
    strip_prefix_ci(rest, "open.spotify.com/")
}

fn is_short_link(input: &str) -> bool {
    let rest = strip_prefix_ci(input, "https://")
        .or_else(|| strip_prefix_ci(input, "http://"))
        .unwrap_or(input);
    strip_prefix_ci(rest, "spotify.link/").is_some() || strip_prefix_ci(rest, "spotify.app.link/").is_some()
}

/// What a Spotify link or URI points at: its lowercased kind and 22-character id.
pub fn reference(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    if let Some(rest) = strip_prefix_ci(t, "spotify:") {
        let mut parts = rest.split(':');
        let first = parts.next()?;
        let (kind, id) = if first.eq_ignore_ascii_case("user") {
            let _user = parts.next()?;
            let kind = parts.next()?;
            let id = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            (kind, id)
        } else {
            let id = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            (first, id)
        };
        return (!kind.is_empty() && is_id(id)).then(|| (kind.to_ascii_lowercase(), id.to_string()));
    }
    if let Some(rest) = open_spotify_path(t) {
        let path = rest.split(['?', '#']).next()?;
        let mut parts = path.split('/').filter(|p| !p.is_empty());
        let mut first = parts.next()?;
        if strip_prefix_ci(first, "intl-").is_some() {
            first = parts.next()?;
        }
        if first.eq_ignore_ascii_case("embed") {
            first = parts.next()?;
        }
        if first.eq_ignore_ascii_case("user") {
            let _user = parts.next()?;
            first = parts.next()?;
        }
        let id = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        return is_id(id).then(|| (first.to_ascii_lowercase(), id.to_string()));
    }
    None
}

pub fn classify(input: &str) -> Input {
    let input = input.trim();
    let kind = |k: &str| match k {
        "episode" | "show" => Some("episode"),
        "artist" => Some("artist"),
        "album" => Some("album"),
        "playlist" => Some("playlist"),
        "audiobook" => Some("audiobook"),
        _ => None,
    };
    if let Some(uri) = track_uri(input) {
        return Input::Track(uri);
    }
    if let Some(rest) = strip_prefix_ci(input, "spotify:") {
        let mut parts = rest.split(':');
        let mut first = parts.next().unwrap_or("");
        if first.eq_ignore_ascii_case("user") {
            let _ = parts.next();
            first = parts.next().unwrap_or("");
        }
        if let Some(k) = kind(&first.to_ascii_lowercase()) {
            return Input::Unsupported(k);
        }
    }
    if is_short_link(input) {
        return Input::Unsupported("shortened");
    }
    if let Some(rest) = open_spotify_path(input) {
        let path = rest.split(['?', '#']).next().unwrap_or("");
        let mut parts = path.split('/').filter(|p| !p.is_empty());
        let mut first = parts.next().unwrap_or("");
        // /intl-de/track/...
        if strip_prefix_ci(first, "intl-").is_some() {
            first = parts.next().unwrap_or("");
        }
        if first.eq_ignore_ascii_case("embed") {
            first = parts.next().unwrap_or("");
        }
        if first.eq_ignore_ascii_case("user") {
            let _ = parts.next();
            first = parts.next().unwrap_or("");
        }
        if let Some(k) = kind(&first.to_ascii_lowercase()) {
            return Input::Unsupported(k);
        }
    }
    Input::Search(input.to_string())
}

/// A track's URI from a link, URI or id (blocked songs are typed as any of them).
pub fn track_uri_of(text: &str) -> Option<String> {
    let t = text.trim();
    track_uri(t).or_else(|| is_id(t).then(|| format!("spotify:track:{t}")))
}

/// Accept Spotify track URIs and open.spotify.com links only (also with a language or embed prefix).
fn track_uri(input: &str) -> Option<String> {
    match reference(input) {
        Some((kind, id)) if kind == "track" => Some(format!("spotify:track:{id}")),
        _ => None,
    }
}

/// What went wrong, in words for the streamer: Spotify's own reason when it gives one.
fn failure(status: u16, body: &[u8]) -> String {
    let error: Value = serde_json::from_slice(body).unwrap_or_default();
    let reason = error["error"]["reason"].as_str().unwrap_or("");
    let message = error["error"]["message"]
        .as_str()
        .or_else(|| error["error_description"].as_str());
    let base = match (status, reason) {
        (404, "NO_ACTIVE_DEVICE") => {
            "Spotify has no active device: start playing something in Spotify first".to_string()
        }
        (401, _) => "Spotify login expired or was revoked".to_string(),
        (403, "PREMIUM_REQUIRED") => "Spotify Premium is required to control the queue".to_string(),
        (403, _) if message.is_some_and(|m| m.contains("scope")) => {
            return "Spotify needs more permissions: disconnect and connect Spotify again in Integrations".into()
        }
        (403, _) => "Spotify denied the request; check the account, app access and Premium status".to_string(),
        (404, _) => "Spotify could not find the requested item or active device".to_string(),
        (429, _) => "Spotify rate limit reached; try again later".to_string(),
        _ => format!("Spotify request failed (HTTP {status})"),
    };
    match message.filter(|m| !m.is_empty() && !base.contains(m)) {
        Some(m) if ![401, 429].contains(&status) => format!("{base} ({m})"),
        _ => base,
    }
}

/// Parse a numeric `Retry-After` header in seconds.
fn retry_after(headers: &[(String, String)]) -> Option<std::time::Duration> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, v)| v.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
}

/// Longest 429 `Retry-After` wait we sleep through automatically; longer waits fail fast.
const MAX_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// A request to Spotify (with an optional JSON body); a failure with its HTTP status.
/// On HTTP 429 with a short `Retry-After` header (up to 5 s), waits and retries once.
pub(crate) async fn call_body(
    base: &str,
    method: &str,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
) -> Result<crate::http::Response, (u16, String)> {
    if token.is_empty() {
        return Err((401, "Spotify login is required".into()));
    }
    let mut headers = vec![("Authorization".into(), format!("Bearer {token}"))];
    let payload = match body {
        Some(bytes) => {
            headers.push(("Content-Type".into(), "application/json".into()));
            bytes
        }
        None => b"",
    };
    let url = format!("{base}{path}");
    for attempt in 0..2 {
        let response = crate::http::request(method, &url, &headers, payload)
            .await
            .map_err(|e| (0, e))?;
        if (200..300).contains(&response.status) {
            return Ok(response);
        }
        if response.status == 429 && attempt == 0 {
            if let Some(wait) = retry_after(&response.headers).filter(|w| *w <= MAX_RETRY_AFTER) {
                tokio::time::sleep(wait).await;
                continue;
            }
        }
        return Err((response.status, failure(response.status, &response.body)));
    }
    unreachable!()
}

/// A request to Spotify; a failure with its HTTP status.
async fn call(base: &str, method: &str, path: &str, token: &str) -> Result<crate::http::Response, (u16, String)> {
    call_body(base, method, path, token, None).await
}

async fn request(base: &str, method: &str, path: &str, token: &str) -> Result<crate::http::Response, String> {
    call(base, method, path, token).await.map_err(|e| e.1)
}

/// A request that asks for the streamer's market (`market=from_token`, which says what plays in their
/// country). A login made before Streamerz asked for that permission is refused it; the same request
/// without it still works, only without the country check.
async fn with_market(base: &str, token: &str, path: &str) -> Result<crate::http::Response, String> {
    let sep = if path.contains('?') { '&' } else { '?' };
    match call(base, "GET", &format!("{path}{sep}market=from_token"), token).await {
        Err((403, e)) if e.contains("more permissions") => request(base, "GET", path, token).await,
        other => other.map_err(|e| e.1),
    }
}

pub async fn currently_playing(token: &str) -> Result<Option<Track>, String> {
    current_at(API, token).await
}

fn player_request(kind: u32, value: &str) -> Result<(&'static str, String), String> {
    let (method, path) = match kind {
        99810 => ("GET", "/me/player/currently-playing".into()),
        99811 => ("GET", "/me/player/devices".into()),
        99812 => ("GET", "/me/player/queue".into()),
        99813 => ("PUT", "/me/player/play".into()),
        99814 => ("PUT", "/me/player/pause".into()),
        99815 => ("POST", "/me/player/next".into()),
        99816 => ("POST", "/me/player/previous".into()),
        99817 => {
            let position = value
                .trim()
                .parse::<u32>()
                .map_err(|_| "Invalid Spotify position".to_string())?;
            ("PUT", format!("/me/player/seek?position_ms={position}"))
        }
        99818 => {
            let volume = value
                .trim()
                .parse::<u8>()
                .map_err(|_| "Invalid Spotify volume".to_string())?;
            if volume > 100 {
                return Err("Invalid Spotify volume".into());
            }
            ("PUT", format!("/me/player/volume?volume_percent={volume}"))
        }
        99821 => {
            let state = value
                .trim()
                .parse::<bool>()
                .map_err(|_| "Invalid Spotify shuffle state".to_string())?;
            ("PUT", format!("/me/player/shuffle?state={state}"))
        }
        99822 => {
            let state = value.trim();
            if !matches!(state, "track" | "context" | "off") {
                return Err("Invalid Spotify repeat state".into());
            }
            ("PUT", format!("/me/player/repeat?state={state}"))
        }
        99823 => ("GET", "/me/player".into()),
        99824 => ("GET", "/me/player/recently-played?limit=50".into()),
        _ => return Err("Unknown Spotify player action".into()),
    };
    Ok((method, path))
}

pub async fn player_action(token: &str, kind: u32, value: &str) -> Result<Value, String> {
    player_action_at(API, token, kind, value).await
}

pub(crate) async fn player_action_at(base: &str, token: &str, kind: u32, value: &str) -> Result<Value, String> {
    if kind >= 99840 {
        return api::run(base, token, kind, value).await;
    }
    if kind == 99820 {
        if value.trim().is_empty() {
            return Err("Spotify device ID is required".into());
        }
        let body = serde_json::to_vec(&serde_json::json!({"device_ids": [value.trim()]})).map_err(|e| e.to_string())?;
        call_body(base, "PUT", "/me/player", token, Some(&body))
            .await
            .map_err(|e| e.1)?;
        return Ok(Value::Null);
    }
    let (method, path) = player_request(kind, value)?;
    let response = request(base, method, &path, token).await?;
    if response.body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&response.body).map_err(|_| "Invalid Spotify response".into())
}
async fn current_at(base: &str, token: &str) -> Result<Option<Track>, String> {
    let r = request(base, "GET", "/me/player/currently-playing", token).await?;
    if r.status == 204 || r.body.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&r.body).map_err(|_| "Invalid Spotify response".to_string())?;
    if value["item"].is_null() {
        return Ok(None);
    }
    track(&value["item"], value["is_playing"].as_bool().unwrap_or(false))
        .map(Some)
        .ok_or_else(|| "Invalid Spotify track".into())
}

/// Resolve a track URI, link, or search phrase to the best matching track.
pub async fn find(token: &str, query: &str) -> Result<Track, String> {
    find_at(API, token, query).await
}
async fn find_at(base: &str, token: &str, query: &str) -> Result<Track, String> {
    let input = query.trim();
    if input.is_empty() {
        return Err("Enter a song name or Spotify track link".into());
    }
    match classify(input) {
        Input::Track(uri) => {
            let id = uri.strip_prefix("spotify:track:").unwrap_or(&uri);
            // from_token: Spotify says whether it plays in the streamer's country.
            let r = with_market(base, token, &format!("/tracks/{id}")).await?;
            let value: Value =
                serde_json::from_slice(&r.body).map_err(|_| "Invalid Spotify response".to_string())?;
            track(&value, false).ok_or_else(|| "Invalid Spotify track".into())
        }
        Input::Unsupported(kind) => Err(match kind {
            "shortened" => "Spotify short links are not supported. Please type in the full title or get the Spotify URI (starts with \"spotify:track:\")".to_string(),
            "episode" => "Episodes are not supported. Please specify a track!".to_string(),
            "artist" => "Artist links are not supported. Please specify a track!".to_string(),
            "album" => "Album links are not supported. Please specify a track!".to_string(),
            "playlist" => "Playlist links are not supported. Please specify a track!".to_string(),
            _ => "Audiobooks are not supported. Please specify a track!".to_string(),
        }),
        Input::Search(text) => {
            if strip_prefix_ci(&text, "spotify:").is_some() || open_spotify_path(&text).is_some() {
                return Err("Enter a valid Spotify track link or URI".into());
            }
            let terms = words(&text);
            if terms.is_empty() {
                return Err("Enter a song name or Spotify track link".into());
            }
            let path = format!(
                "/search?type=track&limit=10&q={}",
                crate::commands::url_encode(&terms.join(" "))
            );
            let r = with_market(base, token, &path).await?;
            let value: Value =
                serde_json::from_slice(&r.body).map_err(|_| "Invalid Spotify response".to_string())?;
            let found: Vec<Track> = value["tracks"]["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| track(t, false))
                .collect();
            best(&text, found).ok_or_else(|| "No Spotify track found".into())
        }
    }
}

/// What the player is doing: the track, how far into it, what is queued after it, and the device.
#[derive(Clone, Debug, Default)]
pub struct Player {
    pub current: Option<Track>,
    pub playing: bool,
    pub progress_ms: u64,
    pub queue: Vec<Track>,
    pub device_id: String,
    pub volume: Option<u32>,
}

pub async fn player(token: &str) -> Result<Player, String> {
    player_at(API, token).await
}
async fn player_at(base: &str, token: &str) -> Result<Player, String> {
    let q = request(base, "GET", "/me/player/queue", token).await?;
    let queue: Value = if q.status == 204 || q.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&q.body).map_err(|_| "Invalid Spotify response".to_string())?
    };
    let mut out = Player {
        queue: queue["queue"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| track(t, false))
            .collect(),
        ..Default::default()
    };
    // 204: nothing is playing anywhere.
    let s = with_market(base, token, "/me/player").await?;
    if s.status != 204 && !s.body.is_empty() {
        let state: Value = serde_json::from_slice(&s.body).map_err(|_| "Invalid Spotify response".to_string())?;
        out.playing = state["is_playing"].as_bool().unwrap_or(false);
        out.progress_ms = state["progress_ms"].as_u64().unwrap_or(0);
        out.current = track(&state["item"], out.playing);
        out.device_id = state["device"]["id"].as_str().unwrap_or("").to_string();
        out.volume = state["device"]["volume_percent"].as_u64().map(|v| v as u32);
    }
    if out.current.is_none() {
        out.current = track(&queue["currently_playing"], false);
    }
    // Nothing is active (Spotify has been idle): the devices it knows can still take a request.
    if out.device_id.is_empty() {
        if let Ok(r) = request(base, "GET", "/me/player/devices", token).await {
            let devices: Value = serde_json::from_slice(&r.body).unwrap_or_default();
            let all = devices["devices"].as_array().cloned().unwrap_or_default();
            let usable = |d: &&Value| {
                d["is_restricted"].as_bool() != Some(true) && d["id"].as_str().is_some_and(|id| !id.is_empty())
            };
            let pick = all
                .iter()
                .filter(usable)
                .find(|d| d["is_active"].as_bool() == Some(true))
                .or_else(|| all.iter().find(usable));
            if let Some(d) = pick {
                out.device_id = d["id"].as_str().unwrap_or("").to_string();
                out.volume = d["volume_percent"].as_u64().map(|v| v as u32);
            }
        }
    }
    Ok(out)
}

/// Add a track (by URI) to the end of the player's queue, on `device` when one is known. A 503 is
/// tried again a few times; a device that is gone is left to Spotify to choose.
pub async fn add_uri(token: &str, uri: &str, device: &str) -> Result<(), String> {
    add_uri_at(API, token, uri, device, std::time::Duration::from_secs(1)).await
}
async fn add_uri_at(
    base: &str,
    token: &str,
    uri: &str,
    device: &str,
    pause: std::time::Duration,
) -> Result<(), String> {
    let path = |device: &str| {
        let mut p = format!("/me/player/queue?uri={}", crate::commands::url_encode(uri));
        if !device.is_empty() {
            p.push_str(&format!("&device_id={}", crate::commands::url_encode(device)));
        }
        p
    };
    let mut device = device;
    let mut error = String::new();
    for attempt in 0..6 {
        match call(base, "POST", &path(device), token).await {
            Ok(_) => return Ok(()),
            // The device we knew is gone: let Spotify use the active one.
            Err((404, e)) if !device.is_empty() => {
                device = "";
                error = e;
            }
            Err((503, e)) => {
                error = e;
                if attempt < 5 {
                    tokio::time::sleep(pause).await;
                }
            }
            Err((_, e)) => return Err(e),
        }
    }
    Err(error)
}

/// Set the volume (0 to 100) of the player.
pub async fn set_volume(token: &str, percent: u32, device: &str) -> Result<(), String> {
    set_volume_at(API, token, percent, device).await
}

async fn set_volume_at(base: &str, token: &str, percent: u32, device: &str) -> Result<(), String> {
    let mut path = format!("/me/player/volume?volume_percent={}", percent.min(100));
    if !device.is_empty() {
        path.push_str(&format!("&device_id={}", crate::commands::url_encode(device)));
    }
    request(base, "PUT", &path, token).await?;
    Ok(())
}

/// Save a track to the streamer's liked songs. Needs the library scope, which a login made before this
/// existed does not have.
pub async fn like(token: &str, uri: &str) -> Result<(), String> {
    like_at(API, token, uri).await
}

async fn like_at(base: &str, token: &str, uri: &str) -> Result<(), String> {
    let id = uri.strip_prefix("spotify:track:").unwrap_or(uri);
    match call(
        base,
        "PUT",
        &format!("/me/library?uris={}", crate::commands::url_encode(uri)),
        token,
    )
    .await
    {
        Ok(_) => Ok(()),
        Err((404 | 405, _)) => request(
            base,
            "PUT",
            &format!("/me/tracks?ids={}", crate::commands::url_encode(id)),
            token,
        )
        .await
        .map(|_| ()),
        Err((403, _)) => Err("Spotify needs to be connected again to like songs (the old login cannot)".into()),
        Err((_, e)) => Err(e),
    }
}

async fn playback_on(base: &str, token: &str, command: &str, device: &str) -> Result<(), String> {
    let (method, path) = match command {
        "play" => ("PUT", "/me/player/play"),
        "pause" => ("PUT", "/me/player/pause"),
        "next" => ("POST", "/me/player/next"),
        "previous" => ("POST", "/me/player/previous"),
        _ => return Err("Unknown Spotify playback command".into()),
    };
    let path = if device.is_empty() {
        path.to_string()
    } else {
        format!("{path}?device_id={}", crate::commands::url_encode(device))
    };
    request(base, method, &path, token).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    pub async fn add_to_queue(token: &str, query: &str) -> Result<String, String> {
        add_at(API, token, query).await
    }

    async fn playback_at(base: &str, token: &str, command: &str) -> Result<(), String> {
        playback_on(base, token, command, "").await
    }

    async fn add_at(base: &str, token: &str, query: &str) -> Result<String, String> {
        let t = find_at(base, token, query).await?;
        add_uri_at(base, token, &t.uri, "", std::time::Duration::from_secs(1)).await?;
        Ok(t.uri)
    }

    #[test]
    fn transfers_to_the_requested_device() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0; 1024];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert_ne!(n, 0);
                    request.extend_from_slice(&buf[..n]);
                    if request
                        .windows(b"{\"device_ids\":[\"test-device\"]}".len())
                        .any(|w| w == b"{\"device_ids\":[\"test-device\"]}")
                    {
                        break;
                    }
                }
                assert!(request.starts_with(b"PUT /me/player HTTP/1.1\r\n"));
                socket
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
            });
            assert!(super::player_action_at(&base, "token", 99820, "test-device")
                .await
                .unwrap()
                .is_null());
            server.await.unwrap();
        });
    }
    #[test]
    fn player_requests_validate_numbers_and_use_expected_endpoints() {
        assert_eq!(
            super::player_request(99810, "").unwrap(),
            ("GET", "/me/player/currently-playing".into())
        );
        assert_eq!(
            super::player_request(99817, "1234").unwrap(),
            ("PUT", "/me/player/seek?position_ms=1234".into())
        );
        assert_eq!(
            super::player_request(99818, "80").unwrap(),
            ("PUT", "/me/player/volume?volume_percent=80".into())
        );
        assert!(super::player_request(99818, "101").is_err());
        assert!(super::player_request(99817, "-1").is_err());
        assert_eq!(
            super::player_request(99821, "true").unwrap(),
            ("PUT", "/me/player/shuffle?state=true".into())
        );
        assert_eq!(
            super::player_request(99822, "track").unwrap(),
            ("PUT", "/me/player/repeat?state=track".into())
        );
        assert!(super::player_request(99822, "off&device_id=other").is_err());
    }
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[test]
    fn track_links_are_validated() {
        let uri = "spotify:track:0123456789ABCDEFGHIJKL";
        assert_eq!(track_uri(uri).as_deref(), Some(uri));
        assert_eq!(track_uri("SPOTIFY:Track:0123456789ABCDEFGHIJKL").as_deref(), Some(uri));
        assert_eq!(
            track_uri("https://open.spotify.com/track/0123456789ABCDEFGHIJKL?si=123").as_deref(),
            Some(uri)
        );
        assert_eq!(
            track_uri("http://open.spotify.com/track/0123456789ABCDEFGHIJKL?si=123#top").as_deref(),
            Some(uri)
        );
        assert_eq!(
            track_uri("open.spotify.com/intl-pt-br/track/0123456789ABCDEFGHIJKL/").as_deref(),
            Some(uri)
        );
        assert_eq!(
            track_uri("HTTPS://OPEN.SPOTIFY.COM/embed/track/0123456789ABCDEFGHIJKL?utm_source=generator").as_deref(),
            Some(uri)
        );
        assert!(track_uri("https://evil.example/track/0123456789ABCDEFGHIJKL").is_none());
        assert!(track_uri("https://evil.example/open.spotify.com/track/0123456789ABCDEFGHIJKL").is_none());
        assert!(track_uri("https://open.spotify.com/track/0123456789ABCDEFGHIJKL/extra").is_none());
        assert!(track_uri("spotify:track:0123456789ABCDEFGHIJKL:extra").is_none());
        assert!(track_uri("spotify:track:bad").is_none());
    }

    #[test]
    fn case_expanding_unicode_never_panics_in_readings_or_classify() {
        // 'İ' (U+0130) is 2 bytes in UTF-8 and lowercases to 3 bytes ("i\u{307}").
        let track = t("Istanbul", &["Artist"], 50);
        assert!(score("İİİ by Artist", &track) >= 0.0);
        assert_eq!(
            classify("İİ open.spotify.com/album/0123456789ABCDEFGHIJKL"),
            Input::Search("İİ open.spotify.com/album/0123456789ABCDEFGHIJKL".into())
        );
        assert_eq!(
            classify("SPOTIFY:USER:demo:PLAYLIST:37i9dQZF1DXcBWIGoYBM5M"),
            Input::Unsupported("playlist")
        );
        assert_eq!(
            classify("https://open.spotify.com/user/demo/playlist/37i9dQZF1DXcBWIGoYBM5M?si=1"),
            Input::Unsupported("playlist")
        );
        assert_eq!(
            classify("http://open.spotify.com/intl-de/embed/album/4aawyAB9vmqN3uQ7FjRGTy"),
            Input::Unsupported("album")
        );
        assert_eq!(classify("http://spotify.app.link/abc"), Input::Unsupported("shortened"));
    }
    #[test]
    fn invalid_spotify_links_are_not_searched() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            for bad in [
                "https://open.spotify.com/track/invalid",
                "http://open.spotify.com/track/invalid",
                "open.spotify.com/intl-en/track/invalid",
                "SPOTIFY:TRACK:invalid",
            ] {
                assert_eq!(
                    find_at("http://127.0.0.1:1", "test", bad).await.unwrap_err(),
                    "Enter a valid Spotify track link or URI",
                    "{bad}"
                );
            }
            for (query, kind) in [
                ("spotify:album:123", "Album"),
                (
                    "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=1",
                    "Playlist",
                ),
                (
                    "https://open.spotify.com/intl-de/artist/0TnOYISbd1XYRBk9myaseg",
                    "Artist",
                ),
                ("https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ", "Episodes"),
            ] {
                let error = find_at("http://127.0.0.1:1", "test", query).await.unwrap_err();
                assert!(
                    error.starts_with(kind) && error.contains("not supported"),
                    "{query}: {error}"
                );
            }
            let short = find_at("http://127.0.0.1:1", "test", "https://spotify.link/abc")
                .await
                .unwrap_err();
            assert!(short.contains("short links"));
        });
    }
    fn t(name: &str, artists: &[&str], popularity: u32) -> Track {
        Track {
            uri: format!("spotify:track:{name}"),
            name: name.into(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            playing: false,
            duration_ms: 200_000,
            explicit: false,
            popularity,
            restricted: false,
        }
    }

    #[test]
    fn a_dash_never_reaches_the_search_and_the_best_track_wins() {
        assert_eq!(words("Fireworks - Katy Perry"), ["fireworks", "katy", "perry"]);
        assert_eq!(
            words("  AC/DC: Back in Black (2020) "),
            ["ac", "dc", "back", "in", "black", "2020"]
        );
        // What Spotify gave for "fireworks katy perry" when the dash was sent: popular tracks that are not it.
        let found = vec![
            t("Katy Perry Fireworks", &["MORA MORI", "eric degueu"], 0),
            t("Abracadabra", &["Lady Gaga"], 90),
            t("Fireworks", &["Katy Perry Karaoke's Band", "Made famous by"], 20),
            t("Firework", &["Katy Perry"], 80),
            t("Dark Horse", &["Katy Perry", "Juicy J"], 85),
        ];
        assert_eq!(best("fireworks - katy perry", found.clone()).unwrap().name, "Firework");
        assert_eq!(best("dark horse", found.clone()).unwrap().name, "Dark Horse");
        // A request that asks for the other recording gets it.
        let versions = vec![
            t("Firework", &["Katy Perry"], 80),
            t("Firework - Acoustic", &["Katy Perry"], 30),
        ];
        assert_eq!(
            best("firework acoustic katy perry", versions.clone()).unwrap().name,
            "Firework - Acoustic"
        );
        assert_eq!(best("firework katy perry", versions).unwrap().name, "Firework");
        // Nothing says who: Spotify's first is kept, not the last of equals.
        let same = vec![t("Gangnam Style", &["PSY"], 0), t("Gangnam Style", &["DJ Adam"], 0)];
        assert_eq!(best("gangnam style", same).unwrap().artists, ["PSY"]);
        assert!(best("anything", vec![]).is_none());
        // Words that mean nothing to any result are not a song.
        assert!(best(
            "gibberishqwerty zzzxxx",
            vec![t("Eu Sou O Gummy Bear", &["Gummy Bear"], 0)]
        )
        .is_none());
        // A typo or two still finds it.
        assert_eq!(
            best("firewrk katy perry", vec![t("Firework", &["Katy Perry"], 0)])
                .unwrap()
                .name,
            "Firework"
        );
    }

    #[test]
    fn failures_say_what_spotify_said() {
        let body = br#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#;
        assert!(failure(404, body).starts_with("Spotify has no active device"));
        assert!(failure(
            403,
            br#"{"error":{"status":403,"message":"Premium required","reason":"PREMIUM_REQUIRED"}}"#
        )
        .contains("Premium"));
        assert!(failure(
            403,
            br#"{"error":{"message":"User not registered in the Developer Dashboard"}}"#
        )
        .contains("not registered in the Developer Dashboard"));
        assert_eq!(failure(500, b""), "Spotify request failed (HTTP 500)");
        assert_eq!(failure(429, b"not json"), "Spotify rate limit reached; try again later");
    }

    #[test]
    fn rate_limit_is_reported() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket
                    .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
            });
            assert_eq!(
                request(&base, "GET", "/", "test").await.err().unwrap(),
                "Spotify rate limit reached; try again later"
            );
            server.await.unwrap();
        });
    }
    #[test]
    fn playback_and_queue_use_the_api() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                for (method, path, body) in [
                    ("GET", "/me/player/currently-playing", r#"{"is_playing":true,"item":{"uri":"spotify:track:0123456789ABCDEFGHIJKL","name":"Song","artists":[{"name":"Artist"}]}}"#),
                    ("GET", "/search?type=track&limit=10&q=some+song&market=from_token", r#"{"tracks":{"items":[{"uri":"spotify:track:0123456789ABCDEFGHIJKL","name":"Some Song","artists":[{"name":"Artist"}],"duration_ms":1000}]}}"#),
                    ("POST", "/me/player/queue?uri=spotify%3Atrack%3A0123456789ABCDEFGHIJKL", ""),
                    ("PUT", "/me/player/play", ""),
                    ("PUT", "/me/player/pause", ""),
                    ("POST", "/me/player/next", ""),
                    ("POST", "/me/player/previous", "")
                ] {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut buf = [0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]);
                    assert!(req.starts_with(&format!("{method} {path} HTTP/1.1\r\n")), "{req}");
                    assert!(req.contains("Authorization: Bearer test\r\n"));
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    socket.write_all(response.as_bytes()).await.unwrap();
                }
            });
            let current = current_at(&base, "test").await.unwrap().unwrap();
            assert_eq!(current.name, "Song");
            assert_eq!(current.artists, ["Artist"]);
            assert!(current.playing);
            assert_eq!(add_at(&base, "test", "some song").await.unwrap(), current.uri);
            for command in ["play", "pause", "next", "previous"] {
                playback_at(&base, "test", command).await.unwrap();
            }
            assert!(playback_at(&base, "test", "invalid").await.is_err());
            server.await.unwrap();
        });
    }

    /// A Spotify that answers these requests in order: (method, path, status, body).
    async fn serve(script: Vec<(&'static str, String, u16, &'static str)>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (method, path, status, body) in script {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                assert!(
                    req.starts_with(&format!("{method} {path} HTTP/1.1\r\n")),
                    "wanted {method} {path}, got {req}"
                );
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (base, server)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn the_queue_uses_the_device_and_survives_a_gone_device_and_a_busy_spotify() {
        const URI: &str = "spotify:track:0123456789ABCDEFGHIJKL";
        const ENC: &str = "spotify%3Atrack%3A0123456789ABCDEFGHIJKL";
        rt().block_on(async {
            let (base, server) = serve(vec![(
                "POST",
                format!("/me/player/queue?uri={ENC}&device_id=dev1"),
                204,
                "",
            )])
            .await;
            add_uri_at(&base, "t", URI, "dev1", std::time::Duration::ZERO)
                .await
                .unwrap();
            server.await.unwrap();
            // The device is gone (404): Spotify picks the active one.
            let gone = r#"{"error":{"status":404,"message":"Device not found","reason":"NO_ACTIVE_DEVICE"}}"#;
            let (base, server) = serve(vec![
                ("POST", format!("/me/player/queue?uri={ENC}&device_id=dev1"), 404, gone),
                ("POST", format!("/me/player/queue?uri={ENC}"), 204, ""),
            ])
            .await;
            add_uri_at(&base, "t", URI, "dev1", std::time::Duration::ZERO)
                .await
                .unwrap();
            server.await.unwrap();
            // 503 is tried again.
            let (base, server) = serve(vec![
                ("POST", format!("/me/player/queue?uri={ENC}"), 503, ""),
                ("POST", format!("/me/player/queue?uri={ENC}"), 503, ""),
                ("POST", format!("/me/player/queue?uri={ENC}"), 204, ""),
            ])
            .await;
            add_uri_at(&base, "t", URI, "", std::time::Duration::ZERO)
                .await
                .unwrap();
            server.await.unwrap();
            // No device at all: Spotify's reason, in words.
            let (base, server) = serve(vec![("POST", format!("/me/player/queue?uri={ENC}"), 404, gone)]).await;
            let error = add_uri_at(&base, "t", URI, "", std::time::Duration::ZERO)
                .await
                .unwrap_err();
            assert!(error.starts_with("Spotify has no active device"), "{error}");
            server.await.unwrap();
        });
    }

    #[test]
    fn the_player_reads_the_track_the_queue_the_device_and_the_volume() {
        rt().block_on(async {
            let queue = r#"{"currently_playing":null,"queue":[
                {"uri":"spotify:track:0123456789ABCDEFGHIJKL","name":"Next","artists":[{"name":"A"},{"name":"B"}],"duration_ms":1000,"explicit":true}]}"#;
            let state = r#"{"is_playing":true,"progress_ms":4000,"device":{"id":"dev1","volume_percent":35},
                "item":{"uri":"spotify:track:ABCDEFGHIJKLMNOPQRSTUV","name":"Now","artists":[{"name":"C"}],"duration_ms":9000}}"#;
            let (base, server) = serve(vec![
                ("GET", "/me/player/queue".into(), 200, queue),
                ("GET", "/me/player?market=from_token".into(), 200, state),
            ])
            .await;
            let p = player_at(&base, "t").await.unwrap();
            server.await.unwrap();
            assert_eq!((p.playing, p.progress_ms, p.device_id.as_str(), p.volume), (true, 4000, "dev1", Some(35)));
            assert_eq!(p.current.unwrap().name, "Now");
            assert_eq!(p.queue.len(), 1);
            assert!(p.queue[0].explicit);
            assert_eq!(p.queue[0].artist(), "A, B");
            // Nothing plays anywhere (204): no track, no device.
            let (base, server) = serve(vec![
                ("GET", "/me/player/queue".into(), 200, r#"{"currently_playing":null,"queue":[]}"#),
                ("GET", "/me/player?market=from_token".into(), 204, ""),
            ])
            .await;
            let p = player_at(&base, "t").await.unwrap();
            server.await.unwrap();
            assert!(p.current.is_none() && !p.playing && p.device_id.is_empty());
        });
    }

    #[test]
    fn a_track_link_is_looked_up_with_the_streamers_market() {
        rt().block_on(async {
            let track = r#"{"uri":"spotify:track:0123456789ABCDEFGHIJKL","name":"T","artists":[{"name":"A"}],"duration_ms":5,"is_playable":false,"restrictions":{"reason":"market"}}"#;
            let (base, server) = serve(vec![("GET", "/tracks/0123456789ABCDEFGHIJKL?market=from_token".into(), 200, track)]).await;
            let t = find_at(&base, "t", "https://open.spotify.com/intl-pt/track/0123456789ABCDEFGHIJKL?si=x").await.unwrap();
            server.await.unwrap();
            assert!(t.restricted, "Spotify says it does not play in the streamer's country");
        });
    }

    #[test]
    fn an_old_login_without_the_country_permission_still_searches() {
        rt().block_on(async {
            let scope = r#"{"error":{"status":403,"message":"Insufficient client scope"}}"#;
            let found = r#"{"tracks":{"items":[{"uri":"spotify:track:0123456789ABCDEFGHIJKL","name":"Firework","artists":[{"name":"Katy Perry"}]}]}}"#;
            let (base, server) = serve(vec![
                ("GET", "/search?type=track&limit=10&q=firework&market=from_token".into(), 403, scope),
                ("GET", "/search?type=track&limit=10&q=firework".into(), 200, found),
            ])
            .await;
            assert_eq!(find_at(&base, "t", "firework").await.unwrap().name, "Firework");
            server.await.unwrap();
        });
    }

    #[test]
    fn classifying_what_a_viewer_typed() {
        assert_eq!(
            classify("https://open.spotify.com/track/0123456789ABCDEFGHIJKL?si=1"),
            Input::Track("spotify:track:0123456789ABCDEFGHIJKL".into())
        );
        assert_eq!(classify("spotify:playlist:abc"), Input::Unsupported("playlist"));
        assert_eq!(
            classify("https://open.spotify.com/show/abc"),
            Input::Unsupported("episode")
        );
        assert_eq!(classify("https://spotify.link/xyz"), Input::Unsupported("shortened"));
        assert_eq!(
            classify("  katy perry firework "),
            Input::Search("katy perry firework".into())
        );
        assert_eq!(
            track_uri_of("0123456789ABCDEFGHIJKL").as_deref(),
            Some("spotify:track:0123456789ABCDEFGHIJKL")
        );
        assert!(track_uri_of("short").is_none());
    }

    #[test]
    fn titles_and_artists_read_either_way_round_and_accents_do_not_matter() {
        let t = |name: &str, artists: &[&str]| Track {
            uri: format!("spotify:track:{name}"),
            name: name.into(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            playing: false,
            duration_ms: 1,
            explicit: false,
            popularity: 0,
            restricted: false,
        };
        let found = vec![
            t("Back To Black Tribute", &["Cover Band"]),
            t("Back to Black", &["Amy Winehouse"]),
            t("Rehab", &["Amy Winehouse"]),
        ];
        for q in [
            "back to black - amy winehouse",
            "amy winehouse - back to black",
            "back to black by amy winehouse",
            "Amy Winehouse Back to Black",
        ] {
            assert_eq!(best(q, found.clone()).unwrap().name, "Back to Black", "{q}");
        }
        let found = vec![t("Despacito", &["Luis Fonsi"]), t("Já Era", &["Anitta"])];
        assert_eq!(best("ja era anitta", found.clone()).unwrap().name, "Já Era");
        assert_eq!(words("Beyoncé - Halo (Ñandú)"), ["beyonce", "halo", "nandu"]);
    }

    #[test]
    fn volume_and_likes_use_the_api() {
        const URI: &str = "spotify:track:0123456789ABCDEFGHIJKL";
        const ENC: &str = "spotify%3Atrack%3A0123456789ABCDEFGHIJKL";
        rt().block_on(async {
            let (base, server) = serve(vec![
                (
                    "PUT",
                    "/me/player/volume?volume_percent=100&device_id=d1".into(),
                    204,
                    "",
                ),
                ("PUT", format!("/me/library?uris={ENC}"), 404, ""),
                ("PUT", "/me/tracks?ids=0123456789ABCDEFGHIJKL".into(), 200, ""),
            ])
            .await;
            set_volume_at(&base, "t", 150, "d1").await.unwrap();
            like_at(&base, "t", URI).await.unwrap();
            server.await.unwrap();
        });
        assert_eq!(
            failure(403, b"{}"),
            "Spotify denied the request; check the account, app access and Premium status"
        );
    }

    #[test]
    fn rate_limit_retries_short_retry_after_and_fails_fast_on_long() {
        assert_eq!(
            retry_after(&[("Retry-After".into(), " 3 ".into())]),
            Some(std::time::Duration::from_secs(3))
        );
        assert_eq!(retry_after(&[("retry-after".into(), "not-a-number".into())]), None);
        rt().block_on(async {
            // Short Retry-After (0s): retries once and succeeds.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let replies: [&[u8]; 2] = [
                    b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                ];
                for reply in replies {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut buf = [0; 4096];
                    assert!(socket.read(&mut buf).await.unwrap() > 0);
                    socket.write_all(reply).await.unwrap();
                }
            });
            let r = request(&base, "GET", "/me", "t").await.unwrap();
            assert_eq!(r.status, 200);
            server.await.unwrap();

            // Long Retry-After (60s): fails immediately without sleeping.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                assert!(socket.read(&mut buf).await.unwrap() > 0);
                socket
                    .write_all(
                        b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            });
            let started = std::time::Instant::now();
            assert_eq!(
                request(&base, "GET", "/me", "t").await.err().unwrap(),
                "Spotify rate limit reached; try again later"
            );
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            server.await.unwrap();

            // Empty token fails before touching the network (also for 99820 transfer).
            assert_eq!(
                player_action_at("http://127.0.0.1:1", "", 99820, "dev1")
                    .await
                    .unwrap_err(),
                "Spotify login is required"
            );
        });
    }

    #[test]
    fn player_handles_204_queue_and_empty_device_ids() {
        rt().block_on(async {
            let devices = r#"{"devices":[{"id":"","is_restricted":false,"is_active":true},{"id":"d2","is_restricted":false,"is_active":false,"volume_percent":60}]}"#;
            let (base, server) = serve(vec![
                ("GET", "/me/player/queue".into(), 204, ""),
                ("GET", "/me/player?market=from_token".into(), 204, ""),
                ("GET", "/me/player/devices".into(), 200, devices),
            ])
            .await;
            let p = player_at(&base, "t").await.unwrap();
            server.await.unwrap();
            assert!(p.current.is_none() && p.queue.is_empty());
            assert_eq!((p.device_id.as_str(), p.volume), ("d2", Some(60)));
        });
    }

    #[test]
    fn saved_token_refreshes_stale_and_clears_revoked_without_looping() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rt().block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let token_url = format!("http://{}/api/token", listener.local_addr().unwrap());
                    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
                    let hits_srv = hits.clone();
                    let server = tokio::spawn(async move {
                        let replies: [(u16, &str); 2] = [
                            (200, r#"{"access_token":"fresh-access","expires_in":3600}"#),
                            (
                                400,
                                r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#,
                            ),
                        ];
                        for (status, body) in replies {
                            let (mut socket, _) = listener.accept().await.unwrap();
                            hits_srv.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let mut buf = [0u8; 4096];
                            let _ = socket.read(&mut buf).await;
                            let resp = format!(
                                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            socket.write_all(resp.as_bytes()).await.unwrap();
                        }
                    });
                    let mut config = crate::Config::default();
                    config.spotify = SpotifyConfig {
                        enabled: true,
                        local_monitor: false,
                        client_id: "cid".into(),
                        token: Token {
                            access_token: "old-access".into(),
                            refresh_token: "keep-refresh".into(),
                            expires_at: 1, // stale
                        },
                    };
                    let e = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
                    // Stale token refreshes, retains the refresh token when omitted, and caches the new one.
                    assert_eq!(saved_token_at(&e, &token_url).await.unwrap(), "fresh-access");
                    assert_eq!(e.config().spotify.token.refresh_token, "keep-refresh");
                    assert_eq!(saved_token_at(&e, &token_url).await.unwrap(), "fresh-access");
                    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);

                    // Revoked refresh token clears saved token so later calls do not loop.
                    e.change(|c| c.spotify.token.expires_at = 1);
                    let err = saved_token_at(&e, &token_url).await.unwrap_err();
                    assert!(err.contains("invalid_grant"), "{err}");
                    assert!(e.config().spotify.token.access_token.is_empty());
                    assert!(e.config().spotify.token.refresh_token.is_empty());
                    assert_eq!(
                        saved_token_at(&e, &token_url).await.unwrap_err(),
                        "Spotify login is required"
                    );
                    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
                    server.await.unwrap();
                })
                .await;
        });
    }

    #[test]
    fn local_track_conversion_keeps_duration_and_handles_empty_artist() {
        let with_artist = Track::from(local::LocalTrack {
            title: "Song".into(),
            artist: "Singer".into(),
            playing: true,
            duration_ms: 215_000,
            ..Default::default()
        });
        assert_eq!(with_artist.duration_ms, 215_000);
        assert_eq!(with_artist.artists, ["Singer"]);
        assert!(with_artist.uri.is_empty());

        let no_artist = Track::from(local::LocalTrack {
            title: "Track".into(),
            artist: String::new(),
            ..Default::default()
        });
        assert!(no_artist.artists.is_empty());
    }

    /// Against the real Spotify: `STREAMERZ_SPOTIFY_CLIENT_ID=... cargo test live_spotify -- --ignored --nocapture`.
    /// Opens the browser to log in. With STREAMERZ_SPOTIFY_QUEUE=<song name> it also queues that song.
    #[test]
    #[ignore]
    fn live_spotify() {
        let id = std::env::var("STREAMERZ_SPOTIFY_CLIENT_ID").expect("set STREAMERZ_SPOTIFY_CLIENT_ID");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let token = login(&id).await.expect("login");
            println!("logged in; refresh token kept: {}", !token.refresh_token.is_empty());
            let refreshed = refresh(&id, &token).await.expect("refresh");
            println!("refreshed: {}", !refreshed.access_token.is_empty());
            let access = refreshed.access_token;
            println!("current: {:?}", currently_playing(&access).await);
            if let Ok(p) = player(&access).await {
                println!(
                    "player: playing={} progress={}ms current={:?} queue={:?}",
                    p.playing,
                    p.progress_ms,
                    p.current.map(|t| t.name),
                    p.queue.iter().map(|t| t.name.as_str()).collect::<Vec<_>>()
                );
            }
            for q in [
                "fireworks - katy perry",
                "Never Gonna Give You Up - Rick Astley",
                "gangnam style",
            ] {
                match find(&access, q).await {
                    Ok(t) => println!(
                        "find {q:?}: {} by {} ({}ms, explicit {})",
                        t.name,
                        t.artist(),
                        t.duration_ms,
                        t.explicit
                    ),
                    Err(e) => println!("find {q:?}: {e}"),
                }
            }
            println!(
                "find: {:?}",
                find(&access, "Never Gonna Give You Up - Rick Astley").await
            );
            if let Ok(song) = std::env::var("STREAMERZ_SPOTIFY_QUEUE") {
                println!("queue {song:?}: {:?}", add_to_queue(&access, &song).await);
            }
        });
    }

    /// Which devices Spotify knows: `... cargo test live_devices -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_devices() {
        let id = std::env::var("STREAMERZ_SPOTIFY_CLIENT_ID").expect("set STREAMERZ_SPOTIFY_CLIENT_ID");
        rt().block_on(async {
            let token = login(&id).await.expect("login").access_token;
            let r = request(API, "GET", "/me/player/devices", &token).await.unwrap();
            println!("devices: {}", String::from_utf8_lossy(&r.body));
        });
    }

    /// The whole chain against the real Spotify, from chat, every command of the template:
    /// `STREAMERZ_SPOTIFY_CLIENT_ID=... STREAMERZ_SPOTIFY_QUEUE="new song" cargo test live_chat -- --ignored
    /// --nocapture`. It queues that song, skips the song that plays, likes it and, if STREAMERZ_SPOTIFY_PLAY
    /// is set, resumes and pauses playback.
    #[test]
    #[ignore]
    fn live_chat() {
        let id = std::env::var("STREAMERZ_SPOTIFY_CLIENT_ID").expect("set STREAMERZ_SPOTIFY_CLIENT_ID");
        let song = std::env::var("STREAMERZ_SPOTIFY_QUEUE").expect("set STREAMERZ_SPOTIFY_QUEUE");
        let transport = std::env::var("STREAMERZ_SPOTIFY_PLAY").is_ok();
        rt().block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let token = login(&id).await.expect("login");
                    let mut config = crate::Config {
                        spotify: SpotifyConfig {
                            enabled: true,
                            local_monitor: false,
                            client_id: id,
                            token,
                        },
                        ..Default::default()
                    };
                    crate::templates::songs::made().add_to(&mut config).unwrap();
                    let mut values = crate::groups::values(&config, "Song requests");
                    values.insert("voteSkipCount".into(), 1.into());
                    values.insert("skipCooldown".into(), 0.into());
                    crate::groups::set_values(&mut config, "Song requests", values).unwrap();
                    config.commands.iter_mut().for_each(|c| {
                        c.insert("userCooldown".into(), 0.into());
                        c.insert("globalCooldown".into(), 0.into());
                    });
                    let e = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
                    e.dev_mode();
                    let mut script: Vec<(&str, String)> = vec![
                        ("viewer", "!song".into()),
                        ("viewer", "!next".into()),
                        ("viewer", "!queue".into()),
                        ("viewer", "!pos".into()),
                        ("viewer", "!sr".into()),
                        ("viewer", "!sr gibberishqwertyuiop zzzxxxcccvvv".into()),
                        ("viewer", format!("!sr {song}")),
                        ("viewer", "!pos".into()),
                        ("viewer", "!queue".into()),
                        ("viewer", format!("!sr {song}")),
                        (
                            "viewer",
                            "!sr https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy".into(),
                        ),
                        ("viewer", "!sr https://spotify.link/abc".into()),
                        ("viewer", "!remove".into()),
                        ("viewer", "!remove".into()),
                        ("viewer", "!skip".into()),
                        ("viewer", "!vol".into()),
                        ("moderator", "!vol".into()),
                    ];
                    if transport {
                        script.push(("moderator", "!play".into()));
                        script.push(("moderator", "!pause".into()));
                    }
                    script.extend([
                        ("moderator", "!songlike".into()),
                        ("viewer", "!voteskip".into()),
                        ("moderator", "!bansong".into()),
                        ("viewer", "!song".into()),
                    ]);
                    for (role, text) in script {
                        let args = serde_json::json!({"args": {"isModerator": role == "moderator"}});
                        e.inject_chat("twitch", role, "", role, &text, &args).unwrap();
                        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
                        let said: Vec<String> = e
                            .dev_take()
                            .unwrap()
                            .iter()
                            .map(|m| m["text"].as_str().unwrap_or("").into())
                            .collect();
                        println!(
                            "[{role}] {text}\n    -> {}",
                            if said.is_empty() {
                                "(nothing)".into()
                            } else {
                                said.join(" / ")
                            }
                        );
                    }
                    println!("requests kept: {:?}", e.song_requests());
                    // The songs that were taken back are skipped by the watcher when they play.
                    tokio::time::sleep(std::time::Duration::from_secs(25)).await;
                    println!("after 25 s, requests kept: {:?}", e.song_requests());
                    let token = e.config().spotify.token.access_token.clone();
                    let p = player(&token).await.unwrap();
                    println!(
                        "SPOTIFY QUEUE NOW: {:?}",
                        p.queue.iter().take(25).map(|t| t.full()).collect::<Vec<_>>()
                    );
                })
                .await
        });
    }
}
