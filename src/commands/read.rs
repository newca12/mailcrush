//! Read command - read and decompress a compressed mail file (.mcr)

use std::fs;
use std::path::Path;
use tracing::info;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use mail_parser::{Encoding, MimeHeaders};

use mailcrush::{CompressionAlgorithm, EmailCompressor, MailCrushError};

/// Run the read command to decompress and display a .mcr file
pub fn run(
    file: &Path,
    output: Option<&Path>,
    raw: bool,
    headers_only: bool,
) -> Result<(), MailCrushError> {
    info!("Reading compressed email: {:?}", file);

    // Read the compressed file
    let data = fs::read(file)?;

    // Verify magic number
    if data.len() < 16 {
        return Err(MailCrushError::ParseError(
            "File too small to be a valid MCR file".to_string(),
        ));
    }

    let magic = &data[0..4];
    let version = if magic == b"MCR5" {
        5
    } else if magic == b"MCR4" {
        4
    } else if magic == b"MCR3" {
        3
    } else {
        return Err(MailCrushError::ParseError(format!(
            "Invalid magic number: expected 'MCR3', 'MCR4', or 'MCR5', got '{}'",
            String::from_utf8_lossy(magic)
        )));
    };

    // Parse header
    let original_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let num_parts = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let structure_uncompressed_len =
        u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

    // Read structure data - compressed for MCR5, uncompressed for MCR3/MCR4
    let (structure_data_vec, parts_offset): (Vec<u8>, usize) = if version >= 5 {
        // MCR5: structure is compressed
        if data.len() < 20 {
            return Err(MailCrushError::ParseError(
                "File too small for MCR5 header".to_string(),
            ));
        }
        let compressed_structure_len =
            u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;

        let structure_start = 20;
        let structure_end = structure_start + compressed_structure_len;
        if structure_end > data.len() {
            return Err(MailCrushError::ParseError(
                "File truncated: compressed structure data extends beyond file".to_string(),
            ));
        }

        // Decompress structure
        let compressed_structure = &data[structure_start..structure_end];
        let decompressed =
            zstd::decode_all(std::io::Cursor::new(compressed_structure)).map_err(|e| {
                MailCrushError::ParseError(format!("Failed to decompress structure: {}", e))
            })?;

        info!(
            "MCR{} file: original_size={}, num_parts={}, structure_len={} (compressed: {})",
            version, original_size, num_parts, structure_uncompressed_len, compressed_structure_len
        );

        (decompressed, structure_end)
    } else {
        // MCR3/MCR4: structure is uncompressed
        let structure_start = 16;
        let structure_end = structure_start + structure_uncompressed_len;
        if structure_end > data.len() {
            return Err(MailCrushError::ParseError(
                "File truncated: structure data extends beyond file".to_string(),
            ));
        }

        info!(
            "MCR{} file: original_size={}, num_parts={}, structure_len={}",
            version, original_size, num_parts, structure_uncompressed_len
        );

        (data[structure_start..structure_end].to_vec(), structure_end)
    };

    let structure_data = &structure_data_vec[..];

    // Parse parts
    #[derive(Debug)]
    #[allow(dead_code)]
    struct PartMeta {
        placeholder_offset: usize,
        algorithm: CompressionAlgorithm,
        original_encoding: Encoding,
        was_base64_decoded: bool,
        _original_body_len: usize,
        compressed_data: Vec<u8>,
        _hash: String,
        base64_meta: Option<Base64MetaLocal>,
    }

    let mut parts: Vec<PartMeta> = Vec::with_capacity(num_parts);
    let mut offset = parts_offset;

    for _ in 0..num_parts {
        if offset + 15 > data.len() {
            return Err(MailCrushError::ParseError(
                "File truncated: part header extends beyond file".to_string(),
            ));
        }

        let placeholder_offset = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        let algo_byte = data[offset];
        let algorithm = match algo_byte {
            0 => CompressionAlgorithm::None,
            1 => CompressionAlgorithm::Lz4,
            2 => CompressionAlgorithm::Zstd,
            3 => CompressionAlgorithm::Gzip,
            _ => {
                return Err(MailCrushError::ParseError(format!(
                    "Unknown compression algorithm: {}",
                    algo_byte
                )));
            }
        };
        offset += 1;

        let encoding_byte = data[offset];
        let original_encoding = match encoding_byte {
            0 => Encoding::None,
            1 => Encoding::QuotedPrintable,
            2 => Encoding::Base64,
            _ => {
                return Err(MailCrushError::ParseError(format!(
                    "Unknown encoding: {}",
                    encoding_byte
                )));
            }
        };
        offset += 1;

        let was_base64_decoded = data[offset] != 0;
        offset += 1;

        let original_body_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        let compressed_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + compressed_len > data.len() {
            return Err(MailCrushError::ParseError(
                "File truncated: compressed data extends beyond file".to_string(),
            ));
        }
        let compressed_data = data[offset..offset + compressed_len].to_vec();
        offset += compressed_len;

        // Read hash (64 bytes)
        if offset + 64 > data.len() {
            return Err(MailCrushError::ParseError(
                "File truncated: hash extends beyond file".to_string(),
            ));
        }
        let hash_bytes = &data[offset..offset + 64];
        let hash = String::from_utf8_lossy(hash_bytes)
            .trim_end_matches('\0')
            .to_string();
        offset += 64;

        // Read base64 metadata (MCR4 only)
        let base64_meta = if version >= 4 {
            if offset + 4 > data.len() {
                return Err(MailCrushError::ParseError(
                    "File truncated: base64 meta length extends beyond file".to_string(),
                ));
            }
            let meta_len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            if meta_len > 0 {
                if offset + meta_len > data.len() {
                    return Err(MailCrushError::ParseError(
                        "File truncated: base64 metadata extends beyond file".to_string(),
                    ));
                }
                let meta_bytes = &data[offset..offset + meta_len];
                offset += meta_len;

                // Deserialize base64 metadata
                let mut meta_offset = 0;
                if meta_offset + 4 > meta_bytes.len() {
                    return Err(MailCrushError::ParseError(
                        "Invalid base64 metadata".to_string(),
                    ));
                }
                let num_lines = u32::from_le_bytes([
                    meta_bytes[meta_offset],
                    meta_bytes[meta_offset + 1],
                    meta_bytes[meta_offset + 2],
                    meta_bytes[meta_offset + 3],
                ]) as usize;
                meta_offset += 4;

                let mut line_lengths = Vec::with_capacity(num_lines);
                let mut line_endings = Vec::with_capacity(num_lines);
                let mut trailing_whitespace = Vec::with_capacity(num_lines);

                for _ in 0..num_lines {
                    if meta_offset + 4 > meta_bytes.len() {
                        return Err(MailCrushError::ParseError(
                            "Invalid base64 metadata: truncated line data".to_string(),
                        ));
                    }
                    let line_len =
                        u16::from_le_bytes([meta_bytes[meta_offset], meta_bytes[meta_offset + 1]])
                            as usize;
                    meta_offset += 2;

                    let is_crlf = meta_bytes[meta_offset] != 0;
                    meta_offset += 1;

                    let ws_len = meta_bytes[meta_offset] as usize;
                    meta_offset += 1;

                    let ws = if ws_len > 0 {
                        if meta_offset + ws_len > meta_bytes.len() {
                            return Err(MailCrushError::ParseError(
                                "Invalid base64 metadata: truncated whitespace".to_string(),
                            ));
                        }
                        let ws_bytes = meta_bytes[meta_offset..meta_offset + ws_len].to_vec();
                        meta_offset += ws_len;
                        ws_bytes
                    } else {
                        Vec::new()
                    };

                    line_lengths.push(line_len);
                    line_endings.push(is_crlf);
                    trailing_whitespace.push(ws);
                }

                let has_trailing_newline = if meta_offset < meta_bytes.len() {
                    meta_bytes[meta_offset] != 0
                } else {
                    false
                };

                Some(Base64MetaLocal {
                    line_lengths,
                    line_endings,
                    trailing_whitespace,
                    has_trailing_newline,
                })
            } else {
                None
            }
        } else {
            None
        };

        parts.push(PartMeta {
            placeholder_offset,
            algorithm,
            original_encoding,
            was_base64_decoded,
            _original_body_len: original_body_len,
            compressed_data,
            _hash: hash,
            base64_meta,
        });
    }

    // Reconstruct the email
    let compressor = EmailCompressor::new(6);

    // Sort parts by placeholder offset for proper reconstruction
    let mut indexed_parts: Vec<(usize, &PartMeta)> = parts.iter().enumerate().collect();
    indexed_parts.sort_by_key(|(_, p)| p.placeholder_offset);

    // Build the reconstructed email
    let mut reconstructed = Vec::with_capacity(original_size);
    let mut structure_pos = 0;

    for (_, part) in &indexed_parts {
        // Copy structure data up to this placeholder
        if part.placeholder_offset > structure_pos {
            reconstructed
                .extend_from_slice(&structure_data[structure_pos..part.placeholder_offset]);
        }
        structure_pos = part.placeholder_offset;

        // Decompress the part data
        let decompressed = compressor.decompress(&part.compressed_data, part.algorithm)?;

        // Re-encode if necessary using metadata for byte-identical reconstruction
        let body_data = if part.was_base64_decoded {
            if let Some(ref meta) = part.base64_meta {
                // Use metadata for byte-identical base64 reconstruction
                encode_base64_with_meta(&decompressed, meta)
            } else {
                // Fallback: use standard encoding (not byte-identical)
                encode_base64(&decompressed)
            }
        } else {
            // Non-base64 content: use raw bytes directly (already byte-identical)
            decompressed
        };

        reconstructed.extend_from_slice(&body_data);
    }

    // Copy remaining structure data
    if structure_pos < structure_data.len() {
        reconstructed.extend_from_slice(&structure_data[structure_pos..]);
    }

    // Output the result
    if let Some(output_path) = output {
        fs::write(output_path, &reconstructed)?;
        println!("✅ Decompressed email saved to: {:?}", output_path);
        println!("   Original size: {} bytes", reconstructed.len());
    } else if headers_only {
        // Parse and show headers only
        let content = String::from_utf8_lossy(&reconstructed);
        println!("📧 Email Headers");
        println!("{}", "─".repeat(60));

        for line in content.lines() {
            if line.is_empty() {
                // End of headers
                break;
            }
            println!("{}", line);
        }
    } else if raw {
        // Output raw email content
        print!("{}", String::from_utf8_lossy(&reconstructed));
    } else {
        // Pretty print the email
        print_email_summary(&reconstructed)?;
    }

    Ok(())
}

