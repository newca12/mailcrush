//! Stats command - show compression statistics

use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the stats command for a single file
pub fn run(file: &Path) -> Result<(), MailCrushError> {
    info!("Showing stats for: {:?}", file);

    let summary = MailAnalyzer::load_and_analyze(file)?;

    println!("📊 COMPRESSION STATISTICS");
    println!("{}", "=".repeat(50));

    let total_size = summary.total_size;
    let attachment_size: usize = summary
        .parts
        .iter()
        .filter(|p| p.is_attachment)
        .map(|p| p.encoded_size)
        .sum();

    let base64_overhead: usize = summary
        .parts
        .iter()
        .filter(|p| p.is_base64)
        .map(|p| p.encoded_size.saturating_sub(p.size))
        .sum();

    let header_size = summary.parts.first().map(|p| p.offset_start).unwrap_or(0);

    println!(
        "Total size:        {:>10} bytes ({:.2} KB)",
        total_size,
        total_size as f64 / 1024.0
    );
    println!("Header size:       {:>10} bytes", header_size);
    println!("Attachment size:   {:>10} bytes", attachment_size);
    println!("Base64 overhead:   {:>10} bytes", base64_overhead);
    println!();
    println!(
        "Potential savings: {:>10} bytes ({:.1}%)",
        base64_overhead,
        if total_size > 0 {
            base64_overhead as f64 / total_size as f64 * 100.0
        } else {
            0.0
        }
    );

    Ok(())
}

/// Run aggregate statistics for multiple files
pub fn run_aggregate(files: &[std::path::PathBuf]) -> Result<(), MailCrushError> {
    info!("Computing aggregate stats for {} files", files.len());

    let mut total_files = 0;
    let mut total_size: usize = 0;
    let mut total_parts: usize = 0;
    let mut total_attachments: usize = 0;
    let mut total_base64_parts: usize = 0;
    let mut total_base64_overhead: usize = 0;
    let mut total_attachment_size: usize = 0;
    let mut failed_files = 0;

    for file in files {
        match MailAnalyzer::load_and_analyze(file) {
            Ok(summary) => {
                total_files += 1;
                total_size += summary.total_size;
                total_parts += summary.parts.len();
                total_attachments += summary.attachment_count;
                total_base64_parts += summary.base64_count;

                for part in &summary.parts {
                    if part.is_base64 {
                        total_base64_overhead += part.encoded_size.saturating_sub(part.size);
                    }
                    if part.is_attachment {
                        total_attachment_size += part.encoded_size;
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Failed to process {:?}: {}", file, e);
                failed_files += 1;
            }
        }
    }

    println!("📊 AGGREGATE COMPRESSION STATISTICS");
    println!("{}", "=".repeat(60));
    println!();
    println!("📁 Files processed:    {:>10}", total_files);
    if failed_files > 0 {
        println!("❌ Files failed:       {:>10}", failed_files);
    }
    println!();
    println!(
        "📦 Total size:         {:>10} bytes ({:.2} MB)",
        total_size,
        total_size as f64 / 1024.0 / 1024.0
    );
    println!("📄 Total parts:        {:>10}", total_parts);
    println!("📎 Total attachments:  {:>10}", total_attachments);
    println!("🔐 Base64 parts:       {:>10}", total_base64_parts);
    println!();
    println!(
        "📎 Attachment size:    {:>10} bytes ({:.2} MB)",
        total_attachment_size,
        total_attachment_size as f64 / 1024.0 / 1024.0
    );
    println!(
        "💾 Base64 overhead:    {:>10} bytes ({:.2} MB)",
        total_base64_overhead,
        total_base64_overhead as f64 / 1024.0 / 1024.0
    );
    println!();

    if total_size > 0 {
        let potential_size = total_size.saturating_sub(total_base64_overhead);
        let savings_pct = total_base64_overhead as f64 / total_size as f64 * 100.0;
        println!(
            "💡 Potential savings:  {:>10} bytes ({:.1}%)",
            total_base64_overhead, savings_pct
        );
        println!(
            "📉 Size after decode:  {:>10} bytes ({:.2} MB)",
            potential_size,
            potential_size as f64 / 1024.0 / 1024.0
        );
    }

    if total_files > 0 {
        println!();
        println!("📈 Averages per email:");
        println!("   Size:        {:>10} bytes", total_size / total_files);
        println!("   Parts:       {:>10}", total_parts / total_files);
        println!("   Attachments: {:>10}", total_attachments / total_files);
    }

    Ok(())
}
