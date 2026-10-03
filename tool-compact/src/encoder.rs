use serde_json::{Map, Value, json};

use crate::error::CompactError;
use crate::types::{CompactTools, FunctionDef, ToolDef};

pub const DEFAULT_CALL_INSTRUCTION: &str = "To call a tool, emit: <<call name {json args}>>";

/// Encode an array of tool definitions into a compact string representation
/// accompanied by tool calling instructions.
pub fn encode_tools(tools: &[ToolDef]) -> Result<CompactTools, CompactError> {
    if tools.is_empty() {
        return Ok(CompactTools {
            compact_definition: String::new(),
            call_instructions: String::new(),
            prompt_injection: String::new(),
            original_tools: Vec::new(),
        });
    }

    let mut signatures = Vec::with_capacity(tools.len());
    for tool in tools {
        let sig = encode_tool(tool)?;
        signatures.push(sig);
    }

    let compact_definition = signatures.join("\n");
    let call_instructions = DEFAULT_CALL_INSTRUCTION.to_string();
    let prompt_injection = format!("Tools:\n{}\n\n{}", compact_definition, call_instructions);

    Ok(CompactTools {
        compact_definition,
        call_instructions,
        prompt_injection,
        original_tools: tools.to_vec(),
    })
}

/// Encode a single tool into its compact signature line:
/// `name(param:type, opt_param?:type) - Description`
pub fn encode_tool(tool: &ToolDef) -> Result<String, CompactError> {
    let func = &tool.function;
    let name = &func.name;

    let mut params_str = String::new();
    if let Some(params_val) = &func.parameters {
        params_str = encode_parameters(name, params_val)?;
    }

    let mut sig = format!("{}({})", name, params_str);
    if let Some(desc) = &func.description {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            sig.push_str(" - ");
            sig.push_str(trimmed);
        }
    }

    Ok(sig)
}

/// Parse parameters JSON schema into signature parameters:
/// `title:str, start:datetime, duration_min?:int, attendees?:[str], visibility?:public|private`
fn encode_parameters(tool_name: &str, params: &Value) -> Result<String, CompactError> {
    let obj = params.as_object().ok_or_else(|| {
        CompactError::SchemaError(format!(
            "parameters for tool '{}' must be an object schema",
            tool_name
        ))
    })?;

    // Check for unsupported top-level keywords (fail-closed)
    for unsupported in ["oneOf", "anyOf", "allOf", "$ref"] {
        if obj.contains_key(unsupported) {
            return Err(CompactError::SchemaError(format!(
                "tool '{}' uses unsupported schema feature '{}'",
                tool_name, unsupported
            )));
        }
    }

    let required_set: Vec<&str> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let properties = match obj.get("properties").and_then(Value::as_object) {
        Some(props) => props,
        None => return Ok(String::new()),
    };

    let mut encoded_params = Vec::new();
    for (prop_name, prop_schema) in properties {
        let is_required = required_set.contains(&prop_name.as_str());
        let type_str = encode_type(tool_name, prop_name, prop_schema)?;
        if is_required {
            encoded_params.push(format!("{}:{}", prop_name, type_str));
        } else {
            encoded_params.push(format!("{}?:{}", prop_name, type_str));
        }
    }

    Ok(encoded_params.join(", "))
}

/// Encode a parameter's JSON schema type into a compact type string.
fn encode_type(tool_name: &str, prop_name: &str, schema: &Value) -> Result<String, CompactError> {
    let obj = schema.as_object().ok_or_else(|| {
        CompactError::SchemaError(format!(
            "property '{}' in tool '{}' has non-object schema",
            prop_name, tool_name
        ))
    })?;

    // Enums take precedence
    if let Some(enum_vals) = obj.get("enum").and_then(Value::as_array) {
        let vals: Vec<&str> = enum_vals.iter().filter_map(Value::as_str).collect();
        if vals.is_empty() {
            return Err(CompactError::SchemaError(format!(
                "property '{}' in tool '{}' has empty enum",
                prop_name, tool_name
            )));
        }
        return Ok(vals.join("|"));
    }

    // Format special-casing (e.g. date-time -> datetime)
    if matches!(
        obj.get("format").and_then(Value::as_str),
        Some("date-time" | "datetime")
    ) {
        return Ok("datetime".to_string());
    }

    let raw_type = obj.get("type").and_then(Value::as_str).unwrap_or("any");

    match raw_type {
        "string" => Ok("str".to_string()),
        "integer" => Ok("int".to_string()),
        "number" => Ok("float".to_string()),
        "boolean" => Ok("bool".to_string()),
        "array" => {
            if let Some(items) = obj.get("items") {
                let item_type = encode_type(tool_name, prop_name, items)?;
                Ok(format!("[{}]", item_type))
            } else {
                Ok("[any]".to_string())
            }
        }
        "object" => {
            if let Some(sub_props) = obj.get("properties").and_then(Value::as_object) {
                let sub_req: Vec<&str> = obj
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();

                let mut sub_encoded = Vec::new();
                for (sub_name, sub_schema) in sub_props {
                    let is_req = sub_req.contains(&sub_name.as_str());
                    let sub_t = encode_type(tool_name, sub_name, sub_schema)?;
                    if is_req {
                        sub_encoded.push(format!("{}:{}", sub_name, sub_t));
                    } else {
                        sub_encoded.push(format!("{}?:{}", sub_name, sub_t));
                    }
                }
                Ok(format!("{{{}}}", sub_encoded.join(", ")))
            } else {
                Ok("object".to_string())
            }
        }
        "null" => Ok("null".to_string()),
        "any" => Ok("any".to_string()),
        other => Err(CompactError::SchemaError(format!(
            "property '{}' in tool '{}' has unsupported type '{}'",
            prop_name, tool_name, other
        ))),
    }
}

