use serde_json::Value;

use crate::error::CompactError;
use crate::types::{FunctionCall, ToolCall, ToolDef};

/// Decode all compact tool calls found within `text` and validate them
/// against the provided `tools` schemas.
///
/// Returns standard OpenAI-shaped `ToolCall`s.
/// If no calls are present, returns `Ok(vec![])`.
/// Fails closed if any call uses an unknown tool, misses required parameters,
/// violates enum constraints, or has invalid types.
pub fn decode_calls(text: &str, tools: &[ToolDef]) -> Result<Vec<ToolCall>, CompactError> {
    let raw_calls = extract_raw_calls(text)?;
    if raw_calls.is_empty() {
        return Ok(Vec::new());
    }

    let mut decoded = Vec::with_capacity(raw_calls.len());
    for (idx, (tool_name, args_str)) in raw_calls.into_iter().enumerate() {
        // 1. Locate tool in schema
        let tool_def = tools
            .iter()
            .find(|t| t.function.name == tool_name)
            .ok_or_else(|| CompactError::UnknownTool(tool_name.clone()))?;

        // 2. Parse arguments as JSON
        let args_val: Value =
            serde_json::from_str(&args_str).map_err(|e| CompactError::InvalidArguments {
                tool: tool_name.clone(),
                details: format!("arguments are not valid JSON: {}", e),
            })?;

        // 3. Validate arguments against schema
        validate_arguments(&tool_name, &args_val, &tool_def.function.parameters)?;

        // Canonical compact JSON string representation
        let canonical_arguments = serde_json::to_string(&args_val)?;

        decoded.push(ToolCall {
            id: format!("call_{}", idx + 1),
            kind: "function".to_string(),
            function: FunctionCall {
                name: tool_name,
                arguments: canonical_arguments,
            },
            extra: serde_json::Map::new(),
        });
    }

    Ok(decoded)
}

/// Extract all `(tool_name, json_arguments_str)` from text.
///
/// Handles text before, between, and after calls.
/// Properly handles delimiters like `>>` occurring inside JSON string literals.
pub fn extract_raw_calls(text: &str) -> Result<Vec<(String, String)>, CompactError> {
    let mut calls = Vec::new();
    let marker = "<<call";
    let mut cursor = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    while cursor < len {
        // Find "<<call"
        let remaining: String = chars[cursor..].iter().collect();
        let found_pos = match remaining.find(marker) {
            Some(pos) => cursor + pos,
            None => break,
        };

        // Advance past "<<call"
        let mut idx = found_pos + marker.len();

        // Must be followed by whitespace
        if idx >= len || !chars[idx].is_whitespace() {
            cursor = idx;
            continue;
        }

        // Skip whitespace to reach tool name
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }

        // Read tool name
        let name_start = idx;
        while idx < len && !chars[idx].is_whitespace() && chars[idx] != '{' {
            idx += 1;
        }
        let tool_name: String = chars[name_start..idx].iter().collect();
        if tool_name.is_empty() {
            return Err(CompactError::InvalidSyntax(
                "expected tool name after '<<call'".to_string(),
            ));
        }

        // Skip whitespace to reach opening '{'
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }

        if idx >= len || chars[idx] != '{' {
            return Err(CompactError::InvalidSyntax(format!(
                "expected '{{' after tool name '{}'",
                tool_name
            )));
        }

        // Scan the JSON object while tracking braces and string literals
        let json_start = idx;
        let mut brace_depth = 0;
        let mut in_string = false;
        let mut escaped = false;
        let mut json_end = None;

        while idx < len {
            let c = chars[idx];

            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            json_end = Some(idx + 1);
                            idx += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            idx += 1;
        }

        let json_end_idx = match json_end {
            Some(end) => end,
            None => {
                return Err(CompactError::InvalidSyntax(format!(
                    "unclosed JSON object in tool call for '{}'",
                    tool_name
                )));
            }
        };

        let args_str: String = chars[json_start..json_end_idx].iter().collect();

        // Skip any whitespace after '}' and expect '>>'
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }

        if idx + 1 < len && chars[idx] == '>' && chars[idx + 1] == '>' {
            idx += 2;
        } else {
            return Err(CompactError::InvalidSyntax(format!(
                "expected '>>' after arguments for tool '{}'",
                tool_name
            )));
        }

        calls.push((tool_name, args_str));
        cursor = idx;
    }

    Ok(calls)
}

