//! Show command - display email content like Thunderbird/Outlook

use std::fs;
use std::path::Path;
use tracing::info;

use mailcrush::MailCrushError;

/// Run the show command to display an email like a mail client would
///
/// - `file`: Path to the email file
/// - `text`: If true, show the plain text version instead of HTML
/// - `html2text`: If true, convert the HTML body to formatted plain text
/// - `attachments`: If true, show attachment names and sizes at the bottom
/// - `all_headers`: If true, show all headers (From, To, Cc, Date, Subject); otherwise only From and Subject
/// - `separator`: If true, print a separator line between headers and body
pub fn run(
    file: &Path,
    text: bool,
    html2text: bool,
    attachments: bool,
    all_headers: bool,
    separator: bool,
) -> Result<(), MailCrushError> {
    info!("Showing email: {:?}", file);

    let content = fs::read(file)?;

    let rt = tokio::runtime::Runtime::new().map_err(MailCrushError::IoError)?;
    let output = rt.block_on(mailcrush_extractor::get_text_content(
        &content,
        text,
        html2text,
        attachments,
        all_headers,
        separator,
    ))?;

    print!("{output}");

    Ok(())
}
