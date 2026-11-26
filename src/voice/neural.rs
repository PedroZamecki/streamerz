//! Neural voices: the optional `Streamerz-Voice` worker (sherpa-onnx), the built-in catalog
//! (`assets/voices.json`), resumable verified downloads, Kokoro voice packs/mixes and Pocket TTS
//! voice-cloning recordings.
//!
//! Nothing runs, downloads or allocates until a neural voice is installed or spoken.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Built-in voice catalog (Piper int8 voices, Kokoro multi-lang pack, Pocket TTS pack).
const CATALOG_JSON: &str = include_str!("../../assets/voices.json");

/// SHA-256 of the matching `Streamerz-Voice` release binary (set by CI during release builds).
pub const ENGINE_SHA256: Option<&str> = option_env!("STREAMERZ_VOICE_SHA256");

/// Expected size in bytes of the matching `Streamerz-Voice` release binary (set by CI when known).
pub const ENGINE_SIZE: Option<&str> = option_env!("STREAMERZ_VOICE_SIZE");

/// Maximum download size accepted for a voice archive or binary (512 MB).
const MAX_DOWNLOAD: u64 = 512 << 20;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KokoroSpeaker {
    pub sid: i32,
    pub id: String,
    pub name: String,
    pub language: String,
    pub lang: String,
    pub gender: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub language: String,
    #[serde(default)]
    pub gender: String,
    pub quality: String,
    pub disk_bytes: u64,
    pub ram_mb: u64,
    pub url: String,
    pub sha256: String,
    pub license: String,
    pub commercial: bool,
    pub attribution: String,
    #[serde(default)]
    pub sample_url: String,
    #[serde(default)]
    pub speakers: Vec<KokoroSpeaker>,
}

#[derive(Deserialize)]
struct CatalogFile {
    voices: Vec<CatalogEntry>,
}

/// The bundled voice catalog, parsed once.
pub fn catalog() -> &'static [CatalogEntry] {
    static CAT: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    CAT.get_or_init(|| {
        serde_json::from_str::<CatalogFile>(CATALOG_JSON)
            .expect("bundled assets/voices.json must be valid")
            .voices
    })
}

pub fn catalog_entry(id: &str) -> Option<&'static CatalogEntry> {
    catalog().iter().find(|e| e.id == id)
}

/// All 54 Kokoro speakers from the catalog.
pub fn kokoro_speakers() -> &'static [KokoroSpeaker] {
    catalog_entry("kokoro:multi-lang-v1")
        .map(|e| e.speakers.as_slice())
        .unwrap_or(&[])
}

/// Folder beside `streamerz.json` holding `Streamerz-Voice`, models and recordings (excluded from
/// backups and template exports).
pub fn dir() -> PathBuf {
    #[cfg(test)]
    if let Some(d) = TEST_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return d;
    }
    crate::config_path().with_file_name("voices")
}

#[cfg(test)]
pub static TEST_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
#[cfg(test)]
pub static TEST_BINARY: Mutex<Option<PathBuf>> = Mutex::new(None);
#[cfg(test)]
pub static TEST_ENGINE_URL: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);
#[cfg(test)]
pub static TEST_PAREC: Mutex<Option<PathBuf>> = Mutex::new(None);

/// File name of the voice worker binary for this OS.
pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "Streamerz-Voice.exe"
    } else {
        "Streamerz-Voice"
    }
}

/// Release asset name published by CI for this OS.
pub fn release_asset_name() -> &'static str {
    if cfg!(windows) {
        "Streamerz-Voice-windows-x86_64.exe"
    } else {
        "Streamerz-Voice-linux-x86_64"
    }
}

/// Installed path for the downloaded `Streamerz-Voice` binary.
pub fn binary_path() -> PathBuf {
    dir().join(binary_name())
}

/// Path to use when spawning `Streamerz-Voice` (test override, env override, or installed path).
pub fn resolved_binary() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(p) = TEST_BINARY.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return p.is_file().then_some(p);
    }
    if let Ok(p) = std::env::var("STREAMERZ_VOICE_BIN") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let p = binary_path();
    if p.is_file() {
        return Some(p);
    }
    None
}

pub fn engine_installed() -> bool {
    resolved_binary().is_some()
}

/// Release download URL and expected SHA-256 (if embedded) for `Streamerz-Voice`.
pub fn engine_source() -> Result<(String, Option<String>), String> {
    #[cfg(test)]
    if let Some(s) = TEST_ENGINE_URL.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(s);
    }
    let version = option_env!("STREAMERZ_VERSION")
        .filter(|v| !v.is_empty() && *v != "dev")
        .ok_or_else(|| {
            "This is a development build of Streamerz with no matching release for Streamerz-Voice. Build voice/ locally and set STREAMERZ_VOICE_BIN.".to_string()
        })?;
    let url = format!(
        "https://github.com/{}/releases/download/{version}/{}",
        crate::brand::REPO,
        release_asset_name()
    );
    Ok((url, ENGINE_SHA256.filter(|s| !s.is_empty()).map(str::to_string)))
}

fn safe_folder(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn model_dir(id: &str) -> PathBuf {
    dir().join("models").join(safe_folder(id))
}

pub fn downloads_dir() -> PathBuf {
    dir().join("downloads")
}

pub fn recordings_dir() -> PathBuf {
    dir().join("recordings")
}

pub fn is_model_installed(id: &str) -> bool {
    let d = model_dir(id);
    if !d.is_dir() {
        return false;
    }
    // Verify at least one .onnx file exists so an empty folder is not treated as installed.
    std::fs::read_dir(&d).ok().into_iter().flatten().flatten().any(|e| {
        e.path()
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("onnx"))
    })
}

