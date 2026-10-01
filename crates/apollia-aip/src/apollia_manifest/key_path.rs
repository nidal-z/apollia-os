//! Turn a TOML deserialisation error into the dotted path of the key at fault.
//!
//! The `toml` crate reports a schema mismatch with a byte span and a message,
//! not with a path. This module walks the source text up to that span and
//! rebuilds the path (`agents[1].defaults.model.temprature`): table headers,
//! arrays of tables with their running index, dotted keys, inline tables and
//! arrays. It never decides validity: the document was already parsed by
//! `toml`, so the walk only has to follow a well-formed one.

use std::collections::HashMap;

use super::ApolliaManifestError;

/// One step of a key path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// Where a byte offset falls.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Located {
    path: Vec<Seg>,
    /// `true` when the offset is on a key, `false` when it is on a value or a
    /// table header.
    on_key: bool,
}

/// Build the error for a failed deserialisation of `text`.
pub(super) fn schema_error(text: &str, error: &toml::de::Error) -> ApolliaManifestError {
    let message = error.message().trim().to_string();
    let located = error.span().and_then(|span| locate(text, span.start));
    match (unknown_field(&message), located) {
        (Some(field), Some(loc)) => {
            let mut path = loc.path;
            if !loc.on_key {
                path.push(Seg::Key(field));
            }
            ApolliaManifestError::UnknownKey {
                path: render(&path),
            }
        }
        (Some(field), None) => ApolliaManifestError::UnknownKey { path: field },
        (None, Some(loc)) => ApolliaManifestError::Schema {
            path: render(&loc.path),
            message,
        },
        (None, None) => ApolliaManifestError::Schema {
            path: String::new(),
            message,
        },
    }
}

/// The field name of a serde "unknown field `x`" message.
fn unknown_field(message: &str) -> Option<String> {
    let rest = message.strip_prefix("unknown field `")?;
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn render(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Index(i) => {
                out.push('[');
                out.push_str(&i.to_string());
                out.push(']');
            }
        }
    }
    out
}

/// Path of the element of `text` that covers byte offset `pos`.
fn locate(text: &str, pos: usize) -> Option<Located> {
    let mut scanner = Scanner {
        s: text.as_bytes(),
        i: 0,
        pos,
        arrays: HashMap::new(),
    };
    scanner.document()
}

