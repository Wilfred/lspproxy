# lsp-fiddle

A transparent proxy for Language Server Protocol (LSP) servers that
logs all traffic between your editor and the LSP server.

## Usage

### Proxy Mode

```bash
lsp-fiddle proxy <LSP_SERVER> [LSP_ARGS]...
```

Proxies an LSP server and logs all communication between your editor and the server.

### Minimal Session Mode

```bash
lsp-fiddle minimal
```

Outputs an LSP initialize request followed by a shutdown request to
stdout, suitable for piping directly into an LSP server for testing.

### Environment Variables

- `LSP_LOG_DIR` - Directory to write log files (default: `/tmp/lsp-proxy`)
- `LSP_JSON_LINES` - Set to `1` or `true` for JSON Lines logging mode

### Examples

Proxy rust-analyzer with JSON Lines logging:

```bash
LSP_JSON_LINES=1 LSP_LOG_DIR=./lsp-logs lsp-fiddle proxy rust-analyzer
```

Proxy typescript-language-server with arguments:

```bash
lsp-fiddle proxy typescript-language-server --stdio
```

Test an LSP server with a minimal session:

```bash
lsp-fiddle minimal | rust-analyzer
```

## Use Cases

- Debug LSP communication issues
- Monitor LSP server behavior
- Analyze protocol-level interactions
- Capture traffic for bug reports