/// Total disk bytes used under `voices/`.
pub fn disk_usage() -> u64 {
    fn walk(p: &Path) -> u64 {
        let Ok(rd) = std::fs::read_dir(p) else { return 0 };
        let mut sum = 0u64;
        for entry in rd.flatten() {
            let path = entry.path();
            if let Ok(m) = std::fs::symlink_metadata(&path) {
                if m.is_file() {
                    sum += m.len();
                } else if m.is_dir() {
                    sum += walk(&path);
                }
            }
        }
        sum
    }
    walk(&dir())
}

/// All currently installed neural voices (Piper models + Kokoro speakers when Kokoro is installed).
pub fn installed_voices() -> Vec<super::Voice> {
    let mut out = Vec::new();
    for entry in catalog() {
        if !is_model_installed(&entry.id) {
            continue;
        }
        match entry.engine.as_str() {
            "piper" => out.push(super::Voice {
                id: entry.id.clone(),
                name: format!("{} (Piper)", entry.name),
                language: entry.language.clone(),
                gender: entry.gender.clone(),
                engine: "piper".into(),
            }),
            "kokoro" => {
                for sp in &entry.speakers {
                    out.push(super::Voice {
                        id: format!("kokoro:{}", sp.id),
                        name: format!("{} (Kokoro)", sp.name),
                        language: sp.language.clone(),
                        gender: sp.gender.clone(),
                        engine: "kokoro".into(),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// Live download progress (at most one active download at a time).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub id: String,
    pub label: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: u8,
    pub phase: String,
}

struct ActiveDownload {
    status: Mutex<DownloadStatus>,
    cancel: AtomicBool,
}

static DOWNLOAD: OnceLock<Mutex<Option<Arc<ActiveDownload>>>> = OnceLock::new();

fn download_slot() -> &'static Mutex<Option<Arc<ActiveDownload>>> {
    DOWNLOAD.get_or_init(|| Mutex::new(None))
}

pub fn active_download() -> Option<DownloadStatus> {
    download_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|d| d.status.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

pub fn cancel_download() -> bool {
    let slot = download_slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = slot.as_ref() {
        d.cancel.store(true, Ordering::SeqCst);
        crate::host::changed();
        true
    } else {
        false
    }
}

struct DownloadGuard;
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        *download_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
        crate::host::changed();
    }
}

fn begin_download(id: &str, label: &str, total: u64) -> Result<(Arc<ActiveDownload>, DownloadGuard), String> {
    let mut slot = download_slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cur) = slot.as_ref() {
        let s = cur.status.lock().unwrap_or_else(|e| e.into_inner());
        return Err(format!("already downloading {}", s.label));
    }
    let active = Arc::new(ActiveDownload {
        status: Mutex::new(DownloadStatus {
            id: id.to_string(),
            label: label.to_string(),
            downloaded: 0,
            total,
            percent: 0,
            phase: "downloading".into(),
        }),
        cancel: AtomicBool::new(false),
    });
    *slot = Some(active.clone());
    crate::host::changed();
    Ok((active, DownloadGuard))
}

/// Persistent child process running `Streamerz-Voice`.
struct WorkerProc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    version: String,
}

#[derive(Default)]
struct WorkerMeta {
    running: bool,
    loaded: Option<String>,
    memory: u64,
    version: String,
}

static WORKER_PROC: OnceLock<tokio::sync::Mutex<Option<WorkerProc>>> = OnceLock::new();
static WORKER_META: OnceLock<Mutex<WorkerMeta>> = OnceLock::new();

fn worker_proc() -> &'static tokio::sync::Mutex<Option<WorkerProc>> {
    WORKER_PROC.get_or_init(|| tokio::sync::Mutex::new(None))
}

fn worker_meta() -> &'static Mutex<WorkerMeta> {
    WORKER_META.get_or_init(|| Mutex::new(WorkerMeta::default()))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub installed: bool,
    pub running: bool,
    pub loaded: Option<String>,
    pub memory_bytes: u64,
    pub version: String,
    pub disk_bytes: u64,
    pub kokoro_installed: bool,
    pub pocket_installed: bool,
    pub download: Option<DownloadStatus>,
}

pub fn state() -> EngineState {
    let meta = worker_meta().lock().unwrap_or_else(|e| e.into_inner()).clone();
    EngineState {
        installed: engine_installed(),
        running: meta.running,
        loaded: meta.loaded,
        memory_bytes: meta.memory,
        version: meta.version,
        disk_bytes: disk_usage(),
        kokoro_installed: is_model_installed("kokoro:multi-lang-v1"),
        pocket_installed: is_model_installed("pocket:en-v1"),
        download: active_download(),
    }
}

impl Clone for WorkerMeta {
    fn clone(&self) -> Self {
        Self {
            running: self.running,
            loaded: self.loaded.clone(),
            memory: self.memory,
            version: self.version.clone(),
        }
    }
}

async fn spawn_worker() -> Result<WorkerProc, String> {
    let bin = resolved_binary().ok_or_else(|| {
        "Neural voice engine (Streamerz-Voice) is not installed. Install it from Sounds > Voices.".to_string()
    })?;
    let mut cmd = Command::new(&bin);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", bin.display()))?;
    let stdin = child.stdin.take().ok_or("no stdin on Streamerz-Voice")?;
    let stdout = BufReader::new(child.stdout.take().ok_or("no stdout on Streamerz-Voice")?);
    let mut proc = WorkerProc {
        child,
        stdin,
        stdout,
        version: String::new(),
    };
    let (hello, _) = send_on(&mut proc, &json!({"op": "hello"}))
        .await
        .map_err(WorkerErr::into_message)?;
    proc.version = hello["version"].as_str().unwrap_or("").to_string();
    {
        let mut m = worker_meta().lock().unwrap_or_else(|e| e.into_inner());
        m.running = true;
        m.version = proc.version.clone();
    }
    crate::host::changed();
    Ok(proc)
}

/// Maximum audio response payload accepted from `Streamerz-Voice` (64 MiB).
const MAX_WORKER_AUDIO: usize = 64 << 20;
const MAX_WORKER_HEADER: usize = 16 << 10;

