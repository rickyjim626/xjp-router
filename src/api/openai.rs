use axum::{extract::State, Json, response::{IntoResponse, Response}};
use futures_util::StreamExt;
use crate::{routing::AppState, auth, core::entities::UnifiedRequest};

pub async fn chat_completions(
    State(app): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<crate::api::openai_adapter::OpenAiChatRequest>,
) -> Response {
    // 1) XJPkey 鉴权
    if let Err(e) = auth::extract_xjpkey(&headers) { return e.into_response(); }

    // 2) 适配为 UnifiedRequest
    let unified: UnifiedRequest = crate::api::openai_adapter::to_unified(req);
    let model_name = unified.logical_model.clone();

    // 3) 调用路由
    match app.invoke(unified).await {
        Ok(crate::connectors::ConnectorResponse::Streaming(stream)) => {
            // 将 UnifiedChunk → OpenAI 流式片段
            let mapped = stream.map(move |item| {
                match item {
                    Ok(chunk) => {
                        if chunk.done && chunk.text_delta.is_none() {
                            return Ok(axum::response::sse::Event::default().data("[DONE]"));
                        }
                        let data = crate::api::openai_adapter::from_unified_chunk(&model_name, chunk);
                        let json = serde_json::to_string(&data).unwrap_or("{}".to_string());
                        Ok(axum::response::sse::Event::default().data(json))
                    }
                    Err(e) => {
                        let json = serde_json::json!({
                            "error": { "message": e.to_string() }
                        }).to_string();
                        Ok(axum::response::sse::Event::default().data(json))
                    }
                }
            });
            axum::response::Sse::new(mapped).into_response()
        }
        Ok(crate::connectors::ConnectorResponse::NonStreaming(chunk)) => {
            let body = crate::api::openai_adapter::from_unified_final(&model_name, chunk);
            Json(body).into_response()
        }
        Err(err) => err.into_response(),
    }
}
