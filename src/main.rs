use anyhow::{Context, Result};
use chrono::Local;
use clap::Parser;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(author, version, about = "LSP Proxy - Logs and proxies LSP server communication", long_about = None)]
struct Args {
    /// Path to the LSP server executable
    #[arg(short = 's', long, env = "LSP_SERVER")]
    lsp_server: String,

    /// Arguments to pass to the LSP server
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    server_args: Vec<String>,

    /// Directory to write log files (defaults to current directory)
    #[arg(short = 'l', long, default_value = ".")]
    log_dir: PathBuf,

    /// Log as JSON lines (one JSON object per line) instead of raw JSON-RPC format
    #[arg(short = 'j', long)]
    json_lines: bool,
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
    let args = Args::parse();

    // Create log directory if it doesn't exist
    tokio::fs::create_dir_all(&args.log_dir)
        .await
        .context("Failed to create log directory")?;

    // Create log file paths with timestamp
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let suffix = if args.json_lines { "jsonl" } else { "log" };
    let stdin_log_path = args
        .log_dir
        .join(format!("lsp_stdin_{}.{}", timestamp, suffix));
    let stdout_log_path = args
        .log_dir
        .join(format!("lsp_stdout_{}.{}", timestamp, suffix));
    let stderr_log_path = args.log_dir.join(format!("lsp_stderr_{}.log", timestamp));

    eprintln!("LSP Proxy starting...");
    eprintln!("LSP Server: {} {:?}", args.lsp_server, args.server_args);
    eprintln!("JSON Lines mode: {}", args.json_lines);
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
    let mut child = Command::new(&args.lsp_server)
        .args(&args.server_args)
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

    let json_lines_mode = args.json_lines;

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
