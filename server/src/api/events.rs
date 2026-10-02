//! SSE event stream (ADR 0003): one endpoint, filtered per device.
//!
//! A device opens `GET /api/v1/events?device_id=<uuid>` and receives the
//! notifications addressed to it as server-sent events:
//!
//! - `help_request_new` – a new SOS the device was matched for (ADR 0004);
//! - `help_request_updated` – a status change of a request the device is a
//!   party to (requester/responder) or was originally notified about.
//!
//! Event data is the anonymous public help-request document (ADR 0004:
//! no requester/responder identity on the wire). The hub is in-memory and
//! per-process: notifications reach only devices whose stream is open when
//! the event is published – right for the prototype (SSE only, no push).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use super::help_requests::HelpRequest;
use super::AppState;

/// A notification routed to a set of devices.
#[derive(Debug, Clone)]
pub enum Notification {
    /// A new SOS the receiving devices were matched for.
    HelpRequestNew(HelpRequest),
    /// A status change of a request the receiving devices follow.
    HelpRequestUpdated(HelpRequest),
}

impl Notification {
    /// The SSE `event:` field value.
    fn event_name(&self) -> &'static str {
        match self {
            Self::HelpRequestNew(_) => "help_request_new",
            Self::HelpRequestUpdated(_) => "help_request_updated",
        }
    }

    /// The SSE frame: event name plus the public document as JSON data.
    fn into_sse_event(self) -> Event {
        let name = self.event_name();
        let request = match self {
            Self::HelpRequestNew(request) | Self::HelpRequestUpdated(request) => request,
        };
        let data = serde_json::to_string(&request).expect("HelpRequest serializes to JSON");
        Event::default().event(name).data(data)
    }
}

/// In-memory fan-out hub: routes notifications to the SSE connections of the
/// addressed devices. Cloning shares one routing table; devices can hold
/// several concurrent streams (all receive the notification).
#[derive(Clone, Default)]
pub struct Hub {
    subscribers: Arc<Mutex<HashMap<uuid::Uuid, Vec<mpsc::UnboundedSender<Arc<Notification>>>>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a stream for `device_id`; the returned receiver yields every
    /// notification published to that device from now on.
    pub fn subscribe(&self, device_id: uuid::Uuid) -> mpsc::UnboundedReceiver<Arc<Notification>> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.subscribers
            .lock()
            .expect("hub lock is never poisoned across await")
            .entry(device_id)
            .or_default()
            .push(sender);
        receiver
    }

    /// Delivers `notification` to every open stream of the target devices.
    /// Senders of closed streams are pruned on the way.
    pub fn publish(
        &self,
        targets: impl IntoIterator<Item = uuid::Uuid>,
        notification: Notification,
    ) {
        let notification = Arc::new(notification);
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("hub lock is never poisoned across await");
        for target in targets {
            if let Some(senders) = subscribers.get_mut(&target) {
                senders.retain(|sender| sender.send(Arc::clone(&notification)).is_ok());
            }
        }
    }
}

/// `GET /api/v1/events` query parameters.
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// The device whose notifications this stream delivers.
    pub device_id: uuid::Uuid,
}

/// `GET /api/v1/events?device_id=<uuid>` – the device's notification stream.
///
/// Identity note (ADR 0004): the query's device_id is the caller – whoever
/// knows the UUID receives that device's notifications, exactly like every
/// other endpoint scopes by the path's device_id.
pub async fn events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let receiver = state.hub.subscribe(query.device_id);
    let stream = UnboundedReceiverStream::new(receiver).map(|notification| {
        // Usually the sole reference – unwrap; else clone (same content).
        let notification =
            Arc::try_unwrap(notification).unwrap_or_else(|notification| (*notification).clone());
        Ok(notification.into_sse_event())
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A notification dropped into a hub with no subscriber senders left is
    /// a no-op; publishing to a device with an open stream delivers it.
    #[test]
    fn hub_routes_notifications_to_subscribers_only() {
        let hub = Hub::new();
        let a = uuid::Uuid::new_v4();
        let b = uuid::Uuid::new_v4();
        let mut rx_a = hub.subscribe(a);
        let mut rx_b = hub.subscribe(b);

        let request = HelpRequest {
            id: uuid::Uuid::new_v4(),
            status: "open".to_string(),
            note: None,
            location: super::super::types::LonLat {
                lon: 7.5,
                lat: 47.5,
            },
            radius_m: 500.0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        hub.publish([a], Notification::HelpRequestNew(request.clone()));

        let got = rx_a.try_recv().expect("subscribed device is notified");
        assert!(matches!(*got, Notification::HelpRequestNew(ref r) if r.id == request.id));
        assert!(rx_b.try_recv().is_err(), "other device hears nothing");
    }
}
