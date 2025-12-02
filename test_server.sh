#!/bin/bash
# Simple mock LSP server for testing

echo "Mock LSP server starting..." >&2

# Read from stdin and echo to stdout
while IFS= read -r line; do
    echo "Received: $line" >&2
    echo "Response: $line"
done
