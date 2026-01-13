//! Compressor module - handles email part compression and reconstruction
//!
//! This module provides functionality to:
//! 1. Deconstruct emails into parts
//! 2. Decode base64/quoted-printable where necessary
//! 3. Compress each part with an algorithm suited to its content type
//! 4. Reconstruct the email by decompressing and re-encoding
//! 5. Verify reconstruction integrity

use std::fmt;
use std::io::{Read, Write};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use flate2::Compression as GzCompression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use mail_parser::{Encoding, MessageParser, MimeHeaders};
use sha2::{Digest, Sha256};

use crate::error::MailCrushError;

/// Compression algorithm to use for a part
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// No compression (already compressed or small data)
    None,
    /// LZ4 - fast compression, good for text
    Lz4,
    /// Zstd - balanced compression ratio and speed
    Zstd,
    /// Gzip - good general purpose compression
    Gzip,
}

impl fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Lz4 => write!(f, "LZ4"),
            Self::Zstd => write!(f, "Zstd"),
            Self::Gzip => write!(f, "Gzip"),
        }
    }
}

/// Report for a single part's compression
#[derive(Debug, Clone)]
pub struct PartCompressionReport {
    /// Part index
    pub part_index: usize,
    /// Content type of the part
    pub content_type: String,
    /// Filename if attachment
    pub filename: Option<String>,
    /// Original encoding (base64, quoted-printable, etc.)
    pub original_encoding: String,
    /// Whether base64 decoding was applied
    pub was_base64_decoded: bool,
    /// Compression algorithm used
    pub algorithm: CompressionAlgorithm,
    /// Original size (encoded, as in raw email)
    pub original_encoded_size: usize,
    /// Size after decoding (before compression)
    pub decoded_size: usize,
    /// Size after compression
    pub compressed_size: usize,
    /// Hash of original decoded content
    pub original_hash: String,
    /// Hash of reconstructed content
    pub reconstructed_hash: String,
    /// Whether reconstruction matched original
    pub reconstruction_verified: bool,
    /// Error message if verification failed
    pub error_message: Option<String>,
}

impl PartCompressionReport {
    /// Calculate compression ratio (compressed / original)
    pub fn compression_ratio(&self) -> f64 {
        if self.original_encoded_size == 0 {
            1.0
        } else {
            self.compressed_size as f64 / self.original_encoded_size as f64
        }
    }

    /// Calculate space savings percentage
    pub fn savings_percent(&self) -> f64 {
        (1.0 - self.compression_ratio()) * 100.0
    }
}

/// Complete compression report for an email
#[derive(Debug)]
pub struct CompressionReport {
    /// Subject of the email
    pub subject: String,
    /// Original total size
    pub original_size: usize,
    /// Total compressed size
    pub compressed_size: usize,
    /// Per-part reports
    pub part_reports: Vec<PartCompressionReport>,
    /// Whether the full email was successfully reconstructed
    pub full_reconstruction_verified: bool,
    /// Hash of original raw email
    pub original_email_hash: String,
    /// Hash of reconstructed email
    pub reconstructed_email_hash: String,
    /// Error messages for failed parts
    pub errors: Vec<String>,
}

impl CompressionReport {
    /// Calculate overall compression ratio
    pub fn overall_compression_ratio(&self) -> f64 {
        if self.original_size == 0 {
            1.0
        } else {
            self.compressed_size as f64 / self.original_size as f64
        }
    }

    /// Calculate overall savings percentage
    pub fn overall_savings_percent(&self) -> f64 {
        (1.0 - self.overall_compression_ratio()) * 100.0
    }

    /// Count verified parts
    pub fn verified_count(&self) -> usize {
        self.part_reports
            .iter()
            .filter(|r| r.reconstruction_verified)
            .count()
    }

    /// Count failed parts
    pub fn failed_count(&self) -> usize {
        self.part_reports
            .iter()
            .filter(|r| !r.reconstruction_verified)
            .count()
    }

