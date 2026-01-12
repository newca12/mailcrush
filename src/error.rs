//! Error types for MailCrush operations

use std::fmt;

/// Errors that can occur during mail analysis and processing
#[derive(Debug)]
pub enum MailCrushError {
    /// I/O error when reading/writing files
    IoError(std::io::Error),
    /// Error parsing the email
    ParseError(String),
    /// The mail file is empty
    EmptyMail,
    /// The mail structure is invalid
    InvalidStructure(String),
    /// Configuration error
    ConfigError(String),
}

impl fmt::Display for MailCrushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::ParseError(e) => write!(f, "Mail parsing error: {}", e),
            Self::EmptyMail => write!(f, "Empty mail file"),
            Self::InvalidStructure(e) => write!(f, "Invalid mail structure: {}", e),
            Self::ConfigError(e) => write!(f, "Configuration error: {}", e),
        }
    }
}

impl std::error::Error for MailCrushError {}

impl From<std::io::Error> for MailCrushError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}
