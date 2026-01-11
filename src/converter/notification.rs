//! Claude SDK Message to ACP SessionNotification conversion
//!
//! Converts SDK messages (assistant, system, result, stream events)
//! into ACP session notifications for the client.

use std::time::Instant;

use claude_code_agent_sdk::{
    AssistantMessage, ContentBlock as SdkContentBlock, Message, ResultMessage, StreamEvent,
    ToolResultBlock, ToolResultContent, ToolUseBlock,
};
use dashmap::DashMap;
use sacp::schema::{
    ContentBlock as AcpContentBlock, ContentChunk, Diff, Plan, PlanEntry, PlanEntryPriority,
    PlanEntryStatus, SessionId, SessionNotification, SessionUpdate, Terminal, TextContent,
    ToolCall, ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind as AcpToolKind,
};

use crate::types::{ToolKind, ToolUseEntry};

use super::extract_tool_info;

/// Notification converter for transforming SDK messages to ACP notifications
///
/// Maintains a cache of tool uses to correlate tool_use blocks with their results.
#[derive(Debug)]
pub struct NotificationConverter {
    /// Cache of tool use entries, keyed by tool_use_id
    tool_use_cache: DashMap<String, ToolUseEntry>,
    /// Current working directory for relative path display
    cwd: Option<std::path::PathBuf>,
}

impl Default for NotificationConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationConverter {
    /// Create a new notification converter
    pub fn new() -> Self {
        Self {
            tool_use_cache: DashMap::new(),
            cwd: None,
        }
    }

    /// Create a new notification converter with working directory
    ///
    /// # Arguments
    ///
    /// * `cwd` - The current working directory for computing relative paths
    pub fn with_cwd(cwd: std::path::PathBuf) -> Self {
        Self {
            tool_use_cache: DashMap::new(),
            cwd: Some(cwd),
        }
    }

    /// Convert a SDK Message to ACP session update notifications
    ///
    /// # Arguments
    ///
    /// * `message` - The SDK message to convert
    /// * `session_id` - The session ID for the notifications
    ///
    /// # Returns
    ///
    /// A vector of ACP SessionNotification objects
    pub fn convert_message(&self, message: &Message, session_id: &str) -> Vec<SessionNotification> {
        let start_time = Instant::now();

        // Determine message type for logging
        let message_type = match message {
            Message::Assistant(_) => "Assistant",
            Message::StreamEvent(_) => "StreamEvent",
            Message::Result(_) => "Result",
            Message::System(_) => "System",
            Message::User(_) => "User",
            Message::ControlCancelRequest(_) => "ControlCancelRequest",
        };

        let sid = SessionId::new(session_id.to_string());
        let notifications = match message {
            Message::Assistant(assistant) => self.convert_assistant_message(assistant, &sid),
            Message::StreamEvent(event) => self.convert_stream_event(event, &sid),
            Message::Result(result) => self.convert_result_message(result, &sid),
            Message::System(_) => {
                // System messages are typically internal, not sent as notifications
                vec![]
            }
            Message::User(_) => {
                // User messages are echoed back, usually not needed
                vec![]
            }
            Message::ControlCancelRequest(_) => {
                // Internal control messages
                vec![]
            }
        };

        let elapsed = start_time.elapsed();
        let output_count = notifications.len();

        tracing::trace!(
            message_type = %message_type,
            session_id = %session_id,
            output_count = output_count,
            conversion_duration_us = elapsed.as_micros(),
            "Message conversion completed"
        );

        notifications
    }

    /// Convert an assistant message
    fn convert_assistant_message(
        &self,
        assistant: &AssistantMessage,
        session_id: &SessionId,
    ) -> Vec<SessionNotification> {
        let mut notifications = Vec::new();

        for block in &assistant.message.content {
            match block {
                SdkContentBlock::Text(text) => {
                    notifications.push(self.make_agent_message(session_id, &text.text));
                }
                SdkContentBlock::Thinking(thinking) => {
                    notifications.push(self.make_agent_thought(session_id, &thinking.thinking));
                }
                SdkContentBlock::ToolUse(tool_use) => {
                    // Cache the tool use for later correlation with result
                    self.cache_tool_use(tool_use);
                    notifications.push(self.make_tool_call(session_id, tool_use));
                }
                SdkContentBlock::ToolResult(tool_result) => {
                    notifications.extend(self.make_tool_result(session_id, tool_result));
                }
                SdkContentBlock::Image(_) => {
                    // Images in assistant messages are not typically sent as notifications
                }
            }
        }

        notifications
    }

