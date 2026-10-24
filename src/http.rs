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
        let location = r.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location"));
        match (r.status, location) {
            (301 | 302 | 303 | 307 | 308, Some((_, to))) => {
                url = if to.starts_with("http") {
                    to.clone()
                } else {
                    let (tls, host, port, _) = parse(&url)?;
                    format!("{}://{host}:{port}{to}", if tls { "https" } else { "http" })
                };
                if r.status == 303 {
                    method = "GET".into();
                }
            }
            _ => return Ok(r),
        }
    }
    Err(format!("{url}: too many redirects"))
}

/// One TLS setup for every connection: building one loads and parses the system's CA bundle
/// (OpenSSL), a few MB and a burst of CPU each time.
pub fn tls_connector() -> Result<native_tls::TlsConnector, String> {
    static C: std::sync::OnceLock<Result<native_tls::TlsConnector, String>> = std::sync::OnceLock::new();
    C.get_or_init(|| native_tls::TlsConnector::new().map_err(|e| e.to_string()))
        .clone()
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

/// `tokio_tungstenite::connect_async` with the shared TLS setup.
#[allow(clippy::result_large_err)] // the same error type connect_async returns
pub async fn ws_connect(
    url: &str,
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

async fn once(method: &str, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<Response, String> {
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
        req += "User-Agent: streamerz\r\n";
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
    let get = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    let mut rest = raw.split_off(head_end + 4);
    let chunked = get("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
    let body = if chunked {
        dechunk(&mut io, rest).await?
    } else if let Some(len) = get("content-length").and_then(|v| v.parse::<usize>().ok()) {
        while rest.len() < len.min(MAX_BODY) {
            if read_more(&mut io, &mut rest).await? == 0 {
                break;
            }
        }
        rest.truncate(len);
        rest
    } else if status == 204 || status == 304 || method == "HEAD" {
        Vec::new()
    } else {
        while rest.len() < MAX_BODY && read_more(&mut io, &mut rest).await? > 0 {}
        rest
    };
    Ok(Response { status, headers, body })
}

async fn read_more(io: &mut Box<dyn Io>, buf: &mut Vec<u8>) -> Result<usize, String> {
    let mut chunk = [0u8; 8192];
    match io.read(&mut chunk).await {
        Ok(n) => {
            buf.extend_from_slice(&chunk[..n]);
            Ok(n)
        }
        // Servers that close without TLS close_notify: treat as the end.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(0),
        Err(e) => Err(e.to_string()),
    }
}

async fn dechunk(io: &mut Box<dyn Io>, mut buf: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = loop {
            if let Some(i) = buf.windows(2).position(|w| w == b"\r\n") {
                break i;
            }
            if read_more(io, &mut buf).await? == 0 {
                return Ok(out);
            }
        };
        let size = String::from_utf8_lossy(&buf[..line_end]);
        let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        buf.drain(..line_end + 2);
        if size == 0 || out.len() + size > MAX_BODY {
            return Ok(out);
        }
        while buf.len() < size + 2 {
            if read_more(io, &mut buf).await? == 0 {
                out.extend_from_slice(&buf[..buf.len().min(size)]);
                return Ok(out);
            }
        }
        out.extend_from_slice(&buf[..size]);
        buf.drain(..size + 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
