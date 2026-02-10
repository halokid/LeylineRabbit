#!/bin/bash

echo "Testing Timeout and Retry Functionality..."
echo "=========================================="

# Start normal Flask servers
echo "Starting normal Flask servers..."
cd tests
python upstream_server.py &
SERVER1_PID=$!

python upstream_server_8081.py &
SERVER2_PID=$!

# Wait for servers to start
sleep 3

# Start the gateway
echo "Starting API Gateway with timeout/retry..."
cd ..
cargo run --bin leyline-rabbit &
GATEWAY_PID=$!

# Wait for gateway to start
sleep 2

echo "Testing normal load balancing..."
echo "Request 1:"
curl -s http://localhost:3000/py/ping | jq '.upstream'
echo "Request 2:"
curl -s http://localhost:3000/py/ping | jq '.upstream'

echo ""
echo "Testing timeout scenario..."
echo "Stopping server on port 8080 to simulate timeout/failure..."

# Stop one server to simulate failure
kill $SERVER1_PID 2>/dev/null
sleep 1

echo "Request 3 (should retry and succeed with server 8081):"
curl -s http://localhost:3000/py/ping | jq '.upstream'

echo ""
echo "Testing complete failure scenario..."
echo "Stopping all servers..."

# Stop remaining server
kill $SERVER2_PID 2>/dev/null
sleep 1

echo "Request 4 (should fail after retries):"
curl -s -w "HTTP Status: %{http_code}\n" http://localhost:3000/py/ping

echo ""
echo "Stopping gateway..."
kill $GATEWAY_PID 2>/dev/null

echo "Test completed!"
echo "Check the gateway logs for timeout and retry messages."
