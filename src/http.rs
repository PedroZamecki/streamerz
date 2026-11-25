//! A small HTTP/1.1 client over tokio, with TLS from the OS (SChannel on Windows, OpenSSL on Linux).
//! ponytail: one connection per request (Connection: close); keep-alive per host when the platforms
//! poll their APIs.

use std::borrow::Cow;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_BODY: usize = 16 << 20;
/// The most a response's head (or a chunk's size line) may take: a server can't make it grow forever.
const MAX_HEAD: usize = 64 << 10;

/// scheme, host, port, path and query.
fn parse(url: &str) -> Result<(bool, String, u16, String), String> {
    let (tls, rest) = match url.split_once("://") {
        Some((s, r)) if s.eq_ignore_ascii_case("https") => (true, r),
        Some((s, r)) if s.eq_ignore_ascii_case("http") => (false, r),
        _ => return Err(format!("not an http(s) URL: {url}")),
    };
    // The fragment stays in the browser: it is never sent.
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    let path = request_target(path)?;
    // An IPv6 host is in brackets ("[::1]:8080"); the host kept is the address alone.
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => match v6.split_once(']') {
            Some((h, "")) => (h, None),
            Some((h, p)) => (
                h,
                Some(p.strip_prefix(':').ok_or_else(|| format!("bad host in {url}"))?),
            ),
            None => return Err(format!("bad host in {url}")),
        },
        None => match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        },
    };
    let port = match port {
        Some(p) => p.parse().map_err(|_| format!("bad port in {url}"))?,
        None => default_port(tls),
    };
    if host.is_empty() {
        return Err(format!("no host in {url}"));
    }
    Ok((tls, host.to_string(), port, path))
}

/// The path and query as sent: what a URL can't hold raw (spaces, quotes, non-ASCII) percent-encoded,
/// as browsers and .NET do. A control character would end the request line early, so it is refused.
fn request_target(path: &str) -> Result<String, String> {
    let mut out = String::with_capacity(path.len() + 1);
    if !path.starts_with('/') {
        out.push('/');
    }
    for b in path.bytes() {
        match b {
            0..=0x1f | 0x7f => return Err("control character in the URL".into()),
            b' ' | b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}' | 0x80.. => {
                out += &format!("%{b:02X}")
            }
            _ => out.push(char::from(b)),
        }
    }
    Ok(out)
}

fn default_port(tls: bool) -> u16 {
    if tls {
        443
    } else {
        80
    }
}

/// `host:port` as a URL or a Host header writes it: brackets around an IPv6 address, no default port.
fn authority(tls: bool, host: &str, port: u16) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if port == default_port(tls) {
        host
    } else {
        format!("{host}:{port}")
    }
}

