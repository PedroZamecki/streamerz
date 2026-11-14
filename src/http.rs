//! A small HTTP/1.1 client over tokio, with TLS from the OS (SChannel on Windows, OpenSSL on Linux).
//! ponytail: one connection per request (Connection: close); keep-alive per host when the platforms
//! poll their APIs.

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

/// scheme, host, port, path and query.
fn parse(url: &str) -> Result<(bool, String, u16, String), String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("not an http(s) URL: {url}"));
    };
    let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    let path = if path.starts_with('?') {
        format!("/{path}")
    } else if path.is_empty() {
        "/".into()
    } else {
        path.into()
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || p.is_empty() => {
            (h.to_string(), p.parse().map_err(|_| format!("bad port in {url}"))?)
        }
        _ => (authority.to_string(), if tls { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(format!("no host in {url}"));
    }
    Ok((tls, host, port, path))
}

/// Send a request, following up to 5 redirects. Headers are (name, value); a body gets its
/// Content-Length.
pub async fn request(method: &str, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<Response, String> {
    let mut url = url.to_string();
    let mut method = method.to_string();
    for _ in 0..6 {
        let r = tokio::time::timeout(TIMEOUT, once(&method, &url, headers, body))
            .await
            .map_err(|_| format!("{url}: timed out"))??;
        match redirect(&url, r.status, &r.headers)? {
            Some(to) => {
                url = to;
                if r.status == 303 {
                    method = "GET".into();
                }
            }
            None => return Ok(r),
        }
    }
    Err(format!("{url}: too many redirects"))
}

/// Where a redirect points, if `status` is one.
fn redirect(url: &str, status: u16, headers: &[(String, String)]) -> Result<Option<String>, String> {
    let location = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location"));
    match (status, location) {
        (301 | 302 | 303 | 307 | 308, Some((_, to))) => Ok(Some(if to.starts_with("http") {
            to.clone()
        } else {
            let (tls, host, port, _) = parse(url)?;
            format!("{}://{host}:{port}{to}", if tls { "https" } else { "http" })
        })),
        _ => Ok(None),
    }
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
    let mut url = url.to_string();
    for _ in 0..6 {
        let mut open = tokio::time::timeout(TIMEOUT, open("GET", &url, headers, b""))
            .await
            .map_err(|_| format!("{url}: timed out"))??;
        if let Some(next) = redirect(&url, open.status, &open.headers)? {
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
    body(&mut open, method, MAX_BODY, &mut |piece: &[u8]| {
        out.extend_from_slice(piece)
    })
    .await?;
    Ok(Response {
        status: open.status,
        headers: open.headers,
        body: out,
    })
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
    let default_port = if tls { 443 } else { 80 };
    let host_header = if port == default_port {
        host.clone()
    } else {
        format!("{host}:{port}")
    };
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\n");
    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    if !has("user-agent") {
        req += "User-Agent: Streamerz\r\n";
    }
    for (k, v) in headers {
        req += &format!("{k}: {v}\r\n");
    }
    if !body.is_empty() || matches!(method, "POST" | "PUT" | "PATCH") {
        req += &format!("Content-Length: {}\r\n", body.len());
    }
    req += "\r\n";
    io.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
    io.write_all(body).await.map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    // Read the head.
    let head_end = loop {
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if read_more(&mut io, &mut raw).await? == 0 {
            return Err(format!("{url}: connection closed"));
        }
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{url}: not an HTTP response"))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let rest = raw.split_off(head_end + 4);
    Ok(Open {
        status,
        headers,
        io,
        rest,
    })
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
    let chunked = get("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
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
                if read_more(io, &mut buf).await? == 0 {
                    return Ok((total, false));
                }
            };
            let size = String::from_utf8_lossy(&buf[..line_end]);
            let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
            buf.drain(..line_end + 2);
            if size == 0 {
                return Ok((total, true));
            }
            if total + size > limit {
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
    let want = match length {
        Some(len) => len,
        None if open.status == 204 || open.status == 304 || method == "HEAD" => return Ok((0, true)),
        None => usize::MAX,
    };
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
}
