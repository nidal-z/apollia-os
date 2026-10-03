//! A line scanner over an `apollia.toml` document.
//!
//! Gives every line the dotted path it belongs to, without deserializing: a
//! table header gives its own path, a key line gives `table.key`, and the
//! continuation lines of a multi-line array or string keep the path of the key
//! that opened them. It never fails, so it can run on a file that the
//! deserializer is about to refuse, which is when a path is needed most: to
//! say where a fault is, and to set aside the withdrawn keys before the strict
//! deserialization runs.

/// What one line of the document holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LineKind {
    /// A table or array-of-tables header.
    Header,
    /// A key line, or a continuation line of its value.
    Key,
    /// Blank or comment line.
    Other,
}

/// One line of the document with the path it belongs to.
#[derive(Debug, Clone)]
pub(crate) struct ScannedLine {
    /// Byte offset of the first character of the line.
    pub start: usize,
    /// Byte offset of the end of the line, newline excluded.
    pub end: usize,
    /// What the line holds.
    pub kind: LineKind,
    /// Dotted path: the header path, the key path, or the enclosing table for
    /// a blank or comment line (empty at the root).
    pub path: String,
}

/// Scans `content` line by line.
pub(crate) fn scan(content: &str) -> Vec<ScannedLine> {
    let mut lines = Vec::new();
    let mut table = String::new();
    // An open value: the path it belongs to, and how it closes.
    let mut open: Option<(String, Open)> = None;
    let mut offset = 0;
    for raw in content.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let text = raw.trim_end_matches(['\n', '\r']);
        let end = start + text.len();
        if let Some((path, state)) = open.take() {
            let next = state.advance(text);
            lines.push(ScannedLine {
                start,
                end,
                kind: LineKind::Key,
                path: path.clone(),
            });
            if let Some(next) = next {
                open = Some((path, next));
            }
            continue;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(ScannedLine {
                start,
                end,
                kind: LineKind::Other,
                path: table.clone(),
            });
            continue;
        }
        if let Some(header) = trimmed.strip_prefix('[') {
            let header = header.trim_start_matches('[');
            let name = header.split(']').next().unwrap_or_default();
            table = normalize(name);
            lines.push(ScannedLine {
                start,
                end,
                kind: LineKind::Header,
                path: table.clone(),
            });
            continue;
        }
        let Some(eq) = find_unquoted(trimmed, '=') else {
            lines.push(ScannedLine {
                start,
                end,
                kind: LineKind::Other,
                path: table.clone(),
            });
            continue;
        };
        let key = normalize(&trimmed[..eq]);
        let path = join(&table, &key);
        open = Open::start(&trimmed[eq + 1..]).map(|state| (path.clone(), state));
        lines.push(ScannedLine {
            start,
            end,
            kind: LineKind::Key,
            path,
        });
    }
    lines
}

/// `true` when `path` is `prefix` itself or lies under it.
pub(crate) fn covers(prefix: &str, path: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// 1-based line and column of byte `offset` in `content`.
pub(crate) fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(content.len());
    let before = content.get(..offset).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before.get(line_start..).map_or(0, |s| s.chars().count()) + 1;
    (line, column)
}

/// The dotted path of the line holding byte `offset`.
pub(crate) fn path_at(content: &str, offset: usize) -> String {
    scan(content)
        .into_iter()
        .find(|line| offset >= line.start && offset <= line.end)
        .map(|line| line.path)
        .unwrap_or_default()
}

/// Byte offset of the first character of the line that declares `path`, or of
/// the nearest enclosing table that the document declares.
pub(crate) fn offset_of(content: &str, path: &str) -> Option<usize> {
    let lines = scan(content);
    let mut wanted = path.to_owned();
    loop {
        if let Some(line) = lines
            .iter()
            .find(|l| l.kind != LineKind::Other && l.path == wanted)
        {
            let indent = content
                .get(line.start..line.end)
                .map_or(0, |t| t.len() - t.trim_start().len());
            return Some(line.start + indent);
        }
        match wanted.rfind('.') {
            Some(dot) => wanted.truncate(dot),
            None => return None,
        }
    }
}

/// How an open multi-line value closes.
#[derive(Debug, Clone, Copy)]
enum Open {
    /// Inside a multi-line basic (`"""`) or literal (`'''`) string.
    String(&'static str),
    /// Inside an array or inline table, at this bracket depth.
    Brackets(i32),
}

impl Open {
    /// The state after the value part of a key line, or `None` when the value
    /// closes on that line.
    fn start(value: &str) -> Option<Self> {
        let value = value.trim_start();
        for delim in ["\"\"\"", "'''"] {
            if let Some(rest) = value.strip_prefix(delim) {
                return if rest.contains(delim) {
                    None
                } else {
                    Some(Self::String(delim))
                };
            }
        }
        let depth = bracket_depth(value);
        (depth > 0).then_some(Self::Brackets(depth))
    }

    /// The state after one continuation line.
    fn advance(self, line: &str) -> Option<Self> {
        match self {
            Self::String(delim) => (!line.contains(delim)).then_some(self),
            Self::Brackets(depth) => {
                let depth = depth + bracket_depth(line);
                (depth > 0).then_some(Self::Brackets(depth))
            }
        }
    }
}

/// Net count of opening brackets and braces outside strings and comments.
fn bracket_depth(text: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in text.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => break,
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

/// Byte index of the first `needle` outside quotes.
fn find_unquoted(text: &str, needle: char) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == needle => return Some(i),
            None => {}
        }
    }
    None
}

/// Strips spaces and quotes around each segment of a dotted key.
fn normalize(raw: &str) -> String {
    raw.split('.')
        .map(|segment| segment.trim().trim_matches('"').trim_matches('\''))
        .collect::<Vec<_>>()
        .join(".")
}

fn join(table: &str, key: &str) -> String {
    if table.is_empty() {
        key.to_owned()
    } else {
        format!("{table}.{key}")
    }
}