enum WorkerErr {
    /// Logical error answered by a healthy worker (e.g. bad text or missing file); worker stays alive.
    App(String),
    /// Transport failure, timeout, or unexpected exit; worker must be dropped and restarted next time.
    Transport(String),
}

impl WorkerErr {
    fn into_message(self) -> String {
        match self {
            Self::App(msg) | Self::Transport(msg) => msg,
        }
    }
}

async fn send_on(proc: &mut WorkerProc, req: &Value) -> Result<(Value, Vec<u8>), WorkerErr> {
    let wait_secs = match req["op"].as_str() {
        Some("unpack" | "load" | "speak") => 120,
        _ => 10,
    };
    let fut = async {
        let mut line = serde_json::to_string(req).map_err(|e| WorkerErr::App(e.to_string()))?;
        line.push('\n');
        proc.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| WorkerErr::Transport(format!("Streamerz-Voice write failed: {e}")))?;
        proc.stdin
            .flush()
            .await
            .map_err(|e| WorkerErr::Transport(format!("Streamerz-Voice flush failed: {e}")))?;
        let head = read_header(&mut proc.stdout).await?;
        let raw_len = head.get("bytes").map_or(Ok(0), |n| {
            n.as_u64()
                .ok_or_else(|| WorkerErr::Transport("invalid Streamerz-Voice audio size".into()))
        })?;
        if raw_len > MAX_WORKER_AUDIO as u64 {
            return Err(WorkerErr::Transport(format!(
                "Streamerz-Voice audio payload too large ({raw_len} bytes)"
            )));
        }
        let len = raw_len as usize;
        let mut bytes = vec![0u8; len];
        if len > 0 {
            proc.stdout
                .read_exact(&mut bytes)
                .await
                .map_err(|e| WorkerErr::Transport(format!("Streamerz-Voice audio read failed: {e}")))?;
        }
        if head["ok"] == false {
            return Err(WorkerErr::App(
                head["error"].as_str().unwrap_or("Streamerz-Voice error").to_string(),
            ));
        }
        Ok((head, bytes))
    };
    tokio::time::timeout(std::time::Duration::from_secs(wait_secs), fut)
        .await
        .map_err(|_| WorkerErr::Transport(format!("Streamerz-Voice timed out after {wait_secs}s")))?
}

async fn read_header(input: &mut (impl tokio::io::AsyncBufRead + Unpin)) -> Result<Value, WorkerErr> {
    let mut line = Vec::new();
    input
        .take(MAX_WORKER_HEADER as u64 + 1)
        .read_until(b'\n', &mut line)
        .await
        .map_err(|e| WorkerErr::Transport(format!("Streamerz-Voice read failed: {e}")))?;
    if line.len() > MAX_WORKER_HEADER {
        return Err(WorkerErr::Transport(
            "Streamerz-Voice response header is too large".into(),
        ));
    }
    if line.last() != Some(&b'\n') {
        return Err(WorkerErr::Transport(
            "Streamerz-Voice exited before sending a complete response".into(),
        ));
    }
    let head: Value =
        serde_json::from_slice(&line).map_err(|e| WorkerErr::Transport(format!("bad Streamerz-Voice JSON: {e}")))?;
    if !head["ok"].is_boolean() {
        return Err(WorkerErr::Transport("Streamerz-Voice response has no status".into()));
    }
    Ok(head)
}

fn worker_stopped() {
    {
        let mut m = worker_meta().lock().unwrap_or_else(|e| e.into_inner());
        m.running = false;
        m.loaded = None;
        m.memory = 0;
    }
    crate::host::changed();
}

/// An unfinished exchange can leave a partial response in the pipe. Dropping it kills the child.
struct Exchange(Option<WorkerProc>);

impl Drop for Exchange {
    fn drop(&mut self) {
        if self.0.take().is_some() {
            worker_stopped();
        }
    }
}

impl Exchange {
    async fn stop(&mut self) {
        if let Some(mut proc) = self.0.take() {
            worker_stopped();
            let _ = proc.child.kill().await;
        }
    }
}

/// Sends one request to the persistent `Streamerz-Voice` worker, spawning it lazily if needed.
async fn request(req: Value) -> Result<(Value, Vec<u8>), String> {
    let mut guard = worker_proc().lock().await;
    let proc = match guard.take() {
        Some(proc) => proc,
        None => spawn_worker().await?,
    };
    let mut exchange = Exchange(Some(proc));
    let proc = exchange.0.as_mut().unwrap();
    let result = send_on(proc, &req).await;
    if matches!(result, Err(WorkerErr::Transport(_))) {
        exchange.stop().await;
        return result.map_err(WorkerErr::into_message);
    }
    if matches!(req["op"].as_str(), Some("speak" | "load")) {
        // Even on a synthesis error the worker may have loaded or dropped a model.
        match send_on(proc, &json!({"op": "state"})).await {
            Ok((st, _)) => {
                {
                    let mut m = worker_meta().lock().unwrap_or_else(|e| e.into_inner());
                    m.loaded = st["loaded"].as_str().map(str::to_string);
                    m.memory = st["memory"].as_u64().unwrap_or(0);
                }
                crate::host::changed();
            }
            Err(WorkerErr::Transport(_)) => {
                exchange.stop().await;
                return result.map_err(WorkerErr::into_message);
            }
            Err(WorkerErr::App(_)) => {}
        }
    }
    *guard = exchange.0.take();
    result.map_err(WorkerErr::into_message)
}

/// Stops the `Streamerz-Voice` worker process completely, returning its RAM to the OS.
pub async fn unload() {
    let mut guard = worker_proc().lock().await;
    let proc = guard.take();
    worker_stopped();
    if let Some(mut p) = proc {
        drop(p.stdin);
        if tokio::time::timeout(std::time::Duration::from_secs(2), p.child.wait())
            .await
            .is_err()
        {
            let _ = p.child.kill().await;
        }
    }
}

