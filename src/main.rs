//! MailCrush CLI - A high-efficiency mail lossless compression tool
//!
//! This CLI tool provides commands to analyze, compress, and decompress emails.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use mailcrush::{MailAnalyzer, MailCrushError};

mod commands;

use commands::{analyze, compress, extract, info as info_cmd, list};

/// MailCrush - High-efficiency mail lossless compression tool
#[derive(Parser)]
#[command(name = "mailcrush")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Enable debug output
    #[arg(short, long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze email structure and show detailed information
    Analyze {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Show brief summary only
        #[arg(short, long)]
        brief: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Show basic information about an email
    Info {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    /// List all parts/attachments in an email
    List {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Show only attachments
        #[arg(short, long)]
        attachments: bool,

        /// Show only Base64 encoded parts
        #[arg(short = 'b', long)]
        base64: bool,
    },

    /// Compress an email for efficient storage
    Compress {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Compression level (1-9, default: 6)
        #[arg(short, long, default_value = "6")]
        level: u8,

        /// Dry run - show what would be compressed without actually compressing
        #[arg(long)]
        dry_run: bool,
    },

    /// Extract attachments from an email
    Extract {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output directory for extracted files
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,

        /// Extract specific part by index (1-based)
        #[arg(short, long)]
        part: Option<usize>,

        /// Extract all parts, not just attachments
        #[arg(short, long)]
        all: bool,
    },

    /// Validate email structure
    Validate {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    /// Show compression statistics for an email
    Stats {
        /// Path to the email file (.eml)
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

fn setup_logging(verbose: bool, debug: bool) {
    let level = if debug {
        Level::DEBUG
    } else if verbose {
        Level::INFO
    } else {
        Level::WARN
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

fn main() -> Result<(), MailCrushError> {
    let cli = Cli::parse();

    setup_logging(cli.verbose, cli.debug);

    match cli.command {
        Commands::Analyze { file, brief, format } => {
            analyze::run(&file, brief, &format)?;
        }
        Commands::Info { file } => {
            info_cmd::run(&file)?;
        }
        Commands::List {
            file,
            attachments,
            base64,
        } => {
            list::run(&file, attachments, base64)?;
        }
        Commands::Compress {
            file,
            output,
            level,
            dry_run,
        } => {
            compress::run(&file, output.as_deref(), level, dry_run)?;
        }
        Commands::Extract {
            file,
            output_dir,
            part,
            all,
        } => {
            extract::run(&file, &output_dir, part, all)?;
        }
        Commands::Validate { file } => {
            validate_email(&file)?;
        }
        Commands::Stats { file } => {
            show_stats(&file)?;
        }
    }

    Ok(())
}

fn validate_email(file: &PathBuf) -> Result<(), MailCrushError> {
    info!("Validating email: {:?}", file);

    match MailAnalyzer::load_and_analyze(file) {
        Ok(summary) => {
            println!("✅ Email is valid");
            println!("   Subject: {}", summary.subject);
            println!("   Parts: {}", summary.parts.len());
            println!("   Attachments: {}", summary.attachment_count);
            Ok(())
        }
        Err(e) => {
            println!("❌ Email validation failed: {}", e);
            Err(e)
        }
    }
}

fn show_stats(file: &PathBuf) -> Result<(), MailCrushError> {
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

    let header_size = summary
        .parts
        .first()
        .map(|p| p.offset_start)
        .unwrap_or(0);

    println!("Total size:        {:>10} bytes ({:.2} KB)", total_size, total_size as f64 / 1024.0);
    println!("Header size:       {:>10} bytes", header_size);
    println!("Attachment size:   {:>10} bytes", attachment_size);
    println!("Base64 overhead:   {:>10} bytes", base64_overhead);
    println!();
    println!("Potential savings: {:>10} bytes ({:.1}%)", 
             base64_overhead,
             if total_size > 0 { base64_overhead as f64 / total_size as f64 * 100.0 } else { 0.0 });

    Ok(())
}
