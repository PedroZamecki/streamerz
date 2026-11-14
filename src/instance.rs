//! Keep one production instance per user, without depending on a server port.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// The operating system releases the lock even if the process crashes.
pub fn acquire(dir: &Path) -> io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

/// A restarted process allows the previous process time to release its lock.
pub fn acquire_after_restart(dir: &Path) -> io::Result<File> {
    for _ in 0..50 {
        match acquire(dir) {
            Ok(lock) => return Ok(lock),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(e) => return Err(e),
        }
    }
    acquire(dir)
}

/// Asks the copy that holds the lock to show its window, through its own WebSocket server (as the
/// settings in `config` say). False when it can't be reached (the server is off, or on another port).
pub fn show_running(config: &Path) -> bool {
    let settings = std::fs::read(config)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or_default();
    let ws = &settings["websockets"];
    if ws["autoStart"] == false {
        return false;
    }
    let port = ws["port"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(8080);
    let endpoint = ws["endpoint"].as_str().filter(|e| e.starts_with('/')).unwrap_or("/");
    ask_to_show(port, endpoint)
}

fn ask_to_show(port: u16, endpoint: &str) -> bool {
    use tokio_tungstenite::tungstenite::{client, Message};
    let wait = std::time::Duration::from_secs(2);
    let Ok(stream) = std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), wait) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(wait));
    let _ = stream.set_write_timeout(Some(wait));
    let Ok((mut socket, _)) = client(format!("ws://127.0.0.1:{port}{endpoint}"), stream) else {
        return false;
    };
    let ask = serde_json::json!({"request": "StreamerzShowWindow", "id": "show"}).to_string();
    if socket.send(Message::Text(ask.into())).is_err() {
        return false;
    }
    // The server says hello first; the answer follows.
    for _ in 0..4 {
        let Ok(Message::Text(text)) = socket.read() else {
            return false;
        };
        let reply: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        if reply["id"] == "show" {
            let _ = socket.close(None);
            return reply["status"] == "ok";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_exclusive_and_released_on_close() {
        let dir = std::env::temp_dir().join(format!("streamerz-lock-test-{}", std::process::id()));
        let first = acquire(&dir).unwrap();
        assert!(acquire(&dir).is_err());
        drop(first);
        assert!(acquire(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restart_waits_for_the_previous_lock() {
        let dir = std::env::temp_dir().join(format!("streamerz-restart-lock-{}", std::process::id()));
        let first = acquire(&dir).unwrap();
        let path = dir.clone();
        let child = std::thread::spawn(move || acquire_after_restart(&path));
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(first);
        drop(child.join().unwrap().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A second start reaches the running copy and asks for its window.
    #[test]
    fn a_second_start_asks_the_running_copy_to_show_itself() {
        use tokio_tungstenite::tungstenite::{accept, Message};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut ws = accept(listener.accept().unwrap().0).unwrap();
            ws.send(Message::Text(r#"{"request":"Hello"}"#.into())).unwrap();
            let asked: serde_json::Value = serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
            ws.send(Message::Text(r#"{"id":"show","status":"ok"}"#.into())).unwrap();
            asked
        });
        assert!(ask_to_show(port, "/"));
        assert_eq!(server.join().unwrap()["request"], "StreamerzShowWindow");
        // Nothing listening: false, quickly.
        let free = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert!(!ask_to_show(free, "/"));
    }
}
