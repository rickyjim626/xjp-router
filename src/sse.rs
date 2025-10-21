use axum::response::sse::{Sse, Event};
use futures_util::StreamExt;

use crate::core::entities::UnifiedChunk;

pub fn to_axum_sse<S, E>(stream: S) -> Sse<impl futures_util::Stream<Item = Result<Event, E>>>
where
    S: futures_util::Stream<Item = Result<UnifiedChunk, E>> + Send + 'static,
    E: std::fmt::Display,
{
    let mapped = stream.map(|item| {
        match item {
            Ok(chunk) => {
                let json = serde_json::to_string(&chunk).unwrap_or("{}".to_string());
                Ok(Event::default().data(json))
            }
            Err(e) => Ok(Event::default().data(format!(r#"{{"error":"{e}"}}"#))),
        }
    });
    Sse::new(mapped)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(10)))
}
