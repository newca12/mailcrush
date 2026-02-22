//! mailcrush-extractor – extract text content from raw email bytes.
//!
//! This crate exposes the text-extraction logic used by the `mailcrush show`
//! command so that other projects can obtain the readable text of an email
//! without pulling in the full CLI.
//!
//! The output matches the `mailcrush show` command exactly: a
//! Thunderbird/Outlook-style header block, the body (in the requested format),
//! and an optional attachment footer.
//!
//! # Example
//!
//! ```no_run
//! # async fn demo() -> Result<(), mailcrush_extractor::Error> {
//! let raw_mail = std::fs::read("message.eml")?;
//! // Same as `mailcrush show --text --attachments`
//! let text = mailcrush_extractor::get_text_content(&raw_mail, true, false, true, true, true).await?;
//! println!("{text}");
//! # Ok(())
//! # }
//! ```

use std::fmt;
use std::fmt::Write as _;

use mail_parser::{MessageParser, MimeHeaders};

/// Errors returned by [`get_text_content`].
#[derive(Debug)]
pub enum Error {
    /// The supplied byte slice was empty.
    EmptyMail,

    /// `mail-parser` could not parse the message.
    ParseError(String),

    /// The email contains no text or HTML body.
    NoContent,

    /// An I/O error occurred.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMail => write!(f, "empty mail content"),
            Self::ParseError(msg) => write!(f, "failed to parse email: {msg}"),
            Self::NoContent => write!(f, "no text content available in the email"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Extract the formatted text content from raw email bytes.
///
/// The output replicates the `mailcrush show` command:
///
/// - **Header block** – From / To / Cc / Date / Subject followed by a separator.
/// - **Body** – selected by the `text` and `html2text` flags (see below).
/// - **Attachment footer** – listed when `attachments` is `true`.
///
/// # Body mode
///
/// | `text` | `html2text` | Behaviour |
/// |--------|-------------|-----------|
/// | `true` | _ignored_   | Plain-text body; falls back to stripped HTML. |
/// | `false`| `true`      | HTML body converted to plain text via `html2text`; falls back to plain text. |
/// | `false`| `false`     | Raw HTML body; falls back to plain text. |
///
/// # Header options
///
/// - `all_headers` – when `true`, display From, To, Cc, Date and Subject.
///   When `false` (the default-friendly mode), only Subject is shown.
/// - `separator` – when `true`, print a `─` line between the headers and the body.
pub async fn get_text_content(
    file_content: &Vec<u8>,
    text: bool,
    html2text: bool,
    attachments: bool,
    all_headers: bool,
    separator: bool,
) -> Result<String, Error> {
    if file_content.is_empty() {
        return Err(Error::EmptyMail);
    }

    let message = MessageParser::default()
        .parse(file_content)
        .ok_or_else(|| Error::ParseError("mail-parser returned None".to_string()))?;

    let mut out = String::new();

    // --- Header block (Thunderbird / Outlook style) ---
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

    if all_headers {
        let _ = writeln!(out, "From:    {from}");
        let _ = writeln!(out, "To:      {to}");
        if let Some(ref cc_val) = cc {
            let _ = writeln!(out, "Cc:      {cc_val}");
        }
        let _ = writeln!(out, "Date:    {date}");
    }
    let _ = writeln!(out, "Subject: {subject}");
    if separator {
        let _ = writeln!(out, "{}", "─".repeat(78));
    } else {
        let _ = writeln!(out);
    }

    // --- Body ---
    if text {
        match message.body_text(0) {
            Some(body) => {
                let _ = writeln!(out, "{body}");
            }
            None => match message.body_html(0) {
                Some(html) => {
                    let _ = writeln!(
                        out,
                        "[No plain text version available, showing stripped HTML]\n"
                    );
                    let _ = writeln!(out, "{}", strip_html_tags(&html));
                }
                None => {
                    let _ = writeln!(out, "[No text content available]");
                }
            },
        }
    } else if html2text {
        match message.body_html(0) {
            Some(html) => {
                let rendered =
                    html2text::from_read(html.as_bytes(), 78).unwrap_or_else(|_| html.to_string());
                let _ = writeln!(out, "{rendered}");
            }
            None => match message.body_text(0) {
                Some(body) => {
                    let _ = writeln!(out, "[No HTML version available, showing plain text]\n");
                    let _ = writeln!(out, "{body}");
                }
                None => {
                    let _ = writeln!(out, "[No content available]");
                }
            },
        }
    } else {
        // Default: raw HTML
        match message.body_html(0) {
            Some(html) => {
                let _ = writeln!(out, "{html}");
            }
            None => match message.body_text(0) {
                Some(body) => {
                    let _ = writeln!(out, "[No HTML version available, showing plain text]\n");
                    let _ = writeln!(out, "{body}");
                }
                None => {
                    let _ = writeln!(out, "[No content available]");
                }
            },
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
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", "─".repeat(78));
            let _ = writeln!(out, "📎 Attachments ({}):", attachment_parts.len());
            let _ = writeln!(out);

            for part in &attachment_parts {
                let name = part.attachment_name().unwrap_or("(unnamed)");
                let size = part.contents().len();
                let _ = writeln!(out, "   {} ({})", name, format_size(size));
            }
        }
    }

    Ok(out)
}

/// Format a byte size in a human-readable way.
fn format_size(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Very basic HTML tag stripper used as a last-resort fallback.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut inside_tag = false;
    let mut last_was_newline = false;

    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
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

    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_mail_returns_error() {
        let empty: Vec<u8> = vec![];
        assert!(matches!(
            get_text_content(&empty, false, false, false, true, true).await,
            Err(Error::EmptyMail)
        ));
    }
}
