#!/bin/bash
# Test the JSON lines logging feature

set -e

echo "Testing JSON lines logging..."

# Create a temp directory for logs
TEST_LOG_DIR=$(mktemp -d)
echo "Using log directory: $TEST_LOG_DIR"

# Create a test LSP message
json_message='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":123}}'
content_length=${#json_message}
lsp_message=$(printf "Content-Length: %d\r\n\r\n%s" "$content_length" "$json_message")

# Test with JSON lines mode enabled
echo "Test 1: JSON lines mode enabled (-j flag)"
echo -n "$lsp_message" | timeout 2 cargo run -q -- -s ./test_lsp_server.sh -l "$TEST_LOG_DIR" -j 2>&1 > /dev/null || true

# Check if JSONL file was created
jsonl_stdin=$(ls "$TEST_LOG_DIR"/lsp_stdin_*.jsonl 2>/dev/null | head -1)
if [ -f "$jsonl_stdin" ]; then
    echo "✓ JSONL file created: $jsonl_stdin"
    echo "Content:"
    cat "$jsonl_stdin"

    # Verify it's valid JSON
    if jq empty "$jsonl_stdin" 2>/dev/null; then
        echo "✓ Valid JSON"
    else
        echo "✗ Invalid JSON"
    fi
else
    echo "✗ JSONL file not created"
fi

# Clean up
rm -rf "$TEST_LOG_DIR"

echo ""
echo "Test 2: Raw mode (default, no -j flag)"
TEST_LOG_DIR=$(mktemp -d)
echo -n "$lsp_message" | timeout 2 cargo run -q -- -s ./test_lsp_server.sh -l "$TEST_LOG_DIR" 2>&1 > /dev/null || true

# Check if raw .log file was created
log_stdin=$(ls "$TEST_LOG_DIR"/lsp_stdin_*.log 2>/dev/null | head -1)
if [ -f "$log_stdin" ]; then
    echo "✓ Raw log file created: $log_stdin"
    echo "Content (hexdump):"
    hexdump -C "$log_stdin" | head -5
else
    echo "✗ Log file not created"
fi

# Clean up
rm -rf "$TEST_LOG_DIR"

echo ""
echo "Tests complete!"
