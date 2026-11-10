//! Overlays: an HTML file kept in an action group (so it exports and imports with the group's actions),
//! served on the HTTP port at `/overlays/<group>/<file>`. Streamerz adds a small script to the page
//! (`window.streamerz`: the group's settings, and `on(topic)` / `event(name)` for live data over
//! server-sent events on the same port). Nothing listens until an overlay page asks.

use crate::engine::events::Subs;
use crate::engine::Engine;
use crate::groups;
use crate::Config;
use base64::Engine as _;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const CLIENT: &str = include_str!("../assets/overlays/client.js");

/// Listener ids above the WebSocket connections' (they count up from 1).
static NEXT: AtomicU64 = AtomicU64::new(1 << 62);

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The overlay files of every group: (group, file name).
pub fn list(c: &Config) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for g in groups::names(c) {
        for f in groups::meta(c, &g).files {
            if f.name.to_ascii_lowercase().ends_with(".html") {
                out.push((g.clone(), f.name));
            }
        }
    }
    out
}

/// Where overlays are served, as a browser opens it.
pub fn base(c: &Config) -> String {
    let h = &c.http;
    let host = if h.address == "0.0.0.0" || h.address.is_empty() {
        "127.0.0.1"
    } else {
        &h.address
    };
    format!("http://{host}:{}", h.port)
}

pub fn url(c: &Config, group: &str, file: &str) -> String {
    url_at(&base(c), group, file)
}

pub fn url_at(base: &str, group: &str, file: &str) -> String {
    format!("{base}/overlays/{}/{}", encode(group), encode(file))
}

/// The page with Streamerz's script added at the top of its head. `server` is empty when the page is
/// served by Streamerz, and its address in a copy saved to disk.
pub fn page(html: &str, settings: &Value, group: &str, server: &str) -> String {
    let config = json!({"server": server, "settings": settings, "group": group})
        .to_string()
        .replace("</", "<\\/");
    let script = format!("<script>{}</script>", CLIENT.replace("__STREAMERZ__", &config));
    format!("{}{script}{}", &html[..head_end(html)], &html[head_end(html)..])
}

/// Where the script goes: just inside `<head>`, else after the doctype (before it, a browser would
/// fall into quirks mode), else at the start.
fn head_end(html: &str) -> usize {
    let lower = html.to_ascii_lowercase();
    let after = |tag: &str| {
        lower.match_indices(tag).find_map(|(i, _)| {
            let next = lower.as_bytes().get(i + tag.len()).copied();
            if matches!(next, Some(b'>' | b' ' | b'\t' | b'\r' | b'\n')) {
                lower[i..].find('>').map(|j| i + j + 1)
            } else {
                None
            }
        })
    };
    after("<head").or_else(|| after("<!doctype")).unwrap_or(0)
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn index(c: &Config) -> String {
    let mut rows = String::new();
    for (g, f) in list(c) {
        rows += &format!(
            "<li><a href=\"/overlays/{}/{}\">{} / {}</a></li>",
            encode(&g),
            encode(&f),
            escape(&g),
            escape(&f)
        );
    }
    if rows.is_empty() {
        rows = "<li>No overlays yet. Add an .html file to an action group.</li>".into();
    }
    format!(
        "<!doctype html><meta charset=utf-8><title>Streamerz overlays</title>\
         <style>body{{font:15px sans-serif;background:#18181b;color:#e4e4e7;margin:2em}}a{{color:#a78bfa}}</style>\
         <h1>Streamerz overlays</h1><ul>{rows}</ul>"
    )
}

/// What `GET /overlays...` answers: status, content type, body. None is a stream (see `events`).
pub fn file(c: &Config, path: &str) -> (u16, &'static str, Vec<u8>) {
    let rest = path.trim_start_matches("/overlays").trim_matches('/');
    if rest.is_empty() {
        return (200, "text/html; charset=utf-8", index(c).into_bytes());
    }
    let mut parts = rest.splitn(2, '/');
    let group = crate::donations::urlencoding_decode(parts.next().unwrap_or(""));
    let name = crate::donations::urlencoding_decode(parts.next().unwrap_or(""));
    let Some(f) = groups::meta(c, &group).files.into_iter().find(|f| f.name == name) else {
        return (404, "text/plain; charset=utf-8", b"No such overlay file".to_vec());
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&f.content) else {
        return (500, "text/plain; charset=utf-8", b"The file is damaged".to_vec());
    };
    let kind = mime(&name);
    if kind.starts_with("text/html") {
        let html = String::from_utf8_lossy(&bytes);
        let settings = Value::Object(groups::values(c, &group));
        return (200, kind, page(&html, &settings, &group, "").into_bytes());
    }
    (200, kind, bytes)
}

pub async fn respond(tcp: &mut TcpStream, status: u16, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = if status == 200 {
        "OK"
    } else if status == 404 {
        "Not Found"
    } else {
        "Internal Server Error"
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    tcp.write_all(head.as_bytes()).await?;
    tcp.write_all(body).await?;
    tcp.shutdown().await
}

/// The subscriptions a page asks for: `topics=a,b` and `events=Source.Type,Source.*`.
pub fn subscriptions(query: &str) -> Subs {
    let mut subs = Subs::new();
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else { continue };
        let v = crate::donations::urlencoding_decode(&v.replace('+', " "));
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            match k {
                "topics" => {
                    subs.entry("Overlay".into()).or_default().insert(item.into());
                }
                "events" => {
                    let (source, kind) = item.split_once('.').unwrap_or((item, "*"));
                    subs.entry(source.into()).or_default().insert(kind.into());
                }
                _ => {}
            }
        }
    }
    subs
}

