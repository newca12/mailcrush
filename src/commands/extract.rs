//! Extract command - extract attachments from email

use std::fs;
use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the extract command
pub fn run(
    file: &Path,
    output_dir: &Path,
    part_index: Option<usize>,
    all: bool,
) -> Result<(), MailCrushError> {
    info!("Extracting from email: {:?}", file);

    // Load raw content for extraction
    let raw_content = fs::read(file)?;
    let summary = MailAnalyzer::load_and_analyze(file)?;

    // Ensure output directory exists
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    let parts_to_extract: Vec<_> = if let Some(idx) = part_index {
        // Extract specific part (1-based index)
        if idx == 0 || idx > summary.parts.len() {
            return Err(MailCrushError::InvalidStructure(format!(
                "Part index {} is out of range (1-{})",
                idx,
                summary.parts.len()
            )));
        }
        vec![(idx, &summary.parts[idx - 1])]
    } else if all {
        // Extract all parts
        summary.parts.iter().enumerate().map(|(i, p)| (i + 1, p)).collect()
    } else {
        // Extract only attachments
        summary
            .parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_attachment)
            .map(|(i, p)| (i + 1, p))
            .collect()
    };

    if parts_to_extract.is_empty() {
        println!("📭 No parts to extract.");
        return Ok(());
    }

    let total_parts = parts_to_extract.len();
    println!("📤 Extracting {} part(s) to {:?}", total_parts, output_dir);
    println!("{}", "─".repeat(50));

    let analyzer = MailAnalyzer;
    let mut extracted_count = 0;

    for (idx, part) in &parts_to_extract {
        // Determine filename
        let filename = if let Some(name) = &part.filename {
            name.clone()
        } else {
            // Generate filename from content type
            let ext = guess_extension(&part.content_type);
            format!("part_{}.{}", idx, ext)
        };

        let output_path = output_dir.join(&filename);

        // Extract content using offsets
        match analyzer.extract_part_data(part, &raw_content) {
            Ok(data) => {
                // For now, write the raw (possibly encoded) data
                // TODO: Decode Base64/QuotedPrintable before writing
                fs::write(&output_path, data)?;
                
                println!("  ✅ {} ({} bytes)", filename, data.len());
                
                if part.is_base64 {
                    println!("     ⚠️  Note: Data is still Base64 encoded");
                }
                
                extracted_count += 1;
            }
            Err(e) => {
                println!("  ❌ Failed to extract {}: {}", filename, e);
            }
        }
    }

    println!("{}", "─".repeat(50));
    println!("📊 Extracted {} of {} part(s)", extracted_count, total_parts);

    Ok(())
}

fn guess_extension(content_type: &str) -> &'static str {
    let lower = content_type.to_lowercase();
    
    if lower.contains("text/plain") {
        "txt"
    } else if lower.contains("text/html") {
        "html"
    } else if lower.contains("image/png") {
        "png"
    } else if lower.contains("image/jpeg") || lower.contains("image/jpg") {
        "jpg"
    } else if lower.contains("image/gif") {
        "gif"
    } else if lower.contains("application/pdf") {
        "pdf"
    } else if lower.contains("application/zip") {
        "zip"
    } else if lower.contains("application/json") {
        "json"
    } else if lower.contains("application/xml") || lower.contains("text/xml") {
        "xml"
    } else {
        "bin"
    }
}
