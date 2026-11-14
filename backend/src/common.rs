//! Shared by every module: app state, the one error shape, and the JSON body extractor.

use std::sync::Arc;

use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use deadpool_postgres::Pool;
use serde::de::DeserializeOwned;
use serde_json::json;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub cookie_secure: bool,
    pub origins: Arc<Vec<String>>,
}

/// Every endpoint's error: `{"error": {"code", "message", "details"?}}`. The frontend branches on `code`.
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Vec<String>,
}

impl AppError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), details: Vec::new() }
    }
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "请先登录")
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "找不到该试卷或题目")
    }
    pub fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "服务器内部错误")
    }
    /// At most 20 details are listed; the rest are summarized in one line.
    pub fn with_details(mut self, mut details: Vec<String>) -> Self {
        if details.len() > 20 {
            let rest = details.len() - 20;
            details.truncate(20);
            details.push(format!("另有 {rest} 条错误未列出"));
        }
        self.details = details;
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut error = json!({ "code": self.code, "message": self.message });
        if !self.details.is_empty() {
            error["details"] = json!(self.details);
        }
        (self.status, Json(json!({ "error": error }))).into_response()
    }
}

impl From<deadpool_postgres::PoolError> for AppError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        eprintln!("database pool error: {e}");
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "db_unavailable", "数据库暂时不可用")
    }
}

impl From<tokio_postgres::Error> for AppError {
    fn from(e: tokio_postgres::Error) -> Self {
        eprintln!("database error: {e}");
        if e.is_closed() {
            Self::new(StatusCode::SERVICE_UNAVAILABLE, "db_unavailable", "数据库暂时不可用")
        } else {
            Self::internal()
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

/// Paper ids are opaque strings to clients; anything unparsable simply does not exist.
pub fn parse_id(raw: &str) -> Result<i64> {
    raw.parse().map_err(|_| AppError::not_found())
}

/// axum's `Json`, but its errors use our error shape instead of plain text.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(Body(v)),
            Err(e) => Err(match e.status() {
                StatusCode::PAYLOAD_TOO_LARGE => {
                    AppError::new(StatusCode::PAYLOAD_TOO_LARGE, "file_too_large", "请求内容超过 1 MiB 上限")
                }
                StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::new(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "unsupported_media_type",
                    "请求必须使用 Content-Type: application/json",
                ),
                _ => AppError::bad_request("invalid_json", "请求内容不是有效的 JSON").with_details(vec![e.body_text()]),
            }),
        }
    }
}
