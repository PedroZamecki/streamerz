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

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// `{"t":"xxx..."}` and its newline, `len` bytes in all.
    fn frame_of(len: usize) -> Vec<u8> {
        let mut bytes = b"{\"t\":\"".to_vec();
        bytes.resize(len - 3, b'x');
        bytes.extend_from_slice(b"\"}\n");
        assert_eq!(bytes.len(), len);
        bytes
    }

    #[test]
    fn an_endless_line_is_cut_at_the_limit_instead_of_growing() {
        block_on(async {
            // A renderer that never sends a newline: the read ends, it doesn't wait or grow forever.
            let mut reader = Reader::new(BufReader::new(tokio::io::repeat(b' ')));
            assert_eq!(reader.read().await.unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert!(reader.pending.len() <= MAX_FRAME + 1);
        });
    }

    #[test]
    fn a_frame_may_be_as_long_as_the_limit_and_no_longer() {
        block_on(async {
            let fits = frame_of(MAX_FRAME);
            let mut reader = Reader::new(BufReader::new(fits.as_slice()));
            let value = reader.read().await.unwrap().unwrap();
            assert_eq!(value["t"].as_str().map(str::len), Some(MAX_FRAME - 9));
            let over = frame_of(MAX_FRAME + 1);
            assert!(Reader::new(BufReader::new(over.as_slice())).read().await.is_err());
            // Nor is one written that the other side would refuse.
            let text = "x".repeat(MAX_FRAME);
            let error = write(&mut Vec::new(), &json!({"t": text})).await.unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        });
    }

    #[test]
    fn frames_sent_together_come_one_by_one_and_a_cut_one_is_an_error() {
        block_on(async {
            let bytes = b"{\"n\":1}\n{\"n\":2}\r\n{\"n\":3}\n{\"n\":";
            let mut reader = Reader::new(BufReader::new(bytes.as_slice()));
            for n in 1..=3 {
                assert_eq!(reader.read().await.unwrap(), Some(json!({"n": n})));
            }
            // The renderer died in the middle of a frame.
            assert!(reader.read().await.is_err());
        });
    }

    #[test]
    fn an_empty_line_is_not_a_frame() {
        block_on(async {
            assert!(Reader::new(BufReader::new(b"\n".as_slice())).read().await.is_err());
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
