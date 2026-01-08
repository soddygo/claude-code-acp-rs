//! ACP Agent runner
//!
//! Entry point for running the Claude ACP Agent.

use std::sync::Arc;

use sacp::link::AgentToClient;
use sacp::schema::{
    CancelNotification, InitializeRequest, NewSessionRequest, PromptRequest, SetSessionModeRequest,
};
use sacp::{ByteStreams, JrConnectionCx, MessageCx};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use super::core::ClaudeAcpAgent;
use super::handlers;
use crate::cli::Cli;

// OpenTelemetry imports (only when feature is enabled)
#[cfg(feature = "otel")]
use opentelemetry::global;
#[cfg(feature = "otel")]
use opentelemetry::trace::TracerProvider;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use opentelemetry_sdk::trace::SdkTracerProvider;

// Global storage for OpenTelemetry provider (for proper shutdown)
#[cfg(feature = "otel")]
static OTEL_PROVIDER: std::sync::OnceLock<SdkTracerProvider> = std::sync::OnceLock::new();

/// Shutdown OpenTelemetry provider (flush all pending spans)
///
/// This should be called before the application exits to ensure all
/// telemetry data is properly flushed to the backend.
#[cfg(feature = "otel")]
pub fn shutdown_otel() {
    if let Some(provider) = OTEL_PROVIDER.get() {
        tracing::info!("Shutting down OpenTelemetry provider...");
        if let Err(e) = provider.shutdown() {
            eprintln!("Failed to shutdown OpenTelemetry provider: {:?}", e);
        } else {
            tracing::info!("OpenTelemetry provider shutdown complete");
        }
    }
}

/// Shutdown OpenTelemetry provider (no-op when feature is disabled)
#[cfg(not(feature = "otel"))]
pub fn shutdown_otel() {}

/// Initialize OpenTelemetry tracer provider
///
/// Following OpenTelemetry Rust best practices:
/// 1. Create provider with batch exporter for production use
/// 2. Set global tracer provider for easy access
/// 3. Store provider for proper shutdown
#[cfg(feature = "otel")]
fn init_otel(endpoint: &str, service_name: &str) -> anyhow::Result<SdkTracerProvider> {
    use opentelemetry_sdk::Resource;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name(service_name.to_owned())
                .build(),
        )
        .build();

    // Set global tracer provider (best practice)
    global::set_tracer_provider(provider.clone());

    Ok(provider)
}

/// Build an EnvFilter based on CLI args and RUST_LOG environment variable
///
/// Priority: RUST_LOG environment variable > CLI arguments (-v, -vv, -q)
fn build_env_filter(cli: &Cli) -> tracing_subscriber::EnvFilter {
    // Check if RUST_LOG is set and non-empty
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            // RUST_LOG takes priority - use it directly
            return tracing_subscriber::EnvFilter::new(rust_log);
        }
    }

    // No RUST_LOG set, use CLI arguments to determine level
    let level = cli.log_level();
    tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into())
}

/// Initialize logging with file output (diagnostic mode)
fn init_logging_to_file(cli: &Cli) -> anyhow::Result<()> {
    let filter = build_env_filter(cli);

    let log_path = cli.log_path();

    // Ensure directory exists
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(&log_path)?;

    // Output log file location to stderr (user needs to know)
    eprintln!("Diagnostic mode: logging to {}", log_path.display());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false);

    #[cfg(feature = "otel")]
    {
        if cli.is_otel_enabled() {
            let endpoint = cli.otel_endpoint.as_ref().unwrap();
            let service_name = &cli.otel_service_name;

            eprintln!(
                "OpenTelemetry enabled: endpoint={}, service={}",
                endpoint, service_name
            );

            let provider = init_otel(endpoint, service_name)?;
            let tracer = provider.tracer("claude-code-acp-rs");
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            // Store provider globally for proper shutdown
            drop(OTEL_PROVIDER.set(provider));

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }

    #[cfg(not(feature = "otel"))]
    {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }

    Ok(())
}

/// Initialize logging with stderr output (normal mode)
fn init_logging_to_stderr(cli: &Cli) {
    let filter = build_env_filter(cli);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false);

    #[cfg(feature = "otel")]
    {
        if cli.is_otel_enabled() {
            let endpoint = cli.otel_endpoint.as_ref().unwrap();
            let service_name = &cli.otel_service_name;

            eprintln!(
                "OpenTelemetry enabled: endpoint={}, service={}",
                endpoint, service_name
            );

            let provider = init_otel(endpoint, service_name).expect("Failed to init OpenTelemetry");
            let tracer = provider.tracer("claude-code-acp-rs");
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            // Store provider globally for proper shutdown
            drop(OTEL_PROVIDER.set(provider));

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }

    #[cfg(not(feature = "otel"))]
    {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }
}

/// Initialize logging based on CLI arguments
fn init_logging(cli: &Cli) -> anyhow::Result<()> {
    if cli.is_diagnostic() {
        init_logging_to_file(cli)
    } else {
        init_logging_to_stderr(cli);
        Ok(())
    }
}

/// Run the ACP agent with CLI arguments
///
/// This is the main entry point when using CLI argument parsing.
/// It initializes logging based on CLI args and starts the ACP handler chain.
pub async fn run_acp_with_cli(cli: &Cli) -> anyhow::Result<()> {
    init_logging(cli)?;
    run_acp_server().await.map_err(Into::into)
}

