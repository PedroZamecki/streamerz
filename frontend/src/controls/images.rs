//! Shared picture loader: bounded cache of downloaded image files and batched `"images"` requests
//! to the core. Supports both incremental FIFO queues (chat, emotes, gallery, channel finder) and
//! computed URL sets (user cards).
use crate::send;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

/// Most pending picture URLs kept queued at once by default.
pub const MAX_PENDING: usize = 512;

/// Bounded cache of downloaded image files and pending HTTP(S) picture URLs.
pub struct Pictures {
    files: HashMap<String, PathBuf>,
    unasked: VecDeque<String>,
    asked: VecDeque<String>,
    pending: HashSet<String>,
    asking: bool,
    id: String,
    batch: usize,
    max_files: usize,
    max_pending: usize,
}

impl Pictures {
    pub fn new(id: impl Into<String>, batch: usize, max_files: usize, max_pending: usize) -> Self {
        Self {
            files: HashMap::new(),
            unasked: VecDeque::new(),
            asked: VecDeque::new(),
            pending: HashSet::new(),
            asking: false,
            id: id.into(),
            batch: batch.max(1),
            max_files: max_files.max(1),
            max_pending: max_pending.max(1),
        }
    }

    pub fn get(&self, url: &str) -> Option<&PathBuf> {
        self.files.get(url)
    }

    pub fn files(&self) -> &HashMap<String, PathBuf> {
        &self.files
    }

    pub fn insert_file(&mut self, url: String, path: PathBuf) {
        if self.pending.remove(&url) {
            self.unasked.retain(|item| item != &url);
            self.asked.retain(|item| item != &url);
        }
        self.files.insert(url, path);
    }

    /// Queue an HTTP(S) picture URL if it is neither cached nor already queued.
    pub fn want(&mut self, url: &str) {
        if !url.starts_with("http") || self.files.contains_key(url) || self.pending.contains(url) {
            return;
        }
        while self.unasked.len() + self.asked.len() >= self.max_pending {
            if let Some(dropped) = self.asked.pop_front().or_else(|| self.unasked.pop_front()) {
                self.pending.remove(&dropped);
            }
        }
        let owned = url.to_owned();
        self.pending.insert(owned.clone());
        self.unasked.push_back(owned);
    }

