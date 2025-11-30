#!/bin/bash
set -e

BASE_URL="http://127.0.0.1:11435"

echo "Testing streaming endpoints..."

# Test streaming generate
echo "1. Testing streaming /api/generate..."
curl -s -N -X POST "$BASE_URL/api/generate?stream=true" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "prompt": "Count to 5"
  }'
echo -e "\n"

# Test streaming chat
echo "2. Testing streaming /api/chat..."
curl -s -N -X POST "$BASE_URL/api/chat?stream=true" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [
      {"role": "user", "content": "Count to 5"}
    ]
  }'
echo -e "\n"

echo "Streaming tests completed!"
