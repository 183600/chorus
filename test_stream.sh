#!/bin/bash

# 测试流式API请求

echo "Testing Chorus API with streaming..."

# 启动服务器（后台运行）
echo "Starting Chorus server..."
cargo run > /tmp/chorus_stream.log 2>&1 &
SERVER_PID=$!

# 等待服务器启动
echo "Waiting for server to start..."
sleep 10

# 检查服务器是否正在运行
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "Server failed to start!"
    cat /tmp/chorus_stream.log
    exit 1
fi

echo "Server started with PID: $SERVER_PID"

# 测试流式API请求
echo "Testing streaming API request..."
curl -s -X POST http://127.0.0.1:11435/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "Hello, say hello in streaming mode"}],
    "stream": true
  }' > /tmp/stream_response.json

# 检查响应
if [ -s /tmp/stream_response.json ]; then
    echo "Streaming API request successful!"
    echo "Response preview:"
    head -10 /tmp/stream_response.json
    echo ""
    echo "Total lines: $(wc -l < /tmp/stream_response.json)"
else
    echo "Streaming API request failed!"
    echo "Server logs:"
    tail -20 /tmp/chorus_stream.log
fi

# 停止服务器
echo "Stopping server..."
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo "Streaming test completed."