/// Called when the speech queue drains: if `keep_loaded` is false, stops `Streamerz-Voice` so no
/// RAM stays allocated between speeches.
pub async fn maybe_unload_idle(keep_loaded: bool) {
    if !keep_loaded && worker_meta().lock().unwrap_or_else(|e| e.into_inner()).running {
        unload().await;
    }
}

/// Downloads and verifies the `Streamerz-Voice` binary for this Streamerz version.
pub async fn install_engine() -> Result<(), String> {
    let (url, expected_sha) = engine_source()?;
    let expected_size = ENGINE_SIZE.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let (active, _guard) = begin_download("engine", "Streamerz-Voice", expected_size)?;
    let dl_dir = downloads_dir();
    std::fs::create_dir_all(&dl_dir).map_err(|e| format!("{}: {e}", dl_dir.display()))?;
    let part = dl_dir.join(format!("{}.part", binary_name()));
    let got = download_tracked(&url, &part, expected_size, &active).await?;
    if !got.complete || (expected_size > 0 && got.len != expected_size) {
        return Err("Streamerz-Voice download did not finish; retry to resume.".into());
    }
    let valid_bin = got.magic.starts_with(b"\x7fELF") || got.magic.starts_with(b"MZ") || cfg!(test);
    if !valid_bin {
        let _ = std::fs::remove_file(&part);
        return Err("Downloaded file is not a valid Streamerz-Voice binary.".into());
    }
    if let Some(want) = expected_sha {
        if !want.eq_ignore_ascii_case(&got.sha256) {
            let _ = std::fs::remove_file(&part);
            return Err("Streamerz-Voice SHA-256 verification failed.".into());
        }
    }
    unload().await;
    let target = binary_path();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&part, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(())
}

/// Removes the `Streamerz-Voice` binary (stopping it first). Models stay unless removed separately.
pub async fn remove_engine() -> Result<(), String> {
    unload().await;
    let bin = binary_path();
    if bin.exists() {
        std::fs::remove_file(&bin).map_err(|e| format!("{}: {e}", bin.display()))?;
    }
    let part = downloads_dir().join(format!("{}.part", binary_name()));
    let _ = std::fs::remove_file(part);
    crate::host::changed();
    Ok(())
}

/// Downloads, verifies SHA-256, and unpacks a catalog model into `voices/models/<id>`.
/// Ensures `Streamerz-Voice` is installed first (for unpacking `.tar.bz2` and running the voice).
pub async fn install_model(id: &str) -> Result<(), String> {
    let entry = catalog_entry(id).ok_or_else(|| format!("unknown voice model {id}"))?;
    if !engine_installed() {
        install_engine().await?;
    }
    install_model_from(
        entry.id.as_str(),
        &entry.name,
        &entry.url,
        entry.disk_bytes,
        &entry.sha256,
    )
    .await
}

pub async fn install_model_from(
    id: &str,
    label: &str,
    url: &str,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), String> {
    let (active, _guard) = begin_download(id, label, expected_size)?;
    let dl_dir = downloads_dir();
    std::fs::create_dir_all(&dl_dir).map_err(|e| format!("{}: {e}", dl_dir.display()))?;
    let part = dl_dir.join(format!("{}.tar.bz2.part", safe_folder(id)));
    let got = download_tracked(url, &part, expected_size, &active).await?;
    if !got.complete || (expected_size > 0 && got.len != expected_size) {
        return Err(format!(
            "{label}: download incomplete ({} / {expected_size} bytes); retry to resume.",
            got.len
        ));
    }
    {
        let mut s = active.status.lock().unwrap_or_else(|e| e.into_inner());
        s.phase = "verifying".into();
        s.percent = 100;
    }
    crate::host::changed();
    if !expected_sha256.is_empty() && !expected_sha256.eq_ignore_ascii_case(&got.sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{label}: SHA-256 check failed."));
    }
    {
        let mut s = active.status.lock().unwrap_or_else(|e| e.into_inner());
        s.phase = "unpacking".into();
    }
    crate::host::changed();
    let dest = model_dir(id);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let unpacked = request(json!({
        "op": "unpack",
        "archive": part.to_string_lossy(),
        "into": dest.to_string_lossy(),
    }))
    .await;
    if unpacked.is_ok() {
        let _ = std::fs::remove_file(&part);
    }
    unpacked.map(|_| ())
}

async fn download_tracked(
    url: &str,
    part: &Path,
    expected_size: u64,
    active: &Arc<ActiveDownload>,
) -> Result<crate::http::Download, String> {
    let headers = [("User-Agent".to_string(), "Streamerz".to_string())];
    let last_notify = AtomicU64::new(0);
    let started = std::time::Instant::now();
    let got = crate::http::download_resumable(url, &headers, part, MAX_DOWNLOAD, |done, total| {
        if active.cancel.load(Ordering::SeqCst) {
            return false;
        }
        let total = total.unwrap_or(expected_size);
        let pct = done.saturating_mul(100).checked_div(total).unwrap_or(0).min(100) as u8;
        {
            let mut s = active.status.lock().unwrap_or_else(|e| e.into_inner());
            s.downloaded = done;
            if total > 0 {
                s.total = total;
            }
            s.percent = pct;
        }
        let elapsed = started.elapsed().as_millis() as u64;
        let prev = last_notify.load(Ordering::Relaxed);
        if elapsed.saturating_sub(prev) >= 250 {
            last_notify.store(elapsed, Ordering::Relaxed);
            crate::host::changed();
        }
        true
    })
    .await?;
    if got.status != 200 {
        return Err(format!("HTTP {}", got.status));
    }
    Ok(got)
}

