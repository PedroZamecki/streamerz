//! Finding a channel by name, as the platforms' own sites do: no login, no key. Twitch and Kick answer
//! with a list; YouTube's search page carries it.

use serde_json::{json, Value};

/// A channel found.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Found {
    /// What goes in the channel box (a login, a slug, a channel id).
    pub handle: String,
    pub name: String,
    pub picture: String,
    /// "1.2M followers", as the platform counts them.
    pub detail: String,
    pub live: bool,
}

/// A count as people say it: 1.2M, 340K.
fn compact(n: u64) -> String {
    let tenths = |n: u64, unit: u64| {
        let t = n * 10 / unit;
        if t % 10 == 0 || t >= 100 {
            format!("{}", t / 10)
        } else {
            format!("{}.{}", t / 10, t % 10)
        }
    };
    match n {
        1_000_000.. => format!("{}M", tenths(n, 1_000_000)),
        1_000.. => format!("{}K", tenths(n, 1_000)),
        _ => n.to_string(),
    }
}

/// Whether `platform` can be searched.
pub fn can(platform: &str) -> bool {
    matches!(platform, "twitch" | "kick" | "youtube")
}

/// The channels whose name matches `query` on `platform` (the best first).
pub async fn channels(platform: &str, query: &str) -> Vec<Found> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    match platform {
        "twitch" => twitch(query).await,
        "kick" => kick(query).await,
        "youtube" => youtube(query).await,
        _ => Vec::new(),
    }
}

async fn twitch(query: &str) -> Vec<Found> {
    // twitch.tv's own search, asked anonymously with its web client id.
    let headers = [
        ("Client-Id".to_string(), "kimne78kx3ncx6brgo4mv6wki5h1ko".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    let q = json!({
        "query": "query($q: String!) { searchUsers(userQuery: $q, first: 12) { edges { node { login displayName \
                  profileImageURL(width: 50) followers { totalCount } stream { id } } } } }",
        "variables": {"q": query},
    });
    let Ok(r) = crate::http::request("POST", "https://gql.twitch.tv/gql", &headers, q.to_string().as_bytes()).await
    else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&r.body)
        .map(|v| twitch_found(&v))
        .unwrap_or_default()
}

fn twitch_found(v: &Value) -> Vec<Found> {
    v["data"]["searchUsers"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let n = &e["node"];
            let handle = n["login"].as_str()?.to_string();
            Some(Found {
                name: n["displayName"].as_str().unwrap_or(&handle).to_string(),
                picture: n["profileImageURL"].as_str().unwrap_or("").to_string(),
                detail: n["followers"]["totalCount"].as_u64().map(compact).unwrap_or_default(),
                live: n["stream"].is_object(),
                handle,
            })
        })
        .collect()
}

async fn kick(query: &str) -> Vec<Found> {
    let url = format!("https://kick.com/api/search?searched_word={}", crate::commands::url_encode(query));
    let h = [("Accept".to_string(), "application/json".to_string())];
    let Ok(r) = crate::http::request("GET", &url, &h, b"").await else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&r.body)
        .map(|v| kick_found(&v))
        .unwrap_or_default()
}

fn kick_found(v: &Value) -> Vec<Found> {
    v["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let handle = c["slug"].as_str()?.to_string();
            Some(Found {
                name: c["user"]["username"].as_str().unwrap_or(&handle).to_string(),
                picture: c["user"]["profilePic"].as_str().unwrap_or("").to_string(),
                detail: c["followersCount"].as_u64().map(compact).unwrap_or_default(),
                live: c["isLive"] == true,
                handle,
            })
        })
        .take(12)
        .collect()
}

