//! Analyze command - detailed email structure analysis

use std::path::Path;
use tracing::info;

use mailcrush::{MailAnalyzer, MailCrushError};

/// Run the analyze command
pub fn run(file: &Path, brief: bool, format: &str) -> Result<(), MailCrushError> {
    info!("Analyzing email: {:?}", file);

    let summary = MailAnalyzer::load_and_analyze(file)?;

    match format {
        "json" => {
            print_json_summary(&summary)?;
        }
        _ => {
            if brief {
                summary.print_brief_summary();
            } else {
                summary.print_detailed_summary();
            }
        }
    }

    Ok(())
}

fn print_json_summary(summary: &mailcrush::MailSummary) -> Result<(), MailCrushError> {
    // Simple JSON output without serde_json dependency for now
    println!("{{");
    println!("  \"subject\": \"{}\",", escape_json(&summary.subject));
    println!("  \"from\": \"{}\",", escape_json(&summary.from));
    println!("  \"date\": \"{}\",", escape_json(&summary.date));
    println!("  \"total_size\": {},", summary.total_size);
    println!("  \"parts_count\": {},", summary.parts.len());
    println!("  \"attachment_count\": {},", summary.attachment_count);
    println!("  \"base64_count\": {},", summary.base64_count);
    println!("  \"structure_depth\": {},", summary.structure_depth);
    println!("  \"parts\": [");

    for (i, part) in summary.parts.iter().enumerate() {
        let comma = if i < summary.parts.len() - 1 { "," } else { "" };
        println!("    {{");
        println!(
            "      \"content_type\": \"{}\",",
            escape_json(&part.content_type)
        );
        if let Some(filename) = &part.filename {
            println!("      \"filename\": \"{}\",", escape_json(filename));
        } else {
            println!("      \"filename\": null,");
        }
        println!("      \"size\": {},", part.size);
        println!("      \"encoded_size\": {},", part.encoded_size);
        println!("      \"encoding\": \"{}\",", escape_json(&part.encoding));
        println!("      \"is_attachment\": {},", part.is_attachment);
        println!("      \"is_base64\": {},", part.is_base64);
        println!("      \"offset_start\": {},", part.offset_start);
        println!("      \"offset_end\": {}", part.offset_end);
        println!("    }}{}", comma);
    }

    println!("  ]");
    println!("}}");

    Ok(())
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
