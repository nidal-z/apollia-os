//! The PowerPoint reader: the text of each slide and of its notes.

use std::path::Path;

use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::reader::office::{push_reference, push_text, OfficeArchive};
use crate::reader::ReadError;

/// Extensions [`read_presentation`] opens: a PowerPoint presentation, with or
/// without macros. The legacy binary `.ppt` is not one of them.
pub const PRESENTATION_EXTENSIONS: [&str; 2] = ["pptx", "pptm"];

const SLIDES_FOLDER: &str = "ppt/slides/";
const NOTES_RELATIONSHIP: &str = "/notesSlide";

/// A PowerPoint presentation read as text, slide by slide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    /// Every slide, in the numeric order of its part: slide 10 after slide 9.
    pub slides: Vec<Slide>,
}

/// One slide of a presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    /// Number of the slide, from 1, as its part `ppt/slides/slideN.xml` names it.
    pub number: u32,
    /// The text the slide draws, one paragraph per line; empty when it draws
    /// none.
    pub text: String,
    /// The text of the speaker notes, one paragraph per line; empty when the
    /// slide has none.
    pub notes: String,
}

/// Reads the PowerPoint presentation at `path`: the text and the notes of
/// every slide, with its number.
///
/// # Errors
///
/// [`ReadError::UnsupportedFormat`] when the extension is none of
/// [`PRESENTATION_EXTENSIONS`] (a legacy `.ppt` among them), [`ReadError::Io`]
/// when the file cannot be read, [`ReadError::ArchiveOverCap`] when the archive
/// goes over a cap, and [`ReadError::Corrupt`] when it is not a presentation.
pub fn read_presentation(path: &Path) -> Result<Presentation, ReadError> {
    let mut archive = OfficeArchive::open(path, &PRESENTATION_EXTENSIONS, "pptx, pptm")?;
    let mut numbers: Vec<u32> = archive
        .names()
        .iter()
        .filter_map(|name| slide_number(name))
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.is_empty() && archive.read_part("ppt/presentation.xml")?.is_none() {
        return Err(ReadError::corrupt(path, "no ppt/presentation.xml part"));
    }
    let mut slides = Vec::with_capacity(numbers.len());
    for number in numbers {
        let part = format!("{SLIDES_FOLDER}slide{number}.xml");
        let text = match archive.read_part(&part)? {
            Some(xml) => drawing_text(&xml).map_err(|detail| corrupt(path, &part, &detail))?,
            None => String::new(),
        };
        let notes = match notes_part(&mut archive, number)? {
            Some(notes) => match archive.read_part(&notes)? {
                Some(xml) => drawing_text(&xml).map_err(|detail| corrupt(path, &notes, &detail))?,
                None => String::new(),
            },
            None => String::new(),
        };
        slides.push(Slide {
            number,
            text,
            notes,
        });
    }
    Ok(Presentation { slides })
}

fn corrupt(path: &Path, part: &str, detail: &str) -> ReadError {
    ReadError::corrupt(path, format!("{part}: {detail}"))
}

