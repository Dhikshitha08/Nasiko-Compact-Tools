use crate::decoder::decode_calls;
use crate::error::CompactError;
use crate::types::{FunctionCallDelta, ToolCall, ToolCallDelta, ToolDef};

/// Incremental streaming decoder for compact tool calls.
///
/// Accumulates text chunks and correctly detects complete `<<call name {json}>>` calls
/// even when the marker, tool name, or JSON arguments are split across chunk boundaries.
/// All validation is performed once a complete call is finalized (fail-closed).
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    tools: Vec<ToolDef>,
    /// Full accumulated buffer of all chunks received so far.
    buffer: String,
    /// How many calls have already been completed and validated.
    completed_calls: Vec<ToolCall>,
    emitted_count: usize,
}

impl StreamDecoder {
    /// Create a new `StreamDecoder` with the given tool definitions.
    pub fn new(tools: Vec<ToolDef>) -> Self {
        Self {
            tools,
            buffer: String::new(),
            completed_calls: Vec::new(),
            emitted_count: 0,
        }
    }

    /// Push the next text chunk. Returns any `ToolCallDelta`s for calls completed
    /// in this chunk. Fails closed if a just-completed call is invalid.
    pub fn push_chunk(&mut self, chunk: &str) -> Result<Vec<ToolCallDelta>, CompactError> {
        self.buffer.push_str(chunk);

        // Only attempt decoding if the buffer contains a potential closing `>>`
        // (a complete call cannot exist without it).
        if !self.buffer.contains(">>") {
            return Ok(Vec::new());
        }

        // Count how many complete `<<call ... >>` calls are in the buffer.
        let n_complete = count_complete_calls(&self.buffer);
        if n_complete <= self.completed_calls.len() {
            return Ok(Vec::new());
        }

        // New calls completed — decode and validate the entire buffer.
        let decoded = decode_calls(&self.buffer, &self.tools)?;

        let mut deltas = Vec::new();
        for new_call in &decoded[self.completed_calls.len()..] {
            deltas.push(ToolCallDelta {
                index: self.emitted_count as i64,
                id: Some(new_call.id.clone()),
                kind: Some("function".to_string()),
                function: Some(FunctionCallDelta {
                    name: Some(new_call.function.name.clone()),
                    arguments: Some(new_call.function.arguments.clone()),
                }),
            });
            self.emitted_count += 1;
        }
        self.completed_calls = decoded;

        Ok(deltas)
    }

    /// Finalize the stream. Validates the complete accumulated buffer and returns
    /// all decoded `ToolCall`s. Returns `Ok(vec![])` for plain-text responses.
    pub fn finish(self) -> Result<Vec<ToolCall>, CompactError> {
        decode_calls(&self.buffer, &self.tools)
    }

    /// Convenience helper: feed all chunks and return the final `ToolCall`s.
    pub fn process_chunks<S: AsRef<str>>(
        mut self,
        chunks: &[S],
    ) -> Result<Vec<ToolCall>, CompactError> {
        for chunk in chunks {
            self.push_chunk(chunk.as_ref())?;
        }
        self.finish()
    }
}

