#!/bin/bash

echo "=== 测试1: 非流式响应 (stream: false) ==="
curl -s -X POST http://localhost:11435/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": false
  }' | jq .

echo ""
echo "=== 测试2: 流式响应 (stream: true) ==="
curl -s -X POST http://localhost:11435/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": true
  }'

echo ""
echo ""
echo "=== 测试3: Generate API 非流式 ==="
curl -s -X POST http://localhost:11435/api/generate \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "prompt": "hello",
    "stream": false
  }' | jq .

echo ""
echo "=== 测试4: Generate API 流式 ==="
curl -s -X POST http://localhost:11435/api/generate \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "prompt": "hello",
    "stream": true
  }'
