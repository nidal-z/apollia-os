//! Header-reader tests, driven by headers this module builds byte by byte.
//!
//! Synthesising the bytes rather than checking in a fixture keeps the suite
//! offline and lets a test state exactly the shape it is about: a vocabulary
//! array large enough to push the chat template out of a screening read, a
//! truncation in the middle of a value, a tag the format does not define.

use super::builder::HeaderBuilder;
use super::parse::{parse_header, GgufParseError};

#[test]
fn reads_the_preamble_and_the_general_keys() {
    // GIVEN a header carrying the architecture, the name and the quantisation
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "qwen3")
        .string("general.name", "Qwen3 4B Instruct")
        .u32("general.file_type", 15)
        .build();

    // WHEN the header is read
    let facts = parse_header(&bytes).expect("a well-formed header reads");

    // THEN the preamble and the three values come back, and nothing is truncated
    assert_eq!(facts.version, 3);
    assert_eq!(facts.kv_count, 3);
    assert_eq!(facts.kv_read, 3);
    assert_eq!(facts.architecture.as_deref(), Some("qwen3"));
    assert_eq!(facts.name.as_deref(), Some("Qwen3 4B Instruct"));
    assert_eq!(facts.file_type, Some(15));
    assert!(facts.is_complete());
}

#[test]
fn resolves_architecture_prefixed_hyperparameters() {
    // GIVEN a header whose hyperparameters carry the architecture as a prefix
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "qwen3")
        .u32("qwen3.block_count", 36)
        .u32("qwen3.context_length", 40_960)
        .u32("qwen3.embedding_length", 2560)
        .u32("qwen3.attention.head_count", 32)
        .u32("qwen3.attention.head_count_kv", 8)
        .build();

    // WHEN the header is read
    let facts = parse_header(&bytes).expect("a well-formed header reads");

    // THEN each prefixed key lands in its own field
    assert_eq!(facts.block_count, Some(36));
    assert_eq!(facts.context_length, Some(40_960));
    assert_eq!(facts.embedding_length, Some(2560));
    assert_eq!(facts.head_count, Some(32));
    assert_eq!(facts.head_count_kv, Some(8));
}

#[test]
fn reads_the_pre_tokenizer_ahead_of_the_vocabulary() {
    // GIVEN a header in the order the conversion script emits: the pre-tokenizer
    // identifier first, then a vocabulary array, then the chat template
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "qwen3")
        .string("tokenizer.ggml.model", "gpt2")
        .string("tokenizer.ggml.pre", "qwen2")
        .string_array("tokenizer.ggml.tokens", 4096)
        .string(
            "tokenizer.chat_template",
            "{% for m in messages %}{{ m }}{% endfor %}",
        )
        .build();

    // WHEN the whole header is read
    let facts = parse_header(&bytes).expect("a well-formed header reads");

    // THEN the pre-tokenizer, the vocabulary family and the template all come back
    assert_eq!(facts.tokenizer_pre.as_deref(), Some("qwen2"));
    assert_eq!(facts.tokenizer_model.as_deref(), Some("gpt2"));
    assert!(facts.has_chat_template);
    assert!(facts.is_complete());
}

#[test]
fn a_screening_read_reaches_the_pre_tokenizer_without_the_template() {
    // GIVEN the same header, cut short just past the vocabulary's first entries,
    // which is what a range request over the opening megabyte produces
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "qwen3")
        .string("tokenizer.ggml.pre", "qwen2")
        .string_array("tokenizer.ggml.tokens", 4096)
        .string("tokenizer.chat_template", "{{ never reached }}")
        .build();
    let cut = bytes.len() / 2;

    // WHEN only that prefix is read
    let facts = parse_header(&bytes[..cut]).expect("a truncated header still reads");

    // THEN the pre-tokenizer is known, the template is not, and the result says so
    assert_eq!(facts.tokenizer_pre.as_deref(), Some("qwen2"));
    assert!(!facts.has_chat_template);
    assert!(facts.truncated);
    assert!(!facts.is_complete());
}

