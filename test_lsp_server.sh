#!/bin/bash
# Mock LSP server that speaks the LSP protocol

echo "Mock LSP server starting..." >&2

while true; do
    # Read Content-Length header
    read -r line
    if [[ ! "$line" =~ ^Content-Length ]]; then
        continue
    fi

    # Extract content length
    content_length=$(echo "$line" | sed 's/Content-Length: //' | tr -d '\r\n')

    # Read empty line after headers
    read -r

    # Read the JSON body
    json=$(head -c "$content_length")

    echo "Received message: $json" >&2

    # Send back a response (echo the request as response for testing)
    response="{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"echo\"}"
    response_length=${#response}

    printf "Content-Length: %d\r\n\r\n%s" "$response_length" "$response"
done
