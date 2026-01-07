//! NotebookRead tool for reading Jupyter notebooks
//!
//! Reads Jupyter notebook (.ipynb) files and returns all cells with their outputs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for NotebookRead
#[derive(Debug, Deserialize)]
struct NotebookReadInput {
    /// The absolute path to the notebook file
    notebook_path: String,
}

/// Jupyter notebook structure
#[derive(Debug, Deserialize)]
struct Notebook {
    cells: Vec<NotebookCell>,
    #[serde(default)]
    #[allow(dead_code)]
    metadata: Value,
    #[serde(default)]
    nbformat: u32,
    #[serde(default)]
    nbformat_minor: u32,
}

/// Notebook cell structure
#[derive(Debug, Deserialize, Serialize)]
struct NotebookCell {
    cell_type: String,
    source: CellSource,
    #[serde(default)]
    outputs: Vec<CellOutput>,
    #[serde(default)]
    execution_count: Option<u32>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    metadata: Value,
}

/// Cell source can be a string or array of strings
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum CellSource {
    String(String),
    Lines(Vec<String>),
}

impl CellSource {
    fn as_string(&self) -> String {
        match self {
            CellSource::String(s) => s.clone(),
            CellSource::Lines(lines) => lines.join(""),
        }
    }
}

/// Cell output structure
#[derive(Debug, Deserialize, Serialize)]
struct CellOutput {
    output_type: String,
    #[serde(default)]
    text: Option<CellSource>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ename: Option<String>,
    #[serde(default)]
    evalue: Option<String>,
    #[serde(default)]
    traceback: Option<Vec<String>>,
}

/// NotebookRead tool for reading Jupyter notebooks
#[derive(Debug, Default)]
pub struct NotebookReadTool;

impl NotebookReadTool {
    /// Create a new NotebookRead tool
    pub fn new() -> Self {
        Self
    }

    /// Format a notebook for display
    fn format_notebook(notebook: &Notebook) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "Jupyter Notebook (format {}.{})\n",
            notebook.nbformat, notebook.nbformat_minor
        ));
        output.push_str(&format!("Total cells: {}\n\n", notebook.cells.len()));

        for (i, cell) in notebook.cells.iter().enumerate() {
            // Cell header
            output.push_str(&format!(
                "--- Cell {} ({}) ---\n",
                i + 1,
                cell.cell_type
            ));

            if let Some(id) = &cell.id {
                output.push_str(&format!("ID: {}\n", id));
            }

            if let Some(exec) = cell.execution_count {
                output.push_str(&format!("Execution count: {}\n", exec));
            }

            output.push('\n');

            // Cell source
            let source = cell.source.as_string();
            if cell.cell_type == "code" {
                output.push_str("```\n");
                output.push_str(&source);
                if !source.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```\n");
            } else {
                output.push_str(&source);
                if !source.ends_with('\n') {
                    output.push('\n');
                }
            }

            // Cell outputs
            if !cell.outputs.is_empty() {
                output.push_str("\nOutput:\n");
                for cell_output in &cell.outputs {
                    match cell_output.output_type.as_str() {
                        "stream" => {
                            if let Some(text) = &cell_output.text {
                                output.push_str(&text.as_string());
                            }
                        }
                        "execute_result" | "display_data" => {
                            if let Some(data) = &cell_output.data {
                                if let Some(text) = data.get("text/plain") {
                                    if let Some(lines) = text.as_array() {
                                        for line in lines {
                                            if let Some(s) = line.as_str() {
                                                output.push_str(s);
                                            }
                                        }
                                    } else if let Some(s) = text.as_str() {
                                        output.push_str(s);
                                    }
                                }
                            }
                        }
                        "error" => {
                            if let Some(ename) = &cell_output.ename {
                                output.push_str(&format!("Error: {} ", ename));
                            }
                            if let Some(evalue) = &cell_output.evalue {
                                output.push_str(evalue);
                            }
                            output.push('\n');
                            if let Some(traceback) = &cell_output.traceback {
                                for line in traceback {
                                    // Strip ANSI escape codes
                                    let clean_line = strip_ansi_codes(line);
                                    output.push_str(&clean_line);
                                    output.push('\n');
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            output.push('\n');
        }

        output
    }
}

/// Strip ANSI escape codes from a string
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[async_trait]
impl Tool for NotebookReadTool {
    fn name(&self) -> &str {
        "NotebookRead"
    }

    fn description(&self) -> &str {
        "Reads Jupyter notebooks (.ipynb files) and returns all cells with their outputs, \
         combining code, text, and visualizations. The notebook_path parameter must be \
         an absolute path, not a relative path."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["notebook_path"],
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to read"
                }
            }
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Parse input
        let params: NotebookReadInput = match serde_json::from_value(input) {
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

        // Read the file
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
        let notebook: Notebook = match serde_json::from_str(&content) {
            Ok(n) => n,
            Err(e) => {
                return ToolResult::error(format!("Failed to parse notebook: {}", e))
            }
        };

        // Format for display
        let output = Self::format_notebook(&notebook);

        ToolResult::success(output).with_metadata(json!({
            "path": params.notebook_path,
            "cell_count": notebook.cells.len(),
            "nbformat": notebook.nbformat,
            "nbformat_minor": notebook.nbformat_minor
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn sample_notebook() -> &'static str {
        r##"{
            "cells": [
                {
                    "cell_type": "markdown",
                    "id": "cell-1",
                    "metadata": {},
                    "source": ["# Test Notebook\n", "This is a test."]
                },
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "id": "cell-2",
                    "metadata": {},
                    "source": "print('Hello, World!')",
                    "outputs": [
                        {
                            "output_type": "stream",
                            "name": "stdout",
                            "text": ["Hello, World!\n"]
                        }
                    ]
                }
            ],
            "metadata": {
                "kernelspec": {
                    "display_name": "Python 3",
                    "language": "python",
                    "name": "python3"
                }
            },
            "nbformat": 4,
            "nbformat_minor": 5
        }"##
    }

    #[test]
    fn test_notebook_read_properties() {
        let tool = NotebookReadTool::new();
        assert_eq!(tool.name(), "NotebookRead");
        assert!(tool.description().contains("Jupyter"));
        assert!(tool.description().contains(".ipynb"));
    }

    #[test]
    fn test_notebook_read_input_schema() {
        let tool = NotebookReadTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["notebook_path"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("notebook_path")));
    }

    #[tokio::test]
    async fn test_notebook_read_execute() {
        let temp_dir = TempDir::new().unwrap();
        let notebook_path = temp_dir.path().join("test.ipynb");

        let mut file = fs::File::create(&notebook_path).unwrap();
        write!(file, "{}", sample_notebook()).unwrap();

        let tool = NotebookReadTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({"notebook_path": notebook_path.to_str().unwrap()}),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Test Notebook"));
        assert!(result.content.contains("Hello, World!"));
        assert!(result.content.contains("markdown"));
        assert!(result.content.contains("code"));
    }

    #[tokio::test]
    async fn test_notebook_read_invalid_extension() {
        let temp_dir = TempDir::new().unwrap();
        let tool = NotebookReadTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(json!({"notebook_path": "/tmp/test.py"}), &context)
            .await;

        assert!(result.is_error);
        assert!(result.content.contains(".ipynb"));
    }

    #[tokio::test]
    async fn test_notebook_read_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let tool = NotebookReadTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({"notebook_path": "/tmp/nonexistent_notebook.ipynb"}),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("Failed to read"));
    }

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[31mRed text\x1b[0m normal";
        let output = strip_ansi_codes(input);
        assert_eq!(output, "Red text normal");
    }
}
