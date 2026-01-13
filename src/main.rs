//! MailCrush CLI - A high-efficiency mail lossless compression tool
//!
//! This CLI tool provides commands to analyze, compress, and decompress emails.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use mailcrush::{collect_email_files, BatchStats, MailCrushError};

mod commands;

use commands::{analyze, compress, extract, info as info_cmd, list, read, stats, validate};

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
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Show brief summary only
        #[arg(short, long)]
        brief: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Show basic information about an email
    Info {
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,
    },

    /// List all parts/attachments in an email
    List {
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Show only attachments
        #[arg(short, long)]
        attachments: bool,

        /// Show only Base64 encoded parts
        #[arg(short = 'b', long)]
        base64: bool,
    },

    /// Compress an email for efficient storage
    Compress {
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Output file or directory path
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
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Output directory for extracted files
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,

        /// Extract specific part by index (1-based, only for single file)
        #[arg(short, long)]
        part: Option<usize>,

        /// Extract all parts, not just attachments
        #[arg(short, long)]
        all: bool,
    },

    /// Validate email structure
    Validate {
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,
    },

    /// Show compression statistics for an email
    Stats {
        /// Path to email file or directory
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Process directories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Show aggregate statistics only (for multiple files)
        #[arg(long)]
        aggregate: bool,
    },

    /// Read and decompress a compressed mail file (.mcr)
    Read {
        /// Path to compressed mail file (.mcr)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output file path for decompressed email
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output raw email content
        #[arg(long)]
        raw: bool,

        /// Show headers only
        #[arg(long)]
        headers_only: bool,
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
        Commands::Analyze {
            path,
            recursive,
            brief,
            format,
        } => {
            let files = collect_email_files(&path, recursive)?;
            run_batch(&files, |file| analyze::run(file, brief, &format))?;
        }
        Commands::Info { path, recursive } => {
            let files = collect_email_files(&path, recursive)?;
            run_batch(&files, |file| info_cmd::run(file))?;
        }
        Commands::List {
            path,
            recursive,
            attachments,
            base64,
        } => {
            let files = collect_email_files(&path, recursive)?;
            run_batch(&files, |file| list::run(file, attachments, base64))?;
        }
        Commands::Compress {
            path,
            recursive,
            output,
            level,
            dry_run,
        } => {
            let files = collect_email_files(&path, recursive)?;
            if files.len() > 1 && output.is_some() {
                return Err(MailCrushError::ConfigError(
                    "Cannot specify --output with multiple files. Use a directory as output instead.".to_string()
                ));
            }
            run_batch(&files, |file| compress::run(file, output.as_deref(), level, dry_run))?;
        }
        Commands::Extract {
            path,
            recursive,
            output_dir,
            part,
            all,
        } => {
            let files = collect_email_files(&path, recursive)?;
            if files.len() > 1 && part.is_some() {
                return Err(MailCrushError::ConfigError(
                    "Cannot specify --part with multiple files.".to_string()
                ));
            }
            run_batch(&files, |file| extract::run(file, &output_dir, part, all))?;
        }
        Commands::Validate { path, recursive } => {
            let files = collect_email_files(&path, recursive)?;
            run_batch(&files, |file| validate::run(file))?;
        }
        Commands::Stats {
            path,
            recursive,
            aggregate,
        } => {
            let files = collect_email_files(&path, recursive)?;
            if aggregate && files.len() > 1 {
                stats::run_aggregate(&files)?;
            } else {
                run_batch(&files, |file| stats::run(file))?;
            }
        }
        Commands::Read {
            file,
            output,
            raw,
            headers_only,
        } => {
            read::run(&file, output.as_deref(), raw, headers_only)?;
        }
    }

    Ok(())
}

/// Run a command on multiple files with batch statistics
fn run_batch<F>(files: &[PathBuf], mut op: F) -> Result<(), MailCrushError>
where
    F: FnMut(&std::path::Path) -> Result<(), MailCrushError>,
{
    if files.is_empty() {
        println!("📭 No email files found.");
        return Ok(());
    }

    let mut stats = BatchStats::new();
    stats.total = files.len();

    let show_separator = files.len() > 1;

    for (i, file) in files.iter().enumerate() {
        if show_separator {
            if i > 0 {
                println!();
            }
            println!("━━━ {} ━━━", file.display());
        }

        match op(file) {
            Ok(()) => stats.record_success(),
            Err(e) => {
                eprintln!("❌ Error processing {:?}: {}", file, e);
                stats.record_failure();
            }
        }
    }

    stats.print_summary();

    Ok(())
}