/// Validate parsed arguments against parameters schema.
fn validate_arguments(
    tool_name: &str,
    args: &Value,
    parameters: &Option<Value>,
) -> Result<(), CompactError> {
    let params_obj = match parameters {
        Some(Value::Object(map)) => map,
        _ => return Ok(()),
    };

    let args_obj = args
        .as_object()
        .ok_or_else(|| CompactError::InvalidArguments {
            tool: tool_name.to_string(),
            details: "arguments must be a JSON object".to_string(),
        })?;

    // 1. Check required fields
    if let Some(req_arr) = params_obj.get("required").and_then(Value::as_array) {
        for req in req_arr {
            if let Some(req_name) = req.as_str() {
                match args_obj.get(req_name) {
                    None | Some(Value::Null) => {
                        return Err(CompactError::MissingRequiredArgument {
                            tool: tool_name.to_string(),
                            argument: req_name.to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    // 2. Validate types and enums for supplied properties
    let properties = params_obj.get("properties").and_then(Value::as_object);
    if let Some(props) = properties {
        for (arg_key, arg_val) in args_obj {
            if let Some(prop_schema) = props.get(arg_key).and_then(Value::as_object) {
                validate_property_value(tool_name, arg_key, arg_val, prop_schema)?;
            }
        }
    }

    Ok(())
}

/// Validate a single argument value against its property schema.
fn validate_property_value(
    tool_name: &str,
    prop_name: &str,
    val: &Value,
    schema: &serde_json::Map<String, Value>,
) -> Result<(), CompactError> {
    // Check enum constraints
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|enum_vals| !enum_vals.contains(val))
    {
        return Err(CompactError::InvalidArguments {
            tool: tool_name.to_string(),
            details: format!(
                "value '{:?}' is not valid for enum parameter '{}'",
                val, prop_name
            ),
        });
    }

    // Check type constraints
    if let Some(expected_type) = schema.get("type").and_then(Value::as_str) {
        match expected_type {
            "string" if !val.is_string() => {
                return Err(CompactError::InvalidArguments {
                    tool: tool_name.to_string(),
                    details: format!("parameter '{}' must be a string", prop_name),
                });
            }
            "integer" if !val.is_i64() && !val.is_u64() => {
                return Err(CompactError::InvalidArguments {
                    tool: tool_name.to_string(),
                    details: format!("parameter '{}' must be an integer", prop_name),
                });
            }
            "number" if !val.is_number() => {
                return Err(CompactError::InvalidArguments {
                    tool: tool_name.to_string(),
                    details: format!("parameter '{}' must be a number", prop_name),
                });
            }
            "boolean" if !val.is_boolean() => {
                return Err(CompactError::InvalidArguments {
                    tool: tool_name.to_string(),
                    details: format!("parameter '{}' must be a boolean", prop_name),
                });
            }
            "array" => {
                let arr = val
                    .as_array()
                    .ok_or_else(|| CompactError::InvalidArguments {
                        tool: tool_name.to_string(),
                        details: format!("parameter '{}' must be an array", prop_name),
                    })?;
                if let Some(items_schema) = schema.get("items").and_then(Value::as_object) {
                    for item in arr {
                        validate_property_value(tool_name, prop_name, item, items_schema)?;
                    }
                }
            }
            "object" if !val.is_object() => {
                return Err(CompactError::InvalidArguments {
                    tool: tool_name.to_string(),
                    details: format!("parameter '{}' must be an object", prop_name),
                });
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FunctionDef;
    use serde_json::json;

    fn test_tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                kind: "function".to_string(),
                function: FunctionDef {
                    name: "create_calendar_event".to_string(),
                    description: Some("Create an event in the user's calendar.".to_string()),
                    parameters: Some(json!({
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "start": { "type": "string", "format": "date-time" },
                            "duration_min": { "type": "integer" },
                            "attendees": { "type": "array", "items": { "type": "string" } },
                            "visibility": { "type": "string", "enum": ["public", "private"] }
                        },
                        "required": ["title", "start"]
                    })),
                },
                extra: serde_json::Map::new(),
            },
            ToolDef {
                kind: "function".to_string(),
                function: FunctionDef {
                    name: "send_email".to_string(),
                    description: Some("Send an email".to_string()),
                    parameters: Some(json!({
                        "type": "object",
                        "properties": {
                            "to": { "type": "array", "items": { "type": "string" } },
                            "subject": { "type": "string" },
                            "body": { "type": "string" }
                        },
                        "required": ["to", "subject", "body"]
                    })),
                },
                extra: serde_json::Map::new(),
            },
        ]
    }

    #[test]
    fn test_decode_single_call() {
        let tools = test_tools();
        let text = "Here is your booking: <<call create_calendar_event {\"title\":\"Design review\",\"start\":\"2026-10-05T15:00:00+05:30\"}>> Done!";
        let calls = decode_calls(text, &tools).expect("should decode call");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "create_calendar_event");
        let parsed_args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(parsed_args["title"], "Design review");
    }

    #[test]
    fn test_decode_multiple_calls() {
        let tools = test_tools();
        let text = "<<call send_email {\"to\":[\"sam@example.com\"],\"subject\":\"Hi\",\"body\":\"Hello\"}>>\nAnd next:\n<<call create_calendar_event {\"title\":\"Retro\",\"start\":\"2026-10-04T10:00:00+05:30\"}>>";
        let calls = decode_calls(text, &tools).expect("should decode multiple calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "send_email");
        assert_eq!(calls[1].function.name, "create_calendar_event");
    }

    #[test]
    fn test_plain_text_no_calls() {
        let tools = test_tools();
        let text =
            "Hello! I can help you book calendar events or send emails. What would you like to do?";
        let calls = decode_calls(text, &tools).expect("should handle plain text");
        assert!(calls.is_empty());
    }

    #[test]
    fn test_delimiter_inside_string() {
        let tools = test_tools();
        let text = "<<call send_email {\"to\":[\"sam@example.com\"],\"subject\":\"a >> b\",\"body\":\"x\"}>>";
        let calls = decode_calls(text, &tools).expect("should handle >> inside string");
        assert_eq!(calls.len(), 1);
        let parsed_args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(parsed_args["subject"], "a >> b");
    }

    #[test]
    fn test_fail_closed_unknown_tool() {
        let tools = test_tools();
        let text = "<<call delete_everything {}>>";
        let err = decode_calls(text, &tools).unwrap_err();
        assert_eq!(err.error_slug(), "unknown_tool");
    }

    #[test]
    fn test_fail_closed_missing_required_field() {
        let tools = test_tools();
        let text = "<<call create_calendar_event {\"start\":\"2026-10-05T15:00:00+05:30\"}>>";
        let err = decode_calls(text, &tools).unwrap_err();
        assert_eq!(err.error_slug(), "invalid_arguments");
    }

    #[test]
    fn test_fail_closed_enum_violation() {
        let tools = test_tools();
        let text = "<<call create_calendar_event {\"title\":\"Meeting\",\"start\":\"2026-10-05T15:00:00+05:30\",\"visibility\":\"secret\"}>>";
        let err = decode_calls(text, &tools).unwrap_err();
        assert_eq!(err.error_slug(), "invalid_arguments");
    }

    #[test]
    fn test_fail_closed_type_mismatch() {
        let tools = test_tools();
        let text = "<<call create_calendar_event {\"title\":\"Meeting\",\"start\":\"2026-10-05T15:00:00+05:30\",\"duration_min\":\"thirty\"}>>";
        let err = decode_calls(text, &tools).unwrap_err();
        assert_eq!(err.error_slug(), "invalid_arguments");
    }
}
