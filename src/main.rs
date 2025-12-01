use anyhow::{Context, Result};
use clap::Parser;
use chrono::Local;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::fs::OpenOptions;

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
    let stdin_log_path = args.log_dir.join(format!("lsp_stdin_{}.log", timestamp));
    let stdout_log_path = args.log_dir.join(format!("lsp_stdout_{}.log", timestamp));
    let stderr_log_path = args.log_dir.join(format!("lsp_stderr_{}.log", timestamp));

    eprintln!("LSP Proxy starting...");
    eprintln!("LSP Server: {} {:?}", args.lsp_server, args.server_args);
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

    // Task 1: Proxy stdin from editor to LSP server (with logging)
    let stdin_task = tokio::spawn(async move {
        let mut stdin_log = stdin_log;
        let mut buffer = vec![0u8; 8192];

        loop {
            match proxy_stdin.read(&mut buffer).await {
                Ok(0) => {
                    // EOF reached
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    // Log to file
                    if let Err(e) = stdin_log.write_all(data).await {
                        eprintln!("Failed to write to stdin log: {}", e);
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

        loop {
            match child_stdout.read(&mut buffer).await {
                Ok(0) => {
                    // EOF reached
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    // Log to file
                    if let Err(e) = stdout_log.write_all(data).await {
                        eprintln!("Failed to write to stdout log: {}", e);
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
