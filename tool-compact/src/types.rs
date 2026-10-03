use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn default_function_kind() -> String {
    "function".to_string()
}

/// A tool definition accepted for compaction.
///
/// Models the standard OpenAI tool definition format without depending
/// on external crates.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type", default = "default_function_kind")]
    pub kind: String,
    pub function: FunctionDef,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl ToolDef {
    pub fn new(
        name: impl Into<String>,
        description: Option<String>,
        parameters: Option<Value>,
    ) -> Self {
        Self {
            kind: default_function_kind(),
            function: FunctionDef {
                name: name.into(),
                description,
                parameters,
            },
            extra: Map::new(),
        }
    }
}

/// Function metadata and JSON Schema parameters.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FunctionDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

/// Decoded tool call emitted to clients.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_function_kind")]
    pub kind: String,
    pub function: FunctionCall,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: default_function_kind(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
            extra: Map::new(),
        }
    }
}

/// The name and serialized JSON string arguments of a function call.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A streamed fragment of a tool call.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct ToolCallDelta {
    pub index: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCallDelta>,
}

/// A streamed fragment of a function name and/or argument string.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct FunctionCallDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// Compact representation of tools with execution instructions.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CompactTools {
    /// Formatted signatures of all available tools.
    pub compact_definition: String,
    /// Calling instructions injected into the model prompt.
    pub call_instructions: String,
    /// Combined string to inject into the system prompt or user context.
    pub prompt_injection: String,
    /// Estimated token count savings vs raw JSON schema.
    pub original_tools: Vec<ToolDef>,
}