struct Scanner<'a> {
    s: &'a [u8],
    i: usize,
    pos: usize,
    /// Arrays of tables seen so far, by rendered path, with their length.
    arrays: HashMap<String, usize>,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn starts_with(&self, lit: &[u8]) -> bool {
        self.s[self.i.min(self.s.len())..].starts_with(lit)
    }

    fn covers(&self, start: usize, end: usize) -> bool {
        start <= self.pos && self.pos < end
    }

    /// Skip spaces, tabs, and when `newlines`, line breaks and comments too.
    fn skip_blank(&mut self, newlines: bool) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' => self.i += 1,
                b'\r' | b'\n' if newlines => self.i += 1,
                b'#' if newlines => self.skip_line(),
                _ => break,
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == b'\n' {
                break;
            }
        }
    }

    fn document(&mut self) -> Option<Located> {
        let mut table: Vec<Seg> = Vec::new();
        loop {
            self.skip_blank(true);
            let start = self.i;
            if start >= self.s.len() {
                return None;
            }
            match self.peek()? {
                b'[' if self.starts_with(b"[[") => {
                    self.i += 2;
                    let keys = self.dotted_key()?.into_iter().map(|(k, _)| k);
                    let keys: Vec<String> = keys.collect();
                    self.skip_blank(false);
                    self.i += 2;
                    let (last, parents) = keys.split_last()?;
                    let mut path = self.resolve(parents);
                    path.push(Seg::Key(last.clone()));
                    let len = self.arrays.entry(render(&path)).or_insert(0);
                    *len += 1;
                    path.push(Seg::Index(*len - 1));
                    table = path;
                    if self.covers(start, self.i) {
                        return Some(Located {
                            path: table,
                            on_key: false,
                        });
                    }
                }
                b'[' => {
                    self.i += 1;
                    let keys: Vec<String> =
                        self.dotted_key()?.into_iter().map(|(k, _)| k).collect();
                    self.skip_blank(false);
                    self.i += 1;
                    table = self.resolve(&keys);
                    if self.covers(start, self.i) {
                        return Some(Located {
                            path: table,
                            on_key: false,
                        });
                    }
                }
                _ => {
                    if let Some(found) = self.key_value(&table) {
                        return Some(found);
                    }
                }
            }
            if self.i == start {
                return None;
            }
        }
    }

    /// Path of a header's keys, each array of tables resolved to its last
    /// element.
    fn resolve(&self, keys: &[String]) -> Vec<Seg> {
        let mut path = Vec::new();
        for key in keys {
            path.push(Seg::Key(key.clone()));
            if let Some(len) = self.arrays.get(&render(&path)) {
                path.push(Seg::Index(len.saturating_sub(1)));
            }
        }
        path
    }

    /// `key = value` under `base`. Returns the location when it covers `pos`.
    fn key_value(&mut self, base: &[Seg]) -> Option<Located> {
        let keys = self.dotted_key()?;
        let mut path = base.to_vec();
        for (key, (start, end)) in keys {
            path.push(Seg::Key(key));
            if self.covers(start, end) {
                return Some(Located { path, on_key: true });
            }
        }
        self.skip_blank(false);
        // The `=`.
        self.i += 1;
        self.skip_blank(false);
        self.value(&path)
    }

    /// A dotted key, each part with its byte span.
    fn dotted_key(&mut self) -> Option<Vec<(String, (usize, usize))>> {
        let mut keys = Vec::new();
        loop {
            self.skip_blank(false);
            let start = self.i;
            let key = match self.peek()? {
                b'"' | b'\'' => {
                    let quote = self.s[self.i];
                    self.i += 1;
                    let from = self.i;
                    while self.peek()? != quote {
                        if quote == b'"' && self.peek()? == b'\\' {
                            self.i += 1;
                        }
                        self.i += 1;
                    }
                    let key = String::from_utf8_lossy(&self.s[from..self.i]).into_owned();
                    self.i += 1;
                    key
                }
                _ => {
                    let from = self.i;
                    while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    {
                        self.i += 1;
                    }
                    if from == self.i {
                        return None;
                    }
                    String::from_utf8_lossy(&self.s[from..self.i]).into_owned()
                }
            };
            keys.push((key, (start, self.i)));
            self.skip_blank(false);
            if self.peek() == Some(b'.') {
                self.i += 1;
            } else {
                return Some(keys);
            }
        }
    }

    /// A value at `path`. Returns the deepest element covering `pos`.
    fn value(&mut self, path: &[Seg]) -> Option<Located> {
        let start = self.i;
        let inner = match self.peek()? {
            b'{' => self.inline_table(path),
            b'[' => self.array(path),
            b'"' | b'\'' => {
                self.string();
                None
            }
            _ => {
                while matches!(self.peek(), Some(c) if !matches!(c, b',' | b']' | b'}' | b'\n' | b'\r' | b'#'))
                {
                    self.i += 1;
                }
                None
            }
        };
        if inner.is_some() {
            return inner;
        }
        if self.covers(start, self.i) {
            return Some(Located {
                path: path.to_vec(),
                on_key: false,
            });
        }
        None
    }

    fn inline_table(&mut self, path: &[Seg]) -> Option<Located> {
        self.i += 1;
        loop {
            self.skip_blank(false);
            let before = self.i;
            match self.peek()? {
                b'}' => {
                    self.i += 1;
                    return None;
                }
                b',' => self.i += 1,
                _ => {
                    if let Some(found) = self.key_value(path) {
                        return Some(found);
                    }
                }
            }
            if self.i == before {
                return None;
            }
        }
    }

    fn array(&mut self, path: &[Seg]) -> Option<Located> {
        self.i += 1;
        let mut index = 0;
        loop {
            self.skip_blank(true);
            let before = self.i;
            match self.peek()? {
                b']' => {
                    self.i += 1;
                    return None;
                }
                b',' => self.i += 1,
                _ => {
                    let mut item = path.to_vec();
                    item.push(Seg::Index(index));
                    index += 1;
                    if let Some(found) = self.value(&item) {
                        return Some(found);
                    }
                }
            }
            if self.i == before {
                return None;
            }
        }
    }

    /// Skip a basic, literal, or multi-line string.
    fn string(&mut self) {
        let quote = self.s[self.i];
        let triple: &[u8] = if quote == b'"' { b"\"\"\"" } else { b"'''" };
        if self.starts_with(triple) {
            self.i += 3;
            while self.i < self.s.len() && !self.starts_with(triple) {
                if quote == b'"' && self.s[self.i] == b'\\' {
                    self.i += 1;
                }
                self.i += 1;
            }
            self.i += 3;
            // A closing run may hold up to two extra quotes.
            while self.peek() == Some(quote) {
                self.i += 1;
            }
            return;
        }
        self.i += 1;
        while let Some(c) = self.peek() {
            self.i += 1;
            if quote == b'"' && c == b'\\' {
                self.i += 1;
            } else if c == quote {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_at(text: &str, needle: &str) -> Option<String> {
        let pos = text.find(needle).expect("needle in text");
        locate(text, pos).map(|l| render(&l.path))
    }

    #[test]
    fn test_locate_key_under_second_array_element_names_its_index() {
        // GIVEN two [[agents]] and a nested table under the second
        let text = "[[agents]]\nid = 'a'\n[[agents]]\nid = 'b'\n[agents.defaults.model]\nzz = 1\n";

        // WHEN locating the key `zz`
        let path = path_at(text, "zz");

        // THEN the path names the second element
        assert_eq!(path.as_deref(), Some("agents[1].defaults.model.zz"));
    }

    #[test]
    fn test_locate_key_inside_inline_table_and_array_follows_both() {
        // GIVEN an inline table nested in an array, after a multi-line string
        let text = "a = \"\"\"x\ny = 1\n\"\"\"\nb = [ { c = 1 }, { d = { zz = 2 } } ]\n";

        // WHEN locating the key `zz`
        let path = path_at(text, "zz");

        // THEN the multi-line string is skipped and the indices are followed
        assert_eq!(path.as_deref(), Some("b[1].d.zz"));
    }

    #[test]
    fn test_locate_quoted_and_dotted_keys_are_rendered_unquoted() {
        // GIVEN a dotted key with a quoted part
        let text = "[t]\n\"q k\".zz = 1\n";

        // WHEN locating `zz`
        let path = path_at(text, "zz");

        // THEN both parts appear, the quoted one without quotes
        assert_eq!(path.as_deref(), Some("t.q k.zz"));
    }

    #[test]
    fn test_unknown_field_reads_the_serde_message() {
        // GIVEN a serde unknown-field message and another message
        let unknown = "unknown field `zz`, expected `a`";
        let other = "missing field `a`";

        // WHEN extracting the field
        let (a, b) = (unknown_field(unknown), unknown_field(other));

        // THEN only the first yields a name
        assert_eq!((a.as_deref(), b), (Some("zz"), None));
    }
}
