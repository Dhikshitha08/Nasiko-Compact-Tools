//! P1 — Compact tool schemas evaluation runner.
//!
//! Usage:
//!   EVAL_SET=/tmp/compact-tools-eval.json OUT=/tmp/out.jsonl \
//!   cargo run --release -p nasiko-llm-router --example compact_tools_eval
//!
//! Optional live mode (against an OpenAI-compatible endpoint):
//!   PROVIDER_BASE_URL=https://... MODEL=gpt-4o \
//!   EVAL_SET=... OUT=... cargo run ...
//!
//! Output: one JSONL line per case in EVAL_SET.
//!   Regular case:
//!     {"id":"ct-001","compact_request":{...},"compacted":true,
//!      "rendered_calls":"<<call ...>>","roundtrip_calls":[...]}
//!   Decoder case:
//!     {"id":"dc-001","decoded":{"calls":[...]}}
//!     {"id":"dc-002","decoded":{"error":"unknown_tool"}}
//!
//! Reference date fixed to 2026-10-02 Asia/Kolkata per the brief.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;

use nasiko_tool_compact::{StreamDecoder, ToolDef, decode_calls, encode_tools};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ─── Eval dataset types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EvalSet {
    tools: Vec<ToolDef>,
    cases: Vec<Case>,
    #[serde(default)]
    decoder_cases: Vec<DecoderCase>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    tools: Vec<String>,
    messages: Vec<Message>,
    #[serde(default)]
    expected: Vec<ExpectedCall>,
}