/// Encode data to base64 with MIME line wrapping
fn encode_base64(data: &[u8]) -> Vec<u8> {
    let encoded = BASE64_STANDARD.encode(data);
    let mut result = Vec::with_capacity(encoded.len() + encoded.len() / 76);

    for (i, chunk) in encoded.as_bytes().chunks(76).enumerate() {
        if i > 0 {
            result.extend_from_slice(b"\r\n");
        }
        result.extend_from_slice(chunk);
    }
    result
}

/// Base64 metadata for byte-identical reconstruction (local copy for deserialization)
#[derive(Debug, Clone, Default)]
struct Base64MetaLocal {
    line_lengths: Vec<usize>,
    line_endings: Vec<bool>,
    trailing_whitespace: Vec<Vec<u8>>,
    has_trailing_newline: bool,
}

/// Encode data to base64 using original metadata for byte-identical output
fn encode_base64_with_meta(data: &[u8], meta: &Base64MetaLocal) -> Vec<u8> {
    let encoded = BASE64_STANDARD.encode(data);
    let encoded_bytes = encoded.as_bytes();
    let mut result = Vec::with_capacity(encoded.len() + meta.line_lengths.len() * 2);
    let mut pos = 0;

    for (i, &line_len) in meta.line_lengths.iter().enumerate() {
        // Add base64 characters for this line
        let end = (pos + line_len).min(encoded_bytes.len());
        if pos < encoded_bytes.len() {
            result.extend_from_slice(&encoded_bytes[pos..end]);
        }
        pos = end;

        // Add trailing whitespace if any
        if i < meta.trailing_whitespace.len() {
            result.extend_from_slice(&meta.trailing_whitespace[i]);
        }

        // Add line ending (except for last line without trailing newline)
        let is_last_line = i == meta.line_lengths.len() - 1;
        if !is_last_line || meta.has_trailing_newline {
            if i < meta.line_endings.len() && meta.line_endings[i] {
                result.extend_from_slice(b"\r\n");
            } else if !is_last_line || meta.has_trailing_newline {
                result.extend_from_slice(b"\n");
            }
        }
    }

    // Handle any remaining encoded data (shouldn't happen with correct metadata)
    if pos < encoded_bytes.len() {
        result.extend_from_slice(&encoded_bytes[pos..]);
    }

    result
}

