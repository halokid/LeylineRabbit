# LeylineEnvoy - Advanced API Gateway

LeylineEnvoy is an independent subproject of the Leyline Rabbit project, providing advanced API gateway functionality.

## Overview

LeylineEnvoy is a standalone executable that shares core functionality with the main `leyline-rabbit` project but provides different configurations and ports.

## Features

- ✅ **Round-robin Load Balancing**: Evenly distributes requests across multiple upstream servers
- ✅ **Automatic Failure Retry**: Automatically retries other available servers when a server fails
- ✅ **Request Timeout Control**: Default 10-second timeout
- ✅ **Detailed Logging**: Complete request tracing and failure diagnostics
- ✅ **Independent Configuration**: Independent service configuration and ports

## Configuration

### Listening Port
- **Port**: 4000 (different from leyline-rabbit's 3000)
- **Address**: 127.0.0.1

### Upstream Service Configuration
```rust
UpstreamService::with_config("/api", vec![
    "http://127.0.0.1:8080".to_string(),  // API server 1
    "http://127.0.0.1:8081".to_string(),  // API server 2
], 10, 2)
```

## Building and Running

### Building
```bash
# Build LeylineEnvoy
cargo build -p leyline-envoy

# Or build release version
cargo build --release -p leyline-envoy
```

### Running
```bash
# Run LeylineEnvoy
cargo run -p leyline-envoy

# Or run release version
cargo run --release -p leyline-envoy
```

### Building Standalone Executable
```bash
cargo build --release -p leyline-envoy
# Executable located at: target/release/leyline-envoy
```

## API Endpoints

### Health Check
```bash
curl http://localhost:4000/health
```

### Service Status
```bash
curl http://localhost:4000/envoy/status
```

Response example:
```json
{
  "service": "LeylineEnvoy",
  "version": "0.1.0",
  "status": "healthy",
  "description": "Advanced API Gateway with load balancing and retry",
  "port": 4000
}
```

### Proxy Requests
```bash
# Proxy to /api service
curl http://localhost:4000/api/endpoint
```

## Logging

### Log File Location
- File: `./logs/leyline-envoy.log`
- Format: JSON (for easy log analysis)
- Rotation: Daily automatic rotation

### Log Levels
```bash
# Set log level
RUST_LOG=leyline_envoy=debug cargo run -p leyline-envoy

# Detailed logs
RUST_LOG=leyline_envoy=trace,tower_http=debug cargo run -p leyline-envoy
```

## Differences from LeylineRabbit

| Feature | LeylineRabbit | LeylineEnvoy |
|---------|---------------|--------------|
| Port | 3000 | 4000 |
| Path Prefix | /py | /api |
| Log File | leyline-rabbit.log | leyline-envoy.log |
| Target Scenario | Python Service Gateway | General API Gateway |

## Use Cases

### Scenario 1: Microservices Gateway
As an API gateway in microservices architecture, centrally managing multiple backend services.

### Scenario 2: Load Balancer
Distribute load across multiple API servers to improve availability.

### Scenario 3: Failover
Automatically detect failed servers and retry with healthy servers.

## Configuration Recommendations

### Production Environment
```rust
UpstreamService::with_config("/api", vec![
    "http://api-1.example.com:8080".to_string(),
    "http://api-2.example.com:8080".to_string(),
    "http://api-3.example.com:8080".to_string(),
], 15, 3)  // 15s timeout, retry 3 times
```

### Development Environment
```rust
UpstreamService::with_config("/api", vec![
    "http://localhost:8080".to_string(),
], 30, 1)  // 30s timeout, no retry
```

## Monitoring

### Performance Metrics
- Request latency
- Success rate
- Retry rate
- Server health status

### Log Analysis
```bash
# View all requests
grep "LeylineEnvoy processing request" logs/leyline-envoy.log

# View retry situations
grep "primary server failed" logs/leyline-envoy.log

# View errors
grep "ERROR" logs/leyline-envoy.log
```

## Extension

### Adding New Services
```rust
UpstreamService::new("/web", vec![
    "http://127.0.0.1:3001".to_string(),
]),
UpstreamService::new("/admin", vec![
    "http://127.0.0.1:9000".to_string(),
]),
```

### Custom Configuration
Modify the configuration section in `leyline-envoy/src/main.rs`.

## Contributing

LeylineEnvoy is part of the Leyline Rabbit project. Contributions and suggestions are welcome.

## License

Uses the same license as the main Leyline Rabbit project.
