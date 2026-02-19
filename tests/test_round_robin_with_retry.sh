#!/bin/bash

echo "Testing Round-Robin Load Balancing with Retry..."
echo "================================================="

# Start both Flask servers
echo "Starting Flask servers..."
cd tests
python upstream_server.py &
SERVER1_PID=$!

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

echo "Phase 1: Testing round-robin load balancing (both servers healthy)"
echo "-------------------------------------------------------------------"

# Send 6 requests to test round-robin
for i in {1..6}; do
    echo "Request $i:"
    RESPONSE=$(curl -s http://localhost:3000/py/ping)
    SERVER=$(echo $RESPONSE | jq -r '.upstream')
    PORT=$(echo $RESPONSE | jq -r '.port')
    echo "  -> $SERVER (port: $PORT)"
    echo "---"
    sleep 0.1
done

echo ""
echo "Phase 2: Testing retry functionality"
echo "------------------------------------"

# Stop server 8080 to simulate failure
echo "Stopping server on port 8080..."
kill $SERVER1_PID 2>/dev/null
sleep 1

echo "Sending requests after server 8080 failure:"
echo "Request 1 (should go to 8080, fail, then retry to 8081):"
RESPONSE=$(curl -s http://localhost:3000/py/ping)
if [ $? -eq 0 ]; then
    SERVER=$(echo $RESPONSE | jq -r '.upstream')
    PORT=$(echo $RESPONSE | jq -r '.port')
    echo "  -> Success: $SERVER (port: $PORT)"
else
    echo "  -> Failed as expected"
fi

echo "Request 2 (should go to 8081 directly):"
RESPONSE=$(curl -s http://localhost:3000/py/ping)
SERVER=$(echo $RESPONSE | jq -r '.upstream')
PORT=$(echo $RESPONSE | jq -r '.port')
echo "  -> $SERVER (port: $PORT)"

echo ""
echo "Phase 3: Testing complete failure"
echo "----------------------------------"

# Stop the remaining server
echo "Stopping server on port 8081..."
kill $SERVER2_PID 2>/dev/null
sleep 1

echo "Request after all servers down (should fail):"
curl -s -w "HTTP Status: %{http_code}\n" http://localhost:3000/py/ping

echo ""
echo "Stopping gateway..."
kill $GATEWAY_PID 2>/dev/null

echo "Test completed!"
echo ""
echo "Expected behavior:"
echo "1. Round-robin: Alternating between servers 8080 and 8081"
echo "2. Retry: When 8080 fails, automatically retries to 8081"
echo "3. Failure: When all servers are down, returns error"
echo ""
echo "Check gateway logs for detailed retry information."
