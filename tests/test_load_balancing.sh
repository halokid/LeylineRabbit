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

echo "Testing load balancing..."
echo "Sending 6 requests to /py/ping to verify round-robin distribution"

# Send multiple requests to test load balancing
for i in {1..6}; do
    echo "Request $i:"
    curl -s http://localhost:3000/py/ping | jq '.'
    echo "---"
    sleep 0.1
done

echo "Stopping servers..."

# Stop gateway
kill $GATEWAY_PID 2>/dev/null

# Stop Flask servers
kill $SERVER1_PID 2>/dev/null
kill $SERVER2_PID 2>/dev/null

echo "Load balancing test completed!"
echo "You should see alternating responses from 'flask-server' and 'flask-server-8081'"
