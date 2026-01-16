# MailCrush

A high-efficiency mail **lossless** compression tool that deconstructs emails for maximum compression.

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)


MailCrush is an EDLA project.

The purpose of [edla.org](http://www.edla.org) is to promote the state of the art in various domains.


## Overview

MailCrush is a command-line tool designed to analyze, compress, and decompress email files with maximum efficiency. It deconstructs emails into their MIME parts, applies content-aware compression algorithms, and can perfectly reconstruct the original email byte-for-byte.

### Key Features

- **Lossless Compression**: Perfect reconstruction of original emails verified through hash comparison
- **Content-Aware Compression**: Automatically selects the best compression algorithm (LZ4, Zstd, Gzip) based on content type
- **Base64 Decoding**: Decodes Base64-encoded attachments before compression for better ratios
- **MIME Structure Analysis**: Deep inspection of email structure including nested multipart messages
- **Batch Processing**: Process entire directories of emails recursively
- **Detailed Statistics**: Compression reports with per-part breakdowns

## Installation

### From Source

Requires Rust 2024 edition :

```bash
git clone https://github.com/newca12/mailcrush.git
cd mailcrush
cargo build --release
```

The binary will be available at `target/release/mailcrush`.

## Usage

### Basic Commands

```bash
# Show basic information about an email
mailcrush info email.eml

# Analyze email structure in detail
mailcrush analyze email.eml

# List all parts/attachments
mailcrush list email.eml

# Compress an email
mailcrush compress email.eml -o email.mcr

# Decompress a compressed email
mailcrush read email.mcr -o restored.eml

# Show compression statistics
mailcrush stats email.eml

# Validate email structure
mailcrush validate email.eml

# Extract attachments
mailcrush extract email.eml -o ./attachments/
```

### Command Reference

| Command | Description |
|---------|-------------|
| `analyze` | Analyze email structure and show detailed information |
| `info` | Show basic information about an email |
| `list` | List all parts/attachments in an email |
| `compress` | Compress an email for efficient storage |
| `read` | Read and decompress a compressed mail file (.mcr) |
| `extract` | Extract attachments from an email |
| `validate` | Validate email structure |
| `stats` | Show compression statistics for an email |

### Global Options

- `-v, --verbose`: Enable verbose output
- `-d, --debug`: Enable debug output

### Compress Options

```bash
mailcrush compress <PATH> [OPTIONS]

Options:
  -r, --recursive       Process directories recursively
  -o, --output <PATH>   Output file or directory path
  -l, --level <1-9>     Compression level (default: 6)
  --dry-run             Show what would be compressed without compressing
  -t, --timer           Show timing information
```

### Read Options

```bash
mailcrush read <PATH> [OPTIONS]

Options:
  -r, --recursive       Process directories recursively
  -o, --output <PATH>   Output file or directory for decompressed email(s)
  --raw                 Output raw email content
  --headers-only        Show headers only
  -t, --timer           Show timing information
```

### List Options

```bash
mailcrush list <PATH> [OPTIONS]

Options:
  -r, --recursive       Process directories recursively
  -a, --attachments     Show only attachments
  -b, --base64          Show only Base64 encoded parts
```

### Extract Options

```bash
mailcrush extract <PATH> [OPTIONS]

Options:
  -r, --recursive           Process directories recursively
  -o, --output-dir <PATH>   Output directory for extracted files (default: .)
  -p, --part <INDEX>        Extract specific part by index (1-based)
  -a, --all                 Extract all parts, not just attachments
```

## File Format

MailCrush uses the `.mcr` extension for compressed email files. The format stores:

- Compressed email parts with their metadata
- Original encoding information for perfect reconstruction
- Content hashes for integrity verification

## How It Works

1. **Parse**: The email is parsed using the `mail-parser` library to extract MIME structure
2. **Analyze**: Each part is analyzed for content type, encoding, and compressibility
3. **Decode**: Base64 and quoted-printable encoded parts are decoded to their raw form
4. **Compress**: Each part is compressed using the most suitable algorithm:
   - **LZ4**: Fast compression for text content
   - **Zstd**: Balanced compression for general content
   - **Gzip**: Good general-purpose compression
   - **None**: For already-compressed content (images, archives, etc.)
5. **Verify**: The original email is reconstructed and hash-verified to ensure lossless compression

## Example Compression Report

Here's a real compression report for a sample email with attachments:

```
================================================================================
📦 COMPRESSION REPORT
================================================================================

📋 EMAIL INFO:
  Subject: Remis à 0198410730: MARDI.xls
  Original size: 246027 bytes (240.26 KB)
  Compressed size: 55988 bytes (54.68 KB)
  Compression ratio: 77.2%

✅ VERIFICATION:
  Full email reconstruction: ✓ VERIFIED
  Parts verified: 9/9

🔍 PART DETAILS:
--------------------------------------------------------------------------------
   # | Content-Type                   | Algorithm  |   Original | Compressed |  Savings | Status
--------------------------------------------------------------------------------
   1 | multipart/mixed                | None       |        0 B |        0 B |     0.0% | ✓
   2 | multipart/related              | None       |        0 B |        0 B |     0.0% | ✓
   3 | multipart/alternative          | None       |        0 B |        0 B |     0.0% | ✓
   4 | text/plain                     | Zstd       |      250 B |      208 B |    16.8% | ✓
   5 | text/html                      | Zstd       |     5265 B |     1236 B |    76.5% | ✓
   6 | image/gif (fax_ok.gif)         | None       |    11928 B |     8822 B |    26.0% | ✓
   7 | image/gif (page1.gif)          | None       |    29687 B |    21959 B |    26.0% | ✓
   8 | application/pdf (91624c3d.pdf) | Zstd       |   108527 B |    10531 B |    90.3% | ✓
   9 | application/pdf (Rapport.pdf)  | Zstd       |    87815 B |    13232 B |    84.9% | ✓
--------------------------------------------------------------------------------

📊 SUMMARY:
  Total savings: 190039 bytes (77.2%)
  Verification: ✓ All parts verified successfully

✅ Compressed email saved to: "123270.mcr"
   Original size:         246027 bytes
   Archive size:          70450 bytes
   Space savings:         175577 bytes (71.4%)

   Structure original:    2555 bytes
   Structure compressed:  880 bytes (65.6% saved)
   Parts original:        243472 bytes
   Parts compressed:      55988 bytes
   Parts compression:     77.0%
```

This example shows:
- **77.2% compression ratio** on the email parts
- **71.4% overall space savings** including the archive overhead
- **PDF attachments** compressed with Zstd achieving up to **90.3% savings**
- **Lossless verification** confirming all 9 parts reconstructed correctly

## Dependencies

- [mail-parser](https://crates.io/crates/mail-parser) - Email parsing
- [zstd](https://crates.io/crates/zstd) - Zstandard compression
- [lz4_flex](https://crates.io/crates/lz4_flex) - LZ4 compression
- [flate2](https://crates.io/crates/flate2) - Gzip compression
- [clap](https://crates.io/crates/clap) - Command-line argument parsing
- [sha2](https://crates.io/crates/sha2) - SHA-256 hashing for verification

### License ###
© 2026 Olivier ROLAND. Distributed under the GPLv3 License.
