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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_appender;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| {
            tracing::error!("Failed to create HTTP client: {}", e);
            std::process::exit(1);
        })?;

    /* 
    // Configure upstream services with path prefixes
    let upstream_services = vec![
        UpstreamService::with_config("/api", vec![
            "http://127.0.0.1:8080".to_string(),  // API server 1
            "http://127.0.0.1:8081".to_string(),  // API server 2
        ], 10, 2), // 10s timeout, retry up to 2 servers
        // Future services can be added here, e.g.:
        // UpstreamService::new("/web", vec!["http://127.0.0.1:3001".to_string()]),
    ];
    */

    let upstream_services = vec![
        UpstreamService::with_config("/py", vec![
            "http://127.0.0.1:8082".to_string(),  // Timeout server
            // "http://127.0.0.1:8080".to_string(),  // Normal server
            "http://127.0.0.1:8081".to_string(),  // Normal server
            // "http://127.0.0.1:8082".to_string(),  // Timeout server
        ], 10, 2), // 10s timeout, retry up to 2 servers

        // UpstreamService::with_config("/go", vec![
        //     "http://127.0.0.1:8082".to_string(),  // Timeout server
        //     "http://127.0.0.1:8081".to_string(),  // Normal server
        // ], 10, 2)
    ];

    // Build our application with routes and middleware
    let app = Router::new()
        .route("/health", get(health_handler))
        // .route("/envoy/status", get(envoy_status_handler))
        .fallback(proxy_handler)
        .with_state((client, upstream_services))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let method = request.method();
                    let uri = request.uri();
                    let version = request.version();

                    tracing::info_span!(
                        "envoy_request",
                        method = %method,
                        uri = %uri,
                        version = ?version,
                        user_agent = ?request.headers().get("user-agent"),
                    )
                })
                .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                    tracing::info!(
                        "LeylineRabbit processing request: {} {} {:?}",
                        request.method(),
                        request.uri(),
                        request.version()
                    );
                })
                .on_response(|response: &axum::http::Response<_>, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::info!(
                        "LeylineRabbit completed request: status={}, latency={:?}",
                        response.status(),
                        latency
                    );
                })
                .on_failure(|error: tower_http::classify::ServerErrorsFailureClass, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::error!(
                        "LeylineRabbit request failed: error={:?}, latency={:?}",
                        error,
                        latency
                    );
                })
        );

    // Run our app with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000)); // Different port: 4000
    tracing::info!("LeylineRabbit starting on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "LeylineRabbit OK")
}

// async fn envoy_status_handler() -> impl IntoResponse {
//     (
//         StatusCode::OK,
//         axum::Json(serde_json::json!({
//             "service": "LeylineRabbit",
//             "version": "0.1.0",
//             "status": "healthy",
//             "description": "Advanced API Gateway with load balancing and retry",
//             "port": 4000
//         }))
//     )
// }

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

    // Build the upstream URI template
    let query = req.uri().query();
    let uri_template = if let Some(q) = query {
        format!("{{}}{}?{}", upstream_path, q)
    } else {
        format!("{{}}{}", upstream_path)
    };

    // For MVP, only proxy GET requests
    if req.method() != axum::http::Method::GET {
        return Ok((StatusCode::METHOD_NOT_ALLOWED, "Method not allowed").into_response());
    }

    // First, try the load-balanced server (normal round-robin)
    let primary_upstream_url = upstream_service.get_next_upstream();
    let primary_uri = uri_template.replace("{}", primary_upstream_url);

    tracing::debug!("LeylineRabbit routing to upstream server: {}", primary_upstream_url);

    match client.get(&primary_uri).send().await {
        Ok(response) => {
            if response.status().is_success() {
                tracing::debug!("LeylineRabbit successful response from: {}", primary_upstream_url);
                let body = response.text().await?;
                return Ok((StatusCode::OK, body).into_response());
            } else {
                let status = response.status();
                tracing::warn!("LeylineRabbit primary upstream server {} returned error status: {}", primary_upstream_url, status);
            }
        }
        Err(e) => {
            tracing::warn!("LeylineRabbit failed to connect to primary upstream server {}: {}", primary_upstream_url, e);
        }
    }

    // Primary server failed, now try other servers as fallback
    tracing::info!("LeylineRabbit primary server failed, trying other servers...");

    // Try remaining servers in order (excluding the primary one we just tried)
    let primary_index = upstream_service.upstream_urls.iter().position(|url| url == primary_upstream_url).unwrap();

    for offset in 1..upstream_service.upstream_urls.len() {
        let server_index = (primary_index + offset) % upstream_service.upstream_urls.len();
        let upstream_url = upstream_service.get_upstream_by_index(server_index);
        let upstream_uri = uri_template.replace("{}", upstream_url);

        tracing::debug!("LeylineRabbit retrying with upstream server: {} (fallback {}/{})",
                       upstream_url, offset, upstream_service.upstream_urls.len() - 1);

        match client.get(&upstream_uri).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::debug!("LeylineRabbit successful response from fallback server: {}", upstream_url);
                    let body = response.text().await?;
                    return Ok((StatusCode::OK, body).into_response());
                } else {
                    let status = response.status();
                    tracing::warn!("LeylineRabbit fallback upstream server {} returned error status: {}", upstream_url, status);
                }
            }
            Err(e) => {
                tracing::warn!("LeylineRabbit failed to connect to fallback upstream server {}: {}", upstream_url, e);
            }
        }
    }

    // All servers failed
    tracing::error!("LeylineRabbit all upstream servers failed for service {}", upstream_service.prefix);
    Err(GatewayError::Internal)
}
