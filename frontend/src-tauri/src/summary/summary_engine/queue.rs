//! One request at a time for the built-in model (plan Phase D, D2).
//!
//! The llama-helper sidecar reads one request and writes one response, in order. Two callers at
//! once (a summary and an Ask question) could otherwise read each other's response, and one
//! caller's timeout or cancel would shut the sidecar down under the other. The queue runs each
//! request's whole exchange alone. A request still waiting can be cancelled without touching the
//! one that is running, and a request's timeout only starts when its turn comes.

use std::future::Future;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug, PartialEq, Eq)]
pub enum QueueError {
    /// Cancelled while waiting for its turn; the running request was not affected.
    CancelledWhileWaiting,
}

#[derive(Default)]
pub struct RequestQueue {
    turn: Mutex<()>,
}

impl RequestQueue {
    pub const fn new() -> Self {
        Self { turn: Mutex::const_new(()) }
    }

    /// Wait for this request's turn (or its cancellation), then run `work` alone.
    pub async fn run<F, Fut, T>(&self, cancel: Option<&CancellationToken>, work: F) -> Result<T, QueueError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _turn = match cancel {
            Some(token) => tokio::select! {
                // Prefer cancellation when both are ready, so a cancelled request never starts
                biased;
                _ = token.cancelled() => return Err(QueueError::CancelledWhileWaiting),
                turn = self.turn.lock() => turn,
            },
            None => self.turn.lock().await,
        };
        Ok(work().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn requests_run_one_at_a_time_in_order() {
        let queue = Arc::new(RequestQueue::new());
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));

        let (q1, l1) = (queue.clone(), log.clone());
        let first = tokio::spawn(async move {
            q1.run(None, || async {
                l1.lock().unwrap().push("first starts");
                tokio::time::sleep(Duration::from_millis(60)).await;
                l1.lock().unwrap().push("first ends");
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let (q2, l2) = (queue.clone(), log.clone());
        let second = tokio::spawn(async move {
            q2.run(None, || async {
                l2.lock().unwrap().push("second starts");
            })
            .await
        });

        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(*log.lock().unwrap(), ["first starts", "first ends", "second starts"]);
    }

    #[tokio::test]
    async fn a_waiting_request_can_be_cancelled_without_running() {
        let queue = Arc::new(RequestQueue::new());
        let q1 = queue.clone();
        let running = tokio::spawn(async move {
            q1.run(None, || tokio::time::sleep(Duration::from_millis(80))).await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;

        let cancel = CancellationToken::new();
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (q2, c2, r2) = (queue.clone(), cancel.clone(), ran.clone());
        let waiting = tokio::spawn(async move {
            q2.run(Some(&c2), || async move { r2.store(true, std::sync::atomic::Ordering::SeqCst) }).await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();

        assert_eq!(waiting.await.unwrap(), Err(QueueError::CancelledWhileWaiting));
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(running.await.unwrap().is_ok(), "the running request finished normally");
    }

    #[tokio::test]
    async fn an_already_cancelled_request_never_runs() {
        let queue = RequestQueue::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let _held = queue.turn.lock().await;
        let result = tokio::time::timeout(Duration::from_millis(200), queue.run(Some(&cancel), || async { 1 })).await;
        assert_eq!(result.expect("returns promptly"), Err(QueueError::CancelledWhileWaiting));
    }
}