fn message(source: &str, kind: &str, data: Value) -> String {
    let m = json!({"timeStamp": crate::engine::globals::iso_now(), "event": {"source": source, "type": kind}, "data": data});
    format!("data: {m}\n\n")
}

/// The live feed: the current value of each topic asked for, then every change, until the page closes.
pub async fn events(e: &Rc<Engine>, mut tcp: TcpStream, query: &str) -> std::io::Result<()> {
    let subs = subscriptions(query);
    tcp.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\
          Access-Control-Allow-Origin: *\r\n\r\n",
    )
    .await?;
    let wanted = subs.get("Overlay").cloned().unwrap_or_default();
    let topics = if wanted.contains("*") {
        e.overlay_topics()
    } else {
        wanted.into_iter().collect()
    };
    for t in topics {
        if let Some(v) = e.overlay_value(&t) {
            tcp.write_all(message("Overlay", &t, v).as_bytes()).await?;
        }
    }
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Arc<str>>();
    e.listen(id, &subs, &tx);
    // Stop listening however the feed ends, also when the server is stopped mid-way.
    struct Done<'a>(&'a Engine, u64, tokio::sync::mpsc::UnboundedSender<Arc<str>>);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            self.0.listen(self.1, &Subs::new(), &self.2);
        }
    }
    let _done = Done(e, id, tx);
    let mut buf = [0u8; 256];
    loop {
        tokio::select! {
            m = rx.recv() => {
                let Some(m) = m else { break Ok(()) };
                if let Err(err) = tcp.write_all(format!("data: {m}\n\n").as_bytes()).await { break Err(err); }
            }
            n = tcp.read(&mut buf) => {
                if !matches!(n, Ok(x) if x > 0) { break Ok(()) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_goes_into_the_head_with_the_settings() {
        let out = page(
            "<html><head><title>x</title></head><body></body></html>",
            &json!({"a": "</script>"}),
            "G",
            "",
        );
        assert!(out.starts_with("<html><head><script>"));
        assert!(out.contains("<title>x</title>"));
        assert!(!out.contains("\"</script>\""), "a setting can't close the script");
        assert!(page("<p>hi</p>", &json!({}), "G", "http://h:1").starts_with("<script>"));
        let header = page("<!DOCTYPE html><body><header>x</header>", &json!({}), "G", "");
        assert!(header.starts_with("<!DOCTYPE html><script>"), "{header}");
        assert!(page("<html><HEAD lang=en><p>", &json!({}), "G", "").starts_with("<html><HEAD lang=en><script>"));
    }

    #[test]
    fn pages_ask_for_topics_and_events() {
        let s = subscriptions("topics=viewers,ad&events=Spotify.TrackChanged%2CTwitch.*");
        assert!(s["Overlay"].contains("viewers") && s["Overlay"].contains("ad"));
        assert!(s["Spotify"].contains("TrackChanged") && s["Twitch"].contains("*"));
        assert!(subscriptions("topics=&events=").is_empty());
    }

    #[test]
    fn names_are_encoded_in_urls() {
        assert_eq!(encode("My group/1.html"), "My%20group%2F1.html");
    }
}
