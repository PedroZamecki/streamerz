use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

pub trait ConnectionCommand {
    const CONNECT: Self;
    const DISCONNECT: Self;
}

pub struct Handle<C, S = Arc<AtomicBool>> {
    pub(crate) tx: mpsc::UnboundedSender<C>,
    pub(crate) state: S,
}

impl<C, S: Clone> Clone for Handle<C, S> {
    fn clone(&self) -> Self {
        Self::new(self.tx.clone(), self.state.clone())
    }
}

impl<C, S> Handle<C, S> {
    pub(crate) fn new(tx: mpsc::UnboundedSender<C>, state: S) -> Self {
        Self { tx, state }
    }

    pub fn send(&self, command: C) {
        let _ = self.tx.send(command);
    }
}

impl<C, S: AsRef<AtomicBool>> Handle<C, S> {
    pub fn is_connected(&self) -> bool {
        self.state.as_ref().load(Ordering::Relaxed)
    }
}

impl<C: ConnectionCommand, S: AsRef<AtomicBool> + Default> Handle<C, S> {
    /// A connected handle with no runner: whoever holds the receiver answers it (scenarios).
    pub fn detached() -> (Self, mpsc::UnboundedReceiver<C>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let state = S::default();
        state.as_ref().store(true, Ordering::Relaxed);
        (Self::new(tx, state), rx)
    }

    pub fn connect(&self) {
        self.send(C::CONNECT);
    }

    pub fn disconnect(&self) {
        self.send(C::DISCONNECT);
    }
}
