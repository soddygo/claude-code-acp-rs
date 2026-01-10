//! WebFetch tool for fetching and processing web content
//!
//! Fetches content from URLs and processes it using an AI model.
//! Note: Full implementation requires HTTP client and AI API integration.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for WebFetch
#[derive(Debug, Deserialize)]
struct WebFetchInput {
    /// The URL to fetch content from
    url: String,
    /// The prompt to run on the fetched content
    prompt: String,
}

/// WebFetch tool for fetching and analyzing web content
#[derive(Debug, Default)]
pub struct WebFetchTool;

impl WebFetchTool {
    /// Create a new WebFetch tool
    pub fn new() -> Self {
        Self
    }

    /// Validate URL format
    fn validate_url(url: &str) -> Result<(), String> {
        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("URL must start with http:// or https://".to_string());
        }
        if url.len() < 10 {
            return Err("URL is too short".to_string());
        }
        Ok(())
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        "Fetches content from a specified URL and processes it using an AI model. \
         Takes a URL and a prompt as input, fetches the URL content, converts HTML to markdown, \
         and processes the content with the prompt. Use this tool when you need to retrieve \
         and analyze web content."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url", "prompt"],
            "properties": {
                "url": {
                    "type": "string",
                    "format": "uri",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to run on the fetched content"
                }
            }
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: WebFetchInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate URL
        if let Err(e) = Self::validate_url(&params.url) {
            return ToolResult::error(e);
        }

        // Validate prompt
        if params.prompt.trim().is_empty() {
            return ToolResult::error("Prompt cannot be empty");
        }

        tracing::info!(
            "WebFetch request for URL: {} with prompt: {} (session: {})",
            params.url,
            params.prompt,
            context.session_id
        );

        // Note: Full implementation would:
        // 1. Use reqwest to fetch the URL content
        // 2. Convert HTML to markdown
        // 3. Use AI API to process content with the prompt
        // 4. Return the processed result

        // For now, return a placeholder indicating the tool is available
        // but requires external HTTP client integration
        let output = format!(
            "WebFetch is available but requires HTTP client integration.\n\n\
             Requested URL: {}\n\
             Prompt: {}\n\n\
             To fully implement this tool, add the 'reqwest' crate and configure \
             an AI API for content processing.",
            params.url, params.prompt
        );

        ToolResult::success(output).with_metadata(json!({
            "url": params.url,
            "prompt": params.prompt,
            "status": "stub_implementation"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_web_fetch_properties() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "WebFetch");
        assert!(tool.description().contains("URL"));
        assert!(tool.description().contains("content"));
    }

    #[test]
    fn test_web_fetch_input_schema() {
        let tool = WebFetchTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        assert!(schema["properties"]["prompt"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("url"))
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("prompt"))
        );
    }

    #[test]
    fn test_validate_url() {
        // Valid URLs
        assert!(WebFetchTool::validate_url("https://example.com").is_ok());
        assert!(WebFetchTool::validate_url("http://example.com/path").is_ok());
        assert!(WebFetchTool::validate_url("https://api.example.com/v1/data").is_ok());

        // Invalid URLs
        assert!(WebFetchTool::validate_url("ftp://example.com").is_err());
        assert!(WebFetchTool::validate_url("example.com").is_err());
        assert!(WebFetchTool::validate_url("http://").is_err());
    }

    #[tokio::test]
    async fn test_web_fetch_execute() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebFetchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "url": "https://example.com",
                    "prompt": "Extract the main content"
                }),
                &context,
            )
            .await;

        // Should succeed (stub implementation)
        assert!(!result.is_error);
        assert!(result.content.contains("WebFetch"));
        assert!(result.content.contains("https://example.com"));
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebFetchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "url": "not-a-url",
                    "prompt": "Extract content"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("http"));
    }

    #[tokio::test]
    async fn test_web_fetch_empty_prompt() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebFetchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "url": "https://example.com",
                    "prompt": ""
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("Prompt"));
    }
}