/// The number `N` of a part named `ppt/slides/slideN.xml`.
fn slide_number(name: &str) -> Option<u32> {
    let digits = name
        .strip_prefix(SLIDES_FOLDER)?
        .strip_prefix("slide")?
        .strip_suffix(".xml")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The notes part of slide `number`, as the relationships of the slide name it.
fn notes_part(archive: &mut OfficeArchive<'_>, number: u32) -> Result<Option<String>, ReadError> {
    let rels = format!("{SLIDES_FOLDER}_rels/slide{number}.xml.rels");
    let Some(xml) = archive.read_part(&rels)? else {
        return Ok(None);
    };
    let target = notes_target(&xml).map_err(|detail| corrupt(archive.path(), &rels, &detail))?;
    Ok(target.map(|target| resolve_target(&target)))
}

fn notes_target(xml: &[u8]) -> Result<Option<String>, String> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"Relationship" =>
            {
                let kind = attribute(&element, b"Type", reader.decoder())?.unwrap_or_default();
                if kind.ends_with(NOTES_RELATIONSHIP) {
                    return attribute(&element, b"Target", reader.decoder());
                }
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

fn attribute(
    element: &BytesStart<'_>,
    name: &[u8],
    decoder: Decoder,
) -> Result<Option<String>, String> {
    let Some(attribute) = element
        .try_get_attribute(name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let value = attribute
        .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
        .map_err(|error| error.to_string())?;
    Ok(Some(value.into_owned()))
}

/// The part name a relationship target of a slide points to: relative to
/// `ppt/slides/`, or absolute from the root of the archive.
fn resolve_target(target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_owned();
    }
    let mut parts: Vec<&str> = SLIDES_FOLDER.trim_end_matches('/').split('/').collect();
    for segment in target.split('/') {
        match segment {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The text of a slide or notes part: every `a:t`, one paragraph per line. A
/// slide number field is left out, so the notes do not end with the number of
/// their slide.
fn drawing_text(xml: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut in_text = false;
    let mut slide_number_depth = 0u32;
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) => match element.name().as_ref() {
                b"a:fld"
                    if slide_number_depth > 0 || is_slide_number(&element, reader.decoder())? =>
                {
                    slide_number_depth += 1;
                }
                b"a:t" => in_text = true,
                _ => {}
            },
            Event::Empty(element) if element.name().as_ref() == b"a:br" => {
                flush(&mut lines, &mut line);
            }
            Event::End(element) => match element.name().as_ref() {
                b"a:fld" if slide_number_depth > 0 => slide_number_depth -= 1,
                b"a:t" => in_text = false,
                b"a:p" => flush(&mut lines, &mut line),
                _ => {}
            },
            Event::Text(text) if in_text && slide_number_depth == 0 => {
                push_text(&mut line, &text)?;
            }
            Event::GeneralRef(reference) if in_text && slide_number_depth == 0 => {
                push_reference(&mut line, &reference)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    flush(&mut lines, &mut line);
    Ok(lines.join("\n"))
}

fn is_slide_number(element: &BytesStart<'_>, decoder: Decoder) -> Result<bool, String> {
    Ok(attribute(element, b"type", decoder)?.is_some_and(|kind| kind == "slidenum"))
}

fn flush(lines: &mut Vec<String>, line: &mut String) {
    let text = std::mem::take(line);
    if !text.trim().is_empty() {
        lines.push(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_numbered_slide_parts_are_slides() {
        // GIVEN part names around the slide folder
        let names = [
            "ppt/slides/slide10.xml",
            "ppt/slides/slide2.xml",
            "ppt/slides/_rels/slide2.xml.rels",
            "ppt/slides/slide.xml",
            "ppt/slides/slide1a.xml",
            "ppt/slideLayouts/slideLayout1.xml",
        ];

        // WHEN each is read as a slide part
        let numbers: Vec<Option<u32>> = names.iter().map(|name| slide_number(name)).collect();

        // THEN only the two numbered slides count
        assert_eq!(numbers, [Some(10), Some(2), None, None, None, None]);
    }

    #[test]
    fn test_a_notes_target_resolves_from_the_slide_folder() {
        // GIVEN a relative and an absolute target
        let relative = "../notesSlides/notesSlide3.xml";
        let absolute = "/ppt/notesSlides/notesSlide4.xml";

        // WHEN each is resolved
        let relative = resolve_target(relative);
        let absolute = resolve_target(absolute);

        // THEN both name a part from the root of the archive
        assert_eq!(relative, "ppt/notesSlides/notesSlide3.xml");
        assert_eq!(absolute, "ppt/notesSlides/notesSlide4.xml");
    }

    #[test]
    fn test_line_breaks_split_lines_and_the_slide_number_field_is_dropped() {
        // GIVEN a paragraph with a line break and an entity, then a slide
        // number field
        let xml = concat!(
            "<p:notes xmlns:p=\"urn:p\" xmlns:a=\"urn:a\"><a:p><a:r><a:t>One &amp; two</a:t>",
            "</a:r><a:br/><a:r><a:t>three</a:t></a:r></a:p>",
            "<a:p><a:fld id=\"x\" type=\"slidenum\"><a:t>7</a:t></a:fld></a:p></p:notes>",
        );

        // WHEN its text is read
        let text = drawing_text(xml.as_bytes()).unwrap();

        // THEN the break makes two lines and the number is gone
        assert_eq!(text, "One & two\nthree");
    }
}