async fn youtube(query: &str) -> Vec<Found> {
    // youtube.com's search page filtered to channels; the results are in the page's data.
    let headers = [
        ("Accept-Language".to_string(), "en".to_string()),
        ("Cookie".to_string(), "SOCS=CAI; CONSENT=YES+1".to_string()),
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0".to_string(),
        ),
    ];
    let url = format!(
        "https://www.youtube.com/results?search_query={}&sp=EgIQAg%253D%253D",
        crate::commands::url_encode(query)
    );
    let Ok(r) = crate::http::request("GET", &url, &headers, b"").await else {
        return Vec::new();
    };
    let marker = b"var ytInitialData = ";
    let Some(at) = r.body.windows(marker.len()).position(|w| w == marker) else {
        return Vec::new();
    };
    // The first JSON value after the marker; the rest of the page is left alone.
    let data = serde_json::Deserializer::from_slice(&r.body[at + marker.len()..])
        .into_iter::<Value>()
        .next();
    match data {
        Some(Ok(v)) => {
            let mut out = Vec::new();
            youtube_found(&v, &mut out);
            out.truncate(12);
            out
        }
        _ => Vec::new(),
    }
}

fn youtube_found(v: &Value, out: &mut Vec<Found>) {
    match v {
        Value::Object(o) => {
            if let Some(c) = o.get("channelRenderer") {
                if let Some(id) = c["channelId"].as_str() {
                    let handle = c["navigationEndpoint"]["browseEndpoint"]["canonicalBaseUrl"]
                        .as_str()
                        .filter(|h| h.starts_with("/@"))
                        .map_or_else(|| id.to_string(), |h| h.trim_start_matches('/').to_string());
                    let pic = c["thumbnail"]["thumbnails"][0]["url"].as_str().unwrap_or("");
                    out.push(Found {
                        handle,
                        name: c["title"]["simpleText"].as_str().unwrap_or(id).to_string(),
                        picture: if pic.starts_with("//") { format!("https:{pic}") } else { pic.to_string() },
                        detail: c["videoCountText"]["simpleText"]
                            .as_str()
                            .or_else(|| c["subscriberCountText"]["simpleText"].as_str())
                            .unwrap_or("")
                            .to_string(),
                        live: false,
                    });
                }
                return;
            }
            for x in o.values() {
                youtube_found(x, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                youtube_found(x, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_as_people_say_them() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(1_200), "1.2K");
        assert_eq!(compact(340_000), "340K");
        assert_eq!(compact(8_144_770), "8.1M");
        assert_eq!(compact(2_000_000), "2M");
    }

    #[test]
    fn twitch_and_kick_answers_become_channels() {
        let t = json!({"data": {"searchUsers": {"edges": [
            {"node": {"login": "ana", "displayName": "Ana", "profileImageURL": "https://x/a.png",
                      "followers": {"totalCount": 1500}, "stream": {"id": "1"}}},
            {"node": {"login": "bia", "displayName": "Bia", "followers": {"totalCount": 3}, "stream": null}},
        ]}}});
        let f = twitch_found(&t);
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].handle.as_str(), f[0].detail.as_str(), f[0].live), ("ana", "1.5K", true));
        assert!(!f[1].live);
        let k = json!({"channels": [
            {"slug": "xqc", "user": {"username": "xQc", "profilePic": "https://k/p.webp"}, "followersCount": 1415276, "isLive": false},
        ]});
        let f = kick_found(&k);
        assert_eq!((f[0].handle.as_str(), f[0].name.as_str(), f[0].detail.as_str()), ("xqc", "xQc", "1.4M"));
    }

    #[test]
    fn youtube_channels_in_the_search_data() {
        let d = json!({"a": [{"channelRenderer": {
            "channelId": "UC123", "title": {"simpleText": "Ana"},
            "navigationEndpoint": {"browseEndpoint": {"canonicalBaseUrl": "/@ana"}},
            "thumbnail": {"thumbnails": [{"url": "//yt3.ggpht.com/a"}]},
            "videoCountText": {"simpleText": "12 videos"}}}]});
        let mut out = Vec::new();
        youtube_found(&d, &mut out);
        assert_eq!(out[0].handle, "@ana");
        assert_eq!(out[0].picture, "https://yt3.ggpht.com/a");
        assert_eq!(out[0].detail, "12 videos");
    }
}
