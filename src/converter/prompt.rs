//! ACP Prompt content to Claude SDK content conversion
//!
//! Converts ACP `PromptRequest` content to Claude SDK `UserContentBlock`s.

use claude_code_agent_sdk::UserContentBlock;

/// Prompt content converter
///
/// Handles conversion from ACP prompt content types to Claude SDK content blocks.
#[derive(Debug, Default)]
pub struct PromptConverter;

impl PromptConverter {
    /// Create a new prompt converter
    pub fn new() -> Self {
        Self
    }

    /// Convert ACP prompt content to SDK user content blocks
    ///
    /// # Arguments
    ///
    /// * `content` - The ACP prompt content as JSON array
    ///
    /// # Returns
    ///
    /// A vector of `UserContentBlock`s for the SDK
    pub fn convert_content(&self, content: &[serde_json::Value]) -> Vec<UserContentBlock> {
        content
            .iter()
            .filter_map(|item| self.convert_content_item(item))
            .collect()
    }

    /// Convert a single ACP content item to SDK content block
    #[allow(clippy::unused_self)]
    fn convert_content_item(&self, item: &serde_json::Value) -> Option<UserContentBlock> {
        let content_type = item.get("type")?.as_str()?;

        match content_type {
            "text" => Self::convert_text(item),
            "image" => Self::convert_image(item),
            "resource" => Self::convert_resource(item),
            "resource_link" => Self::convert_resource_link(item),
            "audio" => {
                // Audio is not supported (consistent with TS implementation)
                // The TypeScript reference implementation explicitly ignores audio content blocks
                tracing::debug!(
                    "Audio content blocks are not supported (consistent with TS implementation)"
                );
                None
            }
            _ => {
                tracing::warn!("Unknown content type: {}", content_type);
                None
            }
        }
    }

    /// Convert text content
    fn convert_text(item: &serde_json::Value) -> Option<UserContentBlock> {
        let text = item.get("text")?.as_str()?;
        Some(UserContentBlock::text(text))
    }

    /// Convert image content
    fn convert_image(item: &serde_json::Value) -> Option<UserContentBlock> {
        let source = item.get("source")?;
        let source_type = source.get("type")?.as_str()?;

        match source_type {
            "base64" => {
                let media_type = source.get("media_type")?.as_str()?;
                let data = source.get("data")?.as_str()?;
                UserContentBlock::image_base64(media_type, data).ok()
            }
            "url" => {
                let url = source.get("url")?.as_str()?;
                Some(UserContentBlock::image_url(url))
            }
            _ => {
                tracing::warn!("Unknown image source type: {}", source_type);
                None
            }
        }
    }

    /// Convert resource content (embedded file content)
    fn convert_resource(item: &serde_json::Value) -> Option<UserContentBlock> {
        let resource = item.get("resource")?;
        let uri = resource.get("uri")?.as_str().unwrap_or("");
        let text = resource.get("text")?.as_str()?;

        // Wrap resource content in context tags
        let formatted = format!("<context uri=\"{uri}\">\n{text}\n</context>");
        Some(UserContentBlock::text(formatted))
    }

    /// Convert resource link (file reference)
    fn convert_resource_link(item: &serde_json::Value) -> Option<UserContentBlock> {
        let uri = item.get("uri")?.as_str()?;
        let title = item.get("title").and_then(|t| t.as_str()).unwrap_or(uri);

        // Format as markdown link
        let formatted = format_uri_link(uri, title);
        Some(UserContentBlock::text(formatted))
    }
}

/// Format a URI as a markdown link
fn format_uri_link(uri: &str, title: &str) -> String {
    if uri.starts_with("file://") {
        // Convert file:// URI to path
        let path = uri.strip_prefix("file://").unwrap_or(uri);
        format!("[{title}]({path})")
    } else if uri.starts_with("zed://") {
        // Zed-specific URI
        format!("[{title}]({uri})")
    } else {
        // Generic URI
        format!("[{title}]({uri})")
    }
}

/// Convert a simple text prompt to content blocks
#[allow(dead_code)] // Will be used in Phase 2
pub fn text_to_content(text: &str) -> Vec<UserContentBlock> {
    vec![UserContentBlock::text(text)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_text_content() {
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "text",
            "text": "Hello, world!"
        })];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_image_base64() {
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": "iVBORw0KGgo="
            }
        })];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_image_url() {
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "image",
            "source": {
                "type": "url",
                "url": "https://example.com/image.png"
            }
        })];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_resource() {
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "resource",
            "resource": {
                "uri": "file:///path/to/file.txt",
                "text": "File content here"
            }
        })];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_resource_link() {
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "resource_link",
            "uri": "file:///path/to/file.txt",
            "title": "file.txt"
        })];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_mixed_content() {
        let converter = PromptConverter::new();
        let content = vec![
            json!({"type": "text", "text": "Look at this:"}),
            json!({
                "type": "image",
                "source": {"type": "url", "url": "https://example.com/img.png"}
            }),
            json!({"type": "text", "text": "What do you see?"}),
        ];

        let result = converter.convert_content(&content);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_format_uri_link() {
        assert_eq!(
            format_uri_link("file:///path/to/file.txt", "file.txt"),
            "[file.txt](/path/to/file.txt)"
        );
        assert_eq!(
            format_uri_link("https://example.com", "Example"),
            "[Example](https://example.com)"
        );
    }

    #[test]
    fn test_text_to_content() {
        let result = text_to_content("Hello");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_audio_explicitly_ignored() {
        // Audio content blocks should be silently ignored (consistent with TS implementation)
        let converter = PromptConverter::new();
        let content = vec![json!({
            "type": "audio",
            "data": "base64_audio_data",
            "mimeType": "audio/mp3"
        })];

        let result = converter.convert_content(&content);
        assert_eq!(
            result.len(),
            0,
            "Audio should be ignored and not produce any content blocks"
        );
    }

    #[test]
    fn test_convert_audio_with_other_content() {
        // Audio should be ignored while other content types are processed
        let converter = PromptConverter::new();
        let content = vec![
            json!({"type": "text", "text": "Hello"}),
            json!({"type": "audio", "data": "base64_audio_data", "mimeType": "audio/mp3"}),
            json!({"type": "text", "text": "World"}),
        ];

        let result = converter.convert_content(&content);
        assert_eq!(
            result.len(),
            2,
            "Only text content should be present, audio ignored"
        );
    }
}
