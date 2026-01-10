//! WebSearch tool for searching the web
//!
//! Searches the web and returns results to inform responses.
//! Note: Full implementation requires external search API integration.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for WebSearch
#[derive(Debug, Deserialize)]
struct WebSearchInput {
    /// The search query
    query: String,
    /// Domain filter - only include results from these domains
    #[serde(default)]
    allowed_domains: Option<Vec<String>>,
    /// Domain filter - exclude results from these domains
    #[serde(default)]
    blocked_domains: Option<Vec<String>>,
}

/// WebSearch tool for searching the web
#[derive(Debug, Default)]
pub struct WebSearchTool;

impl WebSearchTool {
    /// Create a new WebSearch tool
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Searches the web and uses the results to inform responses. \
         Provides up-to-date information for current events and recent data. \
         Use this tool for accessing information beyond the model's knowledge cutoff. \
         Returns search result information including links as markdown hyperlinks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "minLength": 2,
                    "description": "The search query to use"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Only include search results from these domains"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Never include search results from these domains"
                }
            }
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: WebSearchInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate query
        if params.query.len() < 2 {
            return ToolResult::error("Search query must be at least 2 characters");
        }

        tracing::info!(
            "WebSearch request for query: {} (session: {})",
            params.query,
            context.session_id
        );

        // Note: Full implementation would:
        // 1. Call an external search API (Google, Bing, etc.)
        // 2. Filter results by allowed/blocked domains
        // 3. Format results as markdown with hyperlinks
        // 4. Return structured search results

        // For now, return a placeholder indicating the tool is available
        // but requires external search API integration
        let mut output = format!(
            "WebSearch is available but requires search API integration.\n\n\
             Search query: {}\n",
            params.query
        );

        if let Some(ref allowed) = params.allowed_domains {
            output.push_str(&format!("Allowed domains: {}\n", allowed.join(", ")));
        }
        if let Some(ref blocked) = params.blocked_domains {
            output.push_str(&format!("Blocked domains: {}\n", blocked.join(", ")));
        }

        output.push_str(
            "\nTo fully implement this tool, integrate with a search API \
             (e.g., Google Custom Search, Bing Search API, or SerpAPI).",
        );

        ToolResult::success(output).with_metadata(json!({
            "query": params.query,
            "allowed_domains": params.allowed_domains,
            "blocked_domains": params.blocked_domains,
            "status": "stub_implementation"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_web_search_properties() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "WebSearch");
        assert!(tool.description().contains("search"));
        assert!(tool.description().contains("web"));
    }

    #[test]
    fn test_web_search_input_schema() {
        let tool = WebSearchTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["allowed_domains"].is_object());
        assert!(schema["properties"]["blocked_domains"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("query"))
        );
    }

    #[tokio::test]
    async fn test_web_search_execute() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebSearchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "query": "Rust programming language"
                }),
                &context,
            )
            .await;

        // Should succeed (stub implementation)
        assert!(!result.is_error);
        assert!(result.content.contains("WebSearch"));
        assert!(result.content.contains("Rust programming language"));
    }

    #[tokio::test]
    async fn test_web_search_with_domains() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebSearchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "query": "Rust docs",
                    "allowed_domains": ["doc.rust-lang.org", "docs.rs"],
                    "blocked_domains": ["stackoverflow.com"]
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("doc.rust-lang.org"));
        assert!(result.content.contains("stackoverflow.com"));
    }

    #[tokio::test]
    async fn test_web_search_short_query() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WebSearchTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "query": "a"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("2 characters"));
    }
}