/// Send a request, following up to 5 redirects. Headers are (name, value); a body gets its
/// Content-Length.
pub async fn request(method: &str, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<Response, String> {
    let mut url = url.to_string();
    let (mut method, mut body, mut headers) = (method, body, Cow::Borrowed(headers));
    for _ in 0..6 {
        let r = tokio::time::timeout(TIMEOUT, once(method, &url, &headers, body))
            .await
            .map_err(|_| format!("{url}: timed out"))??;
        match follow(&url, r.status, &r.headers, &mut headers)? {
            Some(to) => {
                url = to;
                // 303 asks for a GET of another address: the body was for this one.
                if r.status == 303 && method != "HEAD" {
                    method = "GET";
                    body = b"";
                }
            }
            None => return Ok(r),
        }
    }
    Err(format!("{url}: too many redirects"))
}

/// Where a redirect points, if `status` is one: its Location, against `url` when it is relative.
fn redirect(url: &str, status: u16, headers: &[(String, String)]) -> Result<Option<String>, String> {
    let location = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location"));
    let (301 | 302 | 303 | 307 | 308, Some((_, to))) = (status, location) else {
        return Ok(None);
    };
    let to = to.trim();
    if to
        .split_once("://")
        .is_some_and(|(s, _)| s.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return Ok(Some(to.into()));
    }
    let (tls, host, port, path) = parse(url)?;
    let scheme = if tls { "https" } else { "http" };
    if let Some(rest) = to.strip_prefix("//") {
        return Ok(Some(format!("{scheme}://{rest}")));
    }
    let origin = format!("{scheme}://{}", authority(tls, &host, port));
    let dir = path.split('?').next().unwrap_or_default();
    Ok(Some(match to.as_bytes().first() {
        Some(b'/') => format!("{origin}{to}"),
        Some(b'?') => format!("{origin}{dir}{to}"),
        None | Some(b'#') => format!("{origin}{path}"),
        // Relative to the folder of the current path: "b" from "/a/x" is "/a/b".
        _ => format!("{origin}{}/{to}", &dir[..dir.rfind('/').unwrap_or(0)]),
    }))
}

/// The next address after a redirect, if it may be followed. HTTPS never falls back to plain HTTP
/// (the redirect is then the answer, as in .NET), and credentials never go to another site.
fn follow(
    url: &str,
    status: u16,
    got: &[(String, String)],
    headers: &mut Cow<'_, [(String, String)]>,
) -> Result<Option<String>, String> {
    let Some(to) = redirect(url, status, got)? else {
        return Ok(None);
    };
    let ((tls, host, port, _), (to_tls, to_host, to_port, _)) = (parse(url)?, parse(&to)?);
    if tls && !to_tls {
        return Ok(None);
    }
    if (tls, port) != (to_tls, to_port) || !host.eq_ignore_ascii_case(&to_host) {
        let secret = |k: &str| {
            ["authorization", "cookie", "proxy-authorization"]
                .iter()
                .any(|s| k.eq_ignore_ascii_case(s))
        };
        *headers = Cow::Owned(headers.iter().filter(|(k, _)| !secret(k)).cloned().collect());
    }
    Ok(Some(to))
}

/// A file [`download`] wrote.
pub struct Download {
    pub status: u16,
    /// Bytes written.
    pub len: u64,
    /// Every byte the server announced arrived (its Content-Length, the last chunk, or the end).
    pub complete: bool,
    /// The body's SHA-256, in lowercase hex.
    pub sha256: String,
    /// Its first bytes (up to 4), for its file type.
    pub magic: Vec<u8>,
}

/// GETs `url` into the file `to`, following redirects, a piece at a time: a release of tens of MB never
/// sits in memory. Stops at `max` bytes. Each read may take [`TIMEOUT`], the whole download as long as it
/// needs.
pub async fn download(
    url: &str,
    headers: &[(String, String)],
    to: &std::path::Path,
    max: u64,
) -> Result<Download, String> {
    use sha2::Digest;
    use std::io::Write;
    let (mut url, mut headers) = (url.to_string(), Cow::Borrowed(headers));
    for _ in 0..6 {
        let mut open = tokio::time::timeout(TIMEOUT, open("GET", &url, &headers, b""))
            .await
            .map_err(|_| format!("{url}: timed out"))??;
        if let Some(next) = follow(&url, open.status, &open.headers, &mut headers)? {
            url = next;
            continue;
        }
        let file = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut file = std::io::BufWriter::with_capacity(64 << 10, file);
        let mut hash = sha2::Sha256::new();
        let mut magic = Vec::with_capacity(4);
        let mut failed = None;
        let limit = usize::try_from(max).unwrap_or(usize::MAX);
        let (len, complete) = body(&mut open, "GET", limit, &mut |piece: &[u8]| {
            if magic.len() < 4 {
                magic.extend_from_slice(&piece[..piece.len().min(4 - magic.len())]);
            }
            hash.update(piece);
            if let Err(e) = file.write_all(piece) {
                failed.get_or_insert(e);
            }
        })
        .await?;
        if let Some(e) = failed.or_else(|| file.flush().err()) {
            return Err(format!("{}: {e}", to.display()));
        }
        return Ok(Download {
            status: open.status,
            len: len as u64,
            complete,
            sha256: hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            magic,
        });
    }
    Err(format!("{url}: too many redirects"))
}

/// Resumable download into `to` (appending when `to` already holds bytes and the server answers
/// `206 Partial Content`, else restarting cleanly from byte 0). Calls `on_progress(downloaded, total)`
/// as chunks arrive; returning `false` cancels at once and keeps the partial file on disk so a later
/// call resumes where it stopped. Computes the full-file SHA-256 on completion.
pub async fn download_resumable(
    url: &str,
    headers: &[(String, String)],
    to: &std::path::Path,
    max: u64,
    mut on_progress: impl FnMut(u64, Option<u64>) -> bool,
) -> Result<Download, String> {
    use sha2::Digest;
    use std::io::{Read, Write};
    let mut have = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
    if have > max {
        have = 0;
    }
    let (mut url, mut headers) = (url.to_string(), Cow::Borrowed(headers));
    for _ in 0..6 {
        let mut req_headers = headers.to_vec();
        if have > 0 && !req_headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("range")) {
            req_headers.push(("Range".into(), format!("bytes={have}-")));
        }
        let mut open = tokio::time::timeout(TIMEOUT, open("GET", &url, &req_headers, b""))
            .await
            .map_err(|_| format!("{url}: timed out"))??;
        if let Some(next) = follow(&url, open.status, &open.headers, &mut headers)? {
            url = next;
            continue;
        }
        // Range past EOF, or a part that doesn't start where the file stops: restart from 0 cleanly.
        if have > 0 && (open.status == 416 || open.status == 206 && range_start(&open.headers) != Some(have)) {
            let _ = std::fs::remove_file(to);
            have = 0;
            continue;
        }
        let resumed = open.status == 206 && have > 0;
        let start = if resumed { have } else { 0 };
        let content_len = open
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse::<u64>().ok());
        let total = if resumed {
            open.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-range"))
                .and_then(|(_, v)| v.rsplit_once('/')?.1.parse::<u64>().ok())
                .or_else(|| content_len.map(|l| start + l))
        } else {
            content_len
        };
        if open.status != 200 && open.status != 206 {
            return Ok(Download {
                status: open.status,
                len: start,
                complete: false,
                sha256: String::new(),
                magic: Vec::new(),
            });
        }
        let file = if resumed {
            std::fs::OpenOptions::new()
                .append(true)
                .open(to)
                .map_err(|e| format!("{}: {e}", to.display()))?
        } else {
            std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?
        };
        let mut file = std::io::BufWriter::with_capacity(64 << 10, file);
        let mut written = start;
        let mut failed: Option<std::io::Error> = None;
        let mut cancelled = false;
        let limit = usize::try_from(max.saturating_sub(start)).unwrap_or(usize::MAX);
        if !on_progress(written, total) {
            return Err("cancelled".into());
        }
        let (_, complete) = body(&mut open, "GET", limit, &mut |piece: &[u8]| {
            if cancelled || failed.is_some() {
                return;
            }
            if let Err(e) = file.write_all(piece) {
                failed = Some(e);
                return;
            }
            written += piece.len() as u64;
            if !on_progress(written, total) {
                cancelled = true;
            }
        })
        .await?;
        let flush_err = file.flush().err();
        drop(file);
        if cancelled {
            return Err("cancelled".into());
        }
        if let Some(e) = failed.or(flush_err) {
            return Err(format!("{}: {e}", to.display()));
        }
        // Stream the whole file on disk for its SHA-256 and magic (works identically for resumed files).
        let mut f = std::fs::File::open(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut hash = sha2::Sha256::new();
        let mut magic = Vec::with_capacity(4);
        let mut buf = [0u8; 64 << 10];
        let mut len = 0u64;
        loop {
            let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", to.display()))?;
            if n == 0 {
                break;
            }
            if magic.len() < 4 {
                magic.extend_from_slice(&buf[..n.min(4 - magic.len())]);
            }
            hash.update(&buf[..n]);
            len += n as u64;
        }
        return Ok(Download {
            status: 200,
            len,
            complete: complete && total.is_none_or(|t| len == t),
            sha256: hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            magic,
        });
    }
    Err(format!("{url}: too many redirects"))
}

