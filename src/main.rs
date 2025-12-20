use anyhow::{Context, Result};
use chrono::Local;
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");

fn print_help() {
    println!("{} {}", NAME, VERSION);
    println!();
    println!("LSP Proxy - Logs and proxies LSP server communication");
    println!();
    println!("USAGE:");
    println!("    {} [OPTIONS] [-- [LSP_ARGS]...]", NAME);
    println!();
    println!("ENVIRONMENT VARIABLES:");
    println!("    LSP_SERVER       Path to the LSP server executable (required)");
    println!("    LSP_LOG_DIR      Directory to write log files (defaults to /tmp/lsp-proxy)");
    println!("    LSP_JSON_LINES   Set to '1' or 'true' for JSON lines logging mode");
    println!();
    println!("OPTIONS:");
    println!("    --help              Print help information");
    println!("    --version           Print version information");
    println!("    --minimal-session   Send initialize and shutdown requests to stdout");
    println!();
    println!("All other arguments are passed directly to the LSP server.");
}

fn print_version() {
    println!("{} {}", NAME, VERSION);
}

/// Formats a JSON message as an LSP message with Content-Length header
fn format_lsp_message(json: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", json.len(), json)
}

/// Prints a minimal LSP session (initialize + shutdown) to stdout
fn print_minimal_session() {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {}
        }
    });

    let shutdown = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": null
    });

    let initialize_str = serde_json::to_string(&initialize).unwrap();
    let shutdown_str = serde_json::to_string(&shutdown).unwrap();

    print!("{}", format_lsp_message(&initialize_str));
    print!("{}", format_lsp_message(&shutdown_str));
}

/// Parses LSP messages from a buffer and extracts JSON payloads
struct LspMessageParser {
    buffer: Vec<u8>,
}

impl LspMessageParser {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Add data to the buffer and try to extract complete messages
    fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to extract one complete LSP message from the buffer
    /// Returns (headers_and_body, json_payload) if successful
    fn try_parse_message(&mut self) -> Option<(Vec<u8>, String)> {
        // Look for the header separator (\r\n\r\n)
        let header_end = self.find_header_end()?;

        // Parse headers to get Content-Length
        let headers = String::from_utf8_lossy(&self.buffer[..header_end]);
        let content_length = self.parse_content_length(&headers)?;

        // Check if we have the complete message body
        let body_start = header_end + 4; // Skip \r\n\r\n
        let body_end = body_start + content_length;

        if self.buffer.len() < body_end {
            // Don't have complete message yet
            return None;
        }

        // Extract the complete message (headers + body)
        let complete_message = self.buffer.drain(..body_end).collect::<Vec<u8>>();

        // Extract just the JSON body
        let json_bytes = &complete_message[body_start..];
        let json_str = String::from_utf8_lossy(json_bytes).to_string();

        Some((complete_message, json_str))
    }

    fn find_header_end(&self) -> Option<usize> {
        self.buffer.windows(4).position(|w| w == b"\r\n\r\n")
    }

    fn parse_content_length(&self, headers: &str) -> Option<usize> {
        for line in headers.lines() {
            if let Some(value) = line.strip_prefix("Content-Length:") {
                return value.trim().parse().ok();
            }
        }
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Collect command-line arguments
    let args: Vec<String> = env::args().collect();

    // Check for --help, --version, or --minimal-session
    if args.len() > 1 {
        match args[1].as_str() {
            "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--version" => {
                print_version();
                std::process::exit(0);
            }
            "--minimal-session" => {
                print_minimal_session();
                std::process::exit(0);
            }
            _ => {}
        }
    }

    // Get configuration from environment variables
    let lsp_server =
        env::var("LSP_SERVER").context("LSP_SERVER environment variable must be set")?;

    let log_dir = env::var("LSP_LOG_DIR").unwrap_or_else(|_| "/tmp/lsp-proxy".to_string());
    let log_dir = PathBuf::from(log_dir);

    let json_lines = env::var("LSP_JSON_LINES")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    // All arguments (except the program name) are passed to the LSP server
    let server_args: Vec<String> = args.into_iter().skip(1).collect();

    // Create log directory if it doesn't exist
    tokio::fs::create_dir_all(&log_dir)
        .await
        .context("Failed to create log directory")?;

    // Create log file paths with timestamp
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let suffix = if json_lines { "jsonl" } else { "log" };
    let stdin_log_path = log_dir.join(format!("lsp_stdin_{}.{}", timestamp, suffix));
    let stdout_log_path = log_dir.join(format!("lsp_stdout_{}.{}", timestamp, suffix));
    let stderr_log_path = log_dir.join(format!("lsp_stderr_{}.log", timestamp));

    eprintln!("LSP Proxy starting...");
    eprintln!("LSP Server: {} {:?}", lsp_server, server_args);
    eprintln!("JSON Lines mode: {}", json_lines);
    eprintln!("Logging to:");
    eprintln!("  stdin:  {}", stdin_log_path.display());
    eprintln!("  stdout: {}", stdout_log_path.display());
    eprintln!("  stderr: {}", stderr_log_path.display());

    // Open log files
    let stdin_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stdin_log_path)
        .await
        .context("Failed to create stdin log file")?;

