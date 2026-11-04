//! The rest of Spotify's Web API as sub-actions (99840 to 99875): more player controls, the library,
//! follows, playlists, the profile, top items and search. Each takes up to two values; the port carries
//! them as one text, `value` and `extra` split by a new line (`pack` and `unpack`).
use super::{call, find_at, player_at, request, with_market};
use crate::commands::url_encode;
use serde_json::{json, Value};

/// The two values of an action as the port carries them.
pub fn pack(value: &str, extra: &str) -> String {
    if extra.is_empty() {
        value.to_string()
    } else {
        format!("{value}\n{extra}")
    }
}

pub fn unpack_first(text: &str) -> String {
    unpack(text).0.to_string()
}

fn unpack(text: &str) -> (&str, &str) {
    match text.split_once('\n') {
        Some((a, b)) => (a.trim(), b.trim()),
        None => (text.trim(), ""),
    }
}

/// What a Spotify link, URI or bare id points at: its kind and id.
pub fn reference(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    let ok = |id: &str| id.len() == 22 && id.bytes().all(|c| c.is_ascii_alphanumeric());
    if let Some(rest) = t.strip_prefix("spotify:") {
        let (kind, id) = rest.split_once(':')?;
        return ok(id).then(|| (kind.to_string(), id.to_string()));
    }
    if let Some(rest) = t.strip_prefix("https://open.spotify.com/") {
        let rest = match rest.strip_prefix("intl-") {
            Some(r) => r.split_once('/')?.1,
            None => rest,
        };
        let (kind, id) = rest.split_once('/')?;
        let id = id.split(['?', '#', '/']).next()?;
        return ok(id).then(|| (kind.to_string(), id.to_string()));
    }
    None
}

/// A playlist's id from a link, URI or the id itself.
fn playlist_id(text: &str) -> Result<String, String> {
    match reference(text) {
        Some((kind, id)) if kind == "playlist" => Ok(id),
        Some(_) => Err("That is not a Spotify playlist".into()),
        None if text.trim().len() == 22 => Ok(text.trim().to_string()),
        None => Err("Enter a Spotify playlist link, URI or id".into()),
    }
}

async fn send(base: &str, method: &str, path: &str, token: &str, body: Option<Value>) -> Result<Value, String> {
    let Some(body) = body else {
        let r = request(base, method, path, token).await?;
        return Ok(serde_json::from_slice(&r.body).unwrap_or(Value::Null));
    };
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    let headers = [
        ("Authorization".into(), format!("Bearer {token}")),
        ("Content-Type".into(), "application/json".into()),
    ];
    let r = crate::http::request(method, &format!("{base}{path}"), &headers, &bytes).await?;
    if !(200..300).contains(&r.status) {
        return Err(super::failure(r.status, &r.body));
    }
    Ok(serde_json::from_slice(&r.body).unwrap_or(Value::Null))
}

async fn get(base: &str, token: &str, path: &str) -> Result<Value, String> {
    send(base, "GET", path, token, None).await
}

/// The song that plays now, as Spotify describes it (ids of its artist and album included).
async fn playing(base: &str, token: &str) -> Result<Value, String> {
    let r = call(base, "GET", "/me/player/currently-playing", token)
        .await
        .map_err(|e| e.1)?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
    if r.status == 204 || v["item"].is_null() {
        return Err("Nothing is playing".into());
    }
    Ok(v["item"].clone())
}

fn track_id(item: &Value) -> Result<String, String> {
    item["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "That is a local file, Spotify cannot do this with it".into())
}

fn artist_id(item: &Value) -> Result<String, String> {
    item["artists"][0]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Spotify did not say who the artist is".into())
}

/// A player action with a body, starting a device when none is active.
async fn play_body(base: &str, token: &str, body: Value) -> Result<(), String> {
    let first = send(base, "PUT", "/me/player/play", token, Some(body.clone())).await;
    match first {
        Err(e) if e.contains("no active device") => {
            let p = player_at(base, token).await?;
            if p.device_id.is_empty() {
                return Err(e);
            }
            send(
                base,
                "PUT",
                &format!("/me/player/play?device_id={}", url_encode(&p.device_id)),
                token,
                Some(body),
            )
            .await
            .map(|_| ())
        }
        other => other.map(|_| ()),
    }
}

async fn state(base: &str, token: &str) -> Result<Value, String> {
    let v = get(base, token, "/me/player").await?;
    if v.is_null() {
        return Err("Spotify has no active device: start playing something in Spotify first".into());
    }
    Ok(v)
}

fn number(text: &str, what: &str) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .map_err(|_| format!("Invalid Spotify {what}"))
}

