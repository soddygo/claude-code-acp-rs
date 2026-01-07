//! NotebookEdit tool for editing Jupyter notebooks
//!
//! Edits Jupyter notebook (.ipynb) files by replacing, inserting, or deleting cells.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for NotebookEdit
#[derive(Debug, Deserialize)]
struct NotebookEditInput {
    /// The absolute path to the notebook file
    notebook_path: String,
    /// The new source for the cell
    new_source: String,
    /// The cell number (0-indexed) or cell ID to edit
    #[serde(default)]
    cell_number: Option<usize>,
    /// The cell ID to edit (alternative to cell_number)
    #[serde(default)]
    cell_id: Option<String>,
    /// The type of the cell (code or markdown)
    #[serde(default)]
    cell_type: Option<String>,
    /// The edit mode (replace, insert, delete)
    #[serde(default)]
    edit_mode: Option<String>,
}

/// Jupyter notebook structure
#[derive(Debug, Deserialize, Serialize)]
struct Notebook {
    cells: Vec<NotebookCell>,
    metadata: Value,
    nbformat: u32,
    nbformat_minor: u32,
}

/// Notebook cell structure
#[derive(Debug, Deserialize, Serialize, Clone)]
struct NotebookCell {
    cell_type: String,
    source: Value, // Can be string or array of strings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outputs: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default)]
    metadata: Value,
}

/// NotebookEdit tool for editing Jupyter notebooks
#[derive(Debug, Default)]
pub struct NotebookEditTool;

impl NotebookEditTool {
    /// Create a new NotebookEdit tool
    pub fn new() -> Self {
        Self
    }