/// Removes an installed model folder (and unloads if it was currently loaded).
pub async fn remove_model(id: &str) -> Result<(), String> {
    let dest = model_dir(id);
    let was_loaded = worker_meta()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .loaded
        .as_deref()
        .is_some_and(|k| k.contains(&dest.to_string_lossy().into_owned()));
    if was_loaded {
        unload().await;
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    let part = downloads_dir().join(format!("{}.tar.bz2.part", safe_folder(id)));
    let _ = std::fs::remove_file(part);
    crate::host::changed();
    Ok(())
}

/// Downloads (or reuses cached) voice sample and plays it on the Monitor output.
pub async fn play_sample(id: &str) -> Result<PathBuf, String> {
    let entry = catalog_entry(id).ok_or_else(|| format!("unknown voice {id}"))?;
    if entry.sample_url.is_empty() {
        return Err(format!("{} has no audio sample.", entry.name));
    }
    let ext = if entry.sample_url.ends_with(".wav") {
        "wav"
    } else {
        "mp3"
    };
    let sample_dir = crate::config_path().with_file_name("cache").join("voice-samples");
    std::fs::create_dir_all(&sample_dir).map_err(|e| e.to_string())?;
    let path = sample_dir.join(format!("{}.{ext}", safe_folder(id)));
    if !path.is_file() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) < 128 {
        let part = sample_dir.join(format!("{}.{ext}.part", safe_folder(id)));
        let got = crate::http::download(&entry.sample_url, &[], &part, 8 << 20).await?;
        if got.status != 200 || !got.complete || got.len < 128 {
            let _ = std::fs::remove_file(&part);
            return Err(format!(
                "could not download sample for {} (HTTP {})",
                entry.name, got.status
            ));
        }
        std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
    }
    crate::sound::play(&path, "voice-sample", "Monitor", 1.0)?;
    Ok(path)
}

/// Speaks using an installed Piper or Kokoro voice id (`piper:...` or `kokoro:...`).
pub async fn speak(text: &str, voice_id: &str, speed: f64, pitch: f64) -> Result<Vec<u8>, String> {
    let pitch = if pitch.is_finite() { pitch.clamp(0.5, 1.5) } else { 1.0 };
    let speed = if speed.is_finite() { speed.clamp(0.5, 3.0) } else { 1.0 };
    // Compensate synthesis speed when pitch shifting so speaking tempo stays true to `speed`.
    let synth_speed = (speed / pitch).clamp(0.4, 3.0);
    let req = if let Some(sp_id) = voice_id.strip_prefix("kokoro:") {
        let dir = model_dir("kokoro:multi-lang-v1");
        if !is_model_installed("kokoro:multi-lang-v1") {
            return Err("Kokoro voice pack is not installed.".into());
        }
        let sp = kokoro_speakers()
            .iter()
            .find(|s| s.id == sp_id)
            .ok_or_else(|| format!("unknown Kokoro voice {sp_id}"))?;
        json!({
            "op": "speak",
            "kind": "kokoro",
            "dir": dir.to_string_lossy(),
            "lang": sp.lang,
            "speaker": sp.sid,
            "speed": synth_speed,
            "text": text,
        })
    } else if voice_id.starts_with("piper:") {
        let dir = model_dir(voice_id);
        if !is_model_installed(voice_id) {
            return Err(format!("voice {voice_id} is not installed"));
        }
        json!({
            "op": "speak",
            "kind": "piper",
            "dir": dir.to_string_lossy(),
            "speed": synth_speed,
            "text": text,
        })
    } else {
        return Err(format!("not a neural voice: {voice_id}"));
    };
    let (_, wav) = request(req).await?;
    Ok(super::effects::apply_pitch(wav, pitch))
}

/// Speaks using a Kokoro voice mix (`mix` is 2..=4 `(speaker_id, share)` pairs).
pub async fn speak_mix(text: &str, mix: &[(i32, f64)], lang: &str, speed: f64, pitch: f64) -> Result<Vec<u8>, String> {
    if !is_model_installed("kokoro:multi-lang-v1") {
        return Err("Kokoro voice pack is not installed.".into());
    }
    if mix.is_empty() {
        return Err("A voice mix needs at least one voice.".into());
    }
    let pitch = if pitch.is_finite() { pitch.clamp(0.5, 1.5) } else { 1.0 };
    let speed = if speed.is_finite() { speed.clamp(0.5, 3.0) } else { 1.0 };
    let synth_speed = (speed / pitch).clamp(0.4, 3.0);
    let dir = model_dir("kokoro:multi-lang-v1");
    let mix_json: Vec<Value> = mix.iter().map(|(sid, share)| json!([sid, share])).collect();
    let lang = if lang.trim().is_empty() { "en-us" } else { lang.trim() };
    let (_, wav) = request(json!({
        "op": "speak",
        "kind": "kokoro",
        "dir": dir.to_string_lossy(),
        "lang": lang,
        "mix": mix_json,
        "speed": synth_speed,
        "text": text,
    }))
    .await?;
    Ok(super::effects::apply_pitch(wav, pitch))
}

/// Speaks using Pocket TTS cloned from a local reference WAV in `voices/recordings/`.
pub async fn speak_clone(text: &str, recording_file: &str, speed: f64, pitch: f64) -> Result<Vec<u8>, String> {
    if !is_model_installed("pocket:en-v1") {
        return Err("Pocket TTS is not installed. Install it from Sounds > Find voices.".into());
    }
    let ref_path = recordings_dir().join(recording_file);
    if !ref_path.is_file() {
        return Err(format!("reference recording {} is missing", ref_path.display()));
    }
    let pitch = if pitch.is_finite() { pitch.clamp(0.5, 1.5) } else { 1.0 };
    let speed = if speed.is_finite() { speed.clamp(0.5, 3.0) } else { 1.0 };
    let synth_speed = (speed / pitch).clamp(0.4, 3.0);
    let dir = model_dir("pocket:en-v1");
    let (_, wav) = request(json!({
        "op": "speak",
        "kind": "pocket",
        "dir": dir.to_string_lossy(),
        "reference": ref_path.to_string_lossy(),
        "speed": synth_speed,
        "text": text,
    }))
    .await?;
    Ok(super::effects::apply_pitch(wav, pitch))
}

