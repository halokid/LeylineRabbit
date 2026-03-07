use axum::{
    extract::Request,
    http::{StatusCode, HeaderName, HeaderValue},
    response::IntoResponse,
    routing::get,
    Router,
    body::Body,
};
use leyline_error::GatewayError;
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use http_body_util::BodyExt;

// API keys for authentication
const API_KEYS: &[&str] = &[
    "my-secret-api-key-12345",
    "another-api-key-67890",
    "third-api-key-abcdef",
];

#[derive(Debug, Clone)]
struct UpstreamService {
    prefix: String,
    upstream_urls: Vec<String>,
    load_balancer: Arc<LoadBalancer>,
    timeout_seconds: u64,
    max_retries: usize,
}

impl UpstreamService {
    fn new(prefix: impl Into<String>, upstream_urls: Vec<String>) -> Self {
        let len = upstream_urls.len();
        Self::with_config(prefix, upstream_urls, 10, len)
    }

    fn with_config(prefix: impl Into<String>, upstream_urls: Vec<String>, timeout_seconds: u64, max_retries: usize) -> Self {
        let len = upstream_urls.len();
        let load_balancer = Arc::new(LoadBalancer::new(len));
        Self {
            prefix: prefix.into(),
            upstream_urls,
            load_balancer,
            timeout_seconds,
            max_retries: max_retries.min(len), // Don't retry more than available servers
        }
    }

    fn get_next_upstream(&self) -> &str {
        let index = self.load_balancer.next();
        &self.upstream_urls[index]
    }

    fn get_upstream_by_index(&self, index: usize) -> &str {
        &self.upstream_urls[index % self.upstream_urls.len()]
    }
}

#[derive(Debug)]
struct LoadBalancer {
    current: AtomicUsize,
    total: usize,
}

impl LoadBalancer {
    fn new(total: usize) -> Self {
        Self {
            current: AtomicUsize::new(0),
            total,
        }
    }

    fn next(&self) -> usize {
        self.current.fetch_add(1, Ordering::SeqCst) % self.total
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Create HTTP client for proxying requests with timeout
    // Disable connection pooling to avoid socket hang up issues with some servers (like Gin)
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))            // TODO: timeout set work here, is globaly
        .pool_max_idle_per_host(0)  // Disable connection pooling to prevent socket issues
        .build()
        .map_err(|e| {
            tracing::error!("Failed to create HTTP client: {}", e);
            std::process::exit(1);
        })?;

    // Configure upstream services with path prefixes
    let upstream_services = vec![
        UpstreamService::with_config("/py", vec![
            "http://127.0.0.1:8082".to_string(),  // Timeout server
            // "http://127.0.0.1:8080".to_string(),  // Normal server
            "http://127.0.0.1:8081".to_string(),  // Normal server
            // "http://127.0.0.1:8082".to_string(),  // Timeout server
        ], 10, 2), // 10s timeout, retry up to 2 servers
        // Future services can be added here, e.g.:
        // UpstreamService::new("/node", vec!["http://127.0.0.1:3001".to_string()]),
        // UpstreamService::new("/go", vec!["http://127.0.0.1:8081".to_string()]),

        UpstreamService::with_config("/go", vec![
            "http://127.0.0.1:8082".to_string(),  // Timeout server
            "http://127.0.0.1:8081".to_string(),  // Normal server
        ], 10, 2)
    ];

    // Build our application with routes and middleware
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ping", get(ping_handler))
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
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn ping_handler() -> impl IntoResponse {
    (StatusCode::OK, "pong")
}

