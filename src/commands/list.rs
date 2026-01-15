//! List command - list email parts and attachments

use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the list command
pub fn run(file: &Path, attachments_only: bool, base64_only: bool) -> Result<(), MailCrushError> {
    info!("Listing parts for: {:?}", file);

    let summary = MailAnalyzer::load_and_analyze(file)?;

    if attachments_only {
        println!("📎 Attachments in {}", file.display());
        println!("{}", "─".repeat(60));

        let attachments: Vec<_> = summary.parts.iter().filter(|p| p.is_attachment).collect();

        if attachments.is_empty() {
            println!("No attachments found.");
        } else {
            println!(
                "{:<4} {:<30} {:<15} {:<10}",
                "#", "Filename", "Type", "Size"
            );
            println!("{}", "─".repeat(60));

            for (i, part) in attachments.iter().enumerate() {
                let filename = part.filename.as_deref().unwrap_or("unnamed");
                let short_type = part
                    .content_type
                    .split('/')
                    .next_back()
                    .unwrap_or(&part.content_type);
                println!(
                    "{:<4} {:<30} {:<15} {} bytes",
                    i + 1,
                    truncate(filename, 28),
                    truncate(short_type, 13),
                    part.size
                );
            }
        }
    } else if base64_only {
        println!("🔐 Base64 Encoded Parts in {}", file.display());
        println!("{}", "─".repeat(70));

        let base64_parts: Vec<_> = summary.parts.iter().filter(|p| p.is_base64).collect();

        if base64_parts.is_empty() {
            println!("No Base64 encoded parts found.");
        } else {
            println!(
                "{:<4} {:<25} {:<12} {:<12} {:<10}",
                "#", "Type", "Encoded", "Decoded", "Overhead"
            );
            println!("{}", "─".repeat(70));

            for (i, part) in base64_parts.iter().enumerate() {
                let overhead = part.encoded_size.saturating_sub(part.size);
                println!(
                    "{:<4} {:<25} {:<12} {:<12} {} bytes",
                    i + 1,
                    truncate(&part.content_type, 23),
                    format!("{} B", part.encoded_size),
                    format!("{} B", part.size),
                    overhead
                );
            }
        }
    } else {
        println!("📦 All Parts in {}", file.display());
        println!("{}", "─".repeat(80));
        println!(
            "{:<4} {:<25} {:<12} {:<12} {:<10} {:<6}",
            "#", "Type", "Encoding", "Size", "Attach?", "B64?"
        );
        println!("{}", "─".repeat(80));

        for (i, part) in summary.parts.iter().enumerate() {
            let attach = if part.is_attachment { "Yes" } else { "No" };
            let b64 = if part.is_base64 { "Yes" } else { "No" };

            println!(
                "{:<4} {:<25} {:<12} {:<12} {:<10} {:<6}",
                i + 1,
                truncate(&part.content_type, 23),
                truncate(&part.encoding, 10),
                format!("{} B", part.size),
                attach,
                b64
            );

            if let Some(filename) = &part.filename {
                println!("     └─ 📎 {}", filename);
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len - 1])
    } else {
        s.to_string()
    }
}
