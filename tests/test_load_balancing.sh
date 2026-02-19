#!/bin/bash

echo "Testing Load Balancing Functionality..."
echo "======================================"

# Start first Flask server (8080)
echo "Starting Flask server on port 8080..."
cd tests
python upstream_server.py &
SERVER1_PID=$!

# Start second Flask server (8081)
echo "Starting Flask server on port 8081..."
python upstream_server_8081.py &
SERVER2_PID=$!

# Wait for servers to start
sleep 3

# Start the gateway
echo "Starting API Gateway..."
cd ..
cargo run --bin leyline-rabbit &
GATEWAY_PID=$!

# Wait for gateway to start
sleep 2

echo "Testing load balancing (round-robin)..."
echo "Sending 6 requests to /py/ping to verify round-robin distribution"

# Send multiple requests to test load balancing
for i in {1..6}; do
    echo "Request $i:"
    curl -s http://localhost:3000/py/ping | jq '.upstream'
    echo "---"
    sleep 0.1
done

echo ""
echo "Testing retry functionality..."
echo "Stopping server on port 8080 to simulate failure..."

# Stop one server to simulate failure
kill $SERVER1_PID 2>/dev/null
sleep 1

echo "Request after server failure (should retry to 8081):"
curl -s http://localhost:3000/py/ping | jq '.upstream'

echo ""
echo "Testing complete failure..."
echo "Stopping remaining server..."

# Stop the other server
kill $SERVER2_PID 2>/dev/null
sleep 1

echo "Request when all servers down (should fail):"
curl -s -w "HTTP Status: %{http_code}\n" http://localhost:3000/py/ping

echo ""
echo "Stopping gateway..."
kill $GATEWAY_PID 2>/dev/null

echo "Load balancing test completed!"
echo "Expected results:"
echo "1. Round-robin: Alternating between 'flask-server' and 'flask-server-8081'"
echo "2. Retry: After 8080 fails, should succeed with 8081"
echo "3. All fail: Should return error status"
