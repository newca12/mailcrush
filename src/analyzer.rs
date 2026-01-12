//! Mail analysis functionality

use mail_parser::*;
use std::fs;
use std::path::Path;

use crate::error::MailCrushError;

/// Summary of a single mail part (MIME part)
#[derive(Debug, Clone)]
pub struct MailPartSummary {
    /// Content type (e.g., "text/plain", "image/png")
    pub content_type: String,
    /// Filename if this is an attachment
    pub filename: Option<String>,
    /// Decoded content size in bytes
    pub size: usize,
    /// Encoded size in bytes (before decoding)
    pub encoded_size: usize,
    /// Encoding type (e.g., "Base64", "QuotedPrintable")
    pub encoding: String,
    /// Whether this part is an attachment
    pub is_attachment: bool,
    /// Whether this part uses Base64 or QuotedPrintable encoding
    pub is_base64: bool,
    /// Start offset in raw email bytes
    pub offset_start: usize,
    /// End offset in raw email bytes
    pub offset_end: usize,
    /// Child parts (for multipart messages)
    pub children: Vec<MailPartSummary>,
}

/// Complete summary of an analyzed email
#[derive(Debug)]
pub struct MailSummary {
    /// Email subject
    pub subject: String,
    /// Sender address
    pub from: String,
    /// Email date
    pub date: String,
    /// Total size in bytes
    pub total_size: usize,
    /// All mail parts
    pub parts: Vec<MailPartSummary>,
    /// Number of attachments
    pub attachment_count: usize,
    /// Number of Base64 encoded parts
    pub base64_count: usize,
    /// Maximum nesting depth of parts
    pub structure_depth: usize,
    /// Raw byte offsets for reconstruction
    pub raw_offsets: Vec<(usize, usize)>,
}

/// Information needed to reconstruct an email part
#[derive(Debug, Clone)]
pub struct ReconstructionPart {
    /// Content type
    pub content_type: String,
    /// Filename if attachment
    pub filename: Option<String>,
    /// Start offset in raw bytes
    pub offset_start: usize,
    /// End offset in raw bytes
    pub offset_end: usize,
    /// Whether Base64 encoded
    pub is_base64: bool,
    /// Whether an attachment
    pub is_attachment: bool,
}

/// Mail analyzer for parsing and analyzing email structure
pub struct MailAnalyzer;

impl MailAnalyzer {
    /// Load and analyze an email file from the filesystem
    pub fn load_and_analyze<P: AsRef<Path>>(path: P) -> Result<MailSummary, MailCrushError> {
        let content = fs::read(path)?;
        Self::analyze_mail(&content)
    }

    /// Analyze email content from raw bytes
    pub fn analyze_mail(content: &[u8]) -> Result<MailSummary, MailCrushError> {
        if content.is_empty() {
            return Err(MailCrushError::EmptyMail);
        }

        let message = MessageParser::default()
            .parse(content)
            .ok_or_else(|| MailCrushError::ParseError("Failed to parse email".to_string()))?;

        let subject = message
            .subject()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "No Subject".to_string());

        let from = message
            .from()
            .and_then(|a| a.first())
            .map(|a| {
                if let Some(name) = a.name() {
                    format!("{} <{}>", name, a.address().unwrap_or(""))
                } else {
                    a.address().unwrap_or("").to_string()
                }
            })
            .unwrap_or_else(|| "Unknown Sender".to_string());