/// Reconstruct full standard ToolDefs from compact tools representation.
///
/// This satisfies the optional but recommended automated check to verify
/// that schema information (names, types, required/optional, enums) survived compaction.
pub fn decode_tools(compact: &CompactTools) -> Result<Vec<ToolDef>, CompactError> {
    if compact.compact_definition.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut tools = Vec::new();
    for line in compact.compact_definition.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        tools.push(decode_tool_signature(trimmed)?);
    }

    Ok(tools)
}

/// Reconstruct a single `ToolDef` from a compact signature line:
/// `create_calendar_event(title:str, start:datetime, duration_min?:int) - Create an event`
pub fn decode_tool_signature(sig: &str) -> Result<ToolDef, CompactError> {
    let (sig_part, description) = match sig.split_once(" - ") {
        Some((s, desc)) => (s.trim(), Some(desc.trim().to_string())),
        None => (sig.trim(), None),
    };

    let open_paren = sig_part.find('(').ok_or_else(|| {
        CompactError::InvalidSyntax(format!("missing '(' in tool signature: {}", sig_part))
    })?;
    let close_paren = sig_part.rfind(')').ok_or_else(|| {
        CompactError::InvalidSyntax(format!("missing ')' in tool signature: {}", sig_part))
    })?;

    if close_paren < open_paren {
        return Err(CompactError::InvalidSyntax(format!(
            "mismatched parentheses in: {}",
            sig_part
        )));
    }

    let name = sig_part[..open_paren].trim().to_string();
    let inside_params = sig_part[open_paren + 1..close_paren].trim();

    let mut properties = Map::new();
    let mut required = Vec::new();

    if !inside_params.is_empty() {
        // Split parameters by comma while respecting nested brackets/braces
        let param_tokens = split_top_level(inside_params, ',')?;
        for param in param_tokens {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }

            let (p_name, p_type) = param.split_once(':').ok_or_else(|| {
                CompactError::InvalidSyntax(format!(
                    "missing ':' in parameter definition: {}",
                    param
                ))
            })?;

            let p_name = p_name.trim();
            let p_type = p_type.trim();

            let (clean_name, is_required) = if let Some(n) = p_name.strip_suffix('?') {
                (n.trim(), false)
            } else {
                (p_name, true)
            };

            if is_required {
                required.push(Value::String(clean_name.to_string()));
            }

            let prop_schema = decode_type(p_type)?;
            properties.insert(clean_name.to_string(), prop_schema);
        }
    }

    let parameters = json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": Value::Array(required)
    });

    Ok(ToolDef {
        kind: "function".to_string(),
        function: FunctionDef {
            name,
            description,
            parameters: Some(parameters),
        },
        extra: Map::new(),
    })
}

/// Decode a compact type string back into a JSON Schema value.
fn decode_type(type_str: &str) -> Result<Value, CompactError> {
    let t = type_str.trim();

    // Check for enum (pipe-separated values not enclosed in brackets)
    if t.contains('|') && !t.starts_with('[') && !t.starts_with('{') {
        let vals: Vec<Value> = t
            .split('|')
            .map(|s| Value::String(s.trim().to_string()))
            .collect();
        return Ok(json!({
            "type": "string",
            "enum": vals
        }));
    }

    if t == "datetime" {
        return Ok(json!({
            "type": "string",
            "format": "date-time"
        }));
    }

    if t == "str" || t == "string" {
        return Ok(json!({ "type": "string" }));
    }

    if t == "int" || t == "integer" {
        return Ok(json!({ "type": "integer" }));
    }

    if t == "float" || t == "number" {
        return Ok(json!({ "type": "number" }));
    }

    if t == "bool" || t == "boolean" {
        return Ok(json!({ "type": "boolean" }));
    }

    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let items_schema = decode_type(inner.trim())?;
        return Ok(json!({
            "type": "array",
            "items": items_schema
        }));
    }

    if let Some(inner) = t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut sub_props = Map::new();
        let mut sub_req = Vec::new();
        let inner_tokens = split_top_level(inner.trim(), ',')?;
        for token in inner_tokens {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let (sub_name, sub_t) = token.split_once(':').ok_or_else(|| {
                CompactError::InvalidSyntax(format!("missing ':' in nested object: {}", token))
            })?;
            let (clean_sub_name, is_req) = if let Some(n) = sub_name.trim().strip_suffix('?') {
                (n.trim(), false)
            } else {
                (sub_name.trim(), true)
            };
            if is_req {
                sub_req.push(Value::String(clean_sub_name.to_string()));
            }
            sub_props.insert(clean_sub_name.to_string(), decode_type(sub_t)?);
        }
        return Ok(json!({
            "type": "object",
            "properties": Value::Object(sub_props),
            "required": Value::Array(sub_req)
        }));
    }

    if t == "any" {
        return Ok(json!({}));
    }

    Err(CompactError::InvalidSyntax(format!(
        "unknown compact type: {}",
        t
    )))
}

