//! A bounded, allocation-frugal reader for the GGUF key/value header.
//!
//! The format is documented upstream in `ggml/docs/gguf.md`. A file opens with
//! a fixed 24-byte preamble (magic, version, tensor count, KV count) followed
//! by `kv_count` entries, each a length-prefixed key, a type tag, and a value.
//!
//! The reader exists to answer one question before a download starts: will the
//! embedded `llama-server` load this file. That answer lives entirely in the
//! header, so a range request over the first megabytes replaces a multi-gigabyte
//! download. The parser therefore has to tolerate a slice that stops mid-value:
//! [`parse_header`] returns what it read and marks the result truncated rather
//! than refusing a buffer that is merely short.
//!
//! Ordering matters to the caller and is worth recording. `gguf-py` writes in
//! insertion order, and `convert_hf_to_gguf.py` inserts `general.*`, then the
//! architecture hyperparameters, then `tokenizer.ggml.model` and
//! `tokenizer.ggml.pre`, and only then the vocabulary arrays. The pre-tokenizer
//! identifier is thus reachable in the first megabyte, while
//! `tokenizer.chat_template` sits past a vocabulary that runs to several
//! megabytes. Callers screen with a small budget and confirm with a larger one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `GGUF` in little-endian byte order, the first four bytes of every file.
const GGUF_MAGIC: u32 = 0x4655_4747;

/// Longest key the reader will accept, a guard against a length field read out
/// of a buffer that is not in fact a GGUF file.
const MAX_KEY_LEN: u64 = 1024;

/// Longest string value the reader will materialise. Chat templates are the
/// large ones and run to tens of kilobytes; a megabyte is a ceiling rather
/// than a budget.
const MAX_STRING_LEN: u64 = 1024 * 1024;

/// Deepest array nesting the reader follows before refusing. Real files nest
/// one level at most; the guard exists so a corrupt buffer cannot drive the
/// skip routine into unbounded recursion.
const MAX_ARRAY_DEPTH: u32 = 4;

/// How much of the chat template is kept for diagnostics.
const CHAT_TEMPLATE_EXCERPT_BYTES: usize = 240;

/// Failure to read a buffer as a GGUF header.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum GgufParseError {
    /// The first four bytes are not the GGUF magic.
    #[error("not a GGUF file: magic was {found:#010x}, expected {expected:#010x}")]
    BadMagic {
        /// The magic actually read.
        found: u32,
        /// The magic the format requires.
        expected: u32,
    },

    /// The header version is one this reader does not know how to walk.
    #[error("unsupported GGUF version {0}, this build reads versions 2 and 3")]
    UnsupportedVersion(u32),

    /// The buffer ended before the 24-byte preamble was complete.
    #[error("buffer holds {0} bytes, too short for the 24-byte GGUF preamble")]
    PreambleTooShort(usize),

    /// A length prefix exceeded the reader's guard, which means the buffer is
    /// not a GGUF header or has been corrupted in transit.
    #[error("implausible {kind} length {len} at offset {offset}")]
    ImplausibleLength {
        /// What was being read: `"key"` or `"string"`.
        kind: &'static str,
        /// The length the buffer claimed.
        len: u64,
        /// Where in the buffer the length was read.
        offset: usize,
    },

    /// A value carried a type tag outside the documented enumeration.
    #[error("unknown GGUF value type {tag} at offset {offset}")]
    UnknownValueType {
        /// The tag actually read.
        tag: u32,
        /// Where in the buffer the tag was read.
        offset: usize,
    },

    /// Arrays nested more deeply than the reader will follow.
    #[error("GGUF array nested beyond {0} levels")]
    ArrayTooDeep(u32),
}