/// Converts any supported audio recording (WAV, MP3, OGG, M4A, FLAC) into a clean 24 kHz mono WAV
/// stored in `voices/recordings/<id>.wav`. Returns `(filename, duration_ms)`.
pub fn import_recording(source: &Path, id: &str) -> Result<(String, u64), String> {
    if !source.is_file() {
        return Err(format!("recording file not found: {}", source.display()));
    }
    let (wav, duration_ms) = crate::sound::decode_mono_wav(source, 30)?;
    if duration_ms < 1000 {
        return Err("Recording is too short (use 5 to 10 seconds of clear speech).".into());
    }
    let rec_dir = recordings_dir();
    std::fs::create_dir_all(&rec_dir).map_err(|e| format!("{}: {e}", rec_dir.display()))?;
    let filename = format!("{}.wav", safe_folder(id));
    let dest = rec_dir.join(&filename);
    std::fs::write(&dest, &wav).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok((filename, duration_ms))
}

/// Wraps raw mono 16-bit little-endian PCM bytes in a standard RIFF WAVE header.
fn encode_pcm_wav(pcm: &[u8], rate: u32) -> Result<Vec<u8>, String> {
    let mut wav = crate::sound::pcm_wav_header(rate, 1, pcm.len() / 2)?;
    wav.extend_from_slice(pcm);
    Ok(wav)
}

/// Records `seconds` (clamped 3..=12) from the default microphone on Linux via `parec` (PulseAudio /
/// PipeWire) into a temporary 24 kHz mono WAV file that can be passed directly to `import_recording`.
#[cfg(not(windows))]
pub async fn record_mic(seconds: u32) -> Result<PathBuf, String> {
    let secs = seconds.clamp(3, 12);
    const RATE: u32 = 24_000;
    let target_bytes = (RATE as usize) * 2 * (secs as usize);
    let mut cmd = {
        #[cfg(test)]
        if let Some(prog) = TEST_PAREC.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            std::process::Command::new(prog)
        } else {
            crate::sandbox::on_host(std::process::Command::new("parec"))
        }
        #[cfg(not(test))]
        {
            crate::sandbox::on_host(std::process::Command::new("parec"))
        }
    };
    cmd.args(["--format=s16le", "--rate=24000", "--channels=1", "--latency-msec=50"]);
    let mut child = tokio::process::Command::from(cmd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            "Microphone recording requires parec (PulseAudio / PipeWire). Pick a WAV or MP3 file instead.".to_string()
        })?;
    let mut stdout = child.stdout.take().ok_or("no stdout on parec")?;
    let mut pcm = vec![0u8; target_bytes];
    let read_res = tokio::time::timeout(
        std::time::Duration::from_secs(secs as u64 + 3),
        stdout.read_exact(&mut pcm),
    )
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    read_res
        .map_err(|_| "Microphone recording timed out.".to_string())?
        .map_err(|e| format!("Microphone recording failed: {e}"))?;
    let wav = encode_pcm_wav(&pcm, RATE)?;
    let path = std::env::temp_dir().join(format!("streamerz-mic-{}.wav", crate::engine::uuid()));
    std::fs::write(&path, &wav).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Records `seconds` (clamped 3..=12) from the default Windows microphone via WASAPI shared-mode
/// capture (event-driven, resampled by Windows to 24 kHz mono 16-bit PCM).
#[cfg(windows)]
pub async fn record_mic(seconds: u32) -> Result<PathBuf, String> {
    let secs = seconds.clamp(3, 12);
    tokio::task::spawn_blocking(move || record_mic_wasapi(secs))
        .await
        .map_err(|e| format!("Microphone recording failed: {e}"))?
}

