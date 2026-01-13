//! Validate command - validate email structure

use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the validate command
pub fn run(file: &Path) -> Result<(), MailCrushError> {
    info!("Validating email: {:?}", file);

    match MailAnalyzer::load_and_analyze(file) {
        Ok(summary) => {
            println!("✅ Email is valid");
            println!("   Subject: {}", summary.subject);
            println!("   Parts: {}", summary.parts.len());
            println!("   Attachments: {}", summary.attachment_count);
            Ok(())
        }
        Err(e) => {
            println!("❌ Email validation failed: {}", e);
            Err(e)
        }
    }
}