/// Count the number of syntactically complete `<<call name {...}>>` call blocks
/// in the text, without validating their content.
///
/// We count closing `>>` tokens that are outside string literals inside a JSON
/// args object — i.e., a `>>` that closes a `<<call`.
fn count_complete_calls(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let marker = "<<call";
    let mut count = 0;
    let mut pos = 0;

    while pos < len {
        // Find next "<<call"
        let slice: String = chars[pos..].iter().collect();
        let rel = match slice.find(marker) {
            Some(r) => r,
            None => break,
        };
        let mut idx = pos + rel + marker.len();

        // Skip whitespace then tool name then whitespace to reach '{'
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        // skip tool name
        while idx < len && !chars[idx].is_whitespace() && chars[idx] != '{' {
            idx += 1;
        }
        // skip whitespace
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        if idx >= len || chars[idx] != '{' {
            pos = idx;
            continue;
        }

        // Scan the JSON object to find closing '}'
        let mut brace_depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut json_closed = false;

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
                            json_closed = true;
                            idx += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            idx += 1;
        }

        if !json_closed {
            break; // JSON not complete yet
        }

        // Expect ">>" after the closing '}'
        // skip optional whitespace
        let temp = idx;
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        if idx + 1 < len && chars[idx] == '>' && chars[idx + 1] == '>' {
            count += 1;
            pos = idx + 2;
        } else {
            pos = temp + 1; // not a valid close, skip forward
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FunctionDef;
    use serde_json::{Value, json};

    fn calendar_tool() -> ToolDef {
        ToolDef {
            kind: "function".to_string(),
            function: FunctionDef {
                name: "create_calendar_event".to_string(),
                description: Some("Create an event".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "start": { "type": "string", "format": "date-time" },
                        "duration_min": { "type": "integer" },
                        "visibility": { "type": "string", "enum": ["public", "private"] }
                    },
                    "required": ["title", "start"]
                })),
            },
            extra: serde_json::Map::new(),
        }
    }

    fn email_tool() -> ToolDef {
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
        }
    }

    #[test]
    fn test_stream_single_complete_chunk() {
        let tools = vec![calendar_tool()];
        let decoder = StreamDecoder::new(tools);
        let calls = decoder
            .process_chunks(&[
                "<<call create_calendar_event {\"title\":\"Design review\",\"start\":\"2026-10-05T15:00:00+05:30\"}>>",
            ])
            .expect("single complete chunk should decode");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "create_calendar_event");
    }

    #[test]
    fn test_stream_split_marker_eval_dc002() {
        // Exact dc-002 case from the public eval dataset
        let tools = vec![calendar_tool()];
        let decoder = StreamDecoder::new(tools);
        let chunks = [
            "<<ca",
            "ll create_calendar_event {\"title\":\"Ret",
            "ro\",\"start\":\"2026-10-04T10:00:00+05:30\"}>",
            ">",
        ];
        let calls = decoder
            .process_chunks(&chunks)
            .expect("split-marker chunks should decode");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "create_calendar_event");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["title"], "Retro");
        assert_eq!(args["start"], "2026-10-04T10:00:00+05:30");
    }

    #[test]
    fn test_stream_split_at_every_character() {
        let tools = vec![email_tool()];
        let full =
            "<<call send_email {\"to\":[\"a@b.com\"],\"subject\":\"Hi\",\"body\":\"Text\"}>>";
        // Feed one character at a time
        let mut decoder = StreamDecoder::new(tools);
        for c in full.chars() {
            decoder.push_chunk(&c.to_string()).unwrap();
        }
        let calls = decoder.finish().expect("char-by-char feed should decode");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "send_email");
    }

    #[test]
    fn test_stream_plain_text_no_calls() {
        let tools = vec![calendar_tool()];
        let decoder = StreamDecoder::new(tools);
        let chunks = ["Hello ", "there! ", "How can I help you today?"];
        let calls = decoder.process_chunks(&chunks).unwrap();
        assert!(calls.is_empty());
    }

    #[test]
    fn test_stream_delimiter_inside_string() {
        // dc-003: ">>" inside a string argument must not close the call early
        let tools = vec![email_tool()];
        let decoder = StreamDecoder::new(tools);
        let calls = decoder
            .process_chunks(&[
                "<<call send_email {\"to\":[\"sam@example.com\"],\"subject\":\"a >> b\",\"body\":\"x\"}>>",
            ])
            .expect(">> inside string should be handled");
        assert_eq!(calls.len(), 1);
        let args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["subject"], "a >> b");
    }

    #[test]
    fn test_stream_fail_closed_unknown_tool() {
        // dc-004: unknown tool name
        let tools = vec![calendar_tool()];
        let decoder = StreamDecoder::new(tools);
        let err = decoder
            .process_chunks(&["<<call delete_everything {}>>"])
            .unwrap_err();
        assert_eq!(err.error_slug(), "unknown_tool");
    }

    #[test]
    fn test_stream_fail_closed_missing_required_and_bad_enum() {
        // dc-005: missing required "title" + bad enum value "secret"
        let tools = vec![calendar_tool()];
        let decoder = StreamDecoder::new(tools);
        let chunks = [
            "<<call create_calendar_event ",
            "{\"start\":\"2026-10-05T15:00:00+05:30\",",
            "\"visibility\":\"secret\"}>>",
        ];
        let err = decoder.process_chunks(&chunks).unwrap_err();
        assert_eq!(err.error_slug(), "invalid_arguments");
    }
}