    /// Convert a stream event (incremental updates)
    #[allow(clippy::unused_self)]
    fn convert_stream_event(
        &self,
        event: &StreamEvent,
        session_id: &SessionId,
    ) -> Vec<SessionNotification> {
        let event_type = event.event.get("type").and_then(|v| v.as_str());

        match event_type {
            Some("content_block_delta") => {
                if let Some(delta) = event.event.get("delta") {
                    // Text delta
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        return vec![self.make_agent_message_chunk(session_id, text)];
                    }
                    // Thinking delta
                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                        return vec![self.make_agent_thought_chunk(session_id, thinking)];
                    }
                }
                vec![]
            }
            Some("content_block_start") => {
                // Could be used to signal start of a new block
                vec![]
            }
            Some("content_block_stop") => {
                // Could be used to signal end of a block
                vec![]
            }
            _ => vec![],
        }
    }

    /// Convert a result message
    fn convert_result_message(
        &self,
        _result: &ResultMessage,
        _session_id: &SessionId,
    ) -> Vec<SessionNotification> {
        // Result messages update usage statistics but don't typically
        // generate notifications (the prompt response handles completion)
        vec![]
    }

    /// Cache a tool use entry
    fn cache_tool_use(&self, tool_use: &ToolUseBlock) {
        let entry = ToolUseEntry::new(
            tool_use.id.clone(),
            tool_use.name.clone(),
            tool_use.input.clone(),
        );
        self.tool_use_cache.insert(tool_use.id.clone(), entry);
    }

    /// Get a cached tool use entry
    pub fn get_tool_use(&self, tool_use_id: &str) -> Option<ToolUseEntry> {
        self.tool_use_cache.get(tool_use_id).map(|r| r.clone())
    }

    /// Remove a cached tool use entry
    pub fn remove_tool_use(&self, tool_use_id: &str) -> Option<ToolUseEntry> {
        self.tool_use_cache.remove(tool_use_id).map(|(_, v)| v)
    }

    /// Clear all cached tool uses
    pub fn clear_cache(&self) {
        self.tool_use_cache.clear();
    }

    // === Notification builders ===

    /// Make an agent message notification (full text as chunk)
    #[allow(clippy::unused_self)]
    fn make_agent_message(&self, session_id: &SessionId, text: &str) -> SessionNotification {
        // Use AgentMessageChunk since there's no AgentMessage variant
        SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(AcpContentBlock::Text(
                TextContent::new(text),
            ))),
        )
    }

    /// Make an agent message chunk notification (incremental)
    #[allow(clippy::unused_self)]
    fn make_agent_message_chunk(&self, session_id: &SessionId, chunk: &str) -> SessionNotification {
        SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(AcpContentBlock::Text(
                TextContent::new(chunk),
            ))),
        )
    }

    /// Make an agent thought notification (full thought as chunk)
    #[allow(clippy::unused_self)]
    fn make_agent_thought(&self, session_id: &SessionId, thought: &str) -> SessionNotification {
        // Use AgentThoughtChunk since there's no separate thought variant
        SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(AcpContentBlock::Text(
                TextContent::new(thought),
            ))),
        )
    }

    /// Make an agent thought chunk notification (incremental)
    #[allow(clippy::unused_self)]
    fn make_agent_thought_chunk(&self, session_id: &SessionId, chunk: &str) -> SessionNotification {
        SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(AcpContentBlock::Text(
                TextContent::new(chunk),
            ))),
        )
    }

    /// Map local ToolKind to ACP ToolKind
    fn map_tool_kind(kind: ToolKind) -> AcpToolKind {
        match kind {
            ToolKind::Read => AcpToolKind::Read,
            ToolKind::Edit => AcpToolKind::Edit,
            ToolKind::Execute => AcpToolKind::Execute,
            ToolKind::Search => AcpToolKind::Search,
            ToolKind::Think => AcpToolKind::Think,
            ToolKind::Fetch => AcpToolKind::Fetch,
            ToolKind::SwitchMode | ToolKind::Other => AcpToolKind::default(),
        }
    }

    /// Make a tool call notification
    #[allow(clippy::unused_self)]
    fn make_tool_call(
        &self,
        session_id: &SessionId,
        tool_use: &ToolUseBlock,
    ) -> SessionNotification {
        let tool_info = extract_tool_info(&tool_use.name, &tool_use.input, self.cwd.as_ref());

        let tool_call_id = ToolCallId::new(tool_use.id.clone());
        let tool_kind = Self::map_tool_kind(tool_info.kind);

        // For Bash tool, include command in title if description is not available
        let title = if tool_use.name == "Bash" {
            // Get description or command
            let description = tool_use.input.get("description").and_then(|v| v.as_str());
            let command = tool_use.input.get("command").and_then(|v| v.as_str());

            match (description, command) {
                (Some(desc), _) => desc.to_string(),
                (None, Some(cmd)) => {
                    // Truncate long commands for display
                    if cmd.len() > 80 {
                        format!("{}...", &cmd[..77])
                    } else {
                        cmd.to_string()
                    }
                }
                _ => tool_info.title.clone(),
            }
        } else {
            tool_info.title.clone()
        };

        let mut tool_call = ToolCall::new(tool_call_id, &title)
            .kind(tool_kind)
            .status(ToolCallStatus::InProgress) // Tool is being executed
            .raw_input(tool_use.input.clone());

        // Add locations if present
        if let Some(ref locations) = tool_info.locations
            && !locations.is_empty()
        {
            let acp_locations: Vec<ToolCallLocation> = locations
                .iter()
                .map(|loc| {
                    let mut location = ToolCallLocation::new(&loc.path);
                    if let Some(line) = loc.line {
                        location = location.line(line);
                    }
                    location
                })
                .collect();
            tool_call = tool_call.locations(acp_locations);
        }

        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCall(tool_call))
    }

    /// Make tool result notifications
    ///
    /// Returns a vector of notifications:
    /// - ToolCallUpdate for all tools
    /// - Plan notification for TodoWrite tool (when successful)
    fn make_tool_result(
        &self,
        session_id: &SessionId,
        tool_result: &ToolResultBlock,
    ) -> Vec<SessionNotification> {
        let Some(entry) = self.get_tool_use(&tool_result.tool_use_id) else {
            return vec![];
        };

        let output = match &tool_result.content {
            Some(ToolResultContent::Text(text)) => text.clone(),
            Some(ToolResultContent::Blocks(blocks)) => {
                serde_json::to_string(blocks).unwrap_or_default()
            }
            None => String::new(),
        };

        let is_error = tool_result.is_error.unwrap_or(false);
        let status = if is_error {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Completed
        };

        // Build raw_output JSON
        let raw_output = serde_json::json!({
            "content": output,
            "is_error": is_error
        });

        // Build content based on tool type
        let content = self.build_tool_result_content(&entry, &output, is_error);

        let tool_call_id = ToolCallId::new(tool_result.tool_use_id.clone());
        let update_fields = ToolCallUpdateFields::new()
            .status(status)
            .content(content)
            .raw_output(raw_output);
        let update = ToolCallUpdate::new(tool_call_id, update_fields);

        let mut notifications = vec![SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(update),
        )];

        // For TodoWrite, also send a Plan notification
        if entry.name == "TodoWrite" && !is_error {
            if let Some(plan_notification) = self.build_plan_notification(session_id, &entry) {
                notifications.push(plan_notification);
            }
        }

        notifications
    }

    /// Build a Plan notification from TodoWrite input
    fn build_plan_notification(
        &self,
        session_id: &SessionId,
        entry: &ToolUseEntry,
    ) -> Option<SessionNotification> {
        // Extract todos from input
        let todos = entry.input.get("todos")?.as_array()?;

        let plan_entries: Vec<PlanEntry> = todos
            .iter()
            .filter_map(|todo| {
                let content = todo.get("content")?.as_str()?;
                let status_str = todo.get("status")?.as_str()?;

                // Convert TodoWrite status to PlanEntryStatus
                let status = match status_str {
                    "in_progress" => PlanEntryStatus::InProgress,
                    "completed" => PlanEntryStatus::Completed,
                    _ => PlanEntryStatus::Pending,
                };

                // TodoWrite doesn't have priority, default to Medium
                Some(PlanEntry::new(content, PlanEntryPriority::Medium, status))
            })
            .collect();

        if plan_entries.is_empty() {
            return None;
        }

        let plan = Plan::new(plan_entries);
        Some(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::Plan(plan),
        ))
    }

    /// Build tool result content based on tool type
    ///
    /// For Edit/Write tools, returns Diff content.
    /// For other tools, returns text content.
    fn build_tool_result_content(
        &self,
        entry: &ToolUseEntry,
        output: &str,
        is_error: bool,
    ) -> Vec<ToolCallContent> {
        match entry.name.as_str() {
            "Edit" if !is_error => {
                // Extract file_path, old_string, new_string from input
                let file_path = entry
                    .input
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let old_string = entry
                    .input
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let new_string = entry
                    .input
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if !file_path.is_empty() && !new_string.is_empty() {
                    let diff = Diff::new(file_path, new_string).old_text(old_string);
                    vec![ToolCallContent::Diff(diff)]
                } else {
                    vec![output.to_string().into()]
                }
            }
            "Write" if !is_error => {
                // Extract file_path and content from input
                let file_path = entry
                    .input
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = entry
                    .input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if file_path.is_empty() {
                    vec![output.to_string().into()]
                } else {
                    // For Write, old_text is None (new file) or we don't have it
                    let diff = Diff::new(file_path, content);
                    vec![ToolCallContent::Diff(diff)]
                }
            }
            _ => {
                // Default: text content
                vec![output.to_string().into()]
            }
        }
    }

    /// Build Terminal content for embedding a terminal in tool result
    ///
    /// This is used when a tool (like Bash) uses the Terminal API to execute commands.
    /// The terminal_id is obtained from the `terminal/create` response.
    ///
    /// # Arguments
    ///
    /// * `terminal_id` - The terminal ID from CreateTerminalResponse
    ///
    /// # Returns
    ///
    /// A `ToolCallContent::Terminal` that can be included in tool results
    pub fn build_terminal_content(terminal_id: impl Into<String>) -> ToolCallContent {
        let terminal = Terminal::new(terminal_id.into());
        ToolCallContent::Terminal(terminal)
    }

    /// Build a ToolCallUpdate notification with Terminal content
    ///
    /// This is used when a Bash command is executed via the Terminal API.
    /// The client will embed the terminal output based on the terminal_id.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    /// * `tool_use_id` - The tool use ID for the Bash call
    /// * `terminal_id` - The terminal ID from terminal/create
    /// * `status` - The tool call status
    ///
    /// # Returns
    ///
    /// A SessionNotification with the ToolCallUpdate
    pub fn make_terminal_result(
        &self,
        session_id: &SessionId,
        tool_use_id: &str,
        terminal_id: impl Into<String>,
        status: ToolCallStatus,
    ) -> SessionNotification {
        let terminal_content = Self::build_terminal_content(terminal_id);
        let tool_call_id = ToolCallId::new(tool_use_id.to_string());
        let update_fields = ToolCallUpdateFields::new()
            .status(status)
            .content(vec![terminal_content]);
        let update = ToolCallUpdate::new(tool_call_id, update_fields);

        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCallUpdate(update))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_converter_new() {
        let converter = NotificationConverter::new();
        assert!(converter.tool_use_cache.is_empty());
    }

    #[test]
    fn test_cache_tool_use() {
        let converter = NotificationConverter::new();
        let tool_use = ToolUseBlock {
            id: "tool_123".to_string(),
            name: "Read".to_string(),
            input: json!({"file_path": "/test.txt"}),
        };

        converter.cache_tool_use(&tool_use);

        let cached = converter.get_tool_use("tool_123");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "Read");
    }

    #[test]
    fn test_make_agent_message() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");
        let notification = converter.make_agent_message(&session_id, "Hello!");

        assert_eq!(notification.session_id.0.as_ref(), "session-1");
        // Check that it's an AgentMessageChunk update
        assert!(matches!(
            notification.update,
            SessionUpdate::AgentMessageChunk(_)
        ));
    }

    #[test]
    fn test_make_agent_message_chunk() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");
        let notification = converter.make_agent_message_chunk(&session_id, "chunk");

        // Check that it's an AgentMessageChunk update
        assert!(matches!(
            notification.update,
            SessionUpdate::AgentMessageChunk(_)
        ));
    }

    #[test]
    fn test_make_agent_thought() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");
        let notification = converter.make_agent_thought(&session_id, "thinking...");

        // Check that it's an AgentThoughtChunk update
        assert!(matches!(
            notification.update,
            SessionUpdate::AgentThoughtChunk(_)
        ));
    }

    #[test]
    fn test_make_tool_call() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");
        let tool_use = ToolUseBlock {
            id: "tool_456".to_string(),
            name: "Bash".to_string(),
            input: json!({"command": "ls", "description": "List files"}),
        };

        let notification = converter.make_tool_call(&session_id, &tool_use);

        // Check that it's a ToolCall update
        assert!(matches!(notification.update, SessionUpdate::ToolCall(_)));
        if let SessionUpdate::ToolCall(tool_call) = &notification.update {
            assert_eq!(tool_call.tool_call_id.0.as_ref(), "tool_456");
        }
    }

    #[test]
    fn test_remove_tool_use() {
        let converter = NotificationConverter::new();
        let tool_use = ToolUseBlock {
            id: "tool_789".to_string(),
            name: "Edit".to_string(),
            input: json!({}),
        };

        converter.cache_tool_use(&tool_use);
        assert!(converter.get_tool_use("tool_789").is_some());

        let removed = converter.remove_tool_use("tool_789");
        assert!(removed.is_some());
        assert!(converter.get_tool_use("tool_789").is_none());
    }

    #[test]
    fn test_map_tool_kind() {
        assert!(matches!(
            NotificationConverter::map_tool_kind(ToolKind::Read),
            AcpToolKind::Read
        ));
        assert!(matches!(
            NotificationConverter::map_tool_kind(ToolKind::Edit),
            AcpToolKind::Edit
        ));
        assert!(matches!(
            NotificationConverter::map_tool_kind(ToolKind::Execute),
            AcpToolKind::Execute
        ));
    }

    #[test]
    fn test_build_plan_notification() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");

        // Cache a TodoWrite tool use
        let tool_use = ToolUseBlock {
            id: "todo_123".to_string(),
            name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {
                        "content": "Implement feature",
                        "status": "in_progress",
                        "activeForm": "Implementing feature"
                    },
                    {
                        "content": "Write tests",
                        "status": "pending",
                        "activeForm": "Writing tests"
                    },
                    {
                        "content": "Setup project",
                        "status": "completed",
                        "activeForm": "Setting up project"
                    }
                ]
            }),
        };
        converter.cache_tool_use(&tool_use);

        let entry = converter.get_tool_use("todo_123").unwrap();
        let notification = converter.build_plan_notification(&session_id, &entry);

        assert!(notification.is_some());
        let notification = notification.unwrap();

        if let SessionUpdate::Plan(plan) = &notification.update {
            assert_eq!(plan.entries.len(), 3);
            assert_eq!(plan.entries[0].content, "Implement feature");
            assert_eq!(plan.entries[0].status, PlanEntryStatus::InProgress);
            assert_eq!(plan.entries[1].content, "Write tests");
            assert_eq!(plan.entries[1].status, PlanEntryStatus::Pending);
            assert_eq!(plan.entries[2].content, "Setup project");
            assert_eq!(plan.entries[2].status, PlanEntryStatus::Completed);
        } else {
            panic!("Expected Plan update");
        }
    }

    #[test]
    fn test_make_tool_result_todowrite_includes_plan() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");

        // Cache a TodoWrite tool use
        let tool_use = ToolUseBlock {
            id: "todo_456".to_string(),
            name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {
                        "content": "Task 1",
                        "status": "pending",
                        "activeForm": "Doing task 1"
                    }
                ]
            }),
        };
        converter.cache_tool_use(&tool_use);

        // Create a tool result
        let tool_result = ToolResultBlock {
            tool_use_id: "todo_456".to_string(),
            content: Some(ToolResultContent::Text("Todos updated".to_string())),
            is_error: Some(false),
        };

        let notifications = converter.make_tool_result(&session_id, &tool_result);

        // Should have 2 notifications: ToolCallUpdate and Plan
        assert_eq!(notifications.len(), 2);
        assert!(matches!(
            notifications[0].update,
            SessionUpdate::ToolCallUpdate(_)
        ));
        assert!(matches!(notifications[1].update, SessionUpdate::Plan(_)));
    }

    #[test]
    fn test_build_terminal_content() {
        let content = NotificationConverter::build_terminal_content("term-123");
        match content {
            ToolCallContent::Terminal(terminal) => {
                assert_eq!(terminal.terminal_id.0.as_ref(), "term-123");
            }
            _ => panic!("Expected Terminal content"),
        }
    }

    #[test]
    fn test_make_terminal_result() {
        let converter = NotificationConverter::new();
        let session_id = SessionId::new("session-1");

        let notification = converter.make_terminal_result(
            &session_id,
            "tool_789",
            "term-456",
            ToolCallStatus::Completed,
        );

        assert_eq!(notification.session_id.0.as_ref(), "session-1");
        if let SessionUpdate::ToolCallUpdate(update) = &notification.update {
            assert_eq!(update.tool_call_id.0.as_ref(), "tool_789");
            // Check content contains Terminal
            let fields = &update.fields;
            let content = fields.content.as_ref().expect("content should exist");
            assert_eq!(content.len(), 1);
            match &content[0] {
                ToolCallContent::Terminal(terminal) => {
                    assert_eq!(terminal.terminal_id.0.as_ref(), "term-456");
                }
                _ => panic!("Expected Terminal content"),
            }
        } else {
            panic!("Expected ToolCallUpdate");
        }
    }
}