    let stdout_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stdout_log_path)
        .await
        .context("Failed to create stdout log file")?;

    let stderr_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stderr_log_path)
        .await
        .context("Failed to create stderr log file")?;

    // Spawn the LSP server process
    let mut child = Command::new(&lsp_server)
        .args(&server_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn LSP server")?;

    let mut child_stdin = child.stdin.take().context("Failed to open child stdin")?;
    let child_stdout = child.stdout.take().context("Failed to open child stdout")?;
    let child_stderr = child.stderr.take().context("Failed to open child stderr")?;

    let mut proxy_stdin = tokio::io::stdin();
    let mut proxy_stdout = tokio::io::stdout();

    let json_lines_mode = json_lines;

    // Task 1: Proxy stdin from editor to LSP server (with logging)
    let stdin_task = tokio::spawn(async move {
        let mut stdin_log = stdin_log;
        let mut buffer = vec![0u8; 8192];
        let mut parser = LspMessageParser::new();

        loop {
            match proxy_stdin.read(&mut buffer).await {
                Ok(0) => {
                    // EOF reached
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    if json_lines_mode {
                        // Parse LSP messages and log as JSON lines
                        parser.add_data(data);

                        while let Some((_, json_payload)) = parser.try_parse_message() {
                            // Validate and potentially pretty-print the JSON
                            match serde_json::from_str::<serde_json::Value>(&json_payload) {
                                Ok(value) => {
                                    // Write as compact JSON line
                                    if let Ok(compact) = serde_json::to_string(&value) {
                                        let line = format!("{}\n", compact);
                                        if let Err(e) = stdin_log.write_all(line.as_bytes()).await {
                                            eprintln!("Failed to write to stdin log: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse JSON from stdin: {}", e);
                                    // Log the raw payload as fallback
                                    let line = format!("{}\n", json_payload);
                                    if let Err(e) = stdin_log.write_all(line.as_bytes()).await {
                                        eprintln!("Failed to write to stdin log: {}", e);
                                    }
                                }
                            }
                        }
                    } else {
                        // Log raw bytes
                        if let Err(e) = stdin_log.write_all(data).await {
                            eprintln!("Failed to write to stdin log: {}", e);
                        }
                    }

                    // Forward to LSP server
                    if let Err(e) = child_stdin.write_all(data).await {
                        eprintln!("Failed to write to LSP server stdin: {}", e);
                        break;
                    }

                    // Flush to ensure data is sent
                    if let Err(e) = child_stdin.flush().await {
                        eprintln!("Failed to flush LSP server stdin: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from proxy stdin: {}", e);
                    break;
                }
            }
        }
    });

    // Task 2: Proxy stdout from LSP server to editor (with logging)
    let stdout_task = tokio::spawn(async move {
        let mut stdout_log = stdout_log;
        let mut buffer = vec![0u8; 8192];
        let mut child_stdout = child_stdout;
        let mut parser = LspMessageParser::new();

        loop {
            match child_stdout.read(&mut buffer).await {
                Ok(0) => {
                    // EOF reached
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    if json_lines_mode {
                        // Parse LSP messages and log as JSON lines
                        parser.add_data(data);

                        while let Some((_, json_payload)) = parser.try_parse_message() {
                            // Validate and potentially pretty-print the JSON
                            match serde_json::from_str::<serde_json::Value>(&json_payload) {
                                Ok(value) => {
                                    // Write as compact JSON line
                                    if let Ok(compact) = serde_json::to_string(&value) {
                                        let line = format!("{}\n", compact);
                                        if let Err(e) = stdout_log.write_all(line.as_bytes()).await
                                        {
                                            eprintln!("Failed to write to stdout log: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse JSON from stdout: {}", e);
                                    // Log the raw payload as fallback
                                    let line = format!("{}\n", json_payload);
                                    if let Err(e) = stdout_log.write_all(line.as_bytes()).await {
                                        eprintln!("Failed to write to stdout log: {}", e);
                                    }
                                }
                            }
                        }
                    } else {
                        // Log raw bytes
                        if let Err(e) = stdout_log.write_all(data).await {
                            eprintln!("Failed to write to stdout log: {}", e);
                        }
                    }

                    // Forward to proxy stdout
                    if let Err(e) = proxy_stdout.write_all(data).await {
                        eprintln!("Failed to write to proxy stdout: {}", e);
                        break;
                    }

                    // Flush to ensure data is sent
                    if let Err(e) = proxy_stdout.flush().await {
                        eprintln!("Failed to flush proxy stdout: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from LSP server stdout: {}", e);
                    break;
                }
            }
        }
    });

    // Task 3: Log stderr from LSP server
    let stderr_task = tokio::spawn(async move {
        let mut stderr_log = stderr_log;
        let mut reader = BufReader::new(child_stderr);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF reached
                    break;
                }
                Ok(_) => {
                    // Log to file
                    if let Err(e) = stderr_log.write_all(line.as_bytes()).await {
                        eprintln!("Failed to write to stderr log: {}", e);
                    }

                    // Also print to proxy stderr for visibility
                    eprint!("[LSP stderr] {}", line);
                }
                Err(e) => {
                    eprintln!("Error reading from LSP server stderr: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for any task to complete or the child process to exit
    tokio::select! {
        _ = stdin_task => {
            eprintln!("Stdin task completed");
        }
        _ = stdout_task => {
            eprintln!("Stdout task completed");
        }
        _ = stderr_task => {
            eprintln!("Stderr task completed");
        }
        status = child.wait() => {
            match status {
                Ok(exit_status) => {
                    eprintln!("LSP server exited with status: {}", exit_status);
                    std::process::exit(exit_status.code().unwrap_or(1));
                }
                Err(e) => {
                    eprintln!("Failed to wait for LSP server: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
