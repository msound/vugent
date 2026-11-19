//! `GET /api/events`: a Server-Sent Events stream of what changed (ADR 0008). The watcher only runs while at least
//! one browser is connected.

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::State;
use axum::response::sse::{self, KeepAlive, Sse};
use futures_util::{Stream, StreamExt, stream};
use serde_json::json;
use tokio::sync::{broadcast, watch};
use vugent_git::Review;
use vugent_watch::Event;

/// How many events a slow browser may fall behind before it is told that it missed some.
const BACKLOG: usize = 64;

/// The events of one reviewed folder, shared by every browser connected to `/api/events`.
#[derive(Clone)]
pub struct Events(Arc<Hub>);

struct Hub {
    review: Review,
    sender: broadcast::Sender<Event>,
    /// Changes are polled rather than watched: asked for with `--poll`, or since watching failed.
    polling: Arc<AtomicBool>,
    /// Set once the server shuts down, which ends every stream.
    closed: watch::Sender<bool>,
    state: Mutex<HubState>,
}

#[derive(Default)]
struct HubState {
    subscribers: usize,
    watch: Option<vugent_watch::Watch>,
}

impl Events {
    /// Events for the folder of `review`, polled rather than watched with `poll`.
    pub fn new(review: Review, poll: bool) -> Self {
        Self(Arc::new(Hub {
            review,
            sender: broadcast::channel(BACKLOG).0,
            polling: Arc::new(AtomicBool::new(poll)),
            closed: watch::channel(false).0,
            state: Mutex::default(),
        }))
    }

    /// Ends every stream, so that the server can shut down; browsers would otherwise keep their connections open.
    pub fn close(&self) {
        self.0.closed.send_replace(true);
    }

    /// Whether changes are polled rather than watched.
    fn polling(&self) -> bool {
        self.0.polling.load(Ordering::Relaxed)
    }

    /// Starts receiving events, starting the watcher for the first subscriber.
    fn subscribe(&self) -> Subscription {
        let hub = &self.0;
        let receiver = hub.sender.subscribe();
        let mut state = hub.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.subscribers += 1;
        if state.watch.is_none() {
            let sender = hub.sender.clone();
            let polling = hub.polling.clone();
            state.watch = Some(vugent_watch::start(
                hub.review.clone(),
                self.polling(),
                move |event| {
                    if event == Event::Polling {
                        polling.store(true, Ordering::Relaxed);
                    }
                    // Nobody may be listening between the last browser leaving and the watcher stopping.
                    let _ = sender.send(event);
                },
            ));
        }
        Subscription {
            receiver,
            events: self.clone(),
        }
    }

    #[cfg(test)]
    fn watching(&self) -> bool {
        let state = self.0.state.lock().unwrap();
        state.watch.is_some()
    }
}

/// One browser's stream of events. Dropping the last one stops the watcher.
struct Subscription {
    receiver: broadcast::Receiver<Event>,
    events: Events,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut state = self
            .events
            .0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.subscribers -= 1;
        if state.subscribers == 0 {
            state.watch = None;
        }
    }
}

/// `GET /api/events`: first a `mode` event, then `files`, `index`, `refs` and `mode` events as they happen.
pub(crate) async fn events(
    State(events): State<Events>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    // Subscribe before reading the mode, so that a change in between is not missed.
    let subscription = events.subscribe();
    let mode = message(&Event::Polling, events.polling());
    let mut closed = events.0.closed.subscribe();
    let changes = stream::unfold(subscription, |mut subscription| async move {
        let messages = match subscription.receiver.recv().await {
            Ok(event) => vec![message(&event, true)],
            // Some events were dropped: anything may have changed.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                [Event::Files { paths: None }, Event::Index, Event::Refs]
                    .iter()
                    .map(|event| message(event, true))
                    .collect()
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        };
        Some((stream::iter(messages), subscription))
    })
    .flatten();
    let stream = stream::once(async { mode })
        .chain(changes)
        .map(Ok)
        .take_until(async move {
            // The sender lives as long as the server, so this only fails once nothing is served any more.
            let _ = closed.wait_for(|closed| *closed).await;
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// The message that tells the browser about `event`. [`Event::Polling`] is the `mode` message, saying whether
/// changes are polled.
fn message(event: &Event, polling: bool) -> sse::Event {
    let (name, data) = match event {
        Event::Files { paths } => ("files", json!({ "paths": paths })),
        Event::Index => ("index", json!({})),
        Event::Refs => ("refs", json!({})),
        Event::Polling => ("mode", json!({ "polling": polling })),
    };
    sse::Event::default().event(name).data(data.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    fn router(events: Events) -> Router {
        Router::new()
            .route("/api/events", get(super::events))
            .with_state(events)
    }

    /// Reads the stream until a message contains `wanted`, failing after a few seconds.
    async fn read_until(body: &mut axum::body::BodyDataStream, wanted: &str) -> String {
        let mut seen = String::new();
        let read = async {
            while let Some(chunk) = body.next().await {
                seen.push_str(std::str::from_utf8(&chunk.unwrap()).unwrap());
                if seen.contains(wanted) {
                    return;
                }
            }
        };
        if tokio::time::timeout(Duration::from_secs(10), read)
            .await
            .is_err()
        {
            panic!("{wanted:?} did not arrive; got {seen:?}");
        }
        seen
    }

    #[test]
    fn messages_are_json() {
        let files = message(
            &Event::Files {
                paths: Some(vec!["src/lib.rs".to_owned()]),
            },
            false,
        );
        let mode = message(&Event::Polling, true);

        assert_eq!(
            format!("{files:?}"),
            format!(
                "{:?}",
                sse::Event::default()
                    .event("files")
                    .data(r#"{"paths":["src/lib.rs"]}"#)
            )
        );
        assert_eq!(
            format!("{mode:?}"),
            format!(
                "{:?}",
                sse::Event::default()
                    .event("mode")
                    .data(r#"{"polling":true}"#)
            )
        );
    }

    #[tokio::test]
    async fn streams_the_mode_then_changes() {
        let tmp = crate::tests::repo();
        let events = Events::new(Review::new(tmp.path().canonicalize().unwrap()), false);

        let response = router(events.clone())
            .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/event-stream"
        );
        let mut body = response.into_body().into_data_stream();
        let first = read_until(&mut body, "\n\n").await;
        assert_eq!(first, "event: mode\ndata: {\"polling\":false}\n\n");
        assert!(events.watching());

        // Give the watcher time to add its watches.
        tokio::time::sleep(Duration::from_millis(300)).await;
        std::fs::write(tmp.path().join("src/a.txt"), "three\n").unwrap();
        read_until(&mut body, "event: files\ndata: {\"paths\":[\"src/a.txt\"]}").await;

        drop(body);
        assert!(!events.watching());
    }

    #[tokio::test]
    async fn closing_ends_the_stream() {
        let tmp = crate::tests::repo();
        let events = Events::new(Review::new(tmp.path()), true);
        let response = router(events.clone())
            .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let mut body = response.into_body().into_data_stream();
        read_until(&mut body, "{\"polling\":true}").await;

        events.close();

        let rest = tokio::time::timeout(Duration::from_secs(5), body.next())
            .await
            .unwrap();
        assert!(rest.is_none(), "got {rest:?}");
    }
}
