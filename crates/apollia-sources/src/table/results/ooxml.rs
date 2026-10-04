//! Results written into an Office Open XML workbook (`.xlsx`, `.xlsm`) by
//! editing the XML of one sheet inside the archive.
//!
//! Every part is copied as it is stored, compressed bytes included, except
//! three: the sheet written, where the cells of the added columns are inserted
//! as inline strings; the calculation chain, which is dropped so that the
//! spreadsheet recomputes its formulas on opening; and, when the workbook had
//! one, `[Content_Types].xml` and the workbook's relationships, from which
//! that chain is removed. Formatting, charts, pivot tables, comments and macros
//! live in parts that are never rewritten.

use std::io::{Cursor, Read, Write};

use quick_xml::encoding::Decoder;
use quick_xml::escape::escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::reader::{OFFICE_MAX_DECOMPRESSED_BYTES, OFFICE_MAX_ENTRIES};
use crate::table::layout::column_letter;
use crate::table::results::{cell_value, Grid};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const ROOT_RELATIONSHIPS: &str = "_rels/.rels";
const DEFAULT_WORKBOOK: &str = "xl/workbook.xml";

/// A workbook read once and ready to be written with added columns.
#[derive(Clone)]
pub(super) struct OoxmlWorkbook {
    bytes: Vec<u8>,
    sheet_part: String,
    sheet_xml: Vec<u8>,
    last_column: u32,
    calc_chain: Option<CalcChain>,
}

/// The calculation chain of a workbook, and the two parts edited to drop it.
#[derive(Clone)]
struct CalcChain {
    part: String,
    content_types: Vec<u8>,
    relationships_part: String,
    relationships: Vec<u8>,
}

impl std::fmt::Debug for OoxmlWorkbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OoxmlWorkbook")
            .field("bytes", &self.bytes.len())
            .field("sheet_part", &self.sheet_part)
            .field("last_column", &self.last_column)
            .field(
                "calc_chain",
                &self.calc_chain.as_ref().map(|chain| &chain.part),
            )
            .finish()
    }
}

