//! Show command - display email content like Thunderbird/Outlook

use std::fs;
use std::path::Path;
use tracing::info;

use mail_parser::{MessageParser, MimeHeaders};

use mailcrush::MailCrushError;

/// Run the show command to display an email like a mail client would
///
/// - `file`: Path to the email file
/// - `text`: If true, show the plain text version instead of HTML
/// - `html2text`: If true, convert the HTML body to formatted plain text
/// - `attachments`: If true, show attachment names and sizes at the bottom
pub fn run(
    file: &Path,
    text: bool,
    html2text: bool,
    attachments: bool,
) -> Result<(), MailCrushError> {
    info!("Showing email: {:?}", file);

    let content = fs::read(file)?;

    if content.is_empty() {
        return Err(MailCrushError::EmptyMail);
    }

    let message = MessageParser::default()
        .parse(&content)
        .ok_or_else(|| MailCrushError::ParseError("Failed to parse email".to_string()))?;

    // --- Header block (Thunderbird/Outlook style) ---
    let subject = message.subject().unwrap_or("(No Subject)");
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
        .unwrap_or_else(|| "Unknown".to_string());

    let to = message
        .to()
        .map(|addrs| {
            addrs
                .iter()
                .map(|a| {
                    if let Some(name) = a.name() {
                        format!("{} <{}>", name, a.address().unwrap_or(""))
                    } else {
                        a.address().unwrap_or("").to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let cc = message.cc().map(|addrs| {
        addrs
            .iter()
            .map(|a| {
                if let Some(name) = a.name() {
                    format!("{} <{}>", name, a.address().unwrap_or(""))
                } else {
                    a.address().unwrap_or("").to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    });

    let date = message
        .date()
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| "Unknown Date".to_string());

    // Print header
    println!("From:    {}", from);
    println!("To:      {}", to);
    if let Some(ref cc_val) = cc {
        println!("Cc:      {}", cc_val);
    }
    println!("Date:    {}", date);
    println!("Subject: {}", subject);
    println!("{}", "─".repeat(78));

    // --- Body ---
    if text {
        // Show plain text version
        match message.body_text(0) {
            Some(body) => {
                println!("{}", body);
            }
            None => {
                // Fallback: try HTML and strip tags
                match message.body_html(0) {
                    Some(html) => {
                        println!("[No plain text version available, showing stripped HTML]\n");
                        println!("{}", strip_html_tags(&html));
                    }
                    None => {
                        println!("[No text content available]");
                    }
                }
            }
        }
    } else if html2text {
        // Convert HTML to formatted plain text using html2text
        match message.body_html(0) {
            Some(html) => {
                let rendered =
                    html2text::from_read(html.as_bytes(), 78).unwrap_or_else(|_| html.to_string());
                println!("{}", rendered);
            }
            None => {
                // Fallback: show plain text
                match message.body_text(0) {
                    Some(body) => {
                        println!("[No HTML version available, showing plain text]\n");
                        println!("{}", body);
                    }
                    None => {
                        println!("[No content available]");
                    }
                }
            }
        }
    } else {
        // Show HTML version (default)
        match message.body_html(0) {
            Some(html) => {
                println!("{}", html);
            }
            None => {
                // Fallback: show plain text
                match message.body_text(0) {
                    Some(body) => {
                        println!("[No HTML version available, showing plain text]\n");
                        println!("{}", body);
                    }
                    None => {
                        println!("[No content available]");
                    }
                }
            }
        }
    }

    // --- Attachments footer ---
    if attachments {
        let attachment_parts: Vec<_> = message
            .parts
            .iter()
            .filter(|part| part.attachment_name().is_some())
            .collect();

        if !attachment_parts.is_empty() {
            println!();
            println!("{}", "─".repeat(78));
            println!("📎 Attachments ({}):", attachment_parts.len());
            println!();

            for part in &attachment_parts {
                let name = part.attachment_name().unwrap_or("(unnamed)");
                let size = part.contents().len();
                println!("   {} ({})", name, format_size(size));
            }
        }
    }

    Ok(())
}

/// Format a byte size in a human-readable way
fn format_size(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Very basic HTML tag stripper for fallback display
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut inside_tag = false;
    let mut last_was_newline = false;

    for ch in html.chars() {
        match ch {
            '<' => {
                inside_tag = true;
            }
            '>' => {
                inside_tag = false;
            }
            _ if !inside_tag => {
                if ch == '\n' || ch == '\r' {
                    if !last_was_newline {
                        result.push('\n');
                        last_was_newline = true;
                    }
                } else {
                    last_was_newline = false;
                    result.push(ch);
                }
            }
            _ => {}
        }
    }

    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
