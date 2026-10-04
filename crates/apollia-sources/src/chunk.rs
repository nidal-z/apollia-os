//! Cutting a document into the passages an index stores.
//!
//! A reader hands over a document as [`Section`]s: the pages of a PDF, the
//! slides of a presentation, the rows of a sheet, or the lines of a text file.
//! [`chunk_document`] turns them into [`Chunk`]s of about [`TARGET_TOKENS`]
//! tokens, each beginning with the last [`OVERLAP_TOKENS`] tokens of the one
//! before it, so a sentence cut in two is still found whole on one side.
//!
//! A chunk is cut only at a boundary of the document: a paragraph, a page, a
//! slide, a row of a table, or a line. Its own text never spans two pages, two
//! slides or two sheets, so its [`Location`] stays exact. A single paragraph
//! or row longer than the target is the one exception: it is cut between
//! lines, then between words.

use serde::{Deserialize, Serialize};

/// Tokens a chunk aims at, by [`token_estimate`].
pub const TARGET_TOKENS: usize = 400;

/// Tokens of the previous chunk repeated at the start of the next one, by
/// [`token_estimate`].
pub const OVERLAP_TOKENS: usize = 50;

/// An estimate of the tokens `text` costs a model.
///
/// One token per four bytes of UTF-8, rounded up: the usual ratio of the
/// sub-word tokenizers of embeddings models on English and French prose. It
/// is an estimate on purpose. The exact count depends on the model, the
/// index is built before the model is known, and a chunk only needs to stay
/// well inside the window of the model that reads it (512 tokens for the
/// smallest recommended one). Accented letters weigh two bytes, which errs on
/// the side of smaller chunks.
#[must_use]
pub fn token_estimate(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// One part of a document, as a reader delivers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    /// One page of a paged document, numbered from one.
    Page {
        /// Page number.
        page: u32,
        /// Text of the page; blank lines separate its paragraphs.
        text: String,
    },
    /// One slide of a presentation, numbered from one.
    Slide {
        /// Slide number.
        slide: u32,
        /// Text of the slide and its notes; blank lines separate paragraphs.
        text: String,
    },
    /// One sheet of a workbook, as rows of text.
    Sheet {
        /// Name of the sheet.
        name: String,
        /// Each row as its number, from one, and its cells joined in text.
        rows: Vec<(u32, String)>,
    },
    /// The whole of a text file; blank lines separate its paragraphs.
    Text {
        /// Content of the file.
        text: String,
    },
}

/// Where a chunk sits in its document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Location {
    /// A page, numbered from one.
    Page {
        /// Page number.
        page: u32,
    },
    /// A slide, numbered from one.
    Slide {
        /// Slide number.
        slide: u32,
    },
    /// Rows of a sheet, both bounds included.
    Rows {
        /// Name of the sheet.
        sheet: String,
        /// First row.
        first: u32,
        /// Last row.
        last: u32,
    },
    /// Lines of a text file, numbered from one, both bounds included.
    Lines {
        /// First line.
        first: u32,
        /// Last line.
        last: u32,
    },
}

/// One passage of a document, as the index stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The passage, starting with the overlap taken from the chunk before.
    pub text: String,
    /// Where the passage's own text sits.
    pub location: Location,
}

/// A unit that is never split unless it alone exceeds the target: a
/// paragraph, or a row. `first` and `last` are its line or row numbers.
struct Unit {
    text: String,
    first: u32,
    last: u32,
}

/// Cuts a document into chunks, in document order.
// TEST-ONLY: the indexing pass of a folder source is not built yet; reading
// each file of the folder and writing its chunks is the production caller
// this is published for.
#[must_use]
pub fn chunk_document(sections: &[Section]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for section in sections {
        match section {
            Section::Page { page, text } => {
                let units = paragraphs(text);
                pack(&units, &mut chunks, |_, _| Location::Page { page: *page });
            }
            Section::Slide { slide, text } => {
                let units = paragraphs(text);
                pack(&units, &mut chunks, |_, _| Location::Slide {
                    slide: *slide,
                });
            }
            Section::Sheet { name, rows } => {
                let units: Vec<Unit> = rows
                    .iter()
                    .filter(|(_, text)| !text.trim().is_empty())
                    .map(|(row, text)| Unit {
                        text: text.trim().to_owned(),
                        first: *row,
                        last: *row,
                    })
                    .collect();
                pack(&units, &mut chunks, |first, last| Location::Rows {
                    sheet: name.clone(),
                    first,
                    last,
                });
            }
            Section::Text { text } => {
                let units = paragraphs(text);
                pack(&units, &mut chunks, |first, last| Location::Lines {
                    first,
                    last,
                });
            }
        }
    }
    chunks
}