fn range(text: &str) -> Result<&'static str, String> {
    Ok(match text.trim().to_ascii_lowercase().as_str() {
        "" | "medium" | "medium_term" | "6months" => "medium_term",
        "short" | "short_term" | "4weeks" => "short_term",
        "long" | "long_term" | "alltime" => "long_term",
        _ => return Err("The period is short, medium or long".into()),
    })
}

pub async fn run(base: &str, token: &str, kind: u32, text: &str) -> Result<Value, String> {
    let (value, extra) = unpack(text);
    let done = Ok(Value::Null);
    match kind {
        // Player.
        99840 => {
            let body = match reference(value) {
                Some((k, id)) if k == "track" => json!({"uris": [format!("spotify:track:{id}")]}),
                Some((k, id)) if matches!(k.as_str(), "album" | "playlist" | "artist") => {
                    json!({"context_uri": format!("spotify:{k}:{id}")})
                }
                Some(_) => return Err("Spotify cannot play that".into()),
                None => json!({"uris": [find_at(base, token, value).await?.uri]}),
            };
            play_body(base, token, body).await?;
            done
        }
        99841 => {
            let playing = state(base, token).await?["is_playing"].as_bool().unwrap_or(false);
            let path = if playing { "/me/player/pause" } else { "/me/player/play" };
            send(base, "PUT", path, token, None).await?;
            Ok(json!(!playing))
        }
        99842 => {
            send(base, "PUT", "/me/player/seek?position_ms=0", token, None).await?;
            done
        }
        99843 => {
            let delta = number(value, "volume change")?;
            let now = state(base, token).await?["device"]["volume_percent"]
                .as_i64()
                .unwrap_or(50);
            let volume = (now + delta).clamp(0, 100);
            send(
                base,
                "PUT",
                &format!("/me/player/volume?volume_percent={volume}"),
                token,
                None,
            )
            .await?;
            Ok(json!(volume))
        }
        99844 => {
            let delta = number(value, "time")?;
            let now = state(base, token).await?["progress_ms"].as_i64().unwrap_or(0);
            let to = (now + delta).max(0);
            send(base, "PUT", &format!("/me/player/seek?position_ms={to}"), token, None).await?;
            Ok(json!(to))
        }
        // Library and follows.
        99850 => {
            let id = track_id(&playing(base, token).await?)?;
            send(base, "DELETE", &format!("/me/tracks?ids={id}"), token, None).await?;
            done
        }
        99851 => {
            let id = track_id(&playing(base, token).await?)?;
            let v = get(base, token, &format!("/me/tracks/contains?ids={id}")).await?;
            Ok(json!(v[0].as_bool().unwrap_or(false)))
        }
        99852 => get(base, token, "/me/tracks?limit=50").await,
        99853 | 99854 => {
            let id = artist_id(&playing(base, token).await?)?;
            let method = if kind == 99853 { "PUT" } else { "DELETE" };
            send(
                base,
                method,
                &format!("/me/following?type=artist&ids={id}"),
                token,
                None,
            )
            .await?;
            done
        }
        99855 => get(base, token, "/me/following?type=artist&limit=50").await,
        // Playlists.
        99860 => get(base, token, "/me/playlists?limit=50").await,
        99861 => {
            if value.is_empty() {
                return Err("Enter a name for the playlist".into());
            }
            send(
                base,
                "POST",
                "/me/playlists",
                token,
                Some(json!({"name": value, "description": extra, "public": false})),
            )
            .await
        }
        99862..=99864 => {
            let id = playlist_id(value)?;
            let uri = match kind {
                99863 => find_at(base, token, extra).await?.uri,
                _ => {
                    let item = playing(base, token).await?;
                    item["uri"].as_str().unwrap_or_default().to_string()
                }
            };
            if kind == 99864 {
                send(
                    base,
                    "DELETE",
                    &format!("/playlists/{id}/tracks"),
                    token,
                    Some(json!({"tracks": [{"uri": uri}]})),
                )
                .await?;
            } else {
                send(
                    base,
                    "POST",
                    &format!("/playlists/{id}/tracks"),
                    token,
                    Some(json!({"uris": [uri]})),
                )
                .await?;
            }
            done
        }
        99865 => {
            let id = playlist_id(value)?;
            get(base, token, &format!("/playlists/{id}/tracks?limit=50")).await
        }
        99866 => {
            let id = playlist_id(value)?;
            if extra.is_empty() {
                return Err("Enter the new name of the playlist".into());
            }
            send(
                base,
                "PUT",
                &format!("/playlists/{id}"),
                token,
                Some(json!({"name": extra})),
            )
            .await?;
            done
        }
        // Profile, top items, search and details of what plays.
        99870 => get(base, token, "/me").await,
        99871 | 99872 => {
            let what = if kind == 99871 { "tracks" } else { "artists" };
            get(
                base,
                token,
                &format!("/me/top/{what}?time_range={}&limit=20", range(value)?),
            )
            .await
        }
        99873 => {
            if value.is_empty() {
                return Err("Enter what to search for".into());
            }
            let what = match extra.to_ascii_lowercase().as_str() {
                "" | "track" => "track",
                "artist" => "artist",
                "album" => "album",
                "playlist" => "playlist",
                _ => return Err("Search for a track, artist, album or playlist".into()),
            };
            let path = format!("/search?q={}&type={what}&limit=10", url_encode(value));
            let r = with_market(base, token, &path).await?;
            serde_json::from_slice(&r.body).map_err(|_| "Invalid Spotify response".into())
        }
        99874 => {
            let id = artist_id(&playing(base, token).await?)?;
            get(base, token, &format!("/artists/{id}")).await
        }
        99875 => {
            let item = playing(base, token).await?;
            let id = item["album"]["id"].as_str().ok_or("Spotify did not say the album")?;
            get(base, token, &format!("/albums/{id}")).await
        }
        _ => Err("Unknown Spotify action".into()),
    }
}