impl OoxmlWorkbook {
    /// Reads the archive `bytes` and finds the part of the sheet named
    /// `sheet`, its last column and the calculation chain, if any.
    pub(super) fn prepare(bytes: Vec<u8>, sheet: &str) -> Result<Self, String> {
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice()))
            .map_err(|error| format!("not a zip archive: {error}"))?;
        if archive.len() > OFFICE_MAX_ENTRIES {
            return Err(format!(
                "the archive holds more than {OFFICE_MAX_ENTRIES} entries"
            ));
        }
        let mut budget = OFFICE_MAX_DECOMPRESSED_BYTES;
        let root = read_part(&mut archive, ROOT_RELATIONSHIPS, &mut budget)?
            .ok_or_else(|| format!("the archive has no {ROOT_RELATIONSHIPS}"))?;
        let workbook_part = relationships(&root)?
            .into_iter()
            .find(|relation| relation.kind.ends_with("/officeDocument"))
            .map_or_else(
                || DEFAULT_WORKBOOK.to_string(),
                |relation| resolve("", &relation.target),
            );
        let folder = workbook_part
            .rsplit_once('/')
            .map_or("", |(folder, _)| folder)
            .to_string();
        let file = workbook_part
            .rsplit_once('/')
            .map_or(workbook_part.as_str(), |(_, file)| file);
        let relationships_part = if folder.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{folder}/_rels/{file}.rels")
        };
        let workbook = read_part(&mut archive, &workbook_part, &mut budget)?
            .ok_or_else(|| format!("the archive has no {workbook_part}"))?;
        let relationships_xml = read_part(&mut archive, &relationships_part, &mut budget)?
            .ok_or_else(|| format!("the archive has no {relationships_part}"))?;
        let relations = relationships(&relationships_xml)?;
        let sheet_id = sheet_relationship(&workbook, sheet)?
            .ok_or_else(|| format!("the workbook has no sheet named '{sheet}'"))?;
        let sheet_part = relations
            .iter()
            .find(|relation| relation.id == sheet_id)
            .map(|relation| resolve(&folder, &relation.target))
            .ok_or_else(|| format!("the sheet '{sheet}' has no part"))?;
        let sheet_xml = read_part(&mut archive, &sheet_part, &mut budget)?
            .ok_or_else(|| format!("the archive has no {sheet_part}"))?;
        let last_column = last_column(&sheet_xml)?;
        let calc_chain = match relations
            .iter()
            .find(|relation| relation.kind.ends_with("/calcChain"))
        {
            Some(relation) => {
                let part = resolve(&folder, &relation.target);
                let content_types = read_part(&mut archive, CONTENT_TYPES, &mut budget)?
                    .ok_or_else(|| format!("the archive has no {CONTENT_TYPES}"))?;
                let override_name = format!("/{part}");
                Some(CalcChain {
                    content_types: remove_elements(&content_types, |element, decoder| {
                        Ok(element.local_name().as_ref() == b"Override"
                            && attribute(element, b"PartName", decoder)?.as_deref()
                                == Some(override_name.as_str()))
                    })?,
                    relationships: remove_elements(&relationships_xml, |element, decoder| {
                        Ok(element.local_name().as_ref() == b"Relationship"
                            && attribute(element, b"Type", decoder)?
                                .is_some_and(|kind| kind.ends_with("/calcChain")))
                    })?,
                    part,
                    relationships_part,
                })
            }
            None => None,
        };
        Ok(Self {
            bytes,
            sheet_part,
            sheet_xml,
            last_column,
            calc_chain,
        })
    }

    /// The original bytes of the workbook.
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The number of the last column the sheet uses, cells and merged areas
    /// included; 0 for an empty sheet.
    pub(super) fn last_column(&self) -> u32 {
        self.last_column
    }

    /// The workbook with the cells of `grid` added to its sheet.
    pub(super) fn write(&self, grid: &Grid) -> Result<Vec<u8>, String> {
        let new_last = grid
            .values()
            .flat_map(|cells| cells.iter().map(|(column, _)| *column))
            .max()
            .unwrap_or(0)
            .max(self.last_column);
        let sheet = edit_sheet(&self.sheet_xml, grid, new_last)?;
        let mut archive = ZipArchive::new(Cursor::new(self.bytes.as_slice()))
            .map_err(|error| format!("not a zip archive: {error}"))?;
        let mut out = ZipWriter::new(Cursor::new(Vec::with_capacity(self.bytes.len() + 4096)));
        for index in 0..archive.len() {
            let entry = archive
                .by_index_raw(index)
                .map_err(|error| format!("entry {index}: {error}"))?;
            let name = entry.name().to_string();
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .last_modified_time(entry.last_modified().unwrap_or_default());
            let replaced: Option<&[u8]> = if name == self.sheet_part {
                Some(&sheet)
            } else if let Some(chain) = &self.calc_chain {
                if name == chain.part {
                    continue;
                } else if name == CONTENT_TYPES {
                    Some(&chain.content_types)
                } else if name == chain.relationships_part {
                    Some(&chain.relationships)
                } else {
                    None
                }
            } else {
                None
            };
            match replaced {
                Some(content) => {
                    drop(entry);
                    out.start_file(name.as_str(), options)
                        .map_err(|error| format!("{name}: {error}"))?;
                    out.write_all(content)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                None => out
                    .raw_copy_file(entry)
                    .map_err(|error| format!("{name}: {error}"))?,
            }
        }
        out.finish()
            .map(Cursor::into_inner)
            .map_err(|error| format!("cannot finish the archive: {error}"))
    }
}

/// One relationship of a `.rels` part.
struct Relation {
    id: String,
    kind: String,
    target: String,
}

fn read_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    budget: &mut u64,
) -> Result<Option<Vec<u8>>, String> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("{name}: {error}")),
    };
    let mut bytes = Vec::new();
    entry
        .take(budget.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{name}: {error}"))?;
    let read = bytes.len() as u64;
    if read > *budget {
        return Err(format!(
            "the parts read weigh more than {OFFICE_MAX_DECOMPRESSED_BYTES} bytes once decompressed"
        ));
    }
    *budget -= read;
    Ok(Some(bytes))
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
    attribute
        .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
        .map(|value| Some(value.into_owned()))
        .map_err(|error| error.to_string())
}

/// The value of the attribute of `element` whose local name is `local` and
/// which carries a namespace prefix, as `r:id` does.
fn prefixed_attribute(
    element: &BytesStart<'_>,
    local: &[u8],
    decoder: Decoder,
) -> Result<Option<String>, String> {
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        if attribute.key.prefix().is_some() && attribute.key.local_name().as_ref() == local {
            return attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| error.to_string());
        }
    }
    Ok(None)
}

fn relationships(xml: &[u8]) -> Result<Vec<Relation>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut relations = Vec::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"Relationship" =>
            {
                let decoder = reader.decoder();
                relations.push(Relation {
                    id: attribute(&element, b"Id", decoder)?.unwrap_or_default(),
                    kind: attribute(&element, b"Type", decoder)?.unwrap_or_default(),
                    target: attribute(&element, b"Target", decoder)?.unwrap_or_default(),
                });
            }
            Event::Eof => return Ok(relations),
            _ => {}
        }
    }
}