/// Helper to split a string by delimiter only when not nested inside brackets or braces.
fn split_top_level(s: &str, delimiter: char) -> Result<Vec<String>, CompactError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: usize = 0;
    let mut bracket_depth: usize = 0;
    let mut brace_depth: usize = 0;

    for c in s.chars() {
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }

        if c == delimiter && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            parts.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(c);
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_calendar_tool() -> ToolDef {
        ToolDef {
            kind: "function".to_string(),
            function: FunctionDef {
                name: "create_calendar_event".to_string(),
                description: Some("Create an event in the user's calendar.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Event title" },
                        "start": { "type": "string", "format": "date-time", "description": "Start time, ISO 8601" },
                        "duration_min": { "type": "integer", "description": "Duration in minutes" },
                        "attendees": { "type": "array", "items": { "type": "string" }, "description": "Attendee emails" },
                        "visibility": { "type": "string", "enum": ["public", "private"] }
                    },
                    "required": ["title", "start"]
                })),
            },
            extra: Map::new(),
        }
    }

    fn sample_email_tool() -> ToolDef {
        ToolDef {
            kind: "function".to_string(),
            function: FunctionDef {
                name: "send_email".to_string(),
                description: Some("Send an email from the user's account.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "array", "items": { "type": "string" }, "description": "Recipient emails" },
                        "subject": { "type": "string", "description": "Subject line" },
                        "body": { "type": "string", "description": "Plain-text body" },
                        "cc": { "type": "array", "items": { "type": "string" }, "description": "CC emails" }
                    },
                    "required": ["to", "subject", "body"]
                })),
            },
            extra: Map::new(),
        }
    }

    #[test]
    fn test_encode_calendar_tool() {
        let tool = sample_calendar_tool();
        let sig = encode_tool(&tool).expect("encoding should succeed");
        assert_eq!(
            sig,
            "create_calendar_event(title:str, start:datetime, duration_min?:int, attendees?:[str], visibility?:public|private) - Create an event in the user's calendar."
        );
    }

    #[test]
    fn test_encode_email_tool() {
        let tool = sample_email_tool();
        let sig = encode_tool(&tool).expect("encoding should succeed");
        assert_eq!(
            sig,
            "send_email(to:[str], subject:str, body:str, cc?:[str]) - Send an email from the user's account."
        );
    }

    #[test]
    fn test_encode_tools_batch() {
        let tools = vec![sample_calendar_tool(), sample_email_tool()];
        let compact = encode_tools(&tools).expect("batch encode should succeed");
        assert!(
            compact
                .compact_definition
                .contains("create_calendar_event(")
        );
        assert!(compact.compact_definition.contains("send_email("));
        assert!(
            compact
                .call_instructions
                .contains("<<call name {json args}>>")
        );
        assert!(compact.prompt_injection.contains("Tools:"));
    }

    #[test]
    fn test_roundtrip_decode_tools() {
        let tools = vec![sample_calendar_tool(), sample_email_tool()];
        let compact = encode_tools(&tools).expect("batch encode should succeed");
        let decoded = decode_tools(&compact).expect("decode should succeed");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].function.name, "create_calendar_event");
        assert_eq!(decoded[1].function.name, "send_email");

        let cal_params = decoded[0].function.parameters.as_ref().unwrap();
        let req = cal_params["required"].as_array().unwrap();
        assert!(req.contains(&Value::String("title".to_string())));
        assert!(req.contains(&Value::String("start".to_string())));
        assert!(!req.contains(&Value::String("duration_min".to_string())));
    }

    #[test]
    fn test_fail_closed_unsupported_schema() {
        let unsupported_tool = ToolDef {
            kind: "function".to_string(),
            function: FunctionDef {
                name: "complex_tool".to_string(),
                description: None,
                parameters: Some(json!({
                    "type": "object",
                    "oneOf": [{"type": "string"}, {"type": "integer"}]
                })),
            },
            extra: Map::new(),
        };

        let result = encode_tool(&unsupported_tool);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompactError::SchemaError(msg) => {
                assert!(msg.contains("unsupported schema feature 'oneOf'"));
            }
            other => panic!("expected SchemaError, got: {:?}", other),
        }
    }
}