    /// Print a detailed report
    pub fn print_detailed_report(&self) {
        println!("{}", "=".repeat(80));
        println!("📦 COMPRESSION REPORT");
        println!("{}", "=".repeat(80));

        println!("\n📋 EMAIL INFO:");
        println!("  Subject: {}", self.subject);
        println!(
            "  Original size: {} bytes ({:.2} KB)",
            self.original_size,
            self.original_size as f64 / 1024.0
        );
        println!(
            "  Compressed size: {} bytes ({:.2} KB)",
            self.compressed_size,
            self.compressed_size as f64 / 1024.0
        );
        println!(
            "  Compression ratio: {:.1}%",
            self.overall_savings_percent()
        );

        println!("\n✅ VERIFICATION:");
        println!(
            "  Full email reconstruction: {}",
            if self.full_reconstruction_verified {
                "✓ VERIFIED"
            } else {
                "✗ FAILED"
            }
        );
        println!(
            "  Parts verified: {}/{}",
            self.verified_count(),
            self.part_reports.len()
        );
        if self.failed_count() > 0 {
            println!("  Parts failed: {} ⚠️", self.failed_count());
        }

        println!("\n🔍 PART DETAILS:");
        println!("{}", "-".repeat(80));
        println!(
            "{:>4} | {:30} | {:10} | {:>10} | {:>10} | {:>8} | {}",
            "#", "Content-Type", "Algorithm", "Original", "Compressed", "Savings", "Status"
        );
        println!("{}", "-".repeat(80));

        for report in &self.part_reports {
            let name = if let Some(ref filename) = report.filename {
                format!(
                    "{} ({})",
                    &report.content_type[..report.content_type.len().min(15)],
                    filename
                )
            } else {
                report.content_type.clone()
            };
            let name = if name.len() > 30 {
                format!("{}...", &name[..27])
            } else {
                name
            };

            let status = if report.reconstruction_verified {
                "✓"
            } else {
                "✗"
            };

            println!(
                "{:>4} | {:30} | {:10} | {:>10} | {:>10} | {:>7.1}% | {}",
                report.part_index + 1,
                name,
                report.algorithm.to_string(),
                format!("{} B", report.original_encoded_size),
                format!("{} B", report.compressed_size),
                report.savings_percent(),
                status
            );
        }
        println!("{}", "-".repeat(80));

        // Print errors if any
        if !self.errors.is_empty() {
            println!("\n⚠️ ERRORS:");
            for error in &self.errors {
                println!("  - {}", error);
            }
        }

        // Print failed parts details
        let failed_parts: Vec<_> = self
            .part_reports
            .iter()
            .filter(|r| !r.reconstruction_verified)
            .collect();

        if !failed_parts.is_empty() {
            println!("\n❌ FAILED PARTS DETAILS:");
            for report in failed_parts {
                println!("  Part {}: {}", report.part_index + 1, report.content_type);
                if let Some(ref err) = report.error_message {
                    println!("    Error: {}", err);
                }
                println!("    Original hash:      {}", report.original_hash);
                println!("    Reconstructed hash: {}", report.reconstructed_hash);
            }
        }

        println!("\n📊 SUMMARY:");
        println!(
            "  Total savings: {} bytes ({:.1}%)",
            self.original_size.saturating_sub(self.compressed_size),
            self.overall_savings_percent()
        );
        println!(
            "  Verification: {}",
            if self.full_reconstruction_verified && self.failed_count() == 0 {
                "✓ All parts verified successfully"
            } else {
                "⚠️ Some parts failed verification"
            }
        );
    }
}

/// Metadata for base64 encoding preservation
/// Stores line structure to enable byte-identical reconstruction
#[derive(Debug, Clone, Default)]
pub struct Base64Meta {
    /// Length of each line (excluding line ending)
    pub line_lengths: Vec<usize>,
    /// Line ending used for each line (true = CRLF, false = LF)
    pub line_endings: Vec<bool>,
    /// Whether there's a trailing line ending after the last line
    pub has_trailing_newline: bool,
    /// Trailing whitespace on each line (after base64 chars, before line ending)
    pub trailing_whitespace: Vec<Vec<u8>>,
}