/// Where a `206 Partial Content` part starts ("Content-Range: bytes 100-199/200" is 100).
fn range_start(headers: &[(String, String)]) -> Option<u64> {
    let (_, v) = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-range"))?;
    let (unit, range) = v.trim().split_once(' ')?;
    unit.eq_ignore_ascii_case("bytes")
        .then(|| range.split('-').next()?.trim().parse().ok())
        .flatten()
}

/// The wait before retry `n` (1-based) of something that keeps failing: 1, 2, 4, 8, 15 seconds, then 15.
pub fn backoff(n: u32) -> std::time::Duration {
    std::time::Duration::from_secs((1u64 << n.saturating_sub(1).min(4)).min(15))
}

/// One TLS setup for every connection: building one loads and parses the system's CA bundle
/// (OpenSSL), a few MB and a burst of CPU each time.
pub fn tls_connector() -> Result<native_tls::TlsConnector, String> {
    static C: std::sync::OnceLock<Result<native_tls::TlsConnector, String>> = std::sync::OnceLock::new();
    C.get_or_init(|| native_tls::TlsConnector::new().map_err(|e| e.to_string()))
        .clone()
}

pub type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

/// `tokio_tungstenite::connect_async` with the shared TLS setup (a URL, or a request with headers).
#[allow(clippy::result_large_err)] // the same error type connect_async returns
pub async fn ws_connect(
    url: impl tokio_tungstenite::tungstenite::client::IntoClientRequest + Unpin,
) -> Result<(Ws, tokio_tungstenite::tungstenite::handshake::client::Response), tokio_tungstenite::tungstenite::Error> {
    let tls = tls_connector().map_err(|e| tokio_tungstenite::tungstenite::Error::Io(std::io::Error::other(e)))?;
    tokio_tungstenite::connect_async_tls_with_config(
        url,
        None,
        false,
        Some(tokio_tungstenite::Connector::NativeTls(tls)),
    )
    .await
}

/// A response whose head was read: the body is still to come, from `io` after `rest`.
struct Open {
    status: u16,
    headers: Vec<(String, String)>,
    io: Box<dyn Io>,
    rest: Vec<u8>,
}

