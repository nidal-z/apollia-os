//! The Word reader: the paragraphs and the table rows of `word/document.xml`.

use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::reader::office::{push_reference, push_text, OfficeArchive};
use crate::reader::ReadError;

/// Extensions [`read_word`] opens: a Word document, with or without macros.
/// The legacy binary `.doc` is not one of them.
pub const WORD_EXTENSIONS: [&str; 2] = ["docx", "docm"];

const MAIN_PART: &str = "word/document.xml";

/// A Word document read as text: its paragraphs and table rows, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDocument {
    /// Every paragraph and every table row that holds text, in the order of
    /// the document.
    pub segments: Vec<WordSegment>,
}

/// One paragraph, or one row of a table, of a Word document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSegment {
    /// Rank of the segment in the body, from 1. Every paragraph of the body
    /// and every row of a table of the body takes one rank, an empty paragraph
    /// included, so a rank stays where the document puts it.
    pub rank: u32,
    /// Whether the segment is a paragraph or a table row.
    pub kind: SegmentKind,
    /// The text: a paragraph as written, a tab and a line break kept; a table
    /// row as its cells separated by tabs.
    pub text: String,
}

/// What a segment of a Word document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentKind {
    /// A paragraph of the body.
    Paragraph,
    /// A row of a table of the body; a table nested in a cell is read as part
    /// of that cell.
    TableRow,
}

/// Reads the Word document at `path`: the text of its paragraphs and of its
/// table rows, in the order of the document, each with its rank.
///
/// Headers, footers, comments, footnotes and floating text boxes are not read.
///
/// # Errors
///
/// [`ReadError::UnsupportedFormat`] when the extension is none of
/// [`WORD_EXTENSIONS`] (a legacy `.doc` among them), [`ReadError::Io`] when the
/// file cannot be read, [`ReadError::ArchiveOverCap`] when the archive goes
/// over a cap, and [`ReadError::Corrupt`] when it is not a Word document.
pub fn read_word(path: &Path) -> Result<WordDocument, ReadError> {
    let mut archive = OfficeArchive::open(path, &WORD_EXTENSIONS, "docx, docm")?;
    let xml = archive
        .read_part(MAIN_PART)?
        .ok_or_else(|| ReadError::corrupt(archive.path(), format!("no {MAIN_PART} part")))?;
    let segments = parse_document(&xml)
        .map_err(|detail| ReadError::corrupt(path, format!("{MAIN_PART}: {detail}")))?;
    Ok(WordDocument { segments })
}

/// Where the text read goes, and how far the reader is in the body.
#[derive(Default)]
struct Body {
    segments: Vec<WordSegment>,
    rank: u32,
    paragraph: String,
    row: Vec<String>,
    cell: String,
    table_depth: u32,
    text_box_depth: u32,
    properties_depth: u32,
    in_text: bool,
}

impl Body {
    fn sink(&mut self) -> &mut String {
        if self.table_depth > 0 {
            &mut self.cell
        } else {
            &mut self.paragraph
        }
    }

    fn push_break(&mut self, in_paragraph: char) {
        if self.table_depth > 0 {
            self.cell_separator();
        } else {
            self.paragraph.push(in_paragraph);
        }
    }

    fn cell_separator(&mut self) {
        if !self.cell.is_empty() && !self.cell.ends_with(' ') {
            self.cell.push(' ');
        }
    }

    fn emit(&mut self, kind: SegmentKind, text: String) {
        self.rank = self.rank.saturating_add(1);
        if !text.trim().is_empty() {
            self.segments.push(WordSegment {
                rank: self.rank,
                kind,
                text,
            });
        }
    }

    fn start(&mut self, name: &[u8]) {
        match name {
            b"w:tbl" => self.table_depth += 1,
            b"w:tr" if self.table_depth == 1 => self.row.clear(),
            b"w:tc" if self.table_depth == 1 => self.cell.clear(),
            b"w:pPr" => self.properties_depth += 1,
            b"w:t" => self.in_text = true,
            _ => {}
        }
    }

