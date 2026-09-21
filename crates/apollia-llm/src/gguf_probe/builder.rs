//! A GGUF header writer, for tests only.
//!
//! The reader has to cope with headers no fixture conveniently provides: one
//! cut in the middle of a vocabulary, one carrying a value tag the format does
//! not define, one whose key length is a corruption rather than a length. The
//! builder emits those byte for byte, so the suite needs no network, no model
//! file, and no checked-in binary.

#![cfg(test)]

/// Accumulates key/value pairs and seals them behind a GGUF preamble.
pub(super) struct HeaderBuilder {
    version: u32,
    tensor_count: u64,
    kv_count: u64,
    body: Vec<u8>,
}

impl HeaderBuilder {
    pub(super) fn new() -> Self {
        Self {
            version: 3,
            tensor_count: 0,
            kv_count: 0,
            body: Vec::new(),
        }
    }

    /// Override the header version, to exercise the version guard.
    pub(super) fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    fn push_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.body
            .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.body.extend_from_slice(bytes);
    }

    fn push_key(&mut self, key: &str) {
        self.push_str(key);
        self.kv_count += 1;
    }

    /// A string-valued pair (tag 8).
    pub(super) fn string(mut self, key: &str, value: &str) -> Self {
        self.push_key(key);
        self.body.extend_from_slice(&8u32.to_le_bytes());
        self.push_str(value);
        self
    }

    /// A `u32`-valued pair (tag 4).
    pub(super) fn u32(mut self, key: &str, value: u32) -> Self {
        self.push_key(key);
        self.body.extend_from_slice(&4u32.to_le_bytes());
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// An array of `count` short strings (tag 9, element tag 8), standing in
    /// for a vocabulary.
    pub(super) fn string_array(mut self, key: &str, count: u64) -> Self {
        self.push_key(key);
        self.body.extend_from_slice(&9u32.to_le_bytes());
        self.body.extend_from_slice(&8u32.to_le_bytes());
        self.body.extend_from_slice(&count.to_le_bytes());
        for i in 0..count {
            let token = format!("tok{i}");
            let bytes = token.as_bytes();
            self.body
                .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            self.body.extend_from_slice(bytes);
        }
        self
    }

    /// An array of `count` `u32` values (tag 9, element tag 4), standing in for
    /// the token-type table the reader steps over by arithmetic.
    pub(super) fn u32_array(mut self, key: &str, count: u64) -> Self {
        self.push_key(key);
        self.body.extend_from_slice(&9u32.to_le_bytes());
        self.body.extend_from_slice(&4u32.to_le_bytes());
        self.body.extend_from_slice(&count.to_le_bytes());
        for i in 0..count {
            self.body.extend_from_slice(&((i % 4) as u32).to_le_bytes());
        }
        self
    }

    /// A pair whose value tag is written verbatim, valid or not.
    pub(super) fn raw_tag_value(mut self, key: &str, tag: u32) -> Self {
        self.push_key(key);
        self.body.extend_from_slice(&tag.to_le_bytes());
        self
    }

    /// A key whose length prefix claims `len` bytes and supplies none.
    pub(super) fn oversized_key_len(mut self, len: u64) -> Self {
        self.kv_count += 1;
        self.body.extend_from_slice(&len.to_le_bytes());
        self
    }

    /// Seal the pairs behind the 24-byte preamble.
    pub(super) fn build(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.body.len());
        out.extend_from_slice(b"GGUF");
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.tensor_count.to_le_bytes());
        out.extend_from_slice(&self.kv_count.to_le_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}