async fn proxy_handler(
    axum::extract::State((client, upstream_services)): axum::extract::State<(Client, Vec<UpstreamService>)>,
    mut req: Request,
) -> Result<impl IntoResponse, GatewayError> {
    // Check API key for proxy requests (check against multiple valid keys)
    let api_key_header = req.headers().get("x-api-key");
    if let Some(api_key) = api_key_header {
        if !API_KEYS.contains(&api_key.to_str().unwrap_or("")) {
            return Ok((StatusCode::UNAUTHORIZED, "Invalid API key").into_response());
        }
    } else {
        return Ok((StatusCode::UNAUTHORIZED, "API key required").into_response());
    }

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

    // Build the upstream URI template
    let query = req.uri().query();
    let uri_template = if let Some(q) = query {
        format!("{{}}{}?{}", upstream_path, q)
    } else {
        format!("{{}}{}", upstream_path)
    };

    // Try each upstream server with retry logic
    let mut last_error = None;
    let start_index = upstream_service.load_balancer.current.load(Ordering::SeqCst);

    for attempt in 0..upstream_service.max_retries {
        let server_index = (start_index + attempt) % upstream_service.upstream_urls.len();
        let upstream_url = upstream_service.get_upstream_by_index(server_index);
        let upstream_uri = uri_template.replace("{}", upstream_url);

        tracing::debug!("attempting request to upstream server: {} (attempt {}/{})",
                       upstream_url, attempt + 1, upstream_service.max_retries);

        // Build the request with the exact same method, headers, and body as the original
        // Convert axum Method to reqwest Method
        let method = match *req.method() {
            axum::http::Method::GET => reqwest::Method::GET,
            axum::http::Method::POST => reqwest::Method::POST,
            axum::http::Method::PUT => reqwest::Method::PUT,
            axum::http::Method::DELETE => reqwest::Method::DELETE,
            axum::http::Method::HEAD => reqwest::Method::HEAD,
            axum::http::Method::OPTIONS => reqwest::Method::OPTIONS,
            axum::http::Method::PATCH => reqwest::Method::PATCH,
            _ => {
                tracing::warn!("Unsupported HTTP method: {}", req.method());
                return Ok((StatusCode::METHOD_NOT_ALLOWED, "Method not supported").into_response());
            }
        };

        let mut request_builder = client.request(method, &upstream_uri);

        // Forward all headers (except problematic ones that can cause socket hang up)
        let headers_to_skip = [
            "host",
            "connection",
            "keep-alive",
            "proxy-authenticate",
            "proxy-authorization",
            "te",
            "trailers",
            "transfer-encoding",
            "upgrade",
        ];

        for (key, value) in req.headers().iter() {
            let key_str = key.as_str().to_lowercase();
            if !headers_to_skip.contains(&key_str.as_str()) {
                if let Ok(k) = key.as_str().parse::<reqwest::header::HeaderName>() {
                    request_builder = request_builder.header(k, value.as_bytes());
                }
            }
        }

        // Forward the request body efficiently
        match req.method() {
            &axum::http::Method::GET | &axum::http::Method::HEAD => {
                // These methods typically don't have bodies - no body to forward
            },
            _ => {
                // For methods with bodies, collect and forward
                // NOTE: This reads the body into memory for simplicity.
                // For true zero-copy streaming, we'd need to use hyper directly
                // instead of reqwest, which is more complex but more efficient
                // for large payloads.
                match req.body_mut().collect().await {
                    Ok(collected) => {
                        let body_bytes = collected.to_bytes();
                        let body_len = body_bytes.len();

                        // Set Content-Length header explicitly to avoid socket hang up issues
                        if let Ok(content_length_header) = "content-length".parse::<reqwest::header::HeaderName>() {
                            request_builder = request_builder.header(content_length_header, body_len.to_string());
                        }

                        request_builder = request_builder.body(body_bytes.to_vec());
                    },
                    Err(e) => {
                        tracing::error!("Failed to read request body: {}", e);
                        return Err(GatewayError::Internal);
                    }
                }
            }
        }

        match request_builder.send().await {
            Ok(response) => {
                let status = response.status();

                // For successful responses, forward everything back
                if status.is_success() || status.is_redirection() || status.is_informational() || status.is_client_error() {
                    tracing::debug!("successful response from: {} with status: {}", upstream_url, status);

                    // Collect response headers first (before consuming the response)
                    let headers: Vec<(String, Vec<u8>)> = response.headers()
                        .iter()
                        .map(|(k, v)| (k.as_str().to_string(), v.as_bytes().to_vec()))
                        .collect();

                    // Collect response body
                    let body = match response.bytes().await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            tracing::error!("Failed to read response body: {}", e);
                            return Err(GatewayError::Internal);
                        }
                    };

                    // Build response with original status and headers
                    let status_code = axum::http::StatusCode::from_u16(status.as_u16())
                        .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

                    let mut response_builder = axum::response::Response::builder()
                        .status(status_code);

                    // Forward response headers
                    for (key, value_bytes) in headers {
                        if let (Ok(k), Ok(v)) = (
                            key.parse::<axum::http::HeaderName>(),
                            axum::http::HeaderValue::from_bytes(&value_bytes)
                        ) {
                            response_builder = response_builder.header(k, v);
                        }
                    }

                    return Ok(response_builder
                        .body(axum::body::Body::from(body))
                        .unwrap());
                } else {
                    // Server errors - try next server
                    tracing::warn!("upstream server {} returned server error status: {}", upstream_url, status);
                    last_error = Some(GatewayError::Config(format!("Upstream server returned server error status: {}", status)));
                }
            }
            Err(e) => {
                // Check if it's a timeout or network error
                if e.is_timeout() {
                    println!("node {} has timeout problem", upstream_url);
                    tracing::warn!("request to upstream server {} timed out after {} seconds", upstream_url, upstream_service.timeout_seconds);
                    last_error = Some(GatewayError::Timeout);
                } else {
                    println!("node {} has problem", upstream_url);
                    tracing::warn!("failed to connect to upstream server {}: {}", upstream_url, e);
                    last_error = Some(GatewayError::HttpRequest(e));
                }
            }
        }

        // If this is not the last attempt, continue to next server
        if attempt < upstream_service.max_retries - 1 {
            tracing::info!("retrying with next upstream server...");
        }
    }

    // All retries failed
    tracing::error!("all upstream servers failed after {} attempts", upstream_service.max_retries);
    Err(last_error.unwrap_or_else(|| GatewayError::Internal))
}
