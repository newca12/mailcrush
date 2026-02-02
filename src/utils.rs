//! Utility functions for file discovery and common operations

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::MailCrushError;

/// Collect email files from a path (file or directory)
///
/// If the path is a file, returns a vector containing just that file.
/// If the path is a directory, returns all .eml files found recursively.
pub fn collect_email_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>, MailCrushError> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.is_dir() {
        return Err(MailCrushError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Path does not exist: {:?}", path),
        )));
    }

    let mut files = Vec::new();
    collect_files_recursive(path, recursive, &mut files)?;

    files.sort();
    Ok(files)
}

fn collect_files_recursive(
    dir: &Path,
    recursive: bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), MailCrushError> {
    let entries = fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            files.push(path);
        } else if path.is_dir() && recursive {
            collect_files_recursive(&path, recursive, files)?;
        }
    }

    Ok(())
}

/// Collect compressed .mcr files from a path (file or directory)
///
/// If the path is a file, returns a vector containing just that file.
/// If the path is a directory, returns all .mcr files found (recursively if specified).
pub fn collect_mcr_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>, MailCrushError> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.is_dir() {
        return Err(MailCrushError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Path does not exist: {:?}", path),
        )));
    }

    let mut files = Vec::new();
    collect_mcr_files_recursive(path, recursive, &mut files)?;

    files.sort();
    Ok(files)
}

fn collect_mcr_files_recursive(
    dir: &Path,
    recursive: bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), MailCrushError> {
    let entries = fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "mcr") {
                files.push(path);
            }
        } else if path.is_dir() && recursive {
            collect_mcr_files_recursive(&path, recursive, files)?;
        }
    }

    Ok(())
}

/// Format a file path for display (relative if possible)
pub fn display_path(path: &Path) -> String {
    path.display().to_string()
}

/// Statistics for batch operations
#[derive(Default)]
pub struct BatchStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub total_time: std::time::Duration,
    /// Total original size in bytes (for compression operations)
    pub total_original_size: u64,
    /// Total archive size in bytes (for compression operations)
    pub total_archive_size: u64,
}

impl BatchStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_success(&mut self) {
        self.success += 1;
    }

    pub fn record_failure(&mut self) {
        self.failed += 1;
    }

    pub fn add_time(&mut self, duration: std::time::Duration) {
        self.total_time += duration;
    }

    pub fn add_compression_stats(&mut self, original_size: u64, archive_size: u64) {
        self.total_original_size += original_size;
        self.total_archive_size += archive_size;
    }

    pub fn print_summary(&self) {
        if self.total > 1 {
            println!();
            println!(
                "📊 Batch Summary: {} processed, {} succeeded, {} failed",
                self.total, self.success, self.failed
            );
            // Print compression stats if available
            if self.total_original_size > 0 {
                let savings = self.total_original_size as i64 - self.total_archive_size as i64;
                let savings_pct = if self.total_original_size > 0 {
                    savings as f64 / self.total_original_size as f64 * 100.0
                } else {
                    0.0
                };
                println!();
                println!(
                    "   Total original size:   {} bytes ({:.2} MB)",
                    self.total_original_size,
                    self.total_original_size as f64 / 1_048_576.0
                );
                println!(
                    "   Total archive size:    {} bytes ({:.2} MB)",
                    self.total_archive_size,
                    self.total_archive_size as f64 / 1_048_576.0
                );
                println!(
                    "   Total space savings:   {} bytes ({:.1}%)",
                    savings, savings_pct
                );
            }
        }
    }
}
