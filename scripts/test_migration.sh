#!/bin/bash
set -e

echo "Testing configuration migration..."

# Test old config
echo "1. Testing legacy config loading..."
cargo run -- --config examples/test-old-config.toml &
PID=$!
sleep 2
kill $PID || true
echo "Legacy config test completed"

# Test environment variable config
echo "2. Testing CHORUS_CONFIG environment variable..."
CHORUS_CONFIG=examples/config.toml cargo run &
PID=$!
sleep 2
kill $PID || true
echo "Environment config test completed"

echo "Migration tests completed!"
