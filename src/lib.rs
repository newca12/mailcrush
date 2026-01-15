//! MailCrush - A high-efficiency mail lossless compression tool
//!
//! This library provides functionality to analyze, deconstruct, and compress emails
//! for maximum compression efficiency.

pub mod analyzer;
pub mod compressor;
pub mod error;
pub mod utils;

pub use analyzer::{MailAnalyzer, MailPartSummary, MailSummary, ReconstructionPart};
pub use compressor::{
    CompressedPart, CompressionAlgorithm, CompressionReport, EmailCompressor, PartCompressionReport,
};
pub use error::MailCrushError;
pub use utils::{BatchStats, collect_email_files, collect_mcr_files};
