//! Compact tool schema encoding and fail-closed decoder.
//!
//! Compresses verbose JSON Schema tool definitions into compact signatures,
//! reducing prompt token overhead while ensuring tool calls returned from models
//! can be decoded and validated back into standard OpenAI-compatible tool calls.

#![forbid(unsafe_code)]

pub mod decoder;
pub mod encoder;
pub mod error;
pub mod stream;
pub mod types;

pub use decoder::{decode_calls, extract_raw_calls};
pub use encoder::{DEFAULT_CALL_INSTRUCTION, decode_tools, encode_tool, encode_tools};
pub use error::CompactError;
pub use stream::StreamDecoder;
pub use types::{
    CompactTools, FunctionCall, FunctionCallDelta, FunctionDef, ToolCall, ToolCallDelta, ToolDef,
};
