//! Compress command - compress email for efficient storage

use std::fs;
use std::path::Path;
use tracing::info;

use mailcrush::{EmailCompressor, MailAnalyzer, MailCrushError};

/// Run the compress command
pub fn run(
    file: &Path,
    output: Option<&Path>,
    level: u8,
    dry_run: bool,
) -> Result<(), MailCrushError> {
    info!("Compressing email: {:?}", file);

    // Read raw email content
    let raw_content = fs::read(file)?;

    if dry_run {
        // Dry run: show analysis without actually compressing
        let summary = MailAnalyzer::load_and_analyze(file)?;

        let base64_overhead: usize = summary
            .parts
            .iter()
            .filter(|p| p.is_base64)
            .map(|p| p.encoded_size.saturating_sub(p.size))
            .sum();

        let potential_size = summary.total_size.saturating_sub(base64_overhead);

        println!("🔍 Compression Analysis (Dry Run)");
        println!("{}", "─".repeat(50));
        println!("Input file:       {:?}", file);
        println!(
            "Original size:    {} bytes ({:.2} KB)",
            summary.total_size,
            summary.total_size as f64 / 1024.0
        );
        println!("Base64 parts:     {}", summary.base64_count);
        println!("Base64 overhead:  {} bytes", base64_overhead);
        println!("Compression level: {}", level);
        println!();
        println!("📊 Estimated Results:");
        println!(
            "  After Base64 decoding: {} bytes ({:.2} KB)",
            potential_size,
            potential_size as f64 / 1024.0
        );

        if summary.total_size > 0 {
            let savings_pct = base64_overhead as f64 / summary.total_size as f64 * 100.0;
            println!(
                "  Potential savings:     {} bytes ({:.1}%)",
                base64_overhead, savings_pct
            );
        }

        println!();
        println!("💡 Note: Additional compression with zstd/lz4 would further reduce size.");

        // Show algorithm selection preview
        println!();
        println!("📦 Compression Algorithm Selection:");
        let compressor = EmailCompressor::new(level);
        for (i, part) in summary.parts.iter().enumerate() {
            let sample_data = vec![0u8; part.size.max(100)];
            let algo = compressor.select_algorithm(&part.content_type, &sample_data);
            let name = part.filename.as_deref().unwrap_or(&part.content_type);
            println!("  Part {}: {} → {}", i + 1, name, algo);
        }

        return Ok(());
    }

    // Create compressor and process email
    let compressor = EmailCompressor::new(level);
    let report = compressor.compress_email(&raw_content)?;

    // Print the detailed report
    report.print_detailed_report();

    // Determine output path
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let mut path = file.to_path_buf();
            path.set_extension("mcr"); // mailcrush format
            path
        }
    };

    // Store compressed data if verification passed
    if report.full_reconstruction_verified {
        // Get compressed parts for storage
        let compressed_parts = compressor.get_compressed_parts(&raw_content)?;

        // MCR v3 format - efficient storage for decompression:
        // [4 bytes: magic "MCR3"]
        // [4 bytes: original size]
        // [4 bytes: number of content parts]
        // [4 bytes: structure_data length]
        // [N bytes: structure_data (email without body content, bodies replaced with placeholders)]
        // For each content part:
        //   [4 bytes: placeholder_offset - where in structure_data this part's placeholder is]
        //   [1 byte: algorithm]
        //   [1 byte: original_encoding]
        //   [1 byte: was_base64_decoded]
        //   [4 bytes: original_body_length (for reconstruction)]
        //   [4 bytes: compressed_data_length]
        //   [N bytes: compressed_data]
        //   [64 bytes: sha256 hash]

        // Parse to get offsets
        let message = mail_parser::MessageParser::default()
            .parse(&raw_content)
            .ok_or_else(|| {
                MailCrushError::ParseError("Failed to parse email for storage".to_string())
            })?;

        // Build structure data by replacing body content with markers
        // We'll use a simple approach: store everything and track where bodies are

        // Collect all content parts with their offsets
        #[allow(dead_code)]
        struct PartInfo {
            part_index: usize,
            offset_body: usize,
            offset_end: usize,
            compressed_part_idx: usize,
        }

        let mut part_infos: Vec<PartInfo> = Vec::new();
        for (cp_idx, cp) in compressed_parts.iter().enumerate() {
            if cp.content_type.to_lowercase().starts_with("multipart/") {
                continue;
            }
            if let Some(msg_part) = message.parts.get(cp.part_index) {
                part_infos.push(PartInfo {
                    part_index: cp.part_index,
                    offset_body: msg_part.offset_body as usize,
                    offset_end: msg_part.offset_end as usize,
                    compressed_part_idx: cp_idx,
                });
            }
        }

        // Sort by offset for proper reconstruction
        part_infos.sort_by_key(|p| p.offset_body);

        // Build structure: original email with bodies replaced by 0-length markers
        // We'll store offset mappings so we can reconstruct
        let mut structure_data = Vec::new();
        let mut last_end = 0usize;
        let mut placeholder_offsets: Vec<usize> = Vec::new();

        for info in &part_infos {
            // Copy everything from last_end to this body start
            structure_data.extend_from_slice(&raw_content[last_end..info.offset_body]);
            // Record where this placeholder is
            placeholder_offsets.push(structure_data.len());
            // Skip the body content (don't copy it)
            last_end = info.offset_end;
        }
        // Copy remainder of email
        structure_data.extend_from_slice(&raw_content[last_end..]);

        let mut output_data = Vec::new();

        // Magic number (version 3)
        output_data.extend_from_slice(b"MCR3");

        // Original size
        output_data.extend_from_slice(&(raw_content.len() as u32).to_le_bytes());

        // Number of content parts
        output_data.extend_from_slice(&(part_infos.len() as u32).to_le_bytes());

        // Structure data
        output_data.extend_from_slice(&(structure_data.len() as u32).to_le_bytes());
        output_data.extend_from_slice(&structure_data);

        // Write each content part
        for (i, info) in part_infos.iter().enumerate() {
            let part = &compressed_parts[info.compressed_part_idx];

            // Placeholder offset
            output_data.extend_from_slice(&(placeholder_offsets[i] as u32).to_le_bytes());

            // Algorithm
            let algo_byte = match part.algorithm {
                mailcrush::CompressionAlgorithm::None => 0u8,
                mailcrush::CompressionAlgorithm::Lz4 => 1u8,
                mailcrush::CompressionAlgorithm::Zstd => 2u8,
                mailcrush::CompressionAlgorithm::Gzip => 3u8,
            };
            output_data.push(algo_byte);

            // Original encoding
            let encoding_byte = match part.original_encoding {
                mail_parser::Encoding::None => 0u8,
                mail_parser::Encoding::QuotedPrintable => 1u8,
                mail_parser::Encoding::Base64 => 2u8,
            };
            output_data.push(encoding_byte);

            // was_base64_decoded
            output_data.push(if part.was_base64_decoded { 1 } else { 0 });

            // Original body length (needed to know how much space the body takes)
            let original_body_len = info.offset_end - info.offset_body;
            output_data.extend_from_slice(&(original_body_len as u32).to_le_bytes());

            // Compressed data length and data
            output_data.extend_from_slice(&(part.data.len() as u32).to_le_bytes());
            output_data.extend_from_slice(&part.data);

            // Hash (64 bytes)
            let hash_bytes = part.original_hash.as_bytes();
            let mut hash_padded = [0u8; 64];
            let copy_len = hash_bytes.len().min(64);
            hash_padded[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);
            output_data.extend_from_slice(&hash_padded);
        }

        // Write output file
        fs::write(&output_path, &output_data)?;

        // Calculate statistics
        let parts_original_size: usize = part_infos
            .iter()
            .map(|p| p.offset_end - p.offset_body)
            .sum();
        let parts_compressed_size: usize = part_infos
            .iter()
            .map(|p| compressed_parts[p.compressed_part_idx].data.len())
            .sum();

        let savings = raw_content.len() as i64 - output_data.len() as i64;
        let savings_pct = if raw_content.len() > 0 {
            savings as f64 / raw_content.len() as f64 * 100.0
        } else {
            0.0
        };

        println!();
        println!("✅ Compressed email saved to: {:?}", output_path);
        println!("   Original size:         {} bytes", raw_content.len());
        println!("   Archive size:          {} bytes", output_data.len());
        println!(
            "   Space savings:         {} bytes ({:.1}%)",
            savings, savings_pct
        );
        println!();
        println!("   Structure size:        {} bytes", structure_data.len());
        println!("   Parts original:        {} bytes", parts_original_size);
        println!("   Parts compressed:      {} bytes", parts_compressed_size);
        if parts_original_size > 0 {
            println!(
                "   Parts compression:     {:.1}%",
                (1.0 - parts_compressed_size as f64 / parts_original_size as f64) * 100.0
            );
        }
    } else {
        println!();
        println!("⚠️ Compression verification failed!");
        println!("   Some parts could not be verified for lossless reconstruction.");
        println!("   The compressed file was NOT saved to preserve data integrity.");

        return Err(MailCrushError::ConfigError(
            "Reconstruction verification failed - compression aborted".to_string(),
        ));
    }

    Ok(())
}
