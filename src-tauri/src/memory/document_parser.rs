//! Document text extraction for various file formats.
//!
//! Supports plain text, Markdown, PDF, and DOCX files.
//! Extracts raw text content for subsequent chunking and storage.

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;

/// Supported document formats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Pdf,
    Docx,
}

/// Detect the document format from file extension.
pub fn detect_format(path: &Path) -> DocumentFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("pdf") => DocumentFormat::Pdf,
        Some("docx") => DocumentFormat::Docx,
        Some("md" | "markdown") => DocumentFormat::Markdown,
        _ => DocumentFormat::PlainText,
    }
}

/// Extract text content from a file, auto-detecting the format.
pub fn extract_text(path: &Path) -> Result<String> {
    let format = detect_format(path);
    extract_text_with_format(path, format)
}

/// Extract text content from a file with an explicit format.
pub fn extract_text_with_format(path: &Path, format: DocumentFormat) -> Result<String> {
    match format {
        DocumentFormat::PlainText | DocumentFormat::Markdown => std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read text file: {}", path.display())),
        DocumentFormat::Pdf => extract_pdf_text(path),
        DocumentFormat::Docx => extract_docx_text(path),
    }
}

/// Extract text from a PDF file using pdf-extract.
fn extract_pdf_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read PDF file: {}", path.display()))?;

    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("Failed to extract text from PDF: {:?}", e))?;

    // Clean up extracted text: normalize whitespace, remove excessive blank lines
    let cleaned = clean_extracted_text(&text);

    tracing::info!(
        "Extracted {} chars from PDF: {}",
        cleaned.len(),
        path.display()
    );

    Ok(cleaned)
}

/// Extract text from a DOCX file by reading the ZIP archive and parsing XML.
fn extract_docx_text(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open DOCX file: {}", path.display()))?;

    let mut archive = zip::ZipArchive::new(file).context("Failed to read DOCX as ZIP archive")?;

    // DOCX stores main content in word/document.xml
    let mut xml_content = String::new();
    archive
        .by_name("word/document.xml")
        .context("DOCX file missing word/document.xml")?
        .read_to_string(&mut xml_content)
        .context("Failed to read word/document.xml")?;

    // Extract text from XML using regex
    let text = extract_text_from_docx_xml(&xml_content);

    // Clean up
    let cleaned = clean_extracted_text(&text);

    tracing::info!(
        "Extracted {} chars from DOCX: {}",
        cleaned.len(),
        path.display()
    );

    Ok(cleaned)
}

/// Extract text content from DOCX XML (word/document.xml).
///
/// DOCX XML uses `<w:t>` elements for text runs and `<w:p>` elements for paragraphs.
/// This function extracts all text content and adds paragraph breaks.
///
/// Uses index-based processing to avoid nested mutable borrow issues with iterators.
fn extract_text_from_docx_xml(xml: &str) -> String {
    let mut result = String::new();
    let mut in_paragraph = false;
    let bytes = xml.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' {
            // Read tag: everything between '<' and '>'
            i += 1;
            let tag_start = i;
            while i < len && bytes[i] != b'>' {
                i += 1;
            }
            let tag = &xml[tag_start..i];
            if i < len {
                i += 1; // skip '>'
            }

            if tag.starts_with("w:t") && !tag.starts_with("w:tbl") {
                // Start of text run -- read until </w:t>
                in_paragraph = true;
                let text_start = i;
                // Find the closing </w:t> tag
                if let Some(pos) = xml[i..].find("</w:t") {
                    let text_content = &xml[text_start..i + pos];
                    // Strip any nested XML tags from the text content
                    let cleaned = strip_xml_tags(text_content);
                    result.push_str(&cleaned);
                    // Skip past the closing </w:t> and its '>'
                    i += pos;
                    while i < len && bytes[i] != b'>' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                }
            } else if tag == "/w:p" || tag.starts_with("/w:p ") {
                // End of paragraph
                if in_paragraph {
                    result.push('\n');
                    in_paragraph = false;
                }
            }
        } else {
            i += 1;
        }
    }

    result
}

