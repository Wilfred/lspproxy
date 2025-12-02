#!/bin/bash
TEST_LOG_DIR=$(mktemp -d)
json_message='{"jsonrpc":"2.0","id":1,"method":"test"}'
content_length=${#json_message}
lsp_message=$(printf "Content-Length: %d\r\n\r\n%s" "$content_length" "$json_message")

echo -n "$lsp_message" | timeout 2 cargo run -q -- -s ./test_lsp_server.sh -l "$TEST_LOG_DIR" 2>&1 > /dev/null || true

echo "=== Raw log content ==="
cat "$TEST_LOG_DIR"/lsp_stdin_*.log
echo ""
echo "=== Byte count ==="
wc -c "$TEST_LOG_DIR"/lsp_stdin_*.log

rm -rf "$TEST_LOG_DIR"
