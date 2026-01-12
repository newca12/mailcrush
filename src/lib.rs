//! MailCrush - A high-efficiency mail lossless compression tool
//!
//! This library provides functionality to analyze, deconstruct, and compress emails
//! for maximum compression efficiency.

pub mod analyzer;
pub mod error;

pub use analyzer::{MailAnalyzer, MailPartSummary, MailSummary, ReconstructionPart};
pub use error::MailCrushError;