/// Print a summary of the email content
fn print_email_summary(raw_content: &[u8]) -> Result<(), MailCrushError> {
    let message = mail_parser::MessageParser::default()
        .parse(raw_content)
        .ok_or_else(|| {
            MailCrushError::ParseError("Failed to parse decompressed email".to_string())
        })?;

    println!("📧 Decompressed Email");
    println!("{}", "═".repeat(60));

    // Headers
    println!("\n📋 Headers:");
    println!("{}", "─".repeat(60));

    if let Some(subject) = message.subject() {
        println!("Subject: {}", subject);
    }
    if let Some(from) = message.from().and_then(|a| a.first()) {
        let from_str = if let Some(name) = from.name() {
            format!("{} <{}>", name, from.address().unwrap_or(""))
        } else {
            from.address().unwrap_or("").to_string()
        };
        println!("From:    {}", from_str);
    }
    if let Some(to) = message.to().and_then(|a| a.first()) {
        let to_str = if let Some(name) = to.name() {
            format!("{} <{}>", name, to.address().unwrap_or(""))
        } else {
            to.address().unwrap_or("").to_string()
        };
        println!("To:      {}", to_str);
    }
    if let Some(date) = message.date() {
        println!("Date:    {}", date);
    }

    // Body preview
    println!("\n📝 Body Preview:");
    println!("{}", "─".repeat(60));

    if let Some(text_body) = message.body_text(0) {
        let preview: String = text_body.chars().take(500).collect();
        println!("{}", preview);
        if text_body.len() > 500 {
            println!("... [truncated, {} chars total]", text_body.len());
        }
    } else if let Some(html_body) = message.body_html(0) {
        // Strip HTML tags for preview
        let text = strip_html_tags(&html_body);
        let preview: String = text.chars().take(500).collect();
        println!("{}", preview);
        if text.len() > 500 {
            println!("... [truncated, {} chars total]", text.len());
        }
    } else {
        println!("(No text body found)");
    }

    // Attachments
    let attachments: Vec<(String, usize)> = message
        .attachments()
        .filter_map(|a| a.attachment_name().map(|n| (n.to_string(), a.len())))
        .collect();

    if !attachments.is_empty() {
        println!("\n📎 Attachments:");
        println!("{}", "─".repeat(60));
        for (name, size) in attachments {
            println!("   • {} ({} bytes)", name, size);
        }
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}

/// Simple HTML tag stripper
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    // Clean up whitespace
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
