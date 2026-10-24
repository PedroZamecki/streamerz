//! Browser logins for Kick and YouTube: OAuth 2 authorization code with PKCE, the code caught by a
//! one-shot listener on loopback, and token refresh. (Twitch uses the device code flow, twitch.rs.)

use crate::commands::url_encode;
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Open `url` in the default browser. (Not `cmd /c start` on Windows: cmd splits the URL at `&`.)
pub fn open_browser(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// A token answer: access token, refresh token (may be empty) and when it expires (Unix seconds).
pub struct Token {
    pub access: String,
    pub refresh: String,
    pub expires_at: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Whether a token expiring at `expires_at` should be refreshed before use (a minute early; 0 is
/// unknown, used as is).
pub fn stale(expires_at: u64) -> bool {
    expires_at != 0 && expires_at <= now() + 60
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random() -> String {
    let r: Vec<u8> = (0..4).flat_map(|_| crate::math::secure_u64().to_le_bytes()).collect();
    b64(&r)
}

/// A PKCE verifier and its S256 challenge.
pub fn pkce() -> (String, String) {
    let v = random();
    let c = b64(&Sha256::digest(v.as_bytes()));
    (v, c)
}

/// One login: the listener is bound first, so the redirect (with its port) is known before the
/// browser opens. `port` 0 picks a free one (Google); Kick needs the fixed one in the user's app.
pub struct Login {
    listener: TcpListener,
    pub redirect: String,
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl Login {
    pub async fn bind(port: u16, path: &str) -> Result<Login, String> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| format!("can't listen on 127.0.0.1:{port}: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (verifier, challenge) = pkce();
        Ok(Login {
            listener,
            redirect: format!("http://127.0.0.1:{port}{path}"),
            verifier,
            challenge,
            state: random(),
        })
    }

    /// Wait for the browser's redirect (5 minutes at most); the code. Other requests (favicon)
    /// are answered and ignored.
    pub async fn code(self) -> Result<String, String> {
        let wait = async {
            loop {
                let (mut s, _) = self.listener.accept().await.map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]);
                let target = head.split_whitespace().nth(1).unwrap_or("");
                let q = query(target);
                let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
                let (page, out) = if let Some(e) = get("error") {
                    (
                        "Login failed, you can close this tab.",
                        Some(Err(format!("login refused: {e}"))),
                    )
                } else if let Some(code) = get("code") {
                    if get("state").as_deref() == Some(self.state.as_str()) {
                        ("Logged in to streamerz, you can close this tab.", Some(Ok(code)))
                    } else {
                        (
                            "Login failed (state), you can close this tab.",
                            Some(Err("state mismatch".into())),
                        )
                    }
                } else {
                    ("", None)
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                );
                let _ = s.write_all(reply.as_bytes()).await;
                if let Some(out) = out {
                    return out;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(300), wait)
            .await
            .map_err(|_| "login timed out".to_string())?
    }
}

/// The query of a request target, decoded.
fn query(target: &str) -> Vec<(String, String)> {
    let q = target.split_once('?').map_or("", |x| x.1);
    q.split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(x) => {
                    out.push(x);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// POST a form to a token endpoint.
pub async fn token(url: &str, form: &[(&str, &str)]) -> Result<Token, String> {
    let body = form
        .iter()
        .map(|(k, v)| format!("{k}={}", url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let headers = [(
        "Content-Type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    )];
    let r = crate::http::request("POST", url, &headers, body.as_bytes()).await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
    let access = v["access_token"].as_str().unwrap_or("");
    if !(200..300).contains(&r.status) || access.is_empty() {
        return Err(format!("token request failed (HTTP {}): {}", r.status, r.text()));
    }
    Ok(Token {
        access: access.into(),
        refresh: v["refresh_token"].as_str().unwrap_or("").into(),
        expires_at: v["expires_in"].as_u64().map_or(0, |s| now() + s),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_and_query() {
        let (v, c) = pkce();
        assert_eq!(v.len(), 43);
        assert_eq!(c, b64(&Sha256::digest(v.as_bytes())));
        let q = query("/callback?code=a%2Fb+c&state=x%");
        assert_eq!(q, [("code".into(), "a/b c".into()), ("state".into(), "x%".into())]);
        assert!(!stale(0));
        assert!(stale(now() + 10));
        assert!(!stale(now() + 3600));
    }

    #[test]
    fn loopback_catches_the_code() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let l = Login::bind(0, "/cb").await.unwrap();
            let (addr, state) = (
                l.redirect.trim_start_matches("http://").replace("/cb", ""),
                l.state.clone(),
            );
            let wait = tokio::spawn(l.code());
            let get = |target: String| {
                let addr = addr.clone();
                async move {
                    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
                    s.write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                        .await
                        .unwrap();
                    let mut out = String::new();
                    s.read_to_string(&mut out).await.unwrap();
                    out
                }
            };
            assert!(get("/favicon.ico".into()).await.starts_with("HTTP/1.1 200"));
            let page = get(format!("/cb?code=abc&state={}", url_encode(&state))).await;
            assert!(page.ends_with("you can close this tab."));
            assert_eq!(wait.await.unwrap().unwrap(), "abc");
        });
    }
}