#[cfg(windows)]
fn record_mic_wasapi(secs: u32) -> Result<PathBuf, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX, WAVE_FORMAT_PCM,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

    const RATE: u32 = 24_000;
    const AHEAD: i64 = 500_000; // 50 ms in 100 ns units
    let target_bytes = (RATE as usize) * 2 * (secs as usize);
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };

    // SAFETY: COM interfaces and event handle are initialized and released on this blocking thread.
    let pcm = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("no audio devices: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .map_err(|e| format!("no default microphone: {e}"))?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("cannot open microphone: {e}"))?;
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                AHEAD,
                0,
                &format,
                None,
            )
            .map_err(|e| format!("cannot initialize microphone capture: {e}"))?;
        let event = CreateEventW(None, false, false, PCWSTR::null()).map_err(|e| e.to_string())?;
        let capture_result = (|| -> Result<Vec<u8>, String> {
            client.SetEventHandle(event).map_err(|e| e.to_string())?;
            let capture: IAudioCaptureClient = client.GetService().map_err(|e| e.to_string())?;
            client.Start().map_err(|e| e.to_string())?;
            let mut out = Vec::with_capacity(target_bytes);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs as u64 + 3);
            while out.len() < target_bytes {
                if std::time::Instant::now() >= deadline {
                    let _ = client.Stop();
                    return Err("Microphone recording timed out.".into());
                }
                if WaitForSingleObject(event, 1000) != WAIT_OBJECT_0 {
                    continue;
                }
                loop {
                    let packet = capture.GetNextPacketSize().map_err(|e| e.to_string())?;
                    if packet == 0 {
                        break;
                    }
                    let mut ptr = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    capture
                        .GetBuffer(&mut ptr, &mut frames, &mut flags, None, None)
                        .map_err(|e| e.to_string())?;
                    let want = (target_bytes - out.len()).min((frames as usize) * 2);
                    if (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0 || ptr.is_null() {
                        out.resize(out.len() + want, 0);
                    } else {
                        let slice = std::slice::from_raw_parts(ptr, want);
                        out.extend_from_slice(slice);
                    }
                    capture.ReleaseBuffer(frames).map_err(|e| e.to_string())?;
                    if out.len() >= target_bytes {
                        break;
                    }
                }
            }
            let _ = client.Stop();
            Ok(out)
        })();
        let _ = CloseHandle(event);
        capture_result?
    };

    let wav = encode_pcm_wav(&pcm, RATE)?;
    let path = std::env::temp_dir().join(format!("streamerz-mic-{}.wav", crate::engine::uuid()));
    std::fs::write(&path, &wav).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_header_is_bounded_and_leaves_binary_payload_unread() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let mut input = &b"{\"ok\":true,\"bytes\":2}\n\x00\xff"[..];
            assert_eq!(read_header(&mut input).await.ok().unwrap()["bytes"], 2);
            assert_eq!(input, b"\x00\xff");

            let mut partial = &b"{\"ok\":true}"[..];
            assert!(matches!(read_header(&mut partial).await, Err(WorkerErr::Transport(_))));
            let overlong = vec![b' '; MAX_WORKER_HEADER * 2];
            let mut input = overlong.as_slice();
            assert!(matches!(read_header(&mut input).await, Err(WorkerErr::Transport(_))));
            assert_eq!(input.len(), MAX_WORKER_HEADER - 1);
            let mut input = &b"{}\n"[..];
            assert!(matches!(read_header(&mut input).await, Err(WorkerErr::Transport(_))));
        });
    }

    #[test]
    fn bundled_catalog_has_valid_hashes_sizes_and_kokoro_speakers() {
        let entries = catalog();
        assert!(entries.len() >= 20, "expected curated catalog, got {}", entries.len());
        let mut ids = std::collections::HashSet::new();
        for e in entries {
            assert!(ids.insert(&e.id), "duplicate voice id {}", e.id);
            assert!(e.url.starts_with("https://"), "bad url {}", e.url);
            assert_eq!(e.sha256.len(), 64, "bad sha256 len for {}", e.id);
            assert!(
                e.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "non-hex sha for {}",
                e.id
            );
            assert!(e.disk_bytes > 1_000_000, "unexpected size for {}", e.id);
            assert!(e.ram_mb >= 50, "unexpected ram for {}", e.id);
            assert!(!e.license.is_empty(), "missing license for {}", e.id);
        }
        assert_eq!(kokoro_speakers().len(), 54);
        // Non-commercial voices are explicitly flagged.
        assert!(!catalog_entry("piper:en_US-ryan").unwrap().commercial);
        assert!(!catalog_entry("piper:en_US-lessac").unwrap().commercial);
        assert!(catalog_entry("piper:pt_BR-faber").unwrap().commercial);
    }

    #[cfg(not(windows))]
    #[test]
    fn worker_protocol_speaks_unpacks_and_unloads_against_fake_binary() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("streamerz-neural-test-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&tmp).unwrap();
        let fake = tmp.join("Streamerz-Voice");
        // Minimal fake worker in python3 speaking our exact JSON + binary WAV protocol.
        std::fs::write(
            &fake,
            r#"#!/usr/bin/env python3
import json, os, struct, sys

loaded = None
stalled = False
def sine_wav():
    rate = 24000
    frames = 2400
    data = b"\x00\x10" * frames
    hdr = struct.pack("<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(data), b"WAVE", b"fmt ", 16, 1, 1, rate, rate*2, 2, 16, b"data", len(data))
    return hdr + data

for line in sys.stdin:
    req = json.loads(line)
    op = req.get("op")
    if op == "hello":
        resp, blob = {"ok": True, "version": "1.0.0-test"}, b""
    elif op == "unpack":
        os.makedirs(req["into"], exist_ok=True)
        open(os.path.join(req["into"], "model.int8.onnx"), "wb").write(b"onnx")
        resp, blob = {"ok": True}, b""
    elif op == "speak":
        loaded = "exit" if req.get("text") == "exit on state" else f"{req.get('kind')}|{req.get('dir')}"
        blob = sine_wav()
        resp = {"ok": True, "rate": 24000, "ms": 5, "bytes": len(blob)}
    elif op == "state":
        if loaded == "exit":
            sys.exit(1)
        resp, blob = {"ok": True, "loaded": loaded, "memory": 12345678 if loaded else 1024}, b""
    elif op == "stall":
        stalled = True
        resp, blob = {"ok": True, "bytes": 2}, b""
    else:
        resp, blob = {"ok": False, "error": "unknown"}, b""
    sys.stdout.buffer.write((json.dumps(resp) + "\n").encode() + blob)
    sys.stdout.buffer.flush()
    if stalled:
        open(sys.argv[0] + ".stalled", "w").close()
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        *TEST_DIR.lock().unwrap() = Some(tmp.join("voices"));
        *TEST_BINARY.lock().unwrap() = Some(fake.clone());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            unload().await;
            // Serve a tiny fake archive locally and verify SHA-256 + unpack + install + speak + unload.
            use sha2::Digest;
            let payload = b"fake-tar-bz2-payload".to_vec();
            let sha = crate::fmt::hex(&sha2::Sha256::digest(&payload));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let served = payload.clone();
            tokio::spawn(async move {
                for _ in 0..2 {
                    let (mut s, _) = listener.accept().await.unwrap();
                    let mut buf = [0u8; 1024];
                    let _ = s.read(&mut buf).await;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", served.len());
                    s.write_all(head.as_bytes()).await.unwrap();
                    s.write_all(&served).await.unwrap();
                }
            });
            let url = format!("http://127.0.0.1:{port}/model.tar.bz2");
            // Bad SHA-256 must fail and leave nothing installed.
            let bad = install_model_from(
                "piper:pt_BR-faber",
                "Faber",
                &url,
                payload.len() as u64,
                &"0".repeat(64),
            )
            .await;
            assert!(bad.is_err(), "mismatched sha256 must fail");
            assert!(!is_model_installed("piper:pt_BR-faber"));

            // Matching SHA-256 succeeds and unpacks.
            install_model_from("piper:pt_BR-faber", "Faber", &url, payload.len() as u64, &sha)
                .await
                .unwrap();
            assert!(is_model_installed("piper:pt_BR-faber"));
            assert!(installed_voices().iter().any(|v| v.id == "piper:pt_BR-faber"));

            // Speaking returns a valid WAV and updates worker state.
            let wav = speak("Olá mundo", "piper:pt_BR-faber", 1.0, 1.2).await.unwrap();
            assert!(wav.starts_with(b"RIFF"));
            let st = state();
            assert!(st.running && st.memory_bytes == 12345678);

            let wav = speak("exit on state", "piper:pt_BR-faber", 1.0, 1.0).await.unwrap();
            assert!(wav.starts_with(b"RIFF"));
            assert!(!state().running, "a failed state refresh clears the worker");
            assert!(speak("retry", "piper:pt_BR-faber", 1.0, 1.0).await.is_ok());

            let marker = fake.with_extension("stalled");
            {
                let exchange = request(json!({"op": "stall"}));
                let until_stalled = async {
                    while !marker.exists() {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                };
                tokio::select! {
                    _ = exchange => panic!("the worker should still be sending the payload"),
                    result = tokio::time::timeout(std::time::Duration::from_secs(3), until_stalled) => result.unwrap(),
                }
            }
            assert!(!state().running, "a cancelled response must discard the worker");
            assert!(speak("retry", "piper:pt_BR-faber", 1.0, 1.0).await.is_ok());

            // keep_loaded = false unloads the worker completely.
            maybe_unload_idle(false).await;
            let st = state();
            assert!(!st.running && st.memory_bytes == 0 && st.loaded.is_none());

            // Removing the model deletes it cleanly.
            remove_model("piper:pt_BR-faber").await.unwrap();
            assert!(!is_model_installed("piper:pt_BR-faber"));
        });

        *TEST_DIR.lock().unwrap() = None;
        *TEST_BINARY.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(not(windows))]
    #[test]
    fn record_mic_captures_pcm_into_valid_wav_and_imports_cleanly() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("streamerz-mic-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let fake_parec = tmp.join("parec");
        std::fs::write(
            &fake_parec,
            "#!/usr/bin/env python3\nimport math, struct, sys\nrate = 24000\nfor i in range(rate * 4):\n    s = int(8000 * math.sin(2 * math.pi * 440 * i / rate))\n    sys.stdout.buffer.write(struct.pack('<h', s))\nsys.stdout.buffer.flush()\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_parec, std::fs::Permissions::from_mode(0o755)).unwrap();

        *TEST_DIR.lock().unwrap() = Some(tmp.join("voices"));
        *TEST_PAREC.lock().unwrap() = Some(fake_parec);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let wav_path = rt.block_on(record_mic(3)).unwrap();
        let bytes = std::fs::read(&wav_path).unwrap();
        assert!(bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE");
        assert_eq!(bytes.len(), 44 + 24_000 * 2 * 3);
        let (imported, ms) = import_recording(&wav_path, "custom:mic-test").unwrap();
        assert!(imported.ends_with(".wav"));
        assert!((2900..=3100).contains(&ms), "expected ~3000ms, got {ms}");

        *TEST_PAREC.lock().unwrap() = None;
        *TEST_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_file(&wav_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(not(windows))]
    #[test]
    fn worker_recovers_cleanly_after_unexpected_crash() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("streamerz-crash-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let flag = tmp.join("crashed-once");
        let fake = tmp.join("Streamerz-Voice");
        std::fs::write(
            &fake,
            format!(
                r#"#!/usr/bin/env python3
import json, os, struct, sys

def sine_wav():
    pcm = struct.pack('<100h', *([0] * 100))
    return b'RIFF' + struct.pack('<I', 36 + len(pcm)) + b'WAVEfmt ' + struct.pack('<IHHIIHH', 16, 1, 1, 24000, 48000, 2, 16) + b'data' + struct.pack('<I', len(pcm)) + pcm

flag = {flag:?}
for line in sys.stdin:
    req = json.loads(line)
    op = req.get("op")
    if op == "hello":
        resp, blob = {{"ok": True, "version": "1.0.0"}}, b""
    elif op == "speak":
        if not os.path.exists(flag):
            open(flag, "w").close()
            sys.exit(1)
        blob = sine_wav()
        resp = {{"ok": True, "rate": 24000, "ms": 4, "bytes": len(blob)}}
    elif op == "state":
        resp, blob = {{"ok": True, "loaded": "piper|ok", "memory": 4096}}, b""
    else:
        resp, blob = {{"ok": True}}, b""
    sys.stdout.buffer.write((json.dumps(resp) + "\n").encode() + blob)
    sys.stdout.buffer.flush()
"#,
                flag = flag.to_string_lossy()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let voices_dir = tmp.join("voices");
        *TEST_DIR.lock().unwrap() = Some(voices_dir);
        *TEST_BINARY.lock().unwrap() = Some(fake);
        let model = model_dir("piper:en_US-amy");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("en_US-amy-medium.onnx"), b"fake").unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            unload().await;
            let first = speak("crash me", "piper:en_US-amy", 1.0, 1.0).await;
            assert!(first.is_err(), "first call must report worker crash");
            assert!(!state().running, "worker state must reset to not running after crash");
            let second = speak("recovered", "piper:en_US-amy", 1.0, 1.0).await.unwrap();
            assert!(second.starts_with(b"RIFF"));
            assert!(state().running);
            unload().await;
        });

        *TEST_DIR.lock().unwrap() = None;
        *TEST_BINARY.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