/// The names in a result, for a text that fits a chat message: "Song - Artist, Song - Artist...".
pub fn summary(value: &Value) -> (usize, String) {
    fn list(v: &Value) -> Option<&Vec<Value>> {
        for key in ["items", "queue", "devices", "tracks", "artists", "albums", "playlists"] {
            match &v[key] {
                Value::Array(a) => return Some(a),
                o @ Value::Object(_) => {
                    if let Some(a) = list(o) {
                        return Some(a);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let items = match value {
        Value::Array(a) => Some(a),
        v => list(v),
    };
    let Some(items) = items else {
        let name = value["display_name"].as_str().or_else(|| value["name"].as_str());
        return (usize::from(name.is_some()), name.unwrap_or_default().to_string());
    };
    let names: Vec<String> = items
        .iter()
        .filter_map(|i| {
            let i = if i["track"].is_object() { &i["track"] } else { i };
            let name = i["name"].as_str()?;
            Some(match i["artists"][0]["name"].as_str() {
                Some(a) => format!("{name} - {a}"),
                None => name.to_string(),
            })
        })
        .collect();
    let text = names.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
    (items.len(), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_say_what_they_point_at() {
        let id = "4cOdK2wGLETKBW3PvgPWqT";
        assert_eq!(
            reference(&format!("https://open.spotify.com/playlist/{id}?si=x")),
            Some(("playlist".into(), id.into()))
        );
        assert_eq!(
            reference(&format!("https://open.spotify.com/intl-pt/album/{id}")),
            Some(("album".into(), id.into()))
        );
        assert_eq!(
            reference(&format!("spotify:artist:{id}")),
            Some(("artist".into(), id.into()))
        );
        assert_eq!(reference("fireworks katy perry"), None);
        assert_eq!(playlist_id(id).unwrap(), id);
        assert!(playlist_id(&format!("spotify:track:{id}")).is_err());
        assert!(playlist_id("nope").is_err());
    }

    #[test]
    fn two_values_travel_as_one_text() {
        assert_eq!(unpack(&pack("a", "b c")), ("a", "b c"));
        assert_eq!(unpack(&pack("a", "")), ("a", ""));
    }

    #[test]
    fn a_result_becomes_a_line_of_names() {
        let v = json!({"items": [
            {"track": {"name": "Fireworks", "artists": [{"name": "Katy Perry"}]}},
            {"name": "Chill"}
        ]});
        assert_eq!(summary(&v), (2, "Fireworks - Katy Perry, Chill".into()));
        let found = json!({"tracks": {"items": [{"name": "A", "artists": [{"name": "B"}]}]}});
        assert_eq!(summary(&found), (1, "A - B".into()));
        assert_eq!(summary(&json!({"display_name": "Pedro"})), (1, "Pedro".into()));
        assert_eq!(summary(&Value::Null), (0, String::new()));
    }

    #[test]
    fn periods_are_checked() {
        assert_eq!(range("long").unwrap(), "long_term");
        assert_eq!(range("").unwrap(), "medium_term");
        assert!(range("year").is_err());
    }
}