/// The relationship identifier of the sheet named `name` in `workbook.xml`.
fn sheet_relationship(workbook: &[u8], name: &str) -> Result<Option<String>, String> {
    let mut reader = Reader::from_reader(workbook);
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"sheet" =>
            {
                let decoder = reader.decoder();
                if attribute(&element, b"name", decoder)?.as_deref() == Some(name) {
                    return prefixed_attribute(&element, b"id", decoder);
                }
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

/// The part a relationship target names, from the folder of the part that
/// holds the relationship, or from the root when the target is absolute.
pub(super) fn resolve(folder: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return normalise(Vec::new(), absolute);
    }
    normalise(
        folder.split('/').filter(|part| !part.is_empty()).collect(),
        target,
    )
}

fn normalise<'a>(mut parts: Vec<&'a str>, target: &'a str) -> String {
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The column number of a cell reference such as `AB12`; `None` when it does
/// not start with letters.
pub(super) fn reference_column(reference: &str) -> Option<u32> {
    let mut number: u32 = 0;
    let mut letters = 0;
    for byte in reference.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' => {
                number = number
                    .checked_mul(26)?
                    .checked_add(u32::from(byte.to_ascii_uppercase() - b'A') + 1)?;
                letters += 1;
            }
            b'$' if letters == 0 => {}
            _ => break,
        }
    }
    (letters > 0).then_some(number)
}

/// The last column a sheet uses: the rightmost cell, written with or without
/// its reference, and the rightmost merged area.
fn last_column(xml: &[u8]) -> Result<u32, String> {
    let mut reader = Reader::from_reader(xml);
    let mut last = 0;
    let mut in_row = 0;
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element) => {
                let decoder = reader.decoder();
                match element.local_name().as_ref() {
                    b"row" => in_row = 0,
                    b"c" => {
                        in_row = attribute(&element, b"r", decoder)?
                            .as_deref()
                            .and_then(reference_column)
                            .unwrap_or(in_row + 1);
                        last = last.max(in_row);
                    }
                    b"mergeCell" => {
                        if let Some(reference) = attribute(&element, b"ref", decoder)? {
                            let end = reference.rsplit(':').next().unwrap_or(&reference);
                            last = last.max(reference_column(end).unwrap_or(0));
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => return Ok(last),
            _ => {}
        }
    }
}

/// `xml` without the elements `drop` selects, their content included.
fn remove_elements(
    xml: &[u8],
    drop: impl Fn(&BytesStart<'_>, Decoder) -> Result<bool, String>,
) -> Result<Vec<u8>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut skipping = 0usize;
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match &event {
            Event::Eof => break,
            Event::Start(element) if skipping > 0 || drop(element, reader.decoder())? => {
                skipping += 1;
                continue;
            }
            Event::End(_) if skipping > 0 => {
                skipping -= 1;
                continue;
            }
            Event::Empty(element) if skipping > 0 || drop(element, reader.decoder())? => {
                continue;
            }
            _ if skipping > 0 => continue,
            _ => {}
        }
        writer
            .write_event(event)
            .map_err(|error| error.to_string())?;
    }
    Ok(writer.into_inner())
}

/// The prefix of a qualified element name, with its colon: `x:` for `x:row`.
fn prefix_of(element: &BytesStart<'_>) -> String {
    element
        .name()
        .prefix()
        .map(|prefix| format!("{}:", String::from_utf8_lossy(prefix.as_ref())))
        .unwrap_or_default()
}

