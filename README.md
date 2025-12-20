# lsp-proxy

A transparent proxy for Language Server Protocol (LSP) servers that logs all traffic between your editor and the LSP server.

## Features

- **Transparent proxying**: Sits between your editor and LSP server, forwarding all communication
- **Complete logging**: Captures stdin, stdout, and stderr to timestamped log files
- **JSON Lines mode**: Optionally logs LSP messages as JSON Lines (`.jsonl`) for easier parsing
- **Raw mode**: Logs complete JSON-RPC messages including headers for debugging protocol issues

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/lsp-proxy`.

## Usage

### Standard Proxy Mode

```bash
LSP_SERVER=<path> lsp-proxy [LSP_ARGS]...
```

### Minimal Session Mode

```bash
lsp-proxy --minimal-session
```

Outputs an LSP initialize request followed by a shutdown request to stdout, suitable for piping directly into an LSP server for testing.

### Environment Variables

- `LSP_SERVER` - Path to the LSP server executable (required)
- `LSP_LOG_DIR` - Directory to write log files (default: `/tmp/lsp-proxy`)
- `LSP_JSON_LINES` - Set to `1` or `true` for JSON Lines logging mode

All command-line arguments are passed directly to the LSP server.

### Examples

Proxy rust-analyzer with JSON Lines logging:

```bash
LSP_SERVER=rust-analyzer LSP_JSON_LINES=1 LSP_LOG_DIR=./lsp-logs lsp-proxy
```

Proxy typescript-language-server with arguments:

```bash
LSP_SERVER=typescript-language-server lsp-proxy --stdio
```

Proxy with custom log directory:

```bash
LSP_SERVER=/usr/bin/clangd LSP_LOG_DIR=/var/log/lsp lsp-proxy
```

Test an LSP server with a minimal session:

```bash
lsp-proxy --minimal-session | rust-analyzer
```

## Log Files

Log files are created with timestamps in the format `YYYYMMDD_HHMMSS`:

- `<timestamp>_lsp_stdin.jsonl` - Messages from editor to server (JSON Lines mode)
- `<timestamp>_lsp_stdout.jsonl` - Messages from server to editor (JSON Lines mode)
- `<timestamp>_lsp_stdin.log` - Raw messages from editor (raw mode)
- `<timestamp>_lsp_stdout.log` - Raw messages from server (raw mode)
- `<timestamp>_lsp_stderr.log` - Diagnostic output from the LSP server

## Use Cases

- Debug LSP communication issues
- Monitor LSP server behavior
- Analyze protocol-level interactions
- Capture traffic for bug reports
