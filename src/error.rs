//! Error types for the RDF extraction library

use thiserror::Error;

/// Result type alias for this library
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during RDF extraction
#[derive(Error, Debug)]
pub enum Error {
    /// Error from the AI service (genai crate)
    #[error("AI service error: {0}")]
    AiService(String),

    /// Error parsing JSON-LD response
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Invalid RDF structure
    #[error("Invalid RDF structure: {0}")]
    InvalidRdf(String),

    /// Missing required field in RDF document
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Extraction error
    #[error("Extraction error: {0}")]
    Extraction(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Network or HTTP error
    #[error("Network error: {0}")]
    Network(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
}

// Allow conversion from genai errors if needed
impl From<Box<dyn std::error::Error + Send + Sync>> for Error {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error::AiService(err.to_string())
    }
}
