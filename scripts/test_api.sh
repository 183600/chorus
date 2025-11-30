#!/bin/bash
set -e

BASE_URL="http://127.0.0.1:11435"

echo "Testing Chorus API endpoints..."

# Test /v1/models
echo "1. Testing /v1/models..."
curl -s "$BASE_URL/v1/models" | jq '.'
echo ""

# Test /api/generate
echo "2. Testing /api/generate..."
curl -s -X POST "$BASE_URL/api/generate" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "prompt": "What is Rust?",
    "stream": false
  }' | jq '.'
echo ""

# Test /api/chat
echo "3. Testing /api/chat..."
curl -s -X POST "$BASE_URL/api/chat" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [
      {"role": "user", "content": "What is Rust?"}
    ],
    "stream": false,
    "include_workflow": true
  }' | jq '.'
echo ""

# Test /v1/completions
echo "4. Testing /v1/completions..."
curl -s -X POST "$BASE_URL/v1/completions" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "prompt": "What is Rust?"
  }' | jq '.'
echo ""

# Test /v1/chat/completions
echo "5. Testing /v1/chat/completions..."
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [
      {"role": "user", "content": "What is Rust?"}
    ]
  }' | jq '.'
echo ""

echo "All tests completed!"