/// The facts a compatibility verdict needs, lifted out of a GGUF header.
///
/// Every field but the preamble is optional: a header may stop short of the key
/// that carries it, and a truncated read is a normal outcome rather than an
/// error. Read [`GgufHeaderFacts::truncated`] before concluding that an absent
/// field is absent from the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GgufHeaderFacts {
    /// Header version, 2 or 3 in practice.
    pub version: u32,
    /// Number of tensors the file declares.
    pub tensor_count: u64,
    /// Number of key/value pairs the file declares.
    pub kv_count: u64,
    /// How many of those pairs this read actually walked.
    pub kv_read: u64,
    /// `general.architecture`, for example `"qwen3"` or `"llama"`.
    pub architecture: Option<String>,
    /// `general.name`, the publisher's label for the weights.
    pub name: Option<String>,
    /// `general.file_type`, the quantisation scheme as a `LLAMA_FTYPE` value.
    pub file_type: Option<u32>,
    /// `general.quantization_version`.
    pub quantization_version: Option<u32>,
    /// `tokenizer.ggml.model`, the vocabulary family (`"gpt2"`, `"llama"`, ...).
    pub tokenizer_model: Option<String>,
    /// `tokenizer.ggml.pre`, the pre-tokenizer identifier.
    ///
    /// This is the field that decides whether the embedded server can load the
    /// file at all. A value the server's build does not know aborts the load
    /// with `unknown pre-tokenizer type`, after the download has completed.
    pub tokenizer_pre: Option<String>,
    /// Whether `tokenizer.chat_template` is present.
    ///
    /// Apollia launches the server with `--jinja`, so a file without a template
    /// has no tool-calling path even though it loads and answers prose.
    pub has_chat_template: bool,
    /// The first bytes of `tokenizer.chat_template`, kept for diagnostics.
    pub chat_template_excerpt: Option<String>,
    /// `{arch}.block_count`, the number of transformer layers.
    pub block_count: Option<u64>,
    /// `{arch}.context_length`, the training context window.
    pub context_length: Option<u64>,
    /// `{arch}.embedding_length`, the model dimension.
    pub embedding_length: Option<u64>,
    /// `{arch}.attention.head_count`.
    pub head_count: Option<u64>,
    /// `{arch}.attention.head_count_kv`, the grouped-query key/value head count.
    pub head_count_kv: Option<u64>,
    /// `{arch}.attention.key_length`, the per-head key dimension.
    ///
    /// Read rather than derived. `embedding_length / head_count` is only a
    /// default: llama.cpp overrides it with this key whenever the file carries
    /// one, and most modern models do. Gemma 3 declares 256 where the division
    /// gives 320, Qwen3 4B declares 128 where it gives 80.
    pub key_length: Option<u64>,
    /// `{arch}.attention.value_length`, the per-head value dimension.
    pub value_length: Option<u64>,
    /// `{arch}.attention.sliding_window`, the short window most layers attend to.
    pub sliding_window: Option<u64>,
    /// `{arch}.attention.sliding_window_pattern`, when the file overrides the
    /// period its architecture would otherwise default to.
    pub sliding_window_pattern: Option<u64>,
    /// `{arch}.expert_count`, the number of experts in a mixture.
    pub expert_count: Option<u64>,
    /// `{arch}.expert_used_count`, the experts evaluated per token.
    ///
    /// The ratio of this to `expert_count` is what makes a mixture fast: a
    /// 30B model that routes to 3B of experts computes like a 3B and weighs
    /// like a 30B.
    pub expert_used_count: Option<u64>,
    /// `{arch}.full_attention_interval`, for hybrid models that keep a real
    /// cache on only some layers.
    pub full_attention_interval: Option<u64>,
    /// `{arch}.attention.key_length_swa`, when the sliding-window layers use a
    /// different per-head dimension from the full-attention ones.
    ///
    /// Gemma 4 does exactly this: 512 on its full layers and 256 on its
    /// windowed ones, so one figure cannot describe the file.
    pub key_length_swa: Option<u64>,
    /// `{arch}.attention.value_length_swa`.
    pub value_length_swa: Option<u64>,
    /// `{arch}.attention.head_count_kv` when the file gives one entry per layer.
    ///
    /// Newer architectures vary the key/value head count down the stack, which
    /// a single number cannot express. Empty when the file carries a scalar.
    pub head_count_kv_per_layer: Vec<u64>,
    /// `{arch}.attention.sliding_window_pattern` when the file gives one flag
    /// per layer instead of a period.
    ///
    /// Non-zero marks a layer that attends to the short window. Empty when the
    /// file carries a period, or nothing at all.
    pub sliding_window_per_layer: Vec<u64>,
    /// `true` when the buffer ended before every declared pair was walked.
    pub truncated: bool,
}

