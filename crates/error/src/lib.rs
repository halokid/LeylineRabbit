use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] axum::http::uri::InvalidUri),

    #[error("Upstream timeout")]
    Timeout,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal server error")]
    Internal,
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            GatewayError::HttpRequest(_) => (StatusCode::BAD_GATEWAY, "Bad Gateway"),
            GatewayError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
            GatewayError::InvalidUri(_) => (StatusCode::BAD_REQUEST, "Bad Request"),
            GatewayError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "Gateway Timeout"),
            GatewayError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Configuration Error"),
            GatewayError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };

        let body = Json(json!({
            "error": error_message,
            "message": self.to_string(),
        }));

        (status, body).into_response()
    }
}
