#!/bin/bash

# 测试API是否正常工作

echo "Testing Chorus API..."

# 测试健康检查
echo "1. Testing health check..."
curl -s http://127.0.0.1:11435/health || echo "Health check failed"

# 启动服务器（后台运行）
echo "2. Starting Chorus server..."
cargo run > /tmp/chorus.log 2>&1 &
SERVER_PID=$!

# 等待服务器启动
echo "3. Waiting for server to start..."
sleep 10

# 检查服务器是否正在运行
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "Server failed to start!"
    cat /tmp/chorus.log
    exit 1
fi

echo "Server started with PID: $SERVER_PID"

# 测试API请求
echo "4. Testing API request..."
curl -s -X POST http://127.0.0.1:11435/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "Hello, say hello"}],
    "stream": false
  }' > /tmp/api_response.json

# 检查响应
if [ -s /tmp/api_response.json ]; then
    echo "API request successful!"
    echo "Response:"
    cat /tmp/api_response.json | jq . 2>/dev/null || cat /tmp/api_response.json
else
    echo "API request failed!"
    echo "Server logs:"
    tail -20 /tmp/chorus.log
fi

# 停止服务器
echo "5. Stopping server..."
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo "Test completed."