#[test]
fn a_truncated_read_never_reports_a_missing_template_as_complete() {
    // GIVEN a header whose declared pair count exceeds what the buffer holds
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "llama")
        .string("tokenizer.ggml.pre", "llama-bpe")
        .build();
    let cut = bytes.len() - 4;

    // WHEN the short buffer is read
    let facts = parse_header(&bytes[..cut]).expect("a truncated header still reads");

    // THEN the read is marked incomplete, so a caller cannot mistake the absent
    // template for a template the file does not have
    assert!(facts.truncated);
    assert!(!facts.is_complete());
    assert!(!facts.has_chat_template);
}

#[test]
fn skips_arrays_of_fixed_width_elements_by_arithmetic() {
    // GIVEN a header holding a large array of token types before a wanted key
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "gemma3")
        .u32_array("tokenizer.ggml.token_type", 50_000)
        .string("tokenizer.ggml.pre", "default")
        .build();

    // WHEN the header is read
    let facts = parse_header(&bytes).expect("a well-formed header reads");

    // THEN the walk stepped over the array and reached the key behind it
    assert_eq!(facts.architecture.as_deref(), Some("gemma3"));
    assert_eq!(facts.tokenizer_pre.as_deref(), Some("default"));
    assert!(facts.is_complete());
}

#[test]
fn refuses_a_buffer_that_is_not_gguf() {
    // GIVEN a buffer whose first four bytes are not the GGUF magic
    let mut bytes = vec![0u8; 64];
    bytes[..4].copy_from_slice(b"ZIP\0");

    // WHEN it is read as a header
    let err = parse_header(&bytes).expect_err("a non-GGUF buffer is refused");

    // THEN the magic is named rather than the buffer being walked as a header
    assert!(matches!(err, GgufParseError::BadMagic { .. }));
}

#[test]
fn refuses_a_buffer_shorter_than_the_preamble() {
    // GIVEN fewer bytes than the 24-byte preamble
    let bytes = b"GGUF\x03\x00\x00\x00".to_vec();

    // WHEN it is read as a header
    let err = parse_header(&bytes).expect_err("a stub buffer is refused");

    // THEN the length is reported, not a truncated parse
    assert_eq!(err, GgufParseError::PreambleTooShort(8));
}

#[test]
fn refuses_a_header_version_this_build_does_not_walk() {
    // GIVEN a header claiming version 1, whose layout differs from 2 and 3
    let bytes = HeaderBuilder::new()
        .version(1)
        .string("general.architecture", "llama")
        .build();

    // WHEN it is read
    let err = parse_header(&bytes).expect_err("an unknown version is refused");

    // THEN the version is named rather than guessed at
    assert_eq!(err, GgufParseError::UnsupportedVersion(1));
}

#[test]
fn refuses_a_value_type_the_format_does_not_define() {
    // GIVEN a header carrying a value tag outside the documented enumeration
    let bytes = HeaderBuilder::new().raw_tag_value("broken.key", 99).build();

    // WHEN it is read
    let err = parse_header(&bytes).expect_err("an unknown tag is refused");

    // THEN the tag is reported instead of the walk drifting through the buffer
    assert!(matches!(
        err,
        GgufParseError::UnknownValueType { tag: 99, .. }
    ));
}

#[test]
fn refuses_an_implausible_key_length() {
    // GIVEN a header whose key length prefix is far beyond any real key
    let bytes = HeaderBuilder::new().oversized_key_len(1 << 40).build();

    // WHEN it is read
    let err = parse_header(&bytes).expect_err("an implausible length is refused");

    // THEN the guard fires rather than the reader attempting the allocation
    assert!(matches!(
        err,
        GgufParseError::ImplausibleLength { kind: "key", .. }
    ));
}

#[test]
fn keeps_only_an_excerpt_of_a_long_chat_template() {
    // GIVEN a chat template longer than the excerpt the facts retain
    let template = "{% for message in messages %}".repeat(200);
    let bytes = HeaderBuilder::new()
        .string("general.architecture", "qwen3")
        .string("tokenizer.chat_template", &template)
        .build();

    // WHEN the header is read
    let facts = parse_header(&bytes).expect("a well-formed header reads");

    // THEN the template is recorded as present and the excerpt stays bounded
    assert!(facts.has_chat_template);
    let excerpt = facts.chat_template_excerpt.expect("an excerpt is kept");
    assert!(excerpt.len() <= 240);
    assert!(template.starts_with(&excerpt));
}