/// Strip XML tags from a string, keeping only text content.
fn strip_xml_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

/// Clean up extracted text: normalize whitespace and remove excessive blank lines.
fn clean_extracted_text(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();

    // Trim each line
    lines = lines.iter().map(|l| l.trim()).collect();

    // Remove runs of more than 2 consecutive empty lines
    let mut result = String::new();
    let mut consecutive_empty = 0;

    for line in lines {
        if line.is_empty() {
            consecutive_empty += 1;
            if consecutive_empty <= 2 {
                result.push('\n');
            }
        } else {
            consecutive_empty = 0;
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_pdf() {
        assert_eq!(detect_format(Path::new("doc.pdf")), DocumentFormat::Pdf);
        assert_eq!(detect_format(Path::new("doc.PDF")), DocumentFormat::Pdf);
        assert_eq!(
            detect_format(Path::new("/path/to/doc.pdf")),
            DocumentFormat::Pdf
        );
    }

    #[test]
    fn test_detect_format_docx() {
        assert_eq!(detect_format(Path::new("doc.docx")), DocumentFormat::Docx);
        assert_eq!(detect_format(Path::new("doc.DOCX")), DocumentFormat::Docx);
    }

    #[test]
    fn test_detect_format_markdown() {
        assert_eq!(
            detect_format(Path::new("readme.md")),
            DocumentFormat::Markdown
        );
        assert_eq!(
            detect_format(Path::new("doc.markdown")),
            DocumentFormat::Markdown
        );
    }

    #[test]
    fn test_detect_format_plain_text() {
        assert_eq!(
            detect_format(Path::new("notes.txt")),
            DocumentFormat::PlainText
        );
        assert_eq!(
            detect_format(Path::new("data.csv")),
            DocumentFormat::PlainText
        );
        assert_eq!(
            detect_format(Path::new("no_extension")),
            DocumentFormat::PlainText
        );
    }

    #[test]
    fn test_clean_extracted_text() {
        let input = "  Hello  \n\n\n\n\n  World  \n  ";
        let cleaned = clean_extracted_text(input);
        assert_eq!(cleaned, "Hello\n\n\nWorld");
    }

    #[test]
    fn test_clean_extracted_text_preserves_paragraphs() {
        let input = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let cleaned = clean_extracted_text(input);
        assert_eq!(
            cleaned,
            "First paragraph.\n\nSecond paragraph.\n\nThird paragraph."
        );
    }

    #[test]
    fn test_extract_text_from_docx_xml_simple() {
        let xml = r#"<w:body><w:p><w:r><w:t>Hello World</w:t></w:r></w:p></w:body>"#;
        let text = extract_text_from_docx_xml(xml);
        assert_eq!(text.trim(), "Hello World");
    }

    #[test]
    fn test_extract_text_from_docx_xml_multiple_paragraphs() {
        let xml = r#"<w:body><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:body>"#;
        let text = extract_text_from_docx_xml(xml);
        let lines: Vec<&str> = text.trim().lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "First");
        assert_eq!(lines[1], "Second");
    }

    #[test]
    fn test_extract_text_from_docx_xml_preserve_space() {
        let xml = r#"<w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r><w:r><w:t>World</w:t></w:r></w:p>"#;
        let text = extract_text_from_docx_xml(xml);
        assert_eq!(text.trim(), "Hello World");
    }

    #[test]
    fn test_extract_text_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, this is a test file.").unwrap();

        let text = extract_text(&file_path).unwrap();
        assert_eq!(text, "Hello, this is a test file.");
    }

    #[test]
    fn test_extract_text_markdown_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "# Heading\n\nSome content.").unwrap();

        let text = extract_text(&file_path).unwrap();
        assert_eq!(text, "# Heading\n\nSome content.");
    }

    #[test]
    fn test_extract_text_nonexistent_file() {
        let result = extract_text(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }
}