/// The sheet `xml` with the cells of `grid` appended to their rows, missing
/// rows inserted in order, and the dimension stretched to `new_last`.
pub(super) fn edit_sheet(xml: &[u8], grid: &Grid, new_last: u32) -> Result<Vec<u8>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len() + grid.len() * 64));
    let mut pending = grid.iter().peekable();
    let mut in_data = false;
    let mut prefix = String::new();
    let mut last_row = 0u32;
    let mut open_row: Option<&Vec<(u32, String)>> = None;
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        let decoder = reader.decoder();
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element)
                if !in_data && element.local_name().as_ref() == b"dimension" =>
            {
                let stretched = stretch_dimension(element, decoder, new_last)?;
                let edited = if matches!(event, Event::Start(_)) {
                    Event::Start(stretched)
                } else {
                    Event::Empty(stretched)
                };
                write_event(&mut writer, edited)?;
                continue;
            }
            Event::Start(element) if element.local_name().as_ref() == b"sheetData" => {
                prefix = prefix_of(element);
                in_data = true;
            }
            Event::Empty(element) if element.local_name().as_ref() == b"sheetData" => {
                prefix = prefix_of(element);
                write_event(&mut writer, Event::Start(element.borrow()))?;
                for (row, cells) in pending.by_ref() {
                    write_row(&mut writer, &prefix, *row, cells)?;
                }
                write_event(&mut writer, Event::End(element.to_end()))?;
                continue;
            }
            Event::Start(element) | Event::Empty(element)
                if in_data && element.local_name().as_ref() == b"row" =>
            {
                let number = attribute(element, b"r", decoder)?
                    .and_then(|r| r.trim().parse::<u32>().ok())
                    .unwrap_or(last_row.saturating_add(1));
                last_row = number;
                while let Some((row, cells)) = pending.next_if(|(row, _)| **row < number) {
                    write_row(&mut writer, &prefix, *row, cells)?;
                }
                if let Some((_, cells)) = pending.next_if(|(row, _)| **row == number) {
                    write_event(&mut writer, Event::Start(without_spans(element)?))?;
                    if matches!(event, Event::Empty(_)) {
                        write_cells(&mut writer, &prefix, number, cells)?;
                        write_event(&mut writer, Event::End(element.to_end()))?;
                    } else {
                        open_row = Some(cells);
                    }
                    continue;
                }
            }
            Event::End(element) if in_data && element.local_name().as_ref() == b"row" => {
                if let Some(cells) = open_row.take() {
                    write_cells(&mut writer, &prefix, last_row, cells)?;
                }
            }
            Event::End(element) if element.local_name().as_ref() == b"sheetData" => {
                for (row, cells) in pending.by_ref() {
                    write_row(&mut writer, &prefix, *row, cells)?;
                }
                in_data = false;
            }
            _ => {}
        }
        write_event(&mut writer, event)?;
    }
    Ok(writer.into_inner())
}

fn write_event(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<(), String> {
    writer.write_event(event).map_err(|error| error.to_string())
}

fn write_row(
    writer: &mut Writer<Vec<u8>>,
    prefix: &str,
    row: u32,
    cells: &[(u32, String)],
) -> Result<(), String> {
    write_raw(writer, &format!("<{prefix}row r=\"{row}\">"))?;
    write_cells(writer, prefix, row, cells)?;
    write_raw(writer, &format!("</{prefix}row>"))
}

fn write_cells(
    writer: &mut Writer<Vec<u8>>,
    prefix: &str,
    row: u32,
    cells: &[(u32, String)],
) -> Result<(), String> {
    for (column, text) in cells {
        let letter = column_letter(*column);
        let text = cell_value(text);
        let escaped = escape(text.as_str());
        write_raw(
            writer,
            &format!(
                "<{prefix}c r=\"{letter}{row}\" t=\"inlineStr\"><{prefix}is><{prefix}t xml:space=\"preserve\">{escaped}</{prefix}t></{prefix}is></{prefix}c>"
            ),
        )?;
    }
    Ok(())
}

fn write_raw(writer: &mut Writer<Vec<u8>>, text: &str) -> Result<(), String> {
    writer
        .get_mut()
        .write_all(text.as_bytes())
        .map_err(|error| error.to_string())
}

/// `element` without its `spans` attribute, an optional hint the added cells
/// would make wrong.
fn without_spans(element: &BytesStart<'_>) -> Result<BytesStart<'static>, String> {
    let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
    let mut copy = BytesStart::new(name);
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        if attribute.key.as_ref() != b"spans" {
            copy.push_attribute(attribute);
        }
    }
    Ok(copy)
}

/// The `dimension` element with its range reaching column `new_last`.
fn stretch_dimension(
    element: &BytesStart<'_>,
    decoder: Decoder,
    new_last: u32,
) -> Result<BytesStart<'static>, String> {
    let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
    let mut copy = BytesStart::new(name);
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        if attribute.key.as_ref() == b"ref" {
            let reference = attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map_err(|error| error.to_string())?
                .into_owned();
            copy.push_attribute(("ref", stretched_range(&reference, new_last).as_str()));
        } else {
            copy.push_attribute(attribute);
        }
    }
    Ok(copy)
}

/// The range `reference` widened so that it ends at column `new_last` or
/// further: `A1:B9` with 4 gives `A1:D9`.
pub(super) fn stretched_range(reference: &str, new_last: u32) -> String {
    let (start, end) = reference.split_once(':').unwrap_or((reference, reference));
    let end_column = reference_column(end).unwrap_or(0);
    if new_last <= end_column {
        return reference.to_string();
    }
    let row: String = end.chars().filter(char::is_ascii_digit).collect();
    format!("{start}:{}{row}", column_letter(new_last))
}