    fn empty(&mut self, name: &[u8]) {
        if self.properties_depth > 0 {
            return;
        }
        match name {
            b"w:p" if self.table_depth == 0 => self.emit(SegmentKind::Paragraph, String::new()),
            b"w:tab" => self.push_break('\t'),
            b"w:br" | b"w:cr" => self.push_break('\n'),
            b"w:noBreakHyphen" => self.sink().push('-'),
            _ => {}
        }
    }

    fn end(&mut self, name: &[u8]) {
        match name {
            b"w:t" => self.in_text = false,
            b"w:pPr" => self.properties_depth = self.properties_depth.saturating_sub(1),
            b"w:p" if self.table_depth == 0 => {
                let text = std::mem::take(&mut self.paragraph);
                self.emit(SegmentKind::Paragraph, text);
            }
            b"w:p" => self.cell_separator(),
            b"w:tc" if self.table_depth == 1 => {
                let cell = std::mem::take(&mut self.cell);
                self.row.push(cell.trim().to_owned());
            }
            b"w:tc" => self.cell_separator(),
            b"w:tr" if self.table_depth == 1 => {
                let row = std::mem::take(&mut self.row);
                let text = if row.iter().all(String::is_empty) {
                    String::new()
                } else {
                    row.join("\t")
                };
                self.emit(SegmentKind::TableRow, text);
            }
            b"w:tbl" => self.table_depth = self.table_depth.saturating_sub(1),
            _ => {}
        }
    }
}

fn parse_document(xml: &[u8]) -> Result<Vec<WordSegment>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut body = Body::default();
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        if let Event::Start(start) = &event {
            if start.name().as_ref() == b"w:txbxContent" {
                body.text_box_depth += 1;
                continue;
            }
        }
        if let Event::End(end) = &event {
            if end.name().as_ref() == b"w:txbxContent" {
                body.text_box_depth = body.text_box_depth.saturating_sub(1);
                continue;
            }
        }
        if body.text_box_depth > 0 && !matches!(event, Event::Eof) {
            continue;
        }
        match event {
            Event::Start(start) => body.start(start.name().as_ref()),
            Event::Empty(empty) => body.empty(empty.name().as_ref()),
            Event::End(end) => body.end(end.name().as_ref()),
            Event::Text(text) if body.in_text => push_text(body.sink(), &text)?,
            Event::GeneralRef(reference) if body.in_text => {
                push_reference(body.sink(), &reference)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(body.segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(inner: &str) -> String {
        format!("<w:document xmlns:w=\"urn:w\"><w:body>{inner}<w:sectPr/></w:body></w:document>")
    }

    #[test]
    fn test_tab_stops_and_text_boxes_add_no_text() {
        // GIVEN a paragraph with tab stops in its properties, a run tab, an
        // entity, and a text box anchored in it
        let xml = body(concat!(
            "<w:p><w:pPr><w:tabs><w:tab w:val=\"left\" w:pos=\"720\"/></w:tabs></w:pPr>",
            "<w:r><w:t>Price</w:t><w:tab/><w:t>10 &amp; 12 &#8364;</w:t></w:r>",
            "<w:r><w:drawing><w:txbxContent><w:p><w:r><w:t>floating</w:t></w:r></w:p>",
            "</w:txbxContent></w:drawing></w:r></w:p>",
        ));

        // WHEN it is parsed
        let segments = parse_document(xml.as_bytes()).unwrap();

        // THEN the paragraph holds its run text only, the text box is skipped
        assert_eq!(segments.len(), 1, "{segments:?}");
        assert_eq!(segments[0].text, "Price\t10 & 12 \u{20ac}");
    }

    #[test]
    fn test_unbalanced_xml_is_an_error() {
        // GIVEN a paragraph that is never closed in a body that is
        let xml = body("<w:p><w:r><w:t>open</w:t></w:r>");

        // WHEN it is parsed
        let parsed = parse_document(xml.as_bytes());

        // THEN the parser reports the mismatch
        assert!(parsed.is_err(), "{parsed:?}");
    }
}