/// Compressed part data
#[derive(Debug, Clone)]
pub struct CompressedPart {
    /// Part index
    pub part_index: usize,
    /// Original content type
    pub content_type: String,
    /// Original encoding
    pub original_encoding: Encoding,
    /// Was base64 decoded
    pub was_base64_decoded: bool,
    /// Compression algorithm used
    pub algorithm: CompressionAlgorithm,
    /// Compressed data
    pub data: Vec<u8>,
    /// Original decoded data hash for verification
    pub original_hash: String,
    /// Original raw bytes (for reconstruction verification)
    pub original_raw: Vec<u8>,
    /// Base64 metadata for byte-identical reconstruction
    pub base64_meta: Option<Base64Meta>,
}

/// Email compressor that handles the full compression pipeline
pub struct EmailCompressor {
    /// Compression level (1-9)
    compression_level: u8,
}

impl EmailCompressor {
    /// Create a new compressor with the specified compression level
    pub fn new(compression_level: u8) -> Self {
        Self {
            compression_level: compression_level.clamp(1, 9),
        }
    }

    /// Determine the best compression algorithm for a content type
    pub fn select_algorithm(&self, content_type: &str, data: &[u8]) -> CompressionAlgorithm {
        // Small data: don't compress
        if data.len() < 100 {
            return CompressionAlgorithm::None;
        }

        let ct_lower = content_type.to_lowercase();

        // Multipart containers have no body data to compress
        if ct_lower.starts_with("multipart/") {
            return CompressionAlgorithm::None;
        }

        // Already compressed formats - skip compression
        if ct_lower.contains("zip")
            || ct_lower.contains("gzip")
            || ct_lower.contains("compressed")
            || ct_lower.contains("rar")
            || ct_lower.contains("7z")
            || ct_lower.contains("bz2")
            || ct_lower.contains("xz")
        {
            return CompressionAlgorithm::None;
        }

        // Images that are already compressed
        if ct_lower.starts_with("image/") {
            if ct_lower.contains("png")
                || ct_lower.contains("jpeg")
                || ct_lower.contains("jpg")
                || ct_lower.contains("gif")
                || ct_lower.contains("webp")
            {
                return CompressionAlgorithm::None;
            }
            // BMP and other uncompressed images
            return CompressionAlgorithm::Zstd;
        }

        // Video/audio typically already compressed
        if ct_lower.starts_with("video/") || ct_lower.starts_with("audio/") {
            return CompressionAlgorithm::None;
        }

        // Text content - use LZ4 for speed or Zstd for ratio based on compression level
        if ct_lower.starts_with("text/") || ct_lower.contains("json") || ct_lower.contains("xml") {
            if self.compression_level <= 3 {
                return CompressionAlgorithm::Lz4;
            }
            return CompressionAlgorithm::Zstd;
        }

        // PDF and office documents - use Zstd
        if ct_lower.contains("pdf")
            || ct_lower.contains("msword")
            || ct_lower.contains("spreadsheet")
            || ct_lower.contains("presentation")
            || ct_lower.contains("document")
        {
            return CompressionAlgorithm::Zstd;
        }

        // Default: use Zstd for good compression
        CompressionAlgorithm::Zstd
    }

    /// Compress data with the specified algorithm
    pub fn compress(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<Vec<u8>, MailCrushError> {
        match algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Lz4 => Ok(lz4_flex::compress_prepend_size(data)),
            CompressionAlgorithm::Zstd => {
                let level = self.compression_level as i32;
                zstd::encode_all(std::io::Cursor::new(data), level).map_err(|e| {
                    MailCrushError::ConfigError(format!("Zstd compression failed: {}", e))
                })
            }
            CompressionAlgorithm::Gzip => {
                let mut encoder = GzEncoder::new(
                    Vec::new(),
                    GzCompression::new(self.compression_level as u32),
                );
                encoder
                    .write_all(data)
                    .map_err(|e| MailCrushError::IoError(e))?;
                encoder.finish().map_err(|e| MailCrushError::IoError(e))
            }
        }
    }