/// The paragraphs of `text`, with their first and last line numbers. A
/// paragraph is a run of non-blank lines.
fn paragraphs(text: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut current: Option<Unit> = None;
    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if line.trim().is_empty() {
            units.extend(current.take());
            continue;
        }
        match current.as_mut() {
            Some(unit) => {
                unit.text.push('\n');
                unit.text.push_str(line.trim_end());
                unit.last = number;
            }
            None => {
                current = Some(Unit {
                    text: line.trim_end().to_owned(),
                    first: number,
                    last: number,
                });
            }
        }
    }
    units.extend(current);
    units
}

/// The most a chunk's own text weighs, so that with the overlap before it a
/// chunk stays within [`TARGET_TOKENS`].
const OWN_TOKENS: usize = TARGET_TOKENS - OVERLAP_TOKENS;

/// Splits a unit longer than [`OWN_TOKENS`] between lines, then between
/// words, into pieces that fit it.
fn split_long(unit: &Unit) -> Vec<Unit> {
    if token_estimate(&unit.text) <= OWN_TOKENS {
        return vec![Unit {
            text: unit.text.clone(),
            first: unit.first,
            last: unit.last,
        }];
    }
    let mut pieces: Vec<Unit> = Vec::new();
    let mut line_number = unit.first;
    for line in unit.text.lines() {
        let mut words = String::new();
        for word in line.split_whitespace() {
            if !words.is_empty() && token_estimate(&words) + token_estimate(word) >= OWN_TOKENS {
                push_piece(&mut pieces, std::mem::take(&mut words), line_number);
            }
            if !words.is_empty() {
                words.push(' ');
            }
            words.push_str(word);
        }
        if !words.is_empty() {
            push_piece(&mut pieces, words, line_number);
        }
        line_number = line_number.saturating_add(1).min(unit.last);
    }
    pieces
}

/// Appends `text` to the last piece when both fit the target together, else
/// starts a new piece.
fn push_piece(pieces: &mut Vec<Unit>, text: String, line: u32) {
    if let Some(last) = pieces.last_mut() {
        if token_estimate(&last.text) + token_estimate(&text) < OWN_TOKENS {
            last.text.push('\n');
            last.text.push_str(&text);
            last.last = line;
            return;
        }
    }
    pieces.push(Unit {
        text,
        first: line,
        last: line,
    });
}

/// The words at the end of `text` worth at most [`OVERLAP_TOKENS`] tokens,
/// joined by single spaces.
fn overlap_tail(text: &str) -> String {
    let mut tail = String::new();
    for word in text.split_whitespace().rev() {
        let candidate = if tail.is_empty() {
            word.to_owned()
        } else {
            format!("{word} {tail}")
        };
        if token_estimate(&candidate) > OVERLAP_TOKENS {
            break;
        }
        tail = candidate;
    }
    tail
}

/// Packs the units of one section into chunks of about the target size, and
/// appends them to `out`. Overlap is taken within the section only, so a
/// chunk's text never reaches into another page, slide or sheet.
fn pack(units: &[Unit], out: &mut Vec<Chunk>, locate: impl Fn(u32, u32) -> Location) {
    let pieces: Vec<Unit> = units.iter().flat_map(split_long).collect();
    let mut body = String::new();
    let mut span: Option<(u32, u32)> = None;
    let mut previous: Option<String> = None;
    let mut flush =
        |body: &mut String, span: &mut Option<(u32, u32)>, previous: &mut Option<String>| {
            if let Some((first, last)) = span.take() {
                let own = std::mem::take(body);
                let text = match previous.as_deref().map(overlap_tail) {
                    Some(tail) if !tail.is_empty() => format!("{tail}\n{own}"),
                    _ => own.clone(),
                };
                out.push(Chunk {
                    text,
                    location: locate(first, last),
                });
                *previous = Some(own);
            }
        };
    for piece in pieces {
        let fits = token_estimate(&body) + token_estimate(&piece.text) <= OWN_TOKENS;
        if span.is_some() && !fits {
            flush(&mut body, &mut span, &mut previous);
        }
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&piece.text);
        span = Some(match span {
            Some((first, _)) => (first, piece.last),
            None => (piece.first, piece.last),
        });
    }
    flush(&mut body, &mut span, &mut previous);
}

#[cfg(test)]
mod tests;
