//! Info command - show basic email information

use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the info command
pub fn run(file: &Path) -> Result<(), MailCrushError> {
    info!("Getting info for: {:?}", file);

    let summary = MailAnalyzer::load_and_analyze(file)?;

    println!("📧 Email Information");
    println!("{}", "─".repeat(40));
    println!("Subject:     {}", summary.subject);
    println!("From:        {}", summary.from);
    println!("Date:        {}", summary.date);
    println!("Size:        {:.2} KB ({} bytes)", 
             summary.total_size as f64 / 1024.0,
             summary.total_size);
    println!("Parts:       {}", summary.parts.len());
    println!("Attachments: {}", summary.attachment_count);

    if summary.attachment_count > 0 {
        println!("\n📎 Attachments:");
        for part in &summary.parts {
            if let Some(filename) = &part.filename {
                println!("   • {} ({} bytes)", filename, part.size);
            }
        }
    }

    Ok(())
}