    /// Create a new cell with the given source and type
    fn create_cell(source: &str, cell_type: &str, cell_id: Option<String>) -> NotebookCell {
        let lines: Vec<String> = source.lines().map(|l| format!("{}\n", l)).collect();
        let source_value = if lines.len() == 1 {
            Value::String(lines[0].clone())
        } else {
            Value::Array(lines.into_iter().map(Value::String).collect())
        };

        NotebookCell {
            cell_type: cell_type.to_string(),
            source: source_value,
            outputs: Vec::new(),
            execution_count: if cell_type == "code" { Some(0) } else { None },
            id: cell_id.or_else(|| Some(uuid::Uuid::new_v4().to_string())),
            metadata: json!({}),
        }
    }
}

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) \
         with new source. The notebook_path parameter must be an absolute path. The cell_number \
         is 0-indexed. Use edit_mode=insert to add a new cell at the index specified by \
         cell_number. Use edit_mode=delete to delete the cell at the index specified by cell_number."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["notebook_path", "new_source"],
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit"
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_number": {
                    "type": "number",
                    "description": "The 0-indexed cell number to edit"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. When inserting a new cell, the new cell will be inserted after the cell with this ID."
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell. Required when using edit_mode=insert."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make. Defaults to replace."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Parse input
        let params: NotebookEditInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate path
        if !std::path::Path::new(&params.notebook_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ipynb"))
        {
            return ToolResult::error("File must have .ipynb extension");
        }

        // Read the existing notebook
        let content = match fs::read_to_string(&params.notebook_path) {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to read notebook '{}': {}",
                    params.notebook_path, e
                ))
            }
        };

        // Parse as notebook
        let mut notebook: Notebook = match serde_json::from_str(&content) {
            Ok(n) => n,
            Err(e) => {
                return ToolResult::error(format!("Failed to parse notebook: {}", e))
            }
        };

        let edit_mode = params.edit_mode.as_deref().unwrap_or("replace");

        // Find the cell index
        let cell_index = if let Some(idx) = params.cell_number {
            idx
        } else if let Some(ref id) = params.cell_id {
            // Find cell by ID
            notebook
                .cells
                .iter()
                .position(|c| c.id.as_deref() == Some(id))
                .unwrap_or(notebook.cells.len())
        } else {
            // Default to first cell for replace, end for insert
            if edit_mode == "insert" {
                notebook.cells.len()
            } else {
                0
            }
        };

        let cell_type = params.cell_type.as_deref().unwrap_or("code");

        match edit_mode {
            "insert" => {
                // Insert a new cell
                if cell_index > notebook.cells.len() {
                    return ToolResult::error(format!(
                        "Cell index {} is out of bounds (notebook has {} cells)",
                        cell_index,
                        notebook.cells.len()
                    ));
                }

                let new_cell = Self::create_cell(&params.new_source, cell_type, None);
                notebook.cells.insert(cell_index, new_cell);

                // Write back
                let output_json = serde_json::to_string_pretty(&notebook)
                    .map_err(|e| format!("Failed to serialize notebook: {}", e));

                match output_json {
                    Ok(json) => {
                        if let Err(e) = fs::write(&params.notebook_path, json) {
                            return ToolResult::error(format!("Failed to write notebook: {}", e));
                        }
                    }
                    Err(e) => return ToolResult::error(e),
                }

                ToolResult::success(format!(
                    "Inserted new {} cell at index {} in {}",
                    cell_type, cell_index, params.notebook_path
                ))
            }
            "delete" => {
                // Delete a cell
                if cell_index >= notebook.cells.len() {
                    return ToolResult::error(format!(
                        "Cell index {} is out of bounds (notebook has {} cells)",
                        cell_index,
                        notebook.cells.len()
                    ));
                }

                let removed = notebook.cells.remove(cell_index);

                // Write back
                let output_json = serde_json::to_string_pretty(&notebook)
                    .map_err(|e| format!("Failed to serialize notebook: {}", e));

                match output_json {
                    Ok(json) => {
                        if let Err(e) = fs::write(&params.notebook_path, json) {
                            return ToolResult::error(format!("Failed to write notebook: {}", e));
                        }
                    }
                    Err(e) => return ToolResult::error(e),
                }

                ToolResult::success(format!(
                    "Deleted {} cell at index {} from {}",
                    removed.cell_type, cell_index, params.notebook_path
                ))
            }
            _ => {
                // Replace (default)
                if cell_index >= notebook.cells.len() {
                    return ToolResult::error(format!(
                        "Cell index {} is out of bounds (notebook has {} cells)",
                        cell_index,
                        notebook.cells.len()
                    ));
                }

                // Update the cell
                {
                    let cell = &mut notebook.cells[cell_index];

                    // Update cell type if specified
                    if params.cell_type.is_some() {
                        cell.cell_type = cell_type.to_string();
                    }

                    // Update source
                    let lines: Vec<String> =
                        params.new_source.lines().map(|l| format!("{}\n", l)).collect();
                    cell.source = if lines.len() == 1 {
                        Value::String(lines[0].clone())
                    } else {
                        Value::Array(lines.into_iter().map(Value::String).collect())
                    };

                    // Clear outputs for code cells
                    if cell.cell_type == "code" {
                        cell.outputs.clear();
                        cell.execution_count = None;
                    }
                }

                // Get cell type for the success message
                let cell_type_str = notebook.cells[cell_index].cell_type.clone();

                // Write back
                let output_json = serde_json::to_string_pretty(&notebook)
                    .map_err(|e| format!("Failed to serialize notebook: {}", e));

                match output_json {
                    Ok(json) => {
                        if let Err(e) = fs::write(&params.notebook_path, json) {
                            return ToolResult::error(format!("Failed to write notebook: {}", e));
                        }
                    }
                    Err(e) => return ToolResult::error(e),
                }

                ToolResult::success(format!(
                    "Replaced cell {} ({}) in {}",
                    cell_index, cell_type_str, params.notebook_path
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::TempDir;

    fn sample_notebook() -> &'static str {
        r##"{
            "cells": [
                {
                    "cell_type": "markdown",
                    "id": "cell-1",
                    "metadata": {},
                    "source": ["# Test Notebook\n"]
                },
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "id": "cell-2",
                    "metadata": {},
                    "source": ["print('Hello')"],
                    "outputs": []
                }
            ],
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5
        }"##
    }

    #[test]
    fn test_notebook_edit_properties() {
        let tool = NotebookEditTool::new();
        assert_eq!(tool.name(), "NotebookEdit");
        assert!(tool.description().contains("Jupyter"));
        assert!(tool.description().contains("cell"));
    }

    #[test]
    fn test_notebook_edit_input_schema() {
        let tool = NotebookEditTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["notebook_path"].is_object());
        assert!(schema["properties"]["new_source"].is_object());
        assert!(schema["properties"]["cell_number"].is_object());
        assert!(schema["properties"]["edit_mode"].is_object());
    }

    #[tokio::test]
    async fn test_notebook_edit_replace() {
        let temp_dir = TempDir::new().unwrap();
        let notebook_path = temp_dir.path().join("test.ipynb");

        let mut file = fs::File::create(&notebook_path).unwrap();
        write!(file, "{}", sample_notebook()).unwrap();

        let tool = NotebookEditTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "notebook_path": notebook_path.to_str().unwrap(),
                    "new_source": "# Updated Title",
                    "cell_number": 0
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Replaced"));

        // Verify the change
        let content = fs::read_to_string(&notebook_path).unwrap();
        assert!(content.contains("Updated Title"));
    }

    #[tokio::test]
    async fn test_notebook_edit_insert() {
        let temp_dir = TempDir::new().unwrap();
        let notebook_path = temp_dir.path().join("test.ipynb");

        let mut file = fs::File::create(&notebook_path).unwrap();
        write!(file, "{}", sample_notebook()).unwrap();

        let tool = NotebookEditTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "notebook_path": notebook_path.to_str().unwrap(),
                    "new_source": "# New Cell",
                    "cell_number": 1,
                    "cell_type": "markdown",
                    "edit_mode": "insert"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Inserted"));

        // Verify the notebook now has 3 cells
        let content = fs::read_to_string(&notebook_path).unwrap();
        let notebook: Notebook = serde_json::from_str(&content).unwrap();
        assert_eq!(notebook.cells.len(), 3);
    }

    #[tokio::test]
    async fn test_notebook_edit_delete() {
        let temp_dir = TempDir::new().unwrap();
        let notebook_path = temp_dir.path().join("test.ipynb");

        let mut file = fs::File::create(&notebook_path).unwrap();
        write!(file, "{}", sample_notebook()).unwrap();

        let tool = NotebookEditTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "notebook_path": notebook_path.to_str().unwrap(),
                    "new_source": "",
                    "cell_number": 0,
                    "edit_mode": "delete"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Deleted"));

        // Verify the notebook now has 1 cell
        let content = fs::read_to_string(&notebook_path).unwrap();
        let notebook: Notebook = serde_json::from_str(&content).unwrap();
        assert_eq!(notebook.cells.len(), 1);
    }

    #[tokio::test]
    async fn test_notebook_edit_invalid_index() {
        let temp_dir = TempDir::new().unwrap();
        let notebook_path = temp_dir.path().join("test.ipynb");

        let mut file = fs::File::create(&notebook_path).unwrap();
        write!(file, "{}", sample_notebook()).unwrap();

        let tool = NotebookEditTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "notebook_path": notebook_path.to_str().unwrap(),
                    "new_source": "test",
                    "cell_number": 99
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("out of bounds"));
    }
}
