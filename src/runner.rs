use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub(crate) struct ReplyErrors {
    pub down: &'static str,
    pub cancelled: &'static str,
    pub timeout: Option<(Duration, &'static str)>,
}

impl ReplyErrors {
    pub const fn new(down: &'static str) -> Self {
        Self {
            down,
            cancelled: "Request cancelled",
            timeout: None,
        }
    }
}

pub(crate) async fn ask<C, T>(
    tx: &mpsc::UnboundedSender<C>,
    errors: ReplyErrors,
    make: impl FnOnce(oneshot::Sender<Result<T, String>>) -> C,
) -> Result<T, String> {
    let (reply, rx) = oneshot::channel();
    tx.send(make(reply)).map_err(|_| errors.down.to_string())?;
    let result = match errors.timeout {
        Some((duration, error)) => tokio::time::timeout(duration, rx)
            .await
            .map_err(|_| error.to_string())?,
        None => rx.await,
    };
    result.map_err(|_| errors.cancelled.to_string())?
}
