//! Example: extract the readable text from an .eml file.
//!
//! Usage:
//!   cargo run -p mailcrush-extractor --example extract_text --features tokio -- [OPTIONS] <email_file>
//!
//! Options:
//!   --text           Show the plain text version instead of HTML
//!   --html2text      Convert the HTML body to formatted plain text
//!   --attachments    Show attachment names and sizes at the bottom
//!   --all-headers    Show all headers (From, To, Cc, Date, Subject)
//!   --no-separator   Hide the separator line between headers and body

use std::env;
use std::process;

use mailcrush_extractor::get_text_content;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut text = false;
    let mut html2text = false;
    let mut attachments = false;
    let mut all_headers = false;
    let mut separator = true;
    let mut path: Option<String> = None;

    for arg in &args {
        match arg.as_str() {
            "--text" => text = true,
            "--html2text" => html2text = true,
            "--attachments" => attachments = true,
            "--all-headers" => all_headers = true,
            "--no-separator" => separator = false,
            other if !other.starts_with('-') => path = Some(other.to_string()),
            other => {
                eprintln!("Unknown option: {other}");
                process::exit(1);
            }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("Usage: extract_text [--text] [--html2text] [--attachments] <email_file>");
            process::exit(1);
        }
    };

    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read {path}: {e}");
            process::exit(1);
        }
    };

    match get_text_content(
        &content,
        text,
        html2text,
        attachments,
        all_headers,
        separator,
    )
    .await
    {
        Ok(output) => print!("{output}"),
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}