        let date = message
            .date()
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| "Unknown Date".to_string());

        let mut attachment_count = 0;
        let mut base64_count = 0;
        let mut raw_offsets = Vec::new();

        let parts = Self::analyze_part_tree(
            &message,
            content,
            0,
            &mut attachment_count,
            &mut base64_count,
            &mut raw_offsets,
        )?;

        let structure_depth = Self::calculate_max_depth(&parts);

        Ok(MailSummary {
            subject,
            from,
            date,
            total_size: content.len(),
            parts,
            attachment_count,
            base64_count,
            structure_depth,
            raw_offsets,
        })
    }

    fn analyze_part_tree(
        message: &Message,
        raw_content: &[u8],
        _depth: usize,
        attachment_count: &mut usize,
        base64_count: &mut usize,
        raw_offsets: &mut Vec<(usize, usize)>,
    ) -> Result<Vec<MailPartSummary>, MailCrushError> {
        let mut parts = Vec::new();

        for (part_idx, part) in message.parts.iter().enumerate() {
            let part_summary = Self::analyze_single_part(
                part,
                raw_content,
                part_idx,
                attachment_count,
                base64_count,
                raw_offsets,
            )?;
            parts.push(part_summary);
        }

        Ok(parts)
    }

    fn analyze_single_part(
        part: &MessagePart,
        _raw_content: &[u8],
        _part_idx: usize,
        attachment_count: &mut usize,
        base64_count: &mut usize,
        raw_offsets: &mut Vec<(usize, usize)>,
    ) -> Result<MailPartSummary, MailCrushError> {
        let content_type_str = part
            .content_type()
            .map(|ct| format!("{}/{:?}", ct.ctype(), ct.subtype()))
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let is_attachment = part.attachment_name().is_some();
        let encoding = part.encoding;
        let is_base64 = matches!(encoding, Encoding::Base64 | Encoding::QuotedPrintable);

        if is_base64 {
            *base64_count += 1;
        }

        if is_attachment {
            *attachment_count += 1;
        }

        let offset_start = part.offset_body as usize;
        let offset_end = part.offset_end as usize;

        raw_offsets.push((offset_start, offset_end));

        let encoded_size = part.raw_len() as usize;
        let actual_size = part.contents().len();

        let filename = part.attachment_name().map(|s| s.to_string());

        Ok(MailPartSummary {
            content_type: content_type_str,
            filename,
            size: actual_size,
            encoded_size,
            encoding: format!("{:?}", encoding),
            is_attachment,
            is_base64,
            offset_start,
            offset_end,
            children: Vec::new(),
        })
    }

    fn calculate_max_depth(parts: &[MailPartSummary]) -> usize {
        parts
            .iter()
            .map(|part| 1 + Self::calculate_max_depth(&part.children))
            .max()
            .unwrap_or(0)
    }

    /// Extract raw part data for compression
    pub fn extract_part_data<'a>(
        &self,
        part: &MailPartSummary,
        raw_content: &'a [u8],
    ) -> Result<&'a [u8], MailCrushError> {
        if part.offset_end > raw_content.len() {
            return Err(MailCrushError::InvalidStructure(format!(
                "Part offset out of bounds: {}-{}",
                part.offset_start, part.offset_end
            )));
        }

        Ok(&raw_content[part.offset_start..part.offset_end])
    }

    /// Flatten a part tree into a list of reconstruction parts
    pub fn flatten_parts(part: &MailPartSummary) -> Vec<ReconstructionPart> {
        let mut parts = vec![ReconstructionPart {
            content_type: part.content_type.clone(),
            filename: part.filename.clone(),
            offset_start: part.offset_start,
            offset_end: part.offset_end,
            is_base64: part.is_base64,
            is_attachment: part.is_attachment,
        }];

        for child in &part.children {
            parts.extend(Self::flatten_parts(child));
        }

        parts
    }
}

impl MailSummary {
    /// Get a list of reconstruction parts for rebuilding the email
    pub fn get_reconstruction_map(&self) -> Vec<ReconstructionPart> {
        self.parts
            .iter()
            .flat_map(|part| MailAnalyzer::flatten_parts(part))
            .collect()
    }

    /// Print a detailed summary of the email structure
    pub fn print_detailed_summary(&self) {
        println!("{}", "=".repeat(80));
        println!("📧 MAIL STRUCTURE ANALYSIS (with byte offsets)");
        println!("{}", "=".repeat(80));

        println!("\n📋 HEADERS:");
        println!("  Subject: {}", self.subject);
        println!("  From: {}", self.from);
        println!("  Date: {}", self.date);

        println!("\n📊 STATISTICS:");
        println!("  Total size: {:.2} KB", self.total_size as f64 / 1024.0);
        println!("  Parts: {}", self.parts.len());
        println!("  Attachments: {}", self.attachment_count);
        println!("  Base64 encoded parts: {}", self.base64_count);
        println!("  Structure depth: {}", self.structure_depth);
        println!("  Raw offsets tracked: {}", self.raw_offsets.len());

        println!("\n🔍 DETAILED STRUCTURE WITH OFFSETS:");
        for (i, part) in self.parts.iter().enumerate() {
            Self::print_part_with_offsets(part, i, 0);
        }

        self.print_compression_analysis();
        self.print_reconstruction_info();
    }

