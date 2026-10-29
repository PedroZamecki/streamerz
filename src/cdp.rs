//! The user's own browser (Chrome, Chromium, Edge, Brave or Vivaldi), driven over the DevTools protocol:
//! a profile of Streamerz's own, a local WebSocket and JSON. No webview is bundled or linked: the
//! browser is a separate process, and its memory goes away when it closes.

use crate::http::Ws;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// How long one DevTools call may take.
const CALL: Duration = Duration::from_secs(30);

/// The browser to use: `STREAMERZ_BROWSER`, else the first Chromium-based one installed.
pub fn find() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("STREAMERZ_BROWSER").map(PathBuf::from) {
        return Some(p);
    }
    if cfg!(windows) {
        let dirs = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .collect::<Vec<_>>();
        let apps = [
            r"Microsoft\Edge\Application\msedge.exe",
            r"Google\Chrome\Application\chrome.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Vivaldi\Application\vivaldi.exe",
        ];
        apps.iter()
            .flat_map(|a| dirs.iter().map(move |d| d.join(a)))
            .find(|p| p.is_file())
    } else {
        let names = [
            "google-chrome-stable",
            "google-chrome",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "brave",
            "microsoft-edge-stable",
            "microsoft-edge",
            "vivaldi-stable",
            "vivaldi",
        ];
        let path = std::env::var_os("PATH")?;
        names
            .iter()
            .flat_map(|n| std::env::split_paths(&path).map(move |d| d.join(n)))
            .find(|p| p.is_file())
    }
}

/// A browser started by Streamerz, and its first tab.
pub struct Browser {
    child: tokio::process::Child,
    ws: Ws,
    next: u64,
    /// The tab's DevTools session.
    page: String,
}

impl Browser {
    /// Starts the browser on `profile` showing `url`: hidden (`headless`), or as a small app window.
    pub async fn launch(profile: &Path, headless: bool, url: &str) -> Result<Browser, String> {
        let exe = find().ok_or(
            "no Chrome, Chromium, Edge, Brave or Vivaldi found (or set STREAMERZ_BROWSER to the browser's path)",
        )?;
        std::fs::create_dir_all(profile).map_err(|e| format!("{}: {e}", profile.display()))?;
        // Where the browser says which port it listens on: none yet, so a stale one isn't read.
        let port_file = profile.join("DevToolsActivePort");
        let _ = std::fs::remove_file(&port_file);
        let mut cmd = tokio::process::Command::new(&exe);
        cmd.arg(format!("--user-data-dir={}", profile.display()))
            .args([
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--mute-audio",
                "--disable-background-networking",
                "--disable-sync",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        if headless {
            cmd.args(["--headless=new", url]);
        } else {
            cmd.args(["--window-size=520,760", &format!("--app={url}")]);
        }
        let mut child = cmd.spawn().map_err(|e| format!("can't start {}: {e}", exe.display()))?;
        let mut active = String::new();
        for _ in 0..150 {
            active = std::fs::read_to_string(&port_file).unwrap_or_default();
            if active.lines().count() >= 2 {
                break;
            }
            if child.try_wait().ok().flatten().is_some() {
                return Err(format!("{} closed at once", exe.display()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut lines = active.lines();
        let (Some(port), Some(path)) = (lines.next(), lines.next()) else {
            return Err(format!("{} didn't open its DevTools port", exe.display()));
        };
        let (ws, _) = crate::http::ws_connect(format!("ws://127.0.0.1:{port}{path}"))
            .await
            .map_err(|e| format!("can't reach the browser: {e}"))?;
        let mut b = Browser {
            child,
            ws,
            next: 0,
            page: String::new(),
        };
        let targets = b.call(None, "Target.getTargets", json!({})).await?;
        let tab = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["type"] == "page")
            .and_then(|t| t["targetId"].as_str())
            .ok_or("the browser has no tab")?
            .to_string();
        let attached = b
            .call(None, "Target.attachToTarget", json!({"targetId": tab, "flatten": true}))
            .await?;
        b.page = attached["sessionId"].as_str().unwrap_or("").to_string();
        Ok(b)
    }

    /// One DevTools call (on the tab with `session`), and its result. Events in between are skipped.
    async fn call(&mut self, session: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = s.into();
        }
        self.ws
            .send(Message::Text(msg.to_string().into()))
            .await
            .map_err(|e| format!("the browser closed ({e})"))?;
        let answer = tokio::time::timeout(CALL, async {
            while let Some(m) = self.ws.next().await {
                let Ok(Message::Text(t)) = m else { continue };
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                if v["id"] == id {
                    return Some(v);
                }
            }
            None
        })
        .await
        .map_err(|_| format!("the browser didn't answer {method}"))?
        .ok_or("the browser closed")?;
        match answer.get("error") {
            Some(e) => Err(format!("{method}: {}", e["message"].as_str().unwrap_or("failed"))),
            None => Ok(answer["result"].clone()),
        }
    }

    /// A call on the tab.
    async fn tab(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let page = self.page.clone();
        self.call(Some(&page), method, params).await
    }

    pub async fn navigate(&mut self, url: &str) -> Result<(), String> {
        self.tab("Page.navigate", json!({"url": url})).await.map(drop)
    }

    /// Runs `js` in the tab and gives its value (a promise's, once it settles).
    pub async fn eval(&mut self, js: &str) -> Result<Value, String> {
        let r = self
            .tab(
                "Runtime.evaluate",
                json!({"expression": js, "awaitPromise": true, "returnByValue": true}),
            )
            .await?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(e["exception"]["description"]
                .as_str()
                .or(e["text"].as_str())
                .unwrap_or("script error")
                .to_string());
        }
        Ok(r["result"]["value"].clone())
    }

    /// The browser's cookies for `url`: name and value.
    pub async fn cookies(&mut self, url: &str) -> Result<Vec<(String, String)>, String> {
        let r = self.tab("Network.getCookies", json!({"urls": [url]})).await?;
        Ok(r["cookies"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["name"].as_str().unwrap_or("").to_string(),
                    c["value"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect())
    }

    /// Sets `cookies` ("a=1; b=2") for `domain` (".tiktok.com").
    pub async fn set_cookies(&mut self, cookies: &str, domain: &str) -> Result<(), String> {
        let list: Vec<Value> = cookies
            .split(';')
            .filter_map(|c| c.trim().split_once('='))
            .map(|(k, v)| json!({"name": k, "value": v, "domain": domain, "path": "/", "secure": true}))
            .collect();
        self.tab("Network.setCookies", json!({"cookies": list})).await.map(drop)
    }

    /// The browser is gone (closed by the user, or crashed).
    pub fn gone(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }

    /// Closes the browser.
    pub async fn close(mut self) {
        let _ = self.call(None, "Browser.close", json!({})).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
    }
}