/// Run the ACP agent
///
/// This is the main entry point for the Claude Code ACP Agent.
/// It sets up the JSON-RPC handler chain and serves requests over stdio.
///
/// For CLI usage with argument parsing, use `run_acp_with_cli()` instead.
pub async fn run_acp() -> Result<(), sacp::Error> {
    // Initialize tracing with default settings
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    run_acp_server().await
}

/// Internal server implementation
///
/// This contains the actual ACP server logic, shared by both `run_acp()` and `run_acp_with_cli()`.
async fn run_acp_server() -> Result<(), sacp::Error> {
    // Check if running in interactive terminal (for debugging)
    let is_tty = atty::is(atty::Stream::Stdin);

    // Print startup banner for easy log identification
    let session_id = uuid::Uuid::new_v4();
    let start_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    tracing::info!("================================================================================");
    tracing::info!("  Claude Code ACP Agent - Session Start");
    tracing::info!("--------------------------------------------------------------------------------");
    tracing::info!("  Version:    {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("  Start Time: {}", start_time);
    tracing::info!("  Session ID: {}", session_id);
    tracing::info!("  PID:        {}", std::process::id());
    tracing::info!("  TTY Mode:   {}", if is_tty { "interactive" } else { "subprocess" });
    tracing::info!("================================================================================");

    if is_tty {
        // Running in interactive terminal - provide helpful message
        eprintln!("Claude Code ACP Agent is running in interactive mode.");
        eprintln!("This agent communicates via ACP protocol over stdin/stdout.");
        eprintln!("To use with an editor, configure it to run this binary.");
        eprintln!("Waiting for ACP protocol messages on stdin...");
        eprintln!("(Press Ctrl+C to exit)");
    } else {
        // Running as subprocess (e.g., from editor) - minimal logging
        tracing::info!("Waiting for ACP protocol messages on stdin...");
    }

    // Create the agent
    let agent = ClaudeAcpAgent::new();
    let config = Arc::new(agent.config().clone());
    let sessions = agent.sessions().clone();

    // Build the handler chain
    AgentToClient::builder()
        .name(agent.name())
        // Handle initialize request
        .on_receive_request(
            {
                let config = config.clone();
                async move |request: InitializeRequest, request_cx, _connection_cx| {
                    tracing::info!(
                        "Received initialize request (protocol version: {:?})",
                        request.protocol_version
                    );
                    let response = handlers::handle_initialize(request, &config);
                    tracing::debug!("Sending initialize response");
                    request_cx.respond(response)
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle session/new request
        .on_receive_request(
            {
                let config = config.clone();
                let sessions = sessions.clone();
                async move |request: NewSessionRequest, request_cx, _connection_cx| {
                    tracing::debug!("Received session/new request");
                    match handlers::handle_new_session(request, &config, &sessions) {
                        Ok(response) => request_cx.respond(response),
                        Err(e) => request_cx.respond_with_error(sacp::util::internal_error(e.to_string())),
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle session/prompt request
        .on_receive_request(
            {
                let config = config.clone();
                let sessions = sessions.clone();
                async move |request: PromptRequest, request_cx, connection_cx| {
                    tracing::debug!(
                        "Received session/prompt request for session {}",
                        request.session_id.0
                    );

                    // Handle the prompt with streaming
                    match handlers::handle_prompt(request, &config, &sessions, connection_cx).await {
                        Ok(response) => request_cx.respond(response),
                        Err(e) => {
                            tracing::error!("Prompt error: {}", e);
                            request_cx.respond_with_error(sacp::util::internal_error(e.to_string()))
                        }
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle session/setMode request
        .on_receive_request(
            {
                let sessions = sessions.clone();
                async move |request: SetSessionModeRequest, request_cx, connection_cx| {
                    tracing::debug!("Received session/setMode request");
                    match handlers::handle_set_mode(request, &sessions, connection_cx).await {
                        Ok(response) => request_cx.respond(response),
                        Err(e) => request_cx.respond_with_error(sacp::util::internal_error(e.to_string())),
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        // Note: SetSessionModel is not yet supported by sacp SDK (JrRequest not implemented)
        // The model selection is returned in NewSessionResponse, but changing it mid-session
        // is not yet available. When sacp adds support, uncomment the following handler.
        // Handle session/cancel notification
        .on_receive_notification(
            {
                let sessions = sessions.clone();
                async move |notification: CancelNotification, _connection_cx| {
                    tracing::debug!(
                        "Received session/cancel notification for session {}",
                        notification.session_id.0
                    );
                    if let Err(e) = handlers::handle_cancel(&notification.session_id.0, &sessions).await {
                        tracing::error!("Cancel error: {}", e);
                    }
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        // Handle unknown messages
        .on_receive_message(
            async move |message: MessageCx, connection_cx: JrConnectionCx<AgentToClient>| {
                tracing::warn!("Received unknown message: {:?}", message.message().method);
                message.respond_with_error(sacp::util::internal_error("Unknown method"), connection_cx)
            },
            sacp::on_receive_message!(),
        )
        // Serve over stdio
        // Note: stdout is used for ACP protocol messages, stderr is for logging
        .serve(ByteStreams::new(
            tokio::io::stdout().compat_write(),
            tokio::io::stdin().compat(),
        ))
        .await
        .map_err(|e| {
            tracing::error!("ACP server error: {}", e);
            e
        })
}
