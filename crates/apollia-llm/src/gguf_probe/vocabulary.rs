//! Whether two GGUF models read the same vocabulary.
//!
//! Speculative decoding has a small model propose tokens that a larger one
//! checks. The proposal is only worth checking when both models number their
//! tokens alike, and two keys of the header say so without loading either
//! file: the vocabulary family (`tokenizer.ggml.model`) and the number of
//! tokens (the length of `tokenizer.ggml.tokens`).

use super::GgufHeaderFacts;

/// Whether the two headers declare the same vocabulary family and the same
/// number of tokens. A header that does not say one of the two is never
/// compatible: an unknown is not a match.
#[must_use]
pub fn same_vocabulary(main: &GgufHeaderFacts, draft: &GgufHeaderFacts) -> bool {
    let family = match (&main.tokenizer_model, &draft.tokenizer_model) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let count = match (main.tokenizer_token_count, draft.tokenizer_token_count) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    family && count
}

#[cfg(test)]
mod tests {
    use super::super::builder::HeaderBuilder;
    use super::super::parse_header;
    use super::*;

    /// The vocabulary keys of a header as its publisher wrote them: the
    /// family, the pre-tokenizer, then the tokens.
    fn recorded(architecture: &str, family: &str, pre: &str, tokens: u64) -> GgufHeaderFacts {
        let bytes = HeaderBuilder::new()
            .string("general.architecture", architecture)
            .string("tokenizer.ggml.model", family)
            .string("tokenizer.ggml.pre", pre)
            .string_array("tokenizer.ggml.tokens", tokens)
            .build();
        parse_header(&bytes).expect("a well-formed header reads")
    }

    #[test]
    fn test_the_token_count_is_the_length_of_the_vocabulary() {
        // GIVEN a header whose vocabulary holds 151936 tokens, cut inside it
        let bytes = HeaderBuilder::new()
            .string("tokenizer.ggml.model", "gpt2")
            .string_array("tokenizer.ggml.tokens", 151_936)
            .build();
        let cut = bytes.len() / 2;

        // WHEN the whole header is read, then only its first half
        let whole = parse_header(&bytes).expect("reads");
        let half = parse_header(&bytes[..cut]).expect("a truncated header still reads");

        // THEN both reads know the count, which sits at the head of the array
        assert_eq!(whole.tokenizer_token_count, Some(151_936));
        assert!(half.truncated);
        assert_eq!(half.tokenizer_token_count, Some(151_936));
    }

    #[test]
    fn test_a_draft_of_the_same_vocabulary_is_compatible() {
        // GIVEN the vocabulary keys of Qwen2.5 7B Instruct and of Qwen2.5
        // Coder 0.5B: the same family, 151936 tokens each
        let main = recorded("qwen2", "gpt2", "qwen2", 151_936);
        let draft = recorded("qwen2", "gpt2", "qwen2", 151_936);

        // WHEN the pair is compared
        let compatible = same_vocabulary(&main, &draft);

        // THEN the small model can draft for the large one
        assert!(compatible);
    }

    #[test]
    fn test_a_draft_of_another_vocabulary_is_not_compatible() {
        // GIVEN Qwen2.5 7B Instruct (BPE, 151936 tokens) and two models that
        // do not share its vocabulary: Llama 3.1 8B, the same family with
        // 128256 tokens, and Mistral 7B, another family with 32768
        let main = recorded("qwen2", "gpt2", "qwen2", 151_936);
        let other_count = recorded("llama", "gpt2", "llama-bpe", 128_256);
        let other_family = recorded("llama", "llama", "default", 32_768);

        // WHEN each pair is compared
        let by_count = same_vocabulary(&main, &other_count);
        let by_family = same_vocabulary(&main, &other_family);

        // THEN neither is proposed
        assert!(!by_count);
        assert!(!by_family);
    }

    #[test]
    fn test_a_header_that_does_not_say_is_not_compatible() {
        // GIVEN a header with a family and no vocabulary, on either side
        let main = recorded("qwen2", "gpt2", "qwen2", 151_936);
        let silent = parse_header(
            &HeaderBuilder::new()
                .string("tokenizer.ggml.model", "gpt2")
                .build(),
        )
        .expect("reads");

        // WHEN it is compared with a complete header, and with itself
        let as_draft = same_vocabulary(&main, &silent);
        let both = same_vocabulary(&silent, &silent);

        // THEN an unknown count never passes for a match
        assert!(!as_draft);
        assert!(!both);
    }
}