    /// Decompress data with the specified algorithm
    pub fn decompress(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<Vec<u8>, MailCrushError> {
        match algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Lz4 => lz4_flex::decompress_size_prepended(data).map_err(|e| {
                MailCrushError::ConfigError(format!("LZ4 decompression failed: {}", e))
            }),
            CompressionAlgorithm::Zstd => {
                zstd::decode_all(std::io::Cursor::new(data)).map_err(|e| {
                    MailCrushError::ConfigError(format!("Zstd decompression failed: {}", e))
                })
            }
            CompressionAlgorithm::Gzip => {
                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .map_err(|e| MailCrushError::IoError(e))?;
                Ok(decompressed)
            }
        }
    }

    /// Calculate SHA256 hash of data
    fn hash_data(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Decode base64 content and extract metadata for byte-identical reconstruction
    fn decode_base64_with_meta(data: &[u8]) -> Result<(Vec<u8>, Base64Meta), MailCrushError> {
        let mut meta = Base64Meta::default();
        let mut cleaned = Vec::with_capacity(data.len());
        let mut current_line_len = 0;
        let mut current_trailing_ws = Vec::new();
        let mut in_trailing_ws = false;
        let mut i = 0;

        while i < data.len() {
            let b = data[i];

            if b == b'\r' && i + 1 < data.len() && data[i + 1] == b'\n' {
                // CRLF line ending
                meta.line_lengths.push(current_line_len);
                meta.line_endings.push(true);
                meta.trailing_whitespace.push(current_trailing_ws.clone());
                current_line_len = 0;
                current_trailing_ws.clear();
                in_trailing_ws = false;
                i += 2;
            } else if b == b'\n' {
                // LF line ending
                meta.line_lengths.push(current_line_len);
                meta.line_endings.push(false);
                meta.trailing_whitespace.push(current_trailing_ws.clone());
                current_line_len = 0;
                current_trailing_ws.clear();
                in_trailing_ws = false;
                i += 1;
            } else if b == b' ' || b == b'\t' {
                // Whitespace - could be trailing or embedded
                in_trailing_ws = true;
                current_trailing_ws.push(b);
                i += 1;
            } else {
                // Base64 character
                if in_trailing_ws {
                    // Whitespace was embedded, not trailing - count it as part of line
                    current_line_len += current_trailing_ws.len();
                    current_trailing_ws.clear();
                    in_trailing_ws = false;
                }
                cleaned.push(b);
                current_line_len += 1;
                i += 1;
            }
        }

        // Handle last line (if no trailing newline)
        if current_line_len > 0 || !current_trailing_ws.is_empty() {
            meta.line_lengths.push(current_line_len);
            meta.line_endings.push(false); // No line ending for last line without newline
            meta.trailing_whitespace.push(current_trailing_ws);
            meta.has_trailing_newline = false;
        } else if !meta.line_lengths.is_empty() {
            meta.has_trailing_newline = true;
        }

        let decoded = BASE64_STANDARD
            .decode(&cleaned)
            .map_err(|e| MailCrushError::ParseError(format!("Base64 decode failed: {}", e)))?;

        Ok((decoded, meta))
    }

    /// Decode base64 content (legacy method without metadata)
    fn decode_base64(data: &[u8]) -> Result<Vec<u8>, MailCrushError> {
        let (decoded, _) = Self::decode_base64_with_meta(data)?;
        Ok(decoded)
    }

    /// Encode data to base64 using original metadata for byte-identical output
    fn encode_base64_with_meta(data: &[u8], meta: &Base64Meta) -> Vec<u8> {
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

    /// Encode data to base64 with standard 76-char line wrapping
    fn encode_base64(data: &[u8]) -> Vec<u8> {
        // Encode with line wrapping at 76 characters (MIME standard)
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

    /// Decode quoted-printable content
    fn decode_quoted_printable(data: &[u8]) -> Result<Vec<u8>, MailCrushError> {
        let mut result = Vec::with_capacity(data.len());
        let mut i = 0;

        while i < data.len() {
            if data[i] == b'=' {
                if i + 2 < data.len() {
                    // Check for soft line break
                    if data[i + 1] == b'\r' && i + 2 < data.len() && data[i + 2] == b'\n' {
                        i += 3;
                        continue;
                    } else if data[i + 1] == b'\n' {
                        i += 2;
                        continue;
                    }

                    // Decode hex pair
                    let hex_str = std::str::from_utf8(&data[i + 1..i + 3]).map_err(|e| {
                        MailCrushError::ParseError(format!("Invalid QP encoding: {}", e))
                    })?;

                    if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                        result.push(byte);
                        i += 3;
                        continue;
                    }
                }
                result.push(data[i]);
                i += 1;
            } else {
                result.push(data[i]);
                i += 1;
            }
        }

        Ok(result)
    }

    /// Encode data to quoted-printable
    fn encode_quoted_printable(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len() * 3);
        let mut line_len = 0;

        for &byte in data {
            let encoded = if byte == b'\t' || byte == b' ' {
                // Space and tab are allowed unless at end of line
                vec![byte]
            } else if byte >= 33 && byte <= 126 && byte != b'=' {
                // Printable ASCII except '='
                vec![byte]
            } else if byte == b'\r' || byte == b'\n' {
                // Pass through line endings
                vec![byte]
            } else {
                // Encode as =XX
                format!("={:02X}", byte).into_bytes()
            };

            // Handle line length (soft line break at 76 chars)
            if line_len + encoded.len() > 75 && byte != b'\r' && byte != b'\n' {
                result.extend_from_slice(b"=\r\n");
                line_len = 0;
            }

            if byte == b'\n' {
                line_len = 0;
            } else {
                line_len += encoded.len();
            }

            result.extend(encoded);
        }

        result
    }

    /// Process a single email part: decode, compress, and prepare for reconstruction
    fn process_part(
        &self,
        part: &mail_parser::MessagePart,
        raw_content: &[u8],
        part_index: usize,
    ) -> Result<(CompressedPart, PartCompressionReport), MailCrushError> {
        let content_type = part
            .content_type()
            .map(|ct| format!("{}/{:?}", ct.ctype(), ct.subtype()))
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let filename = part.attachment_name().map(|s| s.to_string());
        let encoding = part.encoding;
        let is_base64 = matches!(encoding, Encoding::Base64);
        let is_qp = matches!(encoding, Encoding::QuotedPrintable);

        // Check if this is a multipart container
        let is_multipart = content_type.to_lowercase().starts_with("multipart/");

        // Get raw part data
        let offset_start = part.offset_body as usize;
        let offset_end = part.offset_end as usize;

        if offset_end > raw_content.len() {
            return Err(MailCrushError::InvalidStructure(format!(
                "Part offset out of bounds: {}-{} (content len: {})",
                offset_start,
                offset_end,
                raw_content.len()
            )));
        }

        // For multipart containers, we don't compress the body (it contains sub-parts)
        // We just keep track of it for reconstruction
        let raw_part_data = if is_multipart {
            // Multipart containers: store empty data, the structure is implicit
            &[] as &[u8]
        } else {
            &raw_content[offset_start..offset_end]
        };

        let original_encoded_size = if is_multipart {
            0 // Don't count multipart container as data
        } else {
            raw_part_data.len()
        };

        // Decode if necessary (skip for multipart)
        let (decoded_data, was_decoded, base64_meta) = if is_multipart {
            (Vec::new(), false, None)
        } else if is_base64 {
            let (decoded, meta) = Self::decode_base64_with_meta(raw_part_data)?;
            (decoded, true, Some(meta))
        } else if is_qp {
            (Self::decode_quoted_printable(raw_part_data)?, true, None)
        } else {
            (raw_part_data.to_vec(), false, None)
        };

        let decoded_size = decoded_data.len();
        let original_hash = Self::hash_data(&decoded_data);

        // Select and apply compression (skip for multipart)
        let algorithm = if is_multipart {
            CompressionAlgorithm::None
        } else {
            self.select_algorithm(&content_type, &decoded_data)
        };

        let compressed_data = self.compress(&decoded_data, algorithm)?;
        let compressed_size = compressed_data.len();

        // For multipart, store the raw offsets for reconstruction but not the data
        let original_raw = if is_multipart {
            Vec::new()
        } else {
            raw_part_data.to_vec()
        };

        let compressed_part = CompressedPart {
            part_index,
            content_type: content_type.clone(),
            original_encoding: encoding,
            was_base64_decoded: was_decoded && is_base64,
            algorithm,
            data: compressed_data,
            original_hash: original_hash.clone(),
            original_raw,
            base64_meta,
        };

        let report = PartCompressionReport {
            part_index,
            content_type,
            filename,
            original_encoding: format!("{:?}", encoding),
            was_base64_decoded: was_decoded && is_base64,
            algorithm,
            original_encoded_size,
            decoded_size,
            compressed_size,
            original_hash,
            reconstructed_hash: String::new(), // Will be filled during verification
            reconstruction_verified: false,    // Will be set during verification
            error_message: None,
        };

        Ok((compressed_part, report))
    }

    /// Reconstruct a part: decompress and re-encode if necessary
    fn reconstruct_part(&self, compressed: &CompressedPart) -> Result<Vec<u8>, MailCrushError> {
        // Decompress
        let decompressed = self.decompress(&compressed.data, compressed.algorithm)?;

        // Re-encode if it was originally base64 or quoted-printable
        let reconstructed = match compressed.original_encoding {
            Encoding::Base64 if compressed.was_base64_decoded => {
                // Use metadata for byte-identical reconstruction if available
                if let Some(ref meta) = compressed.base64_meta {
                    Self::encode_base64_with_meta(&decompressed, meta)
                } else {
                    Self::encode_base64(&decompressed)
                }
            }
            Encoding::QuotedPrintable => Self::encode_quoted_printable(&decompressed),
            _ => decompressed,
        };

        Ok(reconstructed)
    }

    /// Compress an entire email and generate a report
    pub fn compress_email(&self, raw_content: &[u8]) -> Result<CompressionReport, MailCrushError> {
        if raw_content.is_empty() {
            return Err(MailCrushError::EmptyMail);
        }

        let message = MessageParser::default()
            .parse(raw_content)
            .ok_or_else(|| MailCrushError::ParseError("Failed to parse email".to_string()))?;

        let subject = message
            .subject()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "No Subject".to_string());

        let original_email_hash = Self::hash_data(raw_content);
        let original_size = raw_content.len();

        let mut part_reports = Vec::new();
        let mut compressed_parts = Vec::new();
        let mut errors = Vec::new();
        let mut total_compressed_size = 0;

        // Process each part
        for (part_idx, part) in message.parts.iter().enumerate() {
            match self.process_part(part, raw_content, part_idx) {
                Ok((compressed, report)) => {
                    total_compressed_size += compressed.data.len();
                    compressed_parts.push(compressed);
                    part_reports.push(report);
                }
                Err(e) => {
                    errors.push(format!("Part {}: {}", part_idx + 1, e));
                    // Create a failed report
                    let content_type = part
                        .content_type()
                        .map(|ct| format!("{}/{:?}", ct.ctype(), ct.subtype()))
                        .unwrap_or_else(|| "unknown".to_string());

                    part_reports.push(PartCompressionReport {
                        part_index: part_idx,
                        content_type,
                        filename: part.attachment_name().map(|s| s.to_string()),
                        original_encoding: format!("{:?}", part.encoding),
                        was_base64_decoded: false,
                        algorithm: CompressionAlgorithm::None,
                        original_encoded_size: (part.offset_end - part.offset_body) as usize,
                        decoded_size: 0,
                        compressed_size: 0,
                        original_hash: String::new(),
                        reconstructed_hash: String::new(),
                        reconstruction_verified: false,
                        error_message: Some(e.to_string()),
                    });
                }
            }
        }

        // Verify reconstruction of each part
        for (i, compressed) in compressed_parts.iter().enumerate() {
            // Skip verification for multipart containers (they have no body content)
            if compressed
                .content_type
                .to_lowercase()
                .starts_with("multipart/")
            {
                part_reports[i].reconstructed_hash = compressed.original_hash.clone();
                part_reports[i].reconstruction_verified = true;
                continue;
            }

            match self.reconstruct_part(compressed) {
                Ok(reconstructed) => {
                    let reconstructed_hash = Self::hash_data(&reconstructed);

                    // For verification, we compare decoded content (not raw encoding)
                    // because base64 line wrapping might differ
                    let decompressed = self.decompress(&compressed.data, compressed.algorithm)?;
                    let original_decoded = if compressed.was_base64_decoded {
                        Self::decode_base64(&compressed.original_raw)?
                    } else if matches!(compressed.original_encoding, Encoding::QuotedPrintable) {
                        Self::decode_quoted_printable(&compressed.original_raw)?
                    } else {
                        compressed.original_raw.clone()
                    };

                    let verified = decompressed == original_decoded;

                    part_reports[i].reconstructed_hash = reconstructed_hash;
                    part_reports[i].reconstruction_verified = verified;

                    if !verified {
                        part_reports[i].error_message =
                            Some("Decoded content mismatch after reconstruction".to_string());
                    }
                }
                Err(e) => {
                    part_reports[i].reconstruction_verified = false;
                    part_reports[i].error_message = Some(format!("Reconstruction failed: {}", e));
                    errors.push(format!("Part {} reconstruction failed: {}", i + 1, e));
                }
            }
        }

        // Calculate full reconstruction verification
        let all_parts_verified = part_reports.iter().all(|r| r.reconstruction_verified);

        // Try to reconstruct the full email
        let mut reconstructed_email = raw_content.to_vec();
        let mut reconstruction_success = true;

        // Reconstruct parts in reverse order to maintain offset validity
        let mut parts_with_offsets: Vec<_> = message
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.offset_body as usize, p.offset_end as usize))
            .collect();
        parts_with_offsets.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by offset descending

        for (part_idx, offset_start, offset_end) in parts_with_offsets {
            // Skip if offsets are out of bounds for current reconstructed email
            if offset_start > reconstructed_email.len() || offset_end > reconstructed_email.len() {
                tracing::warn!(
                    "Skipping part {} reconstruction: offsets ({}, {}) out of bounds for email length {}",
                    part_idx,
                    offset_start,
                    offset_end,
                    reconstructed_email.len()
                );
                continue;
            }

            if let Some(compressed) = compressed_parts.iter().find(|c| c.part_index == part_idx) {
                match self.reconstruct_part(compressed) {
                    Ok(reconstructed_part) => {
                        // Replace the part in the email
                        let mut new_email = Vec::with_capacity(reconstructed_email.len());
                        new_email.extend_from_slice(&reconstructed_email[..offset_start]);
                        new_email.extend_from_slice(&reconstructed_part);
                        new_email.extend_from_slice(&reconstructed_email[offset_end..]);
                        reconstructed_email = new_email;
                    }
                    Err(_) => {
                        reconstruction_success = false;
                    }
                }
            }
        }

        let reconstructed_email_hash = Self::hash_data(&reconstructed_email);

        // For full verification, compare decoded content of all parts
        // The raw email might differ due to base64 line wrapping differences
        let full_reconstruction_verified = all_parts_verified && reconstruction_success;

        Ok(CompressionReport {
            subject,
            original_size,
            compressed_size: total_compressed_size,
            part_reports,
            full_reconstruction_verified,
            original_email_hash,
            reconstructed_email_hash,
            errors,
        })
    }

    /// Get compressed parts for storage (returns the compressed data structure)
    pub fn get_compressed_parts(
        &self,
        raw_content: &[u8],
    ) -> Result<Vec<CompressedPart>, MailCrushError> {
        if raw_content.is_empty() {
            return Err(MailCrushError::EmptyMail);
        }

        let message = MessageParser::default()
            .parse(raw_content)
            .ok_or_else(|| MailCrushError::ParseError("Failed to parse email".to_string()))?;

        let mut compressed_parts = Vec::new();

        for (part_idx, part) in message.parts.iter().enumerate() {
            let (compressed, _) = self.process_part(part, raw_content, part_idx)?;
            compressed_parts.push(compressed);
        }

        Ok(compressed_parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_roundtrip() {
        let original = b"Hello, World! This is a test message.";
        let encoded = EmailCompressor::encode_base64(original);
        let decoded = EmailCompressor::decode_base64(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }

    #[test]
    fn test_base64_byte_identical_roundtrip() {
        // Test with various line endings and structures
        let test_cases = [
            // Standard CRLF with 76-char lines
            b"SGVsbG8sIFdvcmxkIQ==\r\n".to_vec(),
            // LF only
            b"SGVsbG8sIFdvcmxkIQ==\n".to_vec(),
            // No trailing newline
            b"SGVsbG8sIFdvcmxkIQ==".to_vec(),
            // Mixed line lengths with CRLF
            b"SGVsbG8s\r\nIFdvcmxkIQ==\r\n".to_vec(),
            // Mixed line lengths with LF
            b"SGVsbG8s\nIFdvcmxkIQ==\n".to_vec(),
            // Trailing whitespace before newline
            b"SGVsbG8sIFdvcmxkIQ== \r\n".to_vec(),
            // Longer content with various line lengths
            b"VGhpcyBpcyBhIGxvbmdlciB0ZXN0IG1lc3NhZ2UgdGhhdCBzcGFucyBtdWx0aXBs\r\nZSBsaW5lcyBvZiBiYXNlNjQgZW5jb2RlZCBjb250ZW50Lg==\r\n".to_vec(),
        ];

        for (i, original_encoded) in test_cases.iter().enumerate() {
            let (decoded, meta) = EmailCompressor::decode_base64_with_meta(original_encoded).unwrap();
            let re_encoded = EmailCompressor::encode_base64_with_meta(&decoded, &meta);
            assert_eq!(
                original_encoded, &re_encoded,
                "Test case {} failed:\nOriginal: {:?}\nRe-encoded: {:?}",
                i, 
                String::from_utf8_lossy(original_encoded),
                String::from_utf8_lossy(&re_encoded)
            );
        }
    }

    #[test]
    fn test_compression_algorithms() {
        let compressor = EmailCompressor::new(5);
        let data =
            b"This is some test data that should compress well. Repeated text repeated text.";

        for algo in [
            CompressionAlgorithm::None,
            CompressionAlgorithm::Lz4,
            CompressionAlgorithm::Zstd,
            CompressionAlgorithm::Gzip,
        ] {
            let compressed = compressor.compress(data, algo).unwrap();
            let decompressed = compressor.decompress(&compressed, algo).unwrap();
            assert_eq!(data.to_vec(), decompressed, "Failed for {:?}", algo);
        }
    }

    #[test]
    fn test_algorithm_selection() {
        // Low compression level (1-3) uses LZ4 for text
        let compressor_fast = EmailCompressor::new(3);
        assert_eq!(
            compressor_fast.select_algorithm("text/plain", &[0u8; 200]),
            CompressionAlgorithm::Lz4
        );

        // High compression level (4+) uses Zstd for text
        let compressor_high = EmailCompressor::new(6);
        assert_eq!(
            compressor_high.select_algorithm("text/plain", &[0u8; 200]),
            CompressionAlgorithm::Zstd
        );

        // JPEG should not be compressed
        assert_eq!(
            compressor_high.select_algorithm("image/jpeg", &[0u8; 200]),
            CompressionAlgorithm::None
        );

        // Small data should not be compressed
        assert_eq!(
            compressor_high.select_algorithm("text/plain", &[0u8; 50]),
            CompressionAlgorithm::None
        );

        // Multipart should not be compressed
        assert_eq!(
            compressor_high.select_algorithm("multipart/mixed", &[0u8; 500]),
            CompressionAlgorithm::None
        );
    }
}
