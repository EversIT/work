use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    /// Also used for "exists but you may not see it", so ids cannot be probed.
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict")]
    Conflict,
    #[error("too many requests")]
    TooManyRequests,
    /// An optional feature (the AI assistant) that this instance has not switched on.
    #[error("{0}")]
    Unavailable(&'static str),
    /// A service this one depends on (the AI provider) failed or did not answer.
    #[error("{0}")]
    BadGateway(&'static str),
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            if db.is_unique_violation() {
                return ApiError::Conflict;
            }
        }
        ApiError::Internal(Box::new(e))
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(e: validator::ValidationErrors) -> Self {
        let fields: Vec<_> = e.field_errors().keys().map(|k| k.to_string()).collect();
        ApiError::BadRequest(format!("invalid fields: {}", fields.join(", ")))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".into()),
            ApiError::Conflict => (StatusCode::CONFLICT, "conflict".into()),
            ApiError::TooManyRequests => {
                (StatusCode::TOO_MANY_REQUESTS, "too many requests".into())
            }
            ApiError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, (*m).into()),
            ApiError::BadGateway(m) => (StatusCode::BAD_GATEWAY, (*m).into()),
            ApiError::Internal(e) => {
                // Details go to the log, never to the client.
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