impl GgufHeaderFacts {
    /// Whether the read reached the end of the declared key/value section.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.truncated && self.kv_read >= self.kv_count
    }
}

/// A cursor that reports exhaustion instead of panicking on a short buffer.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

/// The buffer ended mid-value. Not an error at the public boundary: it is the
/// expected outcome of a range read that stopped where the caller told it to.
struct Exhausted;

type Step<T> = Result<T, StepError>;

enum StepError {
    Exhausted,
    Malformed(GgufParseError),
}

impl From<Exhausted> for StepError {
    fn from(_: Exhausted) -> Self {
        StepError::Exhausted
    }
}

impl From<GgufParseError> for StepError {
    fn from(err: GgufParseError) -> Self {
        StepError::Malformed(err)
    }
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Exhausted> {
        let end = self.pos.checked_add(n).ok_or(Exhausted)?;
        let slice = self.bytes.get(self.pos..end).ok_or(Exhausted)?;
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, Exhausted> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, Exhausted> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Advance past `n` bytes without materialising them.
    ///
    /// A skip that runs past the end still reports exhaustion, so a vocabulary
    /// array that the range read cut in half ends the walk cleanly.
    fn skip(&mut self, n: u64) -> Result<(), Exhausted> {
        let n = usize::try_from(n).map_err(|_| Exhausted)?;
        let end = self.pos.checked_add(n).ok_or(Exhausted)?;
        if end > self.bytes.len() {
            return Err(Exhausted);
        }
        self.pos = end;
        Ok(())
    }

    /// Read a length-prefixed UTF-8 string, replacing invalid sequences rather
    /// than refusing the header over one bad byte in a vocabulary entry.
    fn string(&mut self, max_len: u64, kind: &'static str) -> Step<String> {
        let offset = self.pos;
        let len = self.u64()?;
        if len > max_len {
            return Err(GgufParseError::ImplausibleLength { kind, len, offset }.into());
        }
        let n = usize::try_from(len).map_err(|_| Exhausted)?;
        let bytes = self.take(n)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// The GGUF value type tags, as documented in `ggml/docs/gguf.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueType {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
    Bool,
    String,
    Array,
    U64,
    I64,
    F64,
}

impl ValueType {
    fn from_tag(tag: u32, offset: usize) -> Result<Self, GgufParseError> {
        match tag {
            0 => Ok(Self::U8),
            1 => Ok(Self::I8),
            2 => Ok(Self::U16),
            3 => Ok(Self::I16),
            4 => Ok(Self::U32),
            5 => Ok(Self::I32),
            6 => Ok(Self::F32),
            7 => Ok(Self::Bool),
            8 => Ok(Self::String),
            9 => Ok(Self::Array),
            10 => Ok(Self::U64),
            11 => Ok(Self::I64),
            12 => Ok(Self::F64),
            _ => Err(GgufParseError::UnknownValueType { tag, offset }),
        }
    }

    /// Byte width of a scalar of this type, `None` for the variable-width ones.
    fn scalar_width(self) -> Option<u64> {
        match self {
            Self::U8 | Self::I8 | Self::Bool => Some(1),
            Self::U16 | Self::I16 => Some(2),
            Self::U32 | Self::I32 | Self::F32 => Some(4),
            Self::U64 | Self::I64 | Self::F64 => Some(8),
            Self::String | Self::Array => None,
        }
    }
}

/// Longest array the reader will materialise.
///
/// Per-layer arrays are the only ones worth keeping, and no model has more
/// layers than this. The cap is what stops a vocabulary of 150000 strings from
/// ever being built in memory.
const MAX_MATERIALISED_ARRAY: u64 = 1024;

/// A value reduced to the shapes a verdict consumes.
enum Scalar {
    Unsigned(u64),
    Text(String),
    /// A short array of numbers, one entry per layer.
    Numbers(Vec<u64>),
    Other,
}

/// Read one value, materialising it only when the key is one the caller wants.
///
/// `wanted` drives the choice: a vocabulary array of 150000 strings is walked
/// for its length and discarded, while a four-byte `general.file_type` is kept.
fn read_value(cur: &mut Cursor<'_>, ty: ValueType, wanted: bool) -> Step<Scalar> {
    match ty {
        ValueType::String => {
            if wanted {
                let s = cur.string(MAX_STRING_LEN, "string")?;
                Ok(Scalar::Text(s))
            } else {
                let offset = cur.pos;
                let len = cur.u64()?;
                if len > MAX_STRING_LEN {
                    return Err(GgufParseError::ImplausibleLength {
                        kind: "string",
                        len,
                        offset,
                    }
                    .into());
                }
                cur.skip(len)?;
                Ok(Scalar::Other)
            }
        }
        ValueType::Array => {
            // A wanted key may hold one entry per layer, which is short enough
            // to keep. Everything else is walked and discarded, which is what
            // makes a vocabulary cheap to step over.
            if wanted {
                if let Some(numbers) = read_number_array(cur)? {
                    return Ok(Scalar::Numbers(numbers));
                }
                return Ok(Scalar::Other);
            }
            skip_array(cur, 0)?;
            Ok(Scalar::Other)
        }
        ValueType::U8 | ValueType::Bool => Ok(Scalar::Unsigned(u64::from(cur.take(1)?[0]))),
        ValueType::U16 => {
            let b = cur.take(2)?;
            Ok(Scalar::Unsigned(u64::from(u16::from_le_bytes([
                b[0], b[1],
            ]))))
        }
        ValueType::U32 => Ok(Scalar::Unsigned(u64::from(cur.u32()?))),
        ValueType::U64 => Ok(Scalar::Unsigned(cur.u64()?)),
        ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::F32 => {
            let width = ty.scalar_width().unwrap_or(4);
            cur.skip(width)?;
            Ok(Scalar::Other)
        }
        ValueType::I64 | ValueType::F64 => {
            cur.skip(8)?;
            Ok(Scalar::Other)
        }
    }
}

/// Read a short array of fixed-width numbers, or walk past anything else.
///
/// Returns `Ok(None)` when the array is not one worth keeping, having stepped
/// over it either way, so the caller can carry on reading the header.
fn read_number_array(cur: &mut Cursor<'_>) -> Step<Option<Vec<u64>>> {
    let tag_offset = cur.pos;
    let elem_ty = ValueType::from_tag(cur.u32()?, tag_offset)?;
    let len = cur.u64()?;

    let Some(width) = elem_ty.scalar_width() else {
        // A string or nested array: not a per-layer table. `skip_array` expects
        // to read the header itself, so the walk is finished here instead.
        return skip_elements(cur, elem_ty, len).map(|()| None);
    };

    if len > MAX_MATERIALISED_ARRAY {
        cur.skip(len.checked_mul(width).ok_or(Exhausted)?)?;
        return Ok(None);
    }

    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    for _ in 0..len {
        let bytes = cur.take(usize::try_from(width).map_err(|_| Exhausted)?)?;
        let mut buf = [0u8; 8];
        buf[..bytes.len()].copy_from_slice(bytes);
        out.push(u64::from_le_bytes(buf));
    }
    Ok(Some(out))
}

/// Walk past `len` elements of a variable-width type.
fn skip_elements(cur: &mut Cursor<'_>, elem_ty: ValueType, len: u64) -> Step<()> {
    for _ in 0..len {
        match elem_ty {
            ValueType::String => {
                let offset = cur.pos;
                let slen = cur.u64()?;
                if slen > MAX_STRING_LEN {
                    return Err(GgufParseError::ImplausibleLength {
                        kind: "string",
                        len: slen,
                        offset,
                    }
                    .into());
                }
                cur.skip(slen)?;
            }
            ValueType::Array => skip_array(cur, 1)?,
            _ => return Ok(()),
        }
    }
    Ok(())
}

/// Walk past an array without materialising its elements.
///
/// Fixed-width elements are arithmetic. A string array has to be walked entry
/// by entry, which is what makes the vocabulary expensive and why the caller
/// screens with a small budget before paying for the whole header.
fn skip_array(cur: &mut Cursor<'_>, depth: u32) -> Step<()> {
    if depth >= MAX_ARRAY_DEPTH {
        return Err(GgufParseError::ArrayTooDeep(MAX_ARRAY_DEPTH).into());
    }
    let tag_offset = cur.pos;
    let elem_ty = ValueType::from_tag(cur.u32()?, tag_offset)?;
    let len = cur.u64()?;

    if let Some(width) = elem_ty.scalar_width() {
        let total = len.checked_mul(width).ok_or(Exhausted)?;
        cur.skip(total)?;
        return Ok(());
    }

    for _ in 0..len {
        match elem_ty {
            ValueType::String => {
                let offset = cur.pos;
                let slen = cur.u64()?;
                if slen > MAX_STRING_LEN {
                    return Err(GgufParseError::ImplausibleLength {
                        kind: "string",
                        len: slen,
                        offset,
                    }
                    .into());
                }
                cur.skip(slen)?;
            }
            ValueType::Array => skip_array(cur, depth + 1)?,
            _ => return Ok(()),
        }
    }
    Ok(())
}

/// Keys whose value is kept, independent of the architecture prefix.
const GENERAL_KEYS: &[&str] = &[
    "general.architecture",
    "general.name",
    "general.file_type",
    "general.quantization_version",
    "tokenizer.ggml.model",
    "tokenizer.ggml.pre",
    "tokenizer.chat_template",
];

/// Architecture-prefixed suffixes whose value is kept. The full key is
/// `{arch}.{suffix}`, for example `qwen3.block_count`.
const ARCH_SUFFIXES: &[&str] = &[
    "block_count",
    "context_length",
    "embedding_length",
    "attention.head_count",
    "attention.head_count_kv",
    "attention.key_length",
    "attention.value_length",
    "attention.sliding_window",
    "attention.sliding_window_pattern",
    "attention.key_length_swa",
    "attention.value_length_swa",
    "expert_count",
    "expert_used_count",
    "full_attention_interval",
];

fn is_wanted(key: &str) -> bool {
    if GENERAL_KEYS.contains(&key) {
        return true;
    }
    match key.split_once('.') {
        Some((_, rest)) => ARCH_SUFFIXES.contains(&rest),
        None => false,
    }
}

/// Read a GGUF header out of `bytes`, which may hold only its first megabytes.
///
/// A buffer that stops mid-header yields the pairs that did fit, with
/// [`GgufHeaderFacts::truncated`] set. Only a buffer that is not a GGUF header
/// at all, or that is internally inconsistent, produces an error.
///
/// # Errors
/// - [`GgufParseError::PreambleTooShort`] when fewer than 24 bytes were given.
/// - [`GgufParseError::BadMagic`] when the file is not GGUF.
/// - [`GgufParseError::UnsupportedVersion`] for a header version outside 2..=3.
/// - [`GgufParseError::ImplausibleLength`], [`GgufParseError::UnknownValueType`]
///   or [`GgufParseError::ArrayTooDeep`] when the bytes do not walk as a header.
pub fn parse_header(bytes: &[u8]) -> Result<GgufHeaderFacts, GgufParseError> {
    let mut cur = Cursor::new(bytes);
    let short = || GgufParseError::PreambleTooShort(bytes.len());

    let magic = cur.u32().map_err(|_| short())?;
    if magic != GGUF_MAGIC {
        return Err(GgufParseError::BadMagic {
            found: magic,
            expected: GGUF_MAGIC,
        });
    }
    let version = cur.u32().map_err(|_| short())?;
    if !(2..=3).contains(&version) {
        return Err(GgufParseError::UnsupportedVersion(version));
    }
    let tensor_count = cur.u64().map_err(|_| short())?;
    let kv_count = cur.u64().map_err(|_| short())?;

    let mut facts = GgufHeaderFacts {
        version,
        tensor_count,
        kv_count,
        ..GgufHeaderFacts::default()
    };

    // Architecture-prefixed keys may arrive before `general.architecture` in a
    // hand-written file, so the prefixed values are collected by suffix and
    // resolved once the walk is done.
    let mut by_suffix: BTreeMap<String, u64> = BTreeMap::new();

    for _ in 0..kv_count {
        match read_pair(&mut cur, &mut facts, &mut by_suffix) {
            Ok(()) => facts.kv_read += 1,
            Err(StepError::Exhausted) => {
                facts.truncated = true;
                break;
            }
            Err(StepError::Malformed(err)) => return Err(err),
        }
    }

    facts.block_count = by_suffix.get("block_count").copied();
    facts.context_length = by_suffix.get("context_length").copied();
    facts.embedding_length = by_suffix.get("embedding_length").copied();
    facts.head_count = by_suffix.get("attention.head_count").copied();
    facts.head_count_kv = by_suffix.get("attention.head_count_kv").copied();
    facts.key_length = by_suffix.get("attention.key_length").copied();
    facts.value_length = by_suffix.get("attention.value_length").copied();
    facts.sliding_window = by_suffix.get("attention.sliding_window").copied();
    facts.sliding_window_pattern = by_suffix.get("attention.sliding_window_pattern").copied();
    facts.expert_count = by_suffix.get("expert_count").copied();
    facts.expert_used_count = by_suffix.get("expert_used_count").copied();
    facts.full_attention_interval = by_suffix.get("full_attention_interval").copied();
    facts.key_length_swa = by_suffix.get("attention.key_length_swa").copied();
    facts.value_length_swa = by_suffix.get("attention.value_length_swa").copied();

    Ok(facts)
}

fn read_pair(
    cur: &mut Cursor<'_>,
    facts: &mut GgufHeaderFacts,
    by_suffix: &mut BTreeMap<String, u64>,
) -> Step<()> {
    let key = cur.string(MAX_KEY_LEN, "key")?;
    let tag_offset = cur.pos;
    let ty = ValueType::from_tag(cur.u32()?, tag_offset)?;
    let wanted = is_wanted(&key);
    let value = read_value(cur, ty, wanted)?;

    if !wanted {
        return Ok(());
    }

    match (key.as_str(), value) {
        ("general.architecture", Scalar::Text(s)) => facts.architecture = Some(s),
        ("general.name", Scalar::Text(s)) => facts.name = Some(s),
        ("general.file_type", Scalar::Unsigned(n)) => facts.file_type = u32::try_from(n).ok(),
        ("general.quantization_version", Scalar::Unsigned(n)) => {
            facts.quantization_version = u32::try_from(n).ok();
        }
        ("tokenizer.ggml.model", Scalar::Text(s)) => facts.tokenizer_model = Some(s),
        ("tokenizer.ggml.pre", Scalar::Text(s)) => facts.tokenizer_pre = Some(s),
        ("tokenizer.chat_template", Scalar::Text(s)) => {
            facts.has_chat_template = true;
            facts.chat_template_excerpt = Some(excerpt(&s, CHAT_TEMPLATE_EXCERPT_BYTES));
        }
        (other, Scalar::Unsigned(n)) => {
            if let Some((_, suffix)) = other.split_once('.') {
                by_suffix.insert(suffix.to_owned(), n);
            }
        }
        (other, Scalar::Numbers(values)) => match other.split_once('.') {
            Some((_, "attention.head_count_kv")) => facts.head_count_kv_per_layer = values,
            Some((_, "attention.sliding_window_pattern")) => {
                facts.sliding_window_per_layer = values;
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

/// Truncate to at most `max` bytes without splitting a character.
fn excerpt(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}
