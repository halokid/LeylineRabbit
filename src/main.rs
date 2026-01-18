use axum::{
    extract::Request,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use leyline_error::GatewayError;
use reqwest::Client;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone)]
struct UpstreamService {
    prefix: String,
    upstream_url: String,
}

impl UpstreamService {
    fn new(prefix: impl Into<String>, upstream_url: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            upstream_url: upstream_url.into(),
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing with console and file output
    let file_appender = tracing_appender::rolling::daily("./logs", "leyline-rabbit.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "leyline_rabbit=debug,tower_http=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .with_thread_names(false)
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_target(false)
                .with_thread_ids(false)
                .with_thread_names(false)
                .json()
        )
        .init();

    // Create HTTP client for proxying requests
    let client = Client::new();

    // Configure upstream services with path prefixes
    let upstream_services = vec![
        UpstreamService::new("/py", "http://127.0.0.1:8080"),
        // Future services can be added here, e.g.:
        // UpstreamService::new("/node", "http://127.0.0.1:3001"),
        // UpstreamService::new("/go", "http://127.0.0.1:8081"),
    ];

    // Build our application with routes and middleware
    let app = Router::new()
        .route("/health", get(health_handler))
        .fallback(proxy_handler)
        .with_state((client, upstream_services))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let method = request.method();
                    let uri = request.uri();
                    let version = request.version();

                    tracing::info_span!(
                        "http_request",
                        method = %method,
                        uri = %uri,
                        version = ?version,
                        user_agent = ?request.headers().get("user-agent"),
                    )
                })
                .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                    tracing::info!(
                        "started processing request: {} {} {:?}",
                        request.method(),
                        request.uri(),
                        request.version()
                    );
                })
                .on_response(|response: &axum::http::Response<_>, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::info!(
                        "finished processing request: status={}, latency={:?}",
                        response.status(),
                        latency
                    );
                })
                .on_failure(|error: tower_http::classify::ServerErrorsFailureClass, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::error!(
                        "request failed: error={:?}, latency={:?}",
                        error,
                        latency
                    );
                })
        );

    // Run our app with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn proxy_handler(
    axum::extract::State((client, upstream_services)): axum::extract::State<(Client, Vec<UpstreamService>)>,
    req: Request,
) -> Result<impl IntoResponse, GatewayError> {
    let path = req.uri().path();

    // Find matching upstream service based on path prefix
    let upstream_service = upstream_services
        .iter()
        .find(|service| path.starts_with(&service.prefix))
        .ok_or_else(|| GatewayError::Config("No matching upstream service found".to_string()))?;

    // Remove the prefix from the path to get the upstream path
    let upstream_path = if path == &upstream_service.prefix {
        "/".to_string()
    } else {
        path.strip_prefix(&upstream_service.prefix)
            .unwrap_or(path)
            .to_string()
    };

    // Build the upstream URI
    let query = req.uri().query();
    let upstream_uri = if let Some(q) = query {
        format!("{}{}?{}", upstream_service.upstream_url, upstream_path, q)
    } else {
        format!("{}{}", upstream_service.upstream_url, upstream_path)
    };

    // For MVP, only proxy GET requests
    if req.method() != axum::http::Method::GET {
        return Ok((StatusCode::METHOD_NOT_ALLOWED, "Method not allowed").into_response());
    }

    // Forward the request to upstream
    let response = client
        .get(&upstream_uri)
        .send()
        .await?;

    // Get response data
    let body = response.text().await?;

    // For MVP, return a simple response
    // In a real implementation, we'd properly handle headers and status codes
    Ok((StatusCode::OK, body).into_response())
}
