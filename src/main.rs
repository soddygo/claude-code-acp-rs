//! Claude Code ACP Agent binary
//!
//! Run with: cargo run
//!
//! For help: cargo run -- --help

use clap::Parser;
use claude_code_acp::{cli::Cli, run_acp_with_cli, shutdown_otel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Run the ACP agent and handle errors
    let result = run_acp_with_cli(&cli).await;

    // Shutdown OpenTelemetry to flush all pending spans
    shutdown_otel();

    if let Err(e) = result {
        // Output error to stderr (ACP protocol uses stdout for messages)
        eprintln!("Error: {}", e);

        // If running in interactive mode, show more details
        if atty::is(atty::Stream::Stdin) {
            eprintln!("\nFor debugging, run with --diagnostic to log to a file.");
            eprintln!("Or use -v/-vv/-vvv for more verbose logging.");
        }

        std::process::exit(1);
    }

    Ok(())
}