    /// Queue every HTTP(S) picture URL in `urls` that is neither cached nor already queued.
    pub fn want_all<'a>(&mut self, urls: impl IntoIterator<Item = &'a str>) {
        for url in urls {
            self.want(url);
        }
    }

    /// Drop queued URLs (for example when switching tabs, filters or search results).
    pub fn clear_pending(&mut self) {
        self.unasked.clear();
        self.asked.clear();
        self.pending.clear();
    }

    /// Mark already-asked URLs ready to be queried again after downloads arrive.
    pub fn mark_arrived(&mut self) {
        self.unasked.extend(self.asked.drain(..));
    }

    /// The core downloaded pictures: re-queue unresolved URLs and send the next batch if idle.
    pub fn arrived(&mut self) -> bool {
        self.mark_arrived();
        self.ask()
    }

    /// Build the `"images"` request payload for a batch of URLs.
    pub fn request(&self, urls: Vec<String>) -> Value {
        json!({"request": "StreamerzChat", "op": "images", "urls": urls, "id": self.id})
    }

    /// Take the next batch of unasked URLs, moving them to `asked` so failing URLs cannot starve newer ones.
    pub fn next_batch(&mut self) -> Option<Vec<String>> {
        if self.asking || self.unasked.is_empty() {
            return None;
        }
        let count = self.batch.min(self.unasked.len());
        let mut urls = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(url) = self.unasked.pop_front() {
                urls.push(url.clone());
                self.asked.push_back(url);
            }
        }
        self.asking = true;
        Some(urls)
    }

    /// Send the next queued batch to the core when idle.
    pub fn ask(&mut self) -> bool {
        let Some(urls) = self.next_batch() else {
            return false;
        };
        send(self.request(urls));
        true
    }

    /// Build an `"images"` request for any missing HTTP(S) URLs in a computed set, respecting
    /// `asking` and `batch`.
    pub fn request_for<'a>(&self, urls: impl IntoIterator<Item = &'a str>) -> Option<Value> {
        if self.asking {
            return None;
        }
        let mut missing = Vec::new();
        let mut seen = HashSet::new();
        for url in urls {
            if url.starts_with("http") && !self.files.contains_key(url) && seen.insert(url) {
                missing.push(url.to_owned());
                if missing.len() >= self.batch {
                    break;
                }
            }
        }
        (!missing.is_empty()).then(|| self.request(missing))
    }

    /// Ask the core for any missing HTTP(S) URLs in a computed set (at most one request in flight).
    pub fn ask_for<'a>(&mut self, urls: impl IntoIterator<Item = &'a str>) -> bool {
        let Some(request) = self.request_for(urls) else {
            return false;
        };
        self.asking = true;
        send(request);
        true
    }

    /// Apply an `"images"` reply, evicting stale entries above `max_files` while preserving `active` URLs.
    /// Returns `true` when more unasked URLs remain and another batch should be sent now.
    pub fn apply_reply(&mut self, reply: &Value, active: impl FnOnce() -> HashSet<String>) -> bool {
        self.asking = false;
        let mut resolved = false;
        for (url, file) in reply["files"].as_object().into_iter().flatten() {
            if let Some(file) = file.as_str() {
                self.files.insert(url.clone(), PathBuf::from(file));
                if self.pending.remove(url) {
                    resolved = true;
                }
            }
        }
        if resolved {
            self.unasked.retain(|url| self.pending.contains(url));
            self.asked.retain(|url| self.pending.contains(url));
        }
        if self.files.len() > self.max_files {
            let keep = active();
            self.files.retain(|url, _| keep.contains(url));
            if self.files.len() > self.max_files {
                let excess = self.files.len() - self.max_files;
                let drop_keys: Vec<String> = self.files.keys().take(excess).cloned().collect();
                for key in drop_keys {
                    self.files.remove(&key);
                }
            }
        }
        !self.unasked.is_empty()
    }

    /// Apply an `"images"` reply and immediately send the next queued batch if more URLs wait.
    pub fn apply(&mut self, reply: &Value, active: impl FnOnce() -> HashSet<String>) {
        if self.apply_reply(reply, active) {
            self.ask();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pictures;
    use serde_json::json;
    use std::collections::HashSet;
    use std::path::PathBuf;

    #[test]
    fn ignores_non_http_and_advances_past_failing_batches() {
        let mut pictures = Pictures::new("chat:emoteimages", 2, 10, 10);
        pictures.want("");
        pictures.want("coins");
        pictures.want_all(["https://a/1", "https://a/2", "https://a/3", "https://a/1"]);

        // First batch takes 1 and 2; if neither resolves (e.g. 404), next batch still reaches 3.
        let batch = pictures.next_batch().expect("first batch");
        assert_eq!(batch, vec!["https://a/1".to_owned(), "https://a/2".to_owned()]);
        assert_eq!(
            pictures.request(batch),
            json!({
                "request": "StreamerzChat",
                "op": "images",
                "urls": ["https://a/1", "https://a/2"],
                "id": "chat:emoteimages"
            })
        );
        // While in flight, next_batch returns None.
        assert_eq!(pictures.next_batch(), None);

        let more = pictures.apply_reply(&json!({"files": {}}), HashSet::new);
        assert!(more, "unasked URL 3 remains");
        assert_eq!(pictures.next_batch(), Some(vec!["https://a/3".to_owned()]));

        let more = pictures.apply_reply(&json!({"files": {"https://a/3": "/tmp/3.png"}}), HashSet::new);
        assert!(!more, "all queued URLs have been asked");
        assert_eq!(pictures.get("https://a/3"), Some(&PathBuf::from("/tmp/3.png")));
        assert_eq!(pictures.next_batch(), None, "does not spin on unresolved URLs");

        // When downloads arrive, unresolved URLs become askable again.
        pictures.mark_arrived();
        assert_eq!(
            pictures.next_batch(),
            Some(vec!["https://a/1".to_owned(), "https://a/2".to_owned()])
        );
    }

    #[test]
    fn bounds_pending_and_evicts_only_inactive_files() {
        let mut pictures = Pictures::new("chat:images", 2, 2, 2);
        pictures.want("https://a/1");
        pictures.want("https://a/2");
        let _ = pictures.next_batch();
        let _ = pictures.apply_reply(&json!({"files": {}}), HashSet::new);
        // Adding a 3rd URL evicts oldest already-asked URL first.
        pictures.want("https://a/3");
        assert_eq!(pictures.next_batch(), Some(vec!["https://a/3".to_owned()]));
        let _ = pictures.apply_reply(&json!({"files": {}}), HashSet::new);
        pictures.mark_arrived();
        assert_eq!(
            pictures.next_batch(),
            Some(vec!["https://a/2".to_owned(), "https://a/3".to_owned()])
        );

        // Exceeding max_files preserves active URLs and drops inactive ones.
        let keep = || HashSet::from(["https://a/2".to_owned(), "https://a/3".to_owned()]);
        let _ = pictures.apply_reply(
            &json!({
                "files": {
                    "https://a/1": "/tmp/1.png",
                    "https://a/2": "/tmp/2.png",
                    "https://a/3": "/tmp/3.png"
                }
            }),
            keep,
        );
        assert!(pictures.get("https://a/1").is_none());
        assert_eq!(pictures.get("https://a/2"), Some(&PathBuf::from("/tmp/2.png")));
        assert_eq!(pictures.get("https://a/3"), Some(&PathBuf::from("/tmp/3.png")));
    }

    #[test]
    fn clear_pending_drops_stale_queue() {
        let mut pictures = Pictures::new("gallery:images", 4, 10, 10);
        pictures.want("https://a/old");
        pictures.clear_pending();
        assert_eq!(pictures.next_batch(), None);
        pictures.mark_arrived();
        assert_eq!(pictures.next_batch(), None);
    }

    #[test]
    fn insert_file_resolves_pending_url_without_asking() {
        let mut pictures = Pictures::new("chat:images", 4, 10, 10);
        pictures.want("https://a/emote.png");
        pictures.insert_file("https://a/emote.png".to_owned(), PathBuf::from("/tmp/emote.png"));
        assert_eq!(
            pictures.get("https://a/emote.png"),
            Some(&PathBuf::from("/tmp/emote.png"))
        );
        assert_eq!(pictures.files().len(), 1);
        assert_eq!(pictures.next_batch(), None);
    }

    #[test]
    fn request_for_filters_cached_dedupes_and_respects_batch_and_inflight() {
        let mut pictures = Pictures::new("chat:1:card-images", 2, 10, 10);
        pictures.insert_file("https://a/cached.png".to_owned(), PathBuf::from("/tmp/cached.png"));

        let request = pictures
            .request_for([
                "",
                "coins",
                "https://a/cached.png",
                "https://a/1.png",
                "https://a/1.png",
                "https://a/2.png",
                "https://a/3.png",
            ])
            .expect("missing URLs");
        assert_eq!(
            request,
            json!({
                "request": "StreamerzChat",
                "op": "images",
                "urls": ["https://a/1.png", "https://a/2.png"],
                "id": "chat:1:card-images"
            })
        );

        // Once a request is in flight, computed-set queries wait until the reply clears it.
        pictures.asking = true;
        assert!(pictures.request_for(["https://a/1.png"]).is_none());

        let _ = pictures.apply_reply(&json!({"files": {"https://a/1.png": "/tmp/1.png"}}), HashSet::new);
        let request = pictures
            .request_for(["https://a/1.png", "https://a/2.png"])
            .expect("remaining URL");
        assert_eq!(request["urls"], json!(["https://a/2.png"]));

        let _ = pictures.apply_reply(&json!({"files": {"https://a/2.png": "/tmp/2.png"}}), HashSet::new);
        assert!(pictures.request_for(["https://a/1.png", "https://a/2.png"]).is_none());
    }
}
