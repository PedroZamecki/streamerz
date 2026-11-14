//! Private parent/child protocol. The child inherits anonymous pipes, never a listening socket.

use serde_json::Value;
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Bound a malformed child message before JSON decoding can allocate without a limit.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// Keep partial bytes outside the read future so event delivery can safely cancel a pending read.
pub struct Reader<R> {
    input: R,
    pending: Vec<u8>,
}

impl<R: AsyncBufRead + Unpin> Reader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            pending: Vec::new(),
        }
    }

    pub async fn read(&mut self) -> io::Result<Option<Value>> {
        let remaining = MAX_FRAME + 1 - self.pending.len();
        let count = (&mut self.input)
            .take(remaining as u64)
            .read_until(b'\n', &mut self.pending)
            .await?;
        if count == 0 && self.pending.is_empty() {
            return Ok(None);
        }
        if self.pending.len() > MAX_FRAME || self.pending.last() != Some(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid frontend frame length",
            ));
        }
        let value: Value = serde_json::from_slice(&self.pending)?;
        self.pending.clear();
        if !value.is_object() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frontend frame must be an object",
            ));
        }
        Ok(Some(value))
    }
}

pub async fn write(writer: &mut (impl AsyncWrite + Unpin), value: &Value) -> io::Result<()> {
    if !value.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frontend frame must be an object",
        ));
    }
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frontend frame is too large",
        ));
    }
    writer.write_all(&bytes).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[test]
    fn frames_keep_ids_and_embedded_newlines_and_distinguish_eof() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let expected = json!({"id": "request-1", "text": "first\nsecond", "value": {"enabled": false}});
            let mut bytes = Vec::new();
            write(&mut bytes, &expected).await.unwrap();
            write(&mut bytes, &json!({"id": "request-2"})).await.unwrap();
            let mut reader = Reader::new(BufReader::new(bytes.as_slice()));
            assert_eq!(reader.read().await.unwrap(), Some(expected));
            assert_eq!(reader.read().await.unwrap(), Some(json!({"id": "request-2"})));
            assert_eq!(reader.read().await.unwrap(), None);
        });
    }

    #[test]
    fn broken_or_non_object_frames_are_rejected() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            for bytes in [b"{\"id\":1}".as_slice(), b"[]\n", b"not json\n"] {
                assert!(Reader::new(BufReader::new(bytes)).read().await.is_err());
            }
            assert!(write(&mut Vec::new(), &json!([])).await.is_err());
        });
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use serde_json::json;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    use tokio::io::BufReader;

    #[test]
    fn invalidation_during_a_fragmented_request_keeps_the_partial_frame() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut writer, input) = tokio::io::duplex(128);
            let mut reader = Reader::new(BufReader::new(input));
            writer.write_all(b"{\"id\":").await.unwrap();
            {
                let mut pending = std::pin::pin!(reader.read());
                let mut context = Context::from_waker(Waker::noop());
                assert!(matches!(pending.as_mut().poll(&mut context), Poll::Pending));
            }
            writer.write_all(b"42}\n").await.unwrap();
            assert_eq!(reader.read().await.unwrap(), Some(json!({"id": 42})));
        });
    }
}
