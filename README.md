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

```bash
lsp-proxy --lsp-server <LSP_SERVER> [OPTIONS] [SERVER_ARGS]...
```

### Options

- `-s, --lsp-server <LSP_SERVER>` - Path to the LSP server executable (or set `LSP_SERVER` env var)
- `-l, --log-dir <LOG_DIR>` - Directory for log files (default: current directory)
- `-j, --json-lines` - Log as JSON Lines instead of raw JSON-RPC format
- `[SERVER_ARGS]...` - Arguments to pass through to the LSP server

### Examples

Proxy rust-analyzer with JSON Lines logging:

```bash
lsp-proxy --lsp-server rust-analyzer --json-lines --log-dir ./lsp-logs
```

Proxy with server arguments:

```bash
lsp-proxy -s typescript-language-server -l ./logs -- --stdio
```

## Log Files

Log files are created with timestamps in the format `YYYYMMDD_HHMMSS`:

- `lsp_stdin_<timestamp>.jsonl` - Messages from editor to server (JSON Lines mode)
- `lsp_stdout_<timestamp>.jsonl` - Messages from server to editor (JSON Lines mode)
- `lsp_stdin_<timestamp>.log` - Raw messages from editor (raw mode)
- `lsp_stdout_<timestamp>.log` - Raw messages from server (raw mode)
- `lsp_stderr_<timestamp>.log` - Diagnostic output from the LSP server

## Use Cases

- Debug LSP communication issues
- Monitor LSP server behavior
- Analyze protocol-level interactions
- Capture traffic for bug reports
