# claude-code-acp-rs

[![Crates.io](https://img.shields.io/crates/v/claude-code-acp-rs.svg)](https://crates.io/crates/claude-code-acp-rs)
[![Docs.rs](https://docs.rs/claude-code-acp-rs/badge.svg)](https://docs.rs/claude-code-acp-rs)
[![CI](https://img.shields.io/github/actions/workflow/status/soddygo/claude-code-acp-rs/ci.yml?branch=main)](https://github.com/soddygo/claude-code-acp-rs/actions)
[![Coverage Status](https://coveralls.io/repos/github/soddygo/claude-code-acp-rs/badge.svg?branch=main)](https://coveralls.io/github/soddygo/claude-code-acp-rs?branch=main)

A Rust implementation of Claude Code ACP Agent. Use Claude Code from any ACP-compatible client such as Zed!

This is an alternative to the official [TypeScript implementation](https://github.com/zed-industries/claude-code-acp) (`@zed-industries/claude-code-acp`).

## Installation

### From Cargo

```bash
# Install the rust toolchain first: https://www.rust-lang.org/tools/install
cargo install claude-code-acp-rs
```

### From Source

```bash
git clone https://github.com/soddygo/claude-code-acp-rs.git
cd claude-code-acp-rs
cargo install --path .
```

### With OpenTelemetry Support

To enable distributed tracing with OpenTelemetry:

```bash
# Install with otel feature
cargo install claude-code-acp-rs --features otel

# Or from source
cargo install --path . --features otel
```

## Usage

### Command Line

```bash
# Show help
claude-code-acp-rs --help

# Run with diagnostic mode (logs to file)
claude-code-acp-rs --diagnostic

# Run with verbose logging
claude-code-acp-rs -vv
```

### With Zed Editor

Configure Zed to use this agent by specifying `claude-code-acp-rs` as the agent command.

### Environment Variables

- `ANTHROPIC_BASE_URL`: Custom API base URL
- `ANTHROPIC_AUTH_TOKEN`: Authentication token
- `ANTHROPIC_MODEL`: Model to use (default: claude-sonnet-4-20250514)
- `ANTHROPIC_SMALL_FAST_MODEL`: Model for fast operations

## OpenTelemetry Tracing

When compiled with the `otel` feature, you can enable distributed tracing to debug and monitor the agent:

```bash
# Send traces to Jaeger (default OTLP endpoint)
claude-code-acp-rs --otel-endpoint http://localhost:4317

# With custom service name
claude-code-acp-rs --otel-endpoint http://localhost:4317 --otel-service-name my-claude-agent

# Or use environment variable
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 claude-code-acp-rs
```

### Jaeger Quick Start

```bash
# Start Jaeger with Docker
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/jaeger:latest

# Run the agent with tracing
claude-code-acp-rs --otel-endpoint http://localhost:4317

# View traces at http://localhost:16686
```

## Coexistence with npm Version

This Rust implementation uses the command name `claude-code-acp-rs` to avoid conflicts with the npm package `@zed-industries/claude-code-acp` (which uses `claude-code-acp`).

Both versions can be installed and used on the same system:
- `claude-code-acp` → npm version (TypeScript)
- `claude-code-acp-rs` → Rust version (this project)

## License

* [MIT LICENSE](LICENSE)

## Contribution

[CONTRIBUTING.md](CONTRIBUTING.md)