async fn once(method: &str, url: &str, headers: &[(String, String)], body_bytes: &[u8]) -> Result<Response, String> {
    let mut open = open(method, url, headers, body_bytes).await?;
    let mut out = Vec::new();
    let (_, complete) = body(&mut open, method, MAX_BODY, &mut |piece: &[u8]| {
        out.extend_from_slice(piece)
    })
    .await?;
    // Half an answer would pass for the whole one (a JSON cut short, a file missing its end).
    if !complete {
        return Err(format!("{url}: the answer was cut short or is over 16 MB"));
    }
    Ok(Response {
        status: open.status,
        headers: open.headers,
        body: out,
    })
}

/// The request line and headers, up to the blank line before the body.
fn request_head(
    method: &str,
    host: &str,
    path: &str,
    headers: &[(String, String)],
    len: usize,
) -> Result<String, String> {
    // A line break in a name or value would start another header (or another request).
    let broken = |s: &str| s.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0));
    if broken(method) || method.contains(' ') {
        return Err(format!("bad HTTP method {method:?}"));
    }
    if let Some((k, _)) = headers.iter().find(|(k, v)| broken(k) || k.contains(':') || broken(v)) {
        return Err(format!("bad HTTP header {k:?}"));
    }
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    if !has("user-agent") {
        req += "User-Agent: Streamerz\r\n";
    }
    for (k, v) in headers {
        req += &format!("{k}: {v}\r\n");
    }
    if len > 0 || matches!(method, "POST" | "PUT" | "PATCH") {
        req += &format!("Content-Length: {len}\r\n");
    }
    req += "\r\n";
    Ok(req)
}

/// Sends the request and reads the response's head.
async fn open(method: &str, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<Open, String> {
    let (tls, host, port, path) = parse(url)?;
    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| format!("{host}:{port}: {e}"))?;
    let mut io: Box<dyn Io> = if tls {
        let c = tokio_native_tls::TlsConnector::from(tls_connector()?);
        Box::new(c.connect(&host, tcp).await.map_err(|e| format!("{host}: {e}"))?)
    } else {
        Box::new(tcp)
    };
    let req = request_head(method, &authority(tls, &host, port), &path, headers, body.len())?;
    io.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
    io.write_all(body).await.map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    loop {
        if let Some((status, headers, len)) = head(&raw).map_err(|e| format!("{url}: {e}"))? {
            let rest = raw.split_off(len);
            // An interim answer (100 Continue, 103 Early Hints) comes before the real one.
            if (100..200).contains(&status) && status != 101 {
                raw = rest;
                continue;
            }
            return Ok(Open {
                status,
                headers,
                io,
                rest,
            });
        }
        if read_more(&mut io, &mut raw).await? == 0 {
            return Err(format!("{url}: connection closed"));
        }
    }
}

/// A response's status, its headers, and how many bytes its head took.
type Head = (u16, Vec<(String, String)>, usize);

/// The status, headers and length of the response head at the start of `raw`, once all of it came.
fn head(raw: &[u8]) -> Result<Option<Head>, String> {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if raw.len() > MAX_HEAD {
            Err("the answer's head is too long".into())
        } else {
            Ok(None)
        };
    };
    let head = String::from_utf8_lossy(&raw[..end]);
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.strip_prefix("HTTP/")?.split_whitespace().nth(1)?.parse().ok())
        .filter(|s| (100..600).contains(s))
        .ok_or("not an HTTP response")?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Ok(Some((status, headers, end + 4)))
}