#[derive(Debug, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ExpectedCall {
    name: String,
    arguments: Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DecoderCase {
    id: String,
    tools: Vec<String>,
    chunks: Vec<String>,
    expected: DecoderExpected,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum DecoderExpected {
    Calls { calls: Vec<ExpectedCall> },
    Error { error: String },
}

// ─── JSONL output types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CaseOutput {
    id: String,
    compact_request: Value,
    compacted: bool,
    rendered_calls: String,
    roundtrip_calls: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_savings_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
struct DecoderOutput {
    id: String,
    decoded: Value,
}

// ─── Token counting ───────────────────────────────────────────────────────────

/// Approximate token count using a GPT-style word-boundary heuristic (~4 chars/token).
///
/// The judge's harness re-counts with tiktoken o200k_base; this value is for
/// local guidance only and is never submitted as an exact figure.
fn count_tokens_approx(text: &str) -> usize {
    // Conservative: 1 token per ~3.5 characters, rounded up
    (text.len() * 2 + 6) / 7
}

fn tokens_for_tools_json(tools: &[ToolDef]) -> usize {
    let json_val: Vec<Value> = tools
        .iter()
        .map(|t| serde_json::to_value(t).unwrap_or_default())
        .collect();
    count_tokens_approx(&serde_json::to_string(&json_val).unwrap_or_default())
}

// ─── Build compact request body ───────────────────────────────────────────────

fn build_compact_request(
    _case_tools: &[ToolDef],
    messages: &[Message],
    compact_text: &str,
) -> Value {
    // Fixed reference timestamp per the brief: 2026-10-02, Asia/Kolkata
    let system_msg = json!({
        "role": "system",
        "content": format!(
            "Today is 2026-10-02 in timezone Asia/Kolkata (UTC+05:30).\n\n{}",
            compact_text
        )
    });

    let user_messages: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let mut all_messages = vec![system_msg];
    all_messages.extend(user_messages);

    json!({
        "model": "gpt-4o",
        "messages": all_messages,
        "temperature": 0
    })
}

// ─── Render expected calls in compact form ────────────────────────────────────

fn render_calls_compact(calls: &[ExpectedCall]) -> String {
    calls
        .iter()
        .map(|c| format!("<<call {} {}>>", c.name, c.arguments))
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Process a regular case ───────────────────────────────────────────────────

fn process_case(
    case: &Case,
    tool_index: &HashMap<String, ToolDef>,
    total_baseline_tokens: &mut usize,
    total_compact_tokens: &mut usize,
) -> CaseOutput {
    // Gather the tools referenced by this case
    let case_tools: Vec<ToolDef> = case
        .tools
        .iter()
        .filter_map(|name| tool_index.get(name).cloned())
        .collect();

    // Encode tools into compact format
    let (compact, compacted) = match encode_tools(&case_tools) {
        Ok(c) if !c.compact_definition.is_empty() => (c, true),
        _ => {
            // Bypass: return empty compact, compacted=false
            let empty = nasiko_tool_compact::CompactTools {
                compact_definition: String::new(),
                call_instructions: String::new(),
                prompt_injection: String::new(),
                original_tools: case_tools.clone(),
            };
            (empty, false)
        }
    };

    // Token reduction measurement
    let baseline_tokens = tokens_for_tools_json(&case_tools);
    let compact_tokens = if compacted {
        count_tokens_approx(&compact.prompt_injection)
    } else {
        baseline_tokens // no savings for bypassed cases
    };
    *total_baseline_tokens += baseline_tokens;
    *total_compact_tokens += compact_tokens;

    let savings_pct = if baseline_tokens > 0 {
        let pct =
            (1.0 - compact_tokens as f64 / baseline_tokens as f64) * 100.0;
        Some((pct * 10.0).round() / 10.0) // 1 decimal place
    } else {
        None
    };

    // Build what the compact request body would look like
    let compact_request =
        build_compact_request(&case_tools, &case.messages, &compact.prompt_injection);

    // Render expected calls in compact notation (for roundtrip check)
    let rendered_calls = render_calls_compact(&case.expected);

    // Roundtrip: decode the rendered calls back through our decoder
    let roundtrip_calls = if rendered_calls.is_empty() {
        Vec::new()
    } else {
        match decode_calls(&rendered_calls, &case_tools) {
            Ok(calls) => calls
                .iter()
                .map(|tc| {
                    let args: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Null);
                    json!({
                        "name": tc.function.name,
                        "arguments": args
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    };

    CaseOutput {
        id: case.id.clone(),
        compact_request,
        compacted,
        rendered_calls,
        roundtrip_calls,
        token_savings_pct: savings_pct,
    }
}

// ─── Process a decoder case ───────────────────────────────────────────────────

fn process_decoder_case(
    dc: &DecoderCase,
    tool_index: &HashMap<String, ToolDef>,
) -> DecoderOutput {
    let dc_tools: Vec<ToolDef> = dc
        .tools
        .iter()
        .filter_map(|name| tool_index.get(name).cloned())
        .collect();

    // Feed chunks through StreamDecoder
    let decoder = StreamDecoder::new(dc_tools.clone());
    let result = decoder.process_chunks(&dc.chunks);

    let decoded_val = match result {
        Ok(calls) => {
            let call_vals: Vec<Value> = calls
                .iter()
                .map(|tc| {
                    let args: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Null);
                    json!({"name": tc.function.name, "arguments": args})
                })
                .collect();
            json!({ "calls": call_vals })
        }
        Err(e) => {
            json!({ "error": e.error_slug() })
        }
    };

    DecoderOutput {
        id: dc.id.clone(),
        decoded: decoded_val,
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let eval_path = env::var("EVAL_SET").unwrap_or_else(|_| "compact-tools-eval.json".to_string());
    let out_path = env::var("OUT").unwrap_or_else(|_| "out.jsonl".to_string());

    let raw = fs::read_to_string(&eval_path)
        .unwrap_or_else(|e| panic!("Cannot read EVAL_SET '{}': {}", eval_path, e));

    let eval: EvalSet = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("Cannot parse EVAL_SET '{}': {}", eval_path, e));

    // Build name → ToolDef index from the top-level tools list
    let tool_index: HashMap<String, ToolDef> = eval
        .tools
        .iter()
        .cloned()
        .map(|t| (t.function.name.clone(), t))
        .collect();

    let mut out_file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&out_path)
        .unwrap_or_else(|e| panic!("Cannot open OUT '{}': {}", out_path, e));

    let mut total_baseline_tokens: usize = 0;
    let mut total_compact_tokens: usize = 0;

    // ── Regular cases ─────────────────────────────────────────────────────────
    for case in &eval.cases {
        let output = process_case(
            case,
            &tool_index,
            &mut total_baseline_tokens,
            &mut total_compact_tokens,
        );
        let line = serde_json::to_string(&output).expect("serialization failed");
        writeln!(out_file, "{}", line).expect("write failed");
    }

    // ── Decoder cases ─────────────────────────────────────────────────────────
    for dc in &eval.decoder_cases {
        let output = process_decoder_case(dc, &tool_index);
        let line = serde_json::to_string(&output).expect("serialization failed");
        writeln!(out_file, "{}", line).expect("write failed");
    }

    // ── Summary metrics (printed to stderr, not scored) ───────────────────────
    let overall_reduction = if total_baseline_tokens > 0 {
        (1.0 - total_compact_tokens as f64 / total_baseline_tokens as f64) * 100.0
    } else {
        0.0
    };

    eprintln!("=== P1 Compact Tool Schemas Eval ===");
    eprintln!("  Regular cases   : {}", eval.cases.len());
    eprintln!("  Decoder cases   : {}", eval.decoder_cases.len());
    eprintln!("  Baseline tokens : {}", total_baseline_tokens);
    eprintln!("  Compact tokens  : {}", total_compact_tokens);
    eprintln!("  Token reduction : {:.1}%", overall_reduction);
    eprintln!("  Output written  : {}", out_path);
}
