use crate::{auth, core::entities::UnifiedRequest, routing::AppState};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;

pub async fn messages(
    State(app): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<crate::api::anthropic_adapter::AnthropicMessagesRequest>,
) -> Response {
    // 1) XJPkey 鉴权
    let raw_key = match auth::extract_xjpkey(&headers) {
        Ok(key) => key,
        Err(e) => return e.into_response(),
    };

    // 2) 验证密钥并获取密钥信息
    let key_store = app.key_store();
    let _key_info = match auth::verify_key(&*key_store, &raw_key).await {
        Ok(info) => info,
        Err(e) => return e.into_response(),
    };

    // 3) 适配为 UnifiedRequest
    let unified: UnifiedRequest = crate::api::anthropic_adapter::to_unified(req);
    let model_name = unified.logical_model.clone();

    // 4) 调用路由
    match app.invoke(unified).await {
        Ok(crate::connectors::ConnectorResponse::Streaming(stream)) => {
            // 简化的 Anthropic SSE：仅输出 content_block_delta 与最终 message_stop
            let mut started = false;
            let mapped = stream.map(
                move |item| -> Result<axum::response::sse::Event, std::convert::Infallible> {
                    match item {
                        Ok(chunk) => {
                            let mut ev = axum::response::sse::Event::default();
                            if !started {
                                started = true;
                                ev = ev.event("message_start").data("{}");
                                return Ok(ev);
                            }
                            if let Some(txt) = chunk.text_delta {
                                let data = serde_json::json!({
                                    "type":"content_block_delta",
                                    "delta":{"type":"text_delta","text": txt},
                                    "index":0
                                })
                                .to_string();
                                Ok(axum::response::sse::Event::default()
                                    .event("content_block_delta")
                                    .data(data))
                            } else if chunk.done {
                                Ok(axum::response::sse::Event::default()
                                    .event("message_stop")
                                    .data("{}"))
                            } else {
                                Ok(axum::response::sse::Event::default()
                                    .event("ping")
                                    .data("{}"))
                            }
                        }
                        Err(e) => Ok(axum::response::sse::Event::default()
                            .event("error")
                            .data(e.to_string())),
                    }
                },
            );
            axum::response::Sse::new(mapped).into_response()
        }
        Ok(crate::connectors::ConnectorResponse::NonStreaming(chunk)) => {
            let body = crate::api::anthropic_adapter::final_message_json(&model_name, chunk);
            Json(body).into_response()
        }
        Err(err) => err.into_response(),
    }
}