    /// Print a brief summary of the email
    pub fn print_brief_summary(&self) {
        println!("Subject: {}", self.subject);
        println!("From: {}", self.from);
        println!("Date: {}", self.date);
        println!(
            "Size: {:.2} KB | Parts: {} | Attachments: {}",
            self.total_size as f64 / 1024.0,
            self.parts.len(),
            self.attachment_count
        );
    }

    fn print_part_with_offsets(part: &MailPartSummary, index: usize, depth: usize) {
        let indent = "  ".repeat(depth);
        let prefix = if depth == 0 {
            format!("Part {}:", index + 1)
        } else {
            format!("└─ Subpart {}:", index + 1)
        };

        println!("{}{} {}", indent, prefix, part.content_type);

        if let Some(filename) = &part.filename {
            println!("{}   📎 Attachment: {}", indent, filename);
        }

        println!(
            "{}   📏 Size: {} bytes (encoded: {} bytes)",
            indent, part.size, part.encoded_size
        );
        println!("{}   🔧 Encoding: {}", indent, part.encoding);
        println!(
            "{}   📍 Offsets: {}..{} (length: {})",
            indent,
            part.offset_start,
            part.offset_end,
            part.offset_end - part.offset_start
        );

        if part.is_base64 {
            let overhead = part.encoded_size.saturating_sub(part.size);
            println!("{}   💾 Base64 overhead: {} bytes", indent, overhead);
        }

        for (i, child) in part.children.iter().enumerate() {
            Self::print_part_with_offsets(child, i, depth + 1);
        }
    }

    fn print_compression_analysis(&self) {
        let base64_parts: Vec<&MailPartSummary> = self
            .parts
            .iter()
            .flat_map(|p| Self::flatten_all_parts(p))
            .filter(|p| p.is_base64)
            .collect();

        let total_base64_encoded: usize = base64_parts.iter().map(|p| p.encoded_size).sum();
        let total_base64_original: usize = base64_parts.iter().map(|p| p.size).sum();
        let base64_overhead = total_base64_encoded.saturating_sub(total_base64_original);

        println!("\n💡 COMPRESSION POTENTIAL:");
        println!(
            "  Base64 parts: {} (total encoded: {} bytes)",
            base64_parts.len(),
            total_base64_encoded
        );
        println!("  Base64 original size: {} bytes", total_base64_original);
        println!("  Base64 overhead: {} bytes", base64_overhead);
        println!(
            "  Potential size after decoding: {} bytes",
            self.total_size.saturating_sub(base64_overhead)
        );
        
        if self.total_size > 0 {
            println!(
                "  Potential compression ratio: {:.1}% of original",
                ((self.total_size.saturating_sub(base64_overhead)) as f64 / self.total_size as f64) * 100.0
            );
        }

        if !base64_parts.is_empty() {
            println!("\n  Base64 parts details:");
            for part in base64_parts {
                let overhead = part.encoded_size.saturating_sub(part.size);
                if let Some(filename) = &part.filename {
                    println!(
                        "    - {} ({} bytes → {} bytes, overhead: {})",
                        filename, part.encoded_size, part.size, overhead
                    );
                } else {
                    println!(
                        "    - {} ({} bytes → {} bytes, overhead: {})",
                        part.content_type, part.encoded_size, part.size, overhead
                    );
                }
            }
        }
    }

    fn print_reconstruction_info(&self) {
        println!("\n🔧 RECONSTRUCTION INFO:");
        println!("  Total parts with offsets: {}", self.raw_offsets.len());
        println!("  Offset ranges:");

        for (i, (start, end)) in self.raw_offsets.iter().enumerate() {
            println!("    Part {}: {}..{} ({} bytes)", i + 1, start, end, end - start);
        }
    }

    fn flatten_all_parts(part: &MailPartSummary) -> Vec<&MailPartSummary> {
        let mut parts = vec![part];
        for child in &part.children {
            parts.extend(Self::flatten_all_parts(child));
        }
        parts
    }
}
