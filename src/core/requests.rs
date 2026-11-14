//! In-flight core requests share the existing runtime and may await host input independently.
use crate::engine::Engine;
use futures_util::{future::LocalBoxFuture, stream::FuturesUnordered, StreamExt};
use serde_json::{json, Value};
use std::rc::Rc;

pub type Call = (Value, tokio::sync::oneshot::Sender<Value>);
#[derive(Default)]
pub struct Requests(FuturesUnordered<LocalBoxFuture<'static, Call>>);
impl Requests {
    pub fn push(&mut self, engine: Rc<Engine>, (request, reply): Call) {
        if self.0.len() >= 64 {
            let _ = reply.send(json!({"id": request["id"], "status": "error", "error": "Too many pending requests"}));
            return;
        }
        self.0.push(Box::pin(async move {
            (crate::servers::request(&engine, &request, false, true).await, reply)
        }));
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub async fn next(&mut self) -> Option<Call> {
        self.0.next().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(pending: &mut Requests, engine: Rc<Engine>, value: Value) {
        // The presentation session has gone away. Its receiver does not own the operation.
        let (sender, receiver) = tokio::sync::oneshot::channel();
        drop(receiver);
        pending.push(engine, (value, sender));
    }

    #[test]
    fn a_waiting_action_does_not_block_another_request_or_end_with_its_frontend() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let mut pending = Requests::default();
            push(
                &mut pending,
                engine.clone(),
                json!({"request":"StreamerzRunSubaction", "item":{"type":1002,"value":50}, "id":"wait"}),
            );
            push(&mut pending, engine, json!({"request":"GetInfo", "id":"read"}));
            assert_eq!(pending.next().await.unwrap().0["id"], "read");
            assert_eq!(pending.next().await.unwrap().0["id"], "wait");
            assert!(pending.is_empty());
        });
    }

    #[test]
    fn input_can_be_read_and_answered_through_mcp_requests_without_a_frontend() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let mut pending = Requests::default();
            push(&mut pending, engine.clone(), json!({"request":"StreamerzRunSubaction", "item":{"type":1023,"title":"Question","prompt":"Name","outputVariable":"answer"}, "id":"input"}));
            push(&mut pending, engine.clone(), json!({"request":"GetInfo", "id":"read"}));
            assert_eq!(pending.next().await.unwrap().0["id"], "read");
            let read = crate::servers::answer(&engine, &json!({"request":"StreamerzPrompts"}), true).unwrap();
            let prompt = read["prompts"][0]["id"].clone();
            assert!(prompt.is_number());
            engine.change(|config| config.websockets.enable_auth = true);
            let answer = json!({"request":"StreamerzAnswerPrompt", "promptId":prompt, "value":"Streamer"});
            assert_eq!(crate::servers::answer(&engine, &answer, false).unwrap()["status"], "error");
            assert_eq!(crate::servers::answer(&engine, &answer, true).unwrap()["status"], "ok");
            let result = tokio::time::timeout(std::time::Duration::from_secs(1), pending.next()).await.unwrap().unwrap().0;
            assert_eq!(result["args"]["answer"], "Streamer");
            assert_eq!(result["args"]["success"], true);
            assert_eq!(result["executionStatus"], "completed");
            assert_eq!(crate::servers::answer(&engine, &answer, true).unwrap()["status"], "error");
        });
    }
}
