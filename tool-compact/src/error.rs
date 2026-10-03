use thiserror::Error;

/// Fail-closed error types for tool compacting, decoding, and argument validation.
#[derive(Debug, Error)]
pub enum CompactError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    #[error("missing required argument '{argument}' for tool '{tool}'")]
    MissingRequiredArgument { tool: String, argument: String },

    #[error("invalid argument for tool '{tool}': {details}")]
    InvalidArguments { tool: String, details: String },

    #[error("syntax error in compact tool call: {0}")]
    InvalidSyntax(String),

    #[error("unsupported or invalid schema: {0}")]
    SchemaError(String),

    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl CompactError {
    /// Return the standardized error slug matching the evaluation harness expectations.
    ///
    /// Specifically:
    /// - `"unknown_tool"` for unrecognized tools
    /// - `"invalid_arguments"` for missing parameters, enum violations, type mismatches
    pub fn error_slug(&self) -> &'static str {
        match self {
            Self::UnknownTool(_) => "unknown_tool",
            Self::MissingRequiredArgument { .. } | Self::InvalidArguments { .. } => {
                "invalid_arguments"
            }
            Self::InvalidSyntax(_) => "invalid_syntax",
            Self::SchemaError(_) => "schema_error",
            Self::Json(_) => "json_error",
        }
    }
}