/// Hands the body to `sink` piece by piece, up to `limit` bytes: its length, and whether all of it came.
async fn body(
    open: &mut Open,
    method: &str,
    limit: usize,
    sink: &mut dyn FnMut(&[u8]),
) -> Result<(usize, bool), String> {
    let get = |name: &str| {
        open.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    // These never have a body, whatever their headers say.
    if method == "HEAD" || matches!(open.status, 204 | 304) {
        return Ok((0, true));
    }
    // Chunked is always the last coding ("gzip, chunked").
    let chunked = get("transfer-encoding").is_some_and(|v| {
        v.rsplit(',')
            .next()
            .is_some_and(|c| c.trim().eq_ignore_ascii_case("chunked"))
    });
    let length = get("content-length").and_then(|v| v.parse::<usize>().ok());
    let io = &mut open.io;
    let mut buf = std::mem::take(&mut open.rest);
    let mut total = 0;
    if chunked {
        loop {
            let line_end = loop {
                if let Some(i) = buf.windows(2).position(|w| w == b"\r\n") {
                    break i;
                }
                if buf.len() > MAX_HEAD || read_more(io, &mut buf).await? == 0 {
                    return Ok((total, false));
                }
            };
            // A size that isn't one is a broken body, not its end.
            let Some(size) = chunk_size(&buf[..line_end]) else {
                return Ok((total, false));
            };
            buf.drain(..line_end + 2);
            if size == 0 {
                return Ok((total, true));
            }
            if size > limit - total {
                return Ok((total, false));
            }
            let mut left = size;
            while left > 0 {
                if buf.is_empty() && read_more(io, &mut buf).await? == 0 {
                    return Ok((total, false));
                }
                let take = buf.len().min(left);
                sink(&buf[..take]);
                buf.drain(..take);
                total += take;
                left -= take;
            }
            while buf.len() < 2 {
                if read_more(io, &mut buf).await? == 0 {
                    return Ok((total, false));
                }
            }
            buf.drain(..2);
        }
    }
    let want = length.unwrap_or(usize::MAX);
    loop {
        let take = buf.len().min(want - total).min(limit - total);
        if take > 0 {
            sink(&buf[..take]);
            total += take;
        }
        buf.clear();
        if total == want {
            return Ok((total, true));
        }
        if total >= limit {
            return Ok((total, false));
        }
        if read_more(io, &mut buf).await? == 0 {
            // Without a length, the end of the connection is the end of the body.
            return Ok((total, length.is_none()));
        }
    }
}

/// A chunk's size line ("1a" or "1a;name=value"), in hex.
fn chunk_size(line: &[u8]) -> Option<usize> {
    let size = std::str::from_utf8(line).ok()?.split(';').next()?.trim();
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    usize::from_str_radix(size, 16).ok()
}

/// One read, which may take up to [`TIMEOUT`].
async fn read_more(io: &mut Box<dyn Io>, buf: &mut Vec<u8>) -> Result<usize, String> {
    let mut chunk = [0u8; 16384];
    match tokio::time::timeout(TIMEOUT, io.read(&mut chunk)).await {
        Ok(Ok(n)) => {
            buf.extend_from_slice(&chunk[..n]);
            Ok(n)
        }
        // Servers that close without TLS close_notify: treat as the end.
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(0),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("timed out".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_wait_longer_up_to_a_cap() {
        let secs: Vec<u64> = (1..=7).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 15, 15, 15]);
        assert_eq!(backoff(0).as_secs(), 1);
    }

    #[test]
    fn urls() {
        assert_eq!(parse("https://a.com").unwrap(), (true, "a.com".into(), 443, "/".into()));
        assert_eq!(
            parse("http://h:8899/x?y=1").unwrap(),
            (false, "h".into(), 8899, "/x?y=1".into())
        );
        assert_eq!(parse("http://h?q").unwrap(), (false, "h".into(), 80, "/?q".into()));
        assert!(parse("ftp://h").is_err());
        assert!(parse("http://").is_err());
        assert!(parse("http://h:x").is_err());
        assert!(parse("http://h:65536").is_err());
    }

    #[test]
    fn ipv6_hosts_and_fragments() {
        assert_eq!(
            parse("http://[::1]:8080/a#top").unwrap(),
            (false, "::1".into(), 8080, "/a".into())
        );
        assert_eq!(
            parse("https://[2001:db8::1]").unwrap(),
            (true, "2001:db8::1".into(), 443, "/".into())
        );
        assert_eq!(parse("http://h#x").unwrap(), (false, "h".into(), 80, "/".into()));
        assert!(parse("http://[::1").is_err());
        assert!(parse("http://[::1]x").is_err());
        assert_eq!(authority(false, "::1", 8080), "[::1]:8080");
        assert_eq!(authority(true, "::1", 443), "[::1]");
        assert_eq!(authority(true, "a.com", 8443), "a.com:8443");
        assert_eq!(
            request_head("GET", "[::1]:8080", "/a", &[], 0).unwrap(),
            "GET /a HTTP/1.1\r\nHost: [::1]:8080\r\nConnection: close\r\nUser-Agent: Streamerz\r\n\r\n"
        );
    }

    #[test]
    fn redirects_resolve_against_the_current_address() {
        let to = |from: &str, location: &str| {
            redirect(from, 302, &[("location".into(), location.into())])
                .unwrap()
                .unwrap()
        };
        let from = "https://a.com/dir/page?x=1";
        assert_eq!(to(from, "/b"), "https://a.com/b");
        assert_eq!(to(from, "next?y=2"), "https://a.com/dir/next?y=2");
        assert_eq!(to(from, "?y=2"), "https://a.com/dir/page?y=2");
        assert_eq!(to(from, "//cdn.com/f"), "https://cdn.com/f");
        assert_eq!(to(from, "HTTP://b.com/c"), "HTTP://b.com/c");
        assert_eq!(to("http://[::1]:8080/a", "/b"), "http://[::1]:8080/b");
        assert_eq!(to("http://h:81", "x"), "http://h:81/x");
        assert_eq!(redirect(from, 200, &[("location".into(), "/b".into())]).unwrap(), None);
        assert_eq!(redirect(from, 302, &[]).unwrap(), None);
        assert_eq!(
            parse("HTTPS://A.com/x").unwrap(),
            (true, "A.com".into(), 443, "/x".into())
        );
    }

    #[test]
    fn redirects_keep_credentials_on_their_site_and_never_downgrade() {
        let sent: Vec<(String, String)> = [("Authorization", "Bearer t"), ("Cookie", "c"), ("Accept", "*/*")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into();
        let hop = |from: &str, location: &str| {
            let mut headers = Cow::Borrowed(&sent[..]);
            let next = follow(from, 307, &[("Location".into(), location.into())], &mut headers).unwrap();
            (
                next,
                headers.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(","),
            )
        };
        assert_eq!(
            hop("https://a.com/x", "/y"),
            (Some("https://a.com/y".into()), "Authorization,Cookie,Accept".into())
        );
        assert_eq!(
            hop("https://a.com/x", "https://A.COM/y"),
            (Some("https://A.COM/y".into()), "Authorization,Cookie,Accept".into())
        );
        assert_eq!(
            hop("https://a.com/x", "https://b.com/y"),
            (Some("https://b.com/y".into()), "Accept".into())
        );
        assert_eq!(hop("https://a.com/x", "https://a.com:8443/y").1, "Accept");
        assert_eq!(hop("https://a.com/x", "http://a.com/y").0, None);
    }

    #[test]
    fn resumed_parts_must_start_where_the_file_stops() {
        let start = |v: &str| range_start(&[("Content-Range".into(), v.into())]);
        assert_eq!(start("bytes 100-199/200"), Some(100));
        assert_eq!(start("Bytes 0-9/*"), Some(0));
        assert_eq!(start("bytes */200"), None);
        assert_eq!(start("items 1-2/3"), None);
        assert_eq!(start("bytes -5/9"), None);
        assert_eq!(range_start(&[]), None);
    }

    #[test]
    fn response_heads() {
        let (status, headers, len) = head(b"HTTP/1.1 200 OK\r\nA: 1\r\nb:  two \r\nbad line\r\n\r\nbody")
            .unwrap()
            .unwrap();
        assert_eq!((status, len), (200, 45));
        assert_eq!(headers, [("A".into(), "1".into()), ("b".into(), "two".into())]);
        assert_eq!(head(b"HTTP/1.0 404\r\n\r\n").unwrap().unwrap().0, 404);
        assert_eq!(head(b"HTTP/1.1 200 OK\r\nA: 1\r\n").unwrap(), None);
        for bad in [
            &b"SSH-2.0 200 x\r\n\r\n"[..],
            b"HTTP/1.1 abc\r\n\r\n",
            b"HTTP/1.1 99\r\n\r\n",
            b"\r\n\r\n",
        ] {
            assert!(head(bad).is_err(), "{bad:?}");
        }
        // A head that never ends can't grow without bound.
        assert!(head(&vec![b'a'; MAX_HEAD + 1]).is_err());
        assert_eq!(head(&vec![b'a'; MAX_HEAD]).unwrap(), None);
    }

    #[test]
    fn chunk_sizes() {
        assert_eq!(chunk_size(b"1a"), Some(26));
        assert_eq!(chunk_size(b"1A ; name=value"), Some(26));
        assert_eq!(chunk_size(b"0"), Some(0));
        for bad in [&b""[..], b"zz", b"+5", b"-1", b"ffffffffffffffffff", b"\xff"] {
            assert_eq!(chunk_size(bad), None, "{bad:?}");
        }
    }

    /// The body each raw answer gives a GET (or `method`) with a `limit`, and whether all of it came.
    fn decode(wire: &[u8], method: &str, limit: usize) -> Result<(Vec<u8>, bool), String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (status, headers, len) = head(wire)?.ok_or("no head")?;
            let mut open = Open {
                status,
                headers,
                io: Box::new(tokio::io::join(tokio::io::empty(), tokio::io::sink())),
                rest: wire[len..].to_vec(),
            };
            let mut out = Vec::new();
            let (n, complete) = body(&mut open, method, limit, &mut |p: &[u8]| out.extend_from_slice(p)).await?;
            assert_eq!(n, out.len());
            Ok((out, complete))
        })
    }

    #[test]
    fn bodies_by_length_chunks_or_end() {
        let ok = |wire: &[u8]| decode(wire, "GET", 100).unwrap();
        assert_eq!(
            ok(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcdef"),
            (b"abc".to_vec(), true)
        );
        assert_eq!(
            ok(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc"),
            (b"abc".to_vec(), false)
        );
        assert_eq!(ok(b"HTTP/1.0 200 OK\r\n\r\nto the end"), (b"to the end".to_vec(), true));
        let chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, Chunked\r\n\r\n3;x=y\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        assert_eq!(ok(chunked), (b"abcde".to_vec(), true));
        // Cut short, over the limit, or a size that isn't one: never reported as all of it.
        let te = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        for (rest, got) in [
            ("3\r\nabc\r\n", "abc"),
            ("3\r\nab", "ab"),
            ("3\r\nabc\r\nzz\r\nmore\r\n0\r\n\r\n", "abc"),
            ("ffffffffffffffff\r\nabc", ""),
            ("3\r\nabc\r\n65\r\n", "abc"),
        ] {
            assert_eq!(
                ok(format!("{te}{rest}").as_bytes()),
                (got.as_bytes().to_vec(), false),
                "{rest:?}"
            );
        }
        let long_size = format!("{te}{}", "1".repeat(MAX_HEAD + 1));
        assert_eq!(ok(long_size.as_bytes()), (Vec::new(), false));
        assert_eq!(
            decode(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nabcde", "GET", 2).unwrap(),
            (b"ab".to_vec(), false)
        );
    }

    #[test]
    fn some_answers_have_no_body_whatever_they_say() {
        let empty = (Vec::new(), true);
        assert_eq!(
            decode(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n", "HEAD", 100).unwrap(),
            empty
        );
        assert_eq!(
            decode(b"HTTP/1.1 204 No Content\r\nContent-Length: 5\r\n\r\n", "GET", 100).unwrap(),
            empty
        );
        assert_eq!(
            decode(
                b"HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n",
                "GET",
                100
            )
            .unwrap(),
            empty
        );
    }

    /// An interim 100 or 103 is skipped; a cut or oversized answer is an error, not half a body.
    #[test]
    fn requests_skip_interim_answers_and_refuse_partial_bodies() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = l.local_addr().unwrap().port();
            tokio::spawn(async move {
                let replies: [&[u8]; 2] = [
                    b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Early Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok",
                    b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort",
                ];
                for r in replies {
                    let (mut s, _) = l.accept().await.unwrap();
                    let _ = s.read(&mut [0u8; 1024]).await;
                    s.write_all(r).await.unwrap();
                }
            });
            let url = format!("http://127.0.0.1:{port}/");
            let r = request("POST", &url, &[], b"x").await.unwrap();
            assert_eq!((r.status, r.text().as_str()), (200, "ok"));
            assert!(r.headers.iter().all(|(k, _)| k != "Link"));
            let cut = request("GET", &url, &[], b"").await;
            assert!(matches!(cut, Err(e) if e.contains("cut short")));
        });
    }

    /// Text from chat often lands in a Fetch URL address: it must reach the server as one request.
    #[test]
    fn request_targets_are_encoded_and_lines_cannot_be_injected() {
        let path = |url: &str| parse(url).map(|p| p.3);
        assert_eq!(
            path("http://h/search?q=hello world").unwrap(),
            "/search?q=hello%20world"
        );
        assert_eq!(
            path("http://h/a b?q=café|\"x\"").unwrap(),
            "/a%20b?q=caf%C3%A9%7C%22x%22"
        );
        assert_eq!(path("http://h/?q=a%2Bb&r=[1]").unwrap(), "/?q=a%2Bb&r=[1]");
        assert!(path("http://h/x\r\nHost: evil").is_err());
        assert!(path("http://h/x\ty").is_err());
        let ok = |k: &str, v: &str| request_head("GET", "h", "/", &[(k.into(), v.into())], 0);
        assert!(ok("X-Test", "a b").unwrap().contains("X-Test: a b\r\n"));
        assert!(ok("X-Test", "a\r\nX-Evil: 1").is_err());
        assert!(ok("X-Test\n", "a").is_err());
        assert!(ok("X:Test", "a").is_err());
        assert!(request_head("GET /x HTTP/1.1\r\n", "h", "/", &[], 0).is_err());
        let post = request_head("POST", "h", "/", &[("User-Agent".into(), "Mine".into())], 0).unwrap();
        assert!(post.ends_with("User-Agent: Mine\r\nContent-Length: 0\r\n\r\n"));
        assert!(!post.contains("Streamerz"));
    }

    /// Against a one-shot local server: chunked body, then a redirect to a plain one.
    #[test]
    fn local_server() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = l.local_addr().unwrap().port();
            tokio::spawn(async move {
                let replies: [&[u8]; 2] = [
                    b"HTTP/1.1 302 Found\r\nLocation: /b\r\n\r\n",
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
                ];
                for r in replies {
                    let (mut s, _) = l.accept().await.unwrap();
                    let mut buf = [0u8; 1024];
                    let _ = s.read(&mut buf).await;
                    s.write_all(r).await.unwrap();
                }
            });
            let r = request("GET", &format!("http://127.0.0.1:{port}/a"), &[], b"")
                .await
                .unwrap();
            assert_eq!((r.status, r.text().as_str()), (200, "hello world"));
        });
    }

    /// Bigger than any in-memory body (the 1.7.0 update failed at 16 MB), with and without a length,
    /// and a body cut short.
    #[test]
    fn downloads_stream_to_a_file() {
        use sha2::Digest;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let data: Vec<u8> = (0..(20usize << 20)).map(|i| (i % 251) as u8).collect();
        let want: String = sha2::Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("streamerz-download-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        rt.block_on(async {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = l.local_addr().unwrap().port();
            let served = data.clone();
            tokio::spawn(async move {
                for kind in 0..3 {
                    let (mut s, _) = l.accept().await.unwrap();
                    let mut buf = [0u8; 1024];
                    let _ = s.read(&mut buf).await;
                    match kind {
                        0 => {
                            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", served.len());
                            s.write_all(head.as_bytes()).await.unwrap();
                            s.write_all(&served).await.unwrap();
                        }
                        1 => {
                            s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                                .await
                                .unwrap();
                            for piece in served.chunks(1 << 20) {
                                s.write_all(format!("{:x}\r\n", piece.len()).as_bytes()).await.unwrap();
                                s.write_all(piece).await.unwrap();
                                s.write_all(b"\r\n").await.unwrap();
                            }
                            s.write_all(b"0\r\n\r\n").await.unwrap();
                        }
                        _ => {
                            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", served.len());
                            s.write_all(head.as_bytes()).await.unwrap();
                            s.write_all(&served[..1000]).await.unwrap();
                        }
                    }
                }
            });
            let url = format!("http://127.0.0.1:{port}/f");
            for kind in 0..2 {
                let to = dir.join(format!("f{kind}"));
                let got = download(&url, &[], &to, 64 << 20).await.unwrap();
                assert!(got.complete, "kind {kind}");
                assert_eq!((got.len, got.sha256.as_str()), (data.len() as u64, want.as_str()));
                assert_eq!(std::fs::metadata(&to).unwrap().len(), data.len() as u64);
                assert_eq!(got.magic, data[..4]);
            }
            let got = download(&url, &[], &dir.join("cut"), 64 << 20).await.unwrap();
            assert!(!got.complete);
            assert_eq!(got.len, 1000);
        });
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A download cut mid-way or cancelled keeps its `.part` file and resumes via `Range`.
    #[test]
    fn resumable_download_picks_up_and_verifies_sha256() {
        use sha2::Digest;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let data: Vec<u8> = (0..128_000usize).map(|i| (i % 241) as u8).collect();
        let want: String = sha2::Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("streamerz-resume-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        rt.block_on(async {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = l.local_addr().unwrap().port();
            let served = data.clone();
            tokio::spawn(async move {
                // Request 1: cut short after 40 KB; Request 2: serve the rest as 206 Partial Content.
                for req_idx in 0..2 {
                    let (mut s, _) = l.accept().await.unwrap();
                    let mut buf = [0u8; 2048];
                    let n = s.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let offset = req
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=")?.split('-').next()?.parse::<usize>().ok())
                        .unwrap_or(0);
                    if req_idx == 0 {
                        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", served.len());
                        s.write_all(head.as_bytes()).await.unwrap();
                        s.write_all(&served[..40_000]).await.unwrap();
                    } else {
                        assert_eq!(offset, 40_000, "second request must resume at 40000");
                        let rest = &served[offset..];
                        let head = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\n\r\n",
                            offset,
                            served.len() - 1,
                            served.len(),
                            rest.len()
                        );
                        s.write_all(head.as_bytes()).await.unwrap();
                        s.write_all(rest).await.unwrap();
                    }
                }
            });
            let url = format!("http://127.0.0.1:{port}/model.tar.bz2");
            let to = dir.join("model.part");
            let first = download_resumable(&url, &[], &to, 1 << 20, |_, _| true).await.unwrap();
            assert!(!first.complete);
            assert_eq!(first.len, 40_000);
            let mut saw_progress = false;
            let second = download_resumable(&url, &[], &to, 1 << 20, |done, total| {
                if done > 40_000 && total == Some(data.len() as u64) {
                    saw_progress = true;
                }
                true
            })
            .await
            .unwrap();
            assert!(saw_progress);
            assert!(second.complete);
            assert_eq!(second.len, data.len() as u64);
            assert_eq!(second.sha256, want);
        });
        std::fs::remove_dir_all(dir).unwrap();
    }
}
