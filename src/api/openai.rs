use crate::{auth, core::entities::UnifiedRequest, routing::AppState};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;

pub async fn chat_completions(
    State(app): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<crate::api::openai_adapter::OpenAiChatRequest>,
) -> Response {
    // 1) XJPkey 鉴权
    let raw_key = match auth::extract_xjpkey(&headers) {
        Ok(key) => key,
        Err(e) => return e.into_response(),
    };

    // 2) 验证密钥并获取密钥信息
    let key_store = app.key_store();
    let key_info = match auth::verify_key(&*key_store, &raw_key).await {
        Ok(info) => info,
        Err(e) => return e.into_response(),
    };

    // 3) 适配为 UnifiedRequest
    let unified: UnifiedRequest = crate::api::openai_adapter::to_unified(req);
    let model_name = unified.logical_model.clone();

    // 4) 调用路由（with billing tracking）
    match app.invoke_with_billing(unified, key_info.tenant_id.clone(), key_info.id).await {
        Ok(crate::connectors::ConnectorResponse::Streaming(stream)) => {
            // 将 UnifiedChunk → OpenAI 流式片段
            let mapped = stream.map(
                move |item| -> Result<axum::response::sse::Event, std::convert::Infallible> {
                    match item {
                        Ok(chunk) => {
                            if chunk.done && chunk.text_delta.is_none() {
                                return Ok(axum::response::sse::Event::default().data("[DONE]"));
                            }
                            let data =
                                crate::api::openai_adapter::from_unified_chunk(&model_name, chunk);
                            let json = serde_json::to_string(&data).unwrap_or("{}".to_string());
                            Ok(axum::response::sse::Event::default().data(json))
                        }
                        Err(e) => {
                            let json = serde_json::json!({
                                "error": { "message": e.to_string() }
                            })
                            .to_string();
                            Ok(axum::response::sse::Event::default().data(json))
                        }
                    }
                },
            );
            axum::response::Sse::new(mapped).into_response()
        }
        Ok(crate::connectors::ConnectorResponse::NonStreaming(chunk)) => {
            let body = crate::api::openai_adapter::from_unified_final(&model_name, chunk);
            Json(body).into_response()
        }
        Err(err) => err.into_response(),
    }
}
