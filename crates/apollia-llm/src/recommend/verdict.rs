//! Turn a probed GGUF header into a decision about the embedded engine.
//!
//! The verdict is deliberately three-valued. A binary compatible/incompatible
//! flag would have to choose, for every fact the probe could not establish,
//! between hiding a model that works and offering one that does not. Both are
//! bad, and neither is necessary: the middle value carries the caveats and lets
//! the surface say what it does not know.
//!
//! The separation that matters is between a [`Blocker`], which is a fact about
//! the file that no amount of table maintenance will change, and a [`Caveat`],
//! which is usually a fact about the limits of what was measured.

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;

use super::llama_support::{
    architecture_is_generative, architecture_is_non_generative, pre_tokenizer_is_known,
    LLAMA_CPP_TAG,
};

/// A reason the embedded engine will not run this file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Blocker {
    /// The architecture is one the engine loads, but not for text generation.
    ///
    /// An embedding or audio model offered as a chat engine. No table update
    /// makes this work, because the model does not do the job.
    NotGenerative {
        /// The `general.architecture` value read from the header.
        architecture: String,
    },

    /// The pre-tokenizer identifier is absent from this build's chain.
    ///
    /// The engine aborts the load with `unknown pre-tokenizer type`. Reported
    /// only when the tables were able to speak, which is to say when the
    /// architecture was recognised: a file the generator has never seen at all
    /// yields a caveat instead.
    UnknownPreTokenizer {
        /// The `tokenizer.ggml.pre` value read from the header.
        pre_tokenizer: String,
        /// The build whose chain was consulted.
        llama_cpp_tag: String,
    },

    /// The file is one shard of a split GGUF.
    ///
    /// `llama-server` loads a split model from its first shard and pulls the
    /// rest itself, but the downloader fetches one URL. Offering a shard would
    /// leave the operator with an incomplete set and the runtime's
    /// `GgufSplitIncomplete` error.
    ShardedDownload {
        /// How many shards the file name declares.
        shard_count: u32,
    },

    /// The bytes at the URL are not a GGUF header.
    NotGguf {
        /// What the reader reported.
        detail: String,
    },
}

/// Something the operator should know before choosing this file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Caveat {
    /// The file carries no `tokenizer.chat_template`.
    ///
    /// The runtime launches with `--jinja`, which is how tool calling reaches
    /// the model. A file without a template still answers prose, so this does
    /// not block, but an agent runtime is most of what Apollia is and this
    /// model will not drive it.
    NoChatTemplate,

    /// The architecture is absent from the generated table.
    ///
    /// Most often a model released after the table was last generated. It very
    /// probably works; nothing here can promise it does.
    UnverifiedArchitecture {
        /// The `general.architecture` value read from the header.
        architecture: String,
        /// The build whose table was consulted.
        llama_cpp_tag: String,
    },

    /// The probe stopped before reaching every field the verdict wanted.
    ///
    /// Raising the budget or running a confirming probe resolves it.
    HeaderTruncated {
        /// The fields that were still missing when the buffer ran out.
        missing: Vec<String>,
    },

    /// A byte-level BPE tokenizer declared no pre-tokenizer.
    ///
    /// Legacy conversions predate the field. The engine falls back to its
    /// default, which is usually right and occasionally tokenises subtly wrong.
    /// Only raised for BPE (`tokenizer.ggml.model = "gpt2"`), the one family
    /// the engine consults the field for: a SentencePiece model such as Gemma
    /// has no pre-tokenizer to declare, and flagging it only taught operators
    /// to ignore the caveat.
    NoPreTokenizer,
}

/// What a probe concluded about one GGUF file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Verdict {
    /// Everything the verdict wanted to check was checked and passed.
    Supported,

    /// Nothing blocks the file, but something could not be established or is
    /// worth saying out loud.
    Caveats {
        /// What the operator should know.
        caveats: Vec<Caveat>,
    },

    /// The engine will not run this file.
    Rejected {
        /// Why, one entry per independent reason.
        blockers: Vec<Blocker>,
        /// Caveats gathered alongside, kept so a diagnostic view is complete.
        caveats: Vec<Caveat>,
    },
}

impl Verdict {
    /// Whether the file may be offered to the operator at all.
    #[must_use]
    pub fn is_offerable(&self) -> bool {
        !matches!(self, Self::Rejected { .. })
    }

    /// Whether the file can drive tool calls, which is what an agent needs.
    ///
    /// A model that answers prose but carries no chat template is a poor
    /// default for a runtime whose whole purpose is running agents, so the
    /// ranking demotes it rather than hiding it.
    #[must_use]
    pub fn supports_tool_calling(&self) -> bool {
        match self {
            Self::Supported => true,
            Self::Caveats { caveats } => !caveats.contains(&Caveat::NoChatTemplate),
            Self::Rejected { .. } => false,
        }
    }
}

/// Parse `<prefix>-NNNNN-of-NNNNN.gguf` and return the shard total.
///
/// Mirrors the rule `apollia-cli` applies to installed files. The format is a
/// public invariant of GGUF rather than a detail of either caller, and the CLI
/// does not depend on this crate.
#[must_use]
pub fn shard_count_of(file_name: &str) -> Option<u32> {
    let stem = file_name.strip_suffix(".gguf")?;
    // `-NNNNN-of-NNNNN` is 15 characters, the separator included.
    let (prefix, trailing) = stem.split_at(stem.len().checked_sub(15)?);
    if prefix.is_empty() {
        return None;
    }
    let rest = trailing.strip_prefix('-')?;
    let (index, tail) = rest.split_at(5);
    let total = tail.strip_prefix("-of-")?;
    index.parse::<u32>().ok()?;
    // A single-shard name is not a split model, so it downloads as one file.
    total.parse::<u32>().ok().filter(|t| *t > 1)
}

/// Decide whether the embedded engine will run this file.
///
/// `file_name` is used for the shard check alone; everything else comes from
/// the probed header.
#[must_use]
pub fn assess(file_name: &str, facts: &GgufHeaderFacts) -> Verdict {
    let mut blockers = Vec::new();
    let mut caveats = Vec::new();

    if let Some(shard_count) = shard_count_of(file_name) {
        blockers.push(Blocker::ShardedDownload { shard_count });
    }

    let architecture_known = match facts.architecture.as_deref() {
        Some(arch) if architecture_is_non_generative(arch) => {
            blockers.push(Blocker::NotGenerative {
                architecture: arch.to_owned(),
            });
            true
        }
        Some(arch) if architecture_is_generative(arch) => true,
        Some(arch) => {
            caveats.push(Caveat::UnverifiedArchitecture {
                architecture: arch.to_owned(),
                llama_cpp_tag: LLAMA_CPP_TAG.to_owned(),
            });
            false
        }
        None => false,
    };

    match facts.tokenizer_pre.as_deref() {
        // The pre-tokenizer only blocks when the tables proved they know this
        // corner of the format. On an architecture the generator has never
        // seen, an unrecognised pre-tokenizer says more about the table than
        // about the file, and the architecture caveat already carries that.
        Some(pre) if !pre_tokenizer_is_known(pre) && architecture_known => {
            blockers.push(Blocker::UnknownPreTokenizer {
                pre_tokenizer: pre.to_owned(),
                llama_cpp_tag: LLAMA_CPP_TAG.to_owned(),
            });
        }
        Some(_) => {}
        None if facts.truncated => {}
        // llama.cpp reads `tokenizer.ggml.pre` only for BPE vocabularies.
        None if facts
            .tokenizer_model
            .as_deref()
            .is_some_and(|model| model != "gpt2") => {}
        None => caveats.push(Caveat::NoPreTokenizer),
    }

    if facts.truncated {
        let mut missing = Vec::new();
        if facts.tokenizer_pre.is_none() {
            missing.push("tokenizer.ggml.pre".to_owned());
        }
        if !facts.has_chat_template {
            missing.push("tokenizer.chat_template".to_owned());
        }
        if facts.block_count.is_none() {
            missing.push("block_count".to_owned());
        }
        if !missing.is_empty() {
            caveats.push(Caveat::HeaderTruncated { missing });
        }
    } else if !facts.has_chat_template {
        caveats.push(Caveat::NoChatTemplate);
    }

    if !blockers.is_empty() {
        return Verdict::Rejected { blockers, caveats };
    }
    if caveats.is_empty() {
        return Verdict::Supported;
    }
    Verdict::Caveats { caveats }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_facts(arch: &str, pre: &str) -> GgufHeaderFacts {
        GgufHeaderFacts {
            version: 3,
            kv_count: 8,
            kv_read: 8,
            architecture: Some(arch.to_owned()),
            tokenizer_pre: Some(pre.to_owned()),
            has_chat_template: true,
            block_count: Some(36),
            head_count: Some(32),
            head_count_kv: Some(8),
            embedding_length: Some(2560),
            truncated: false,
            ..GgufHeaderFacts::default()
        }
    }

    #[test]
    fn a_fully_read_supported_model_passes_without_caveats() {
        // GIVEN a complete header for a known architecture and pre-tokenizer
        let facts = complete_facts("qwen3", "qwen2");

        // WHEN it is assessed
        let verdict = assess("Qwen3-4B-Q4_K_M.gguf", &facts);

        // THEN it is supported outright, tool calling included
        assert_eq!(verdict, Verdict::Supported);
        assert!(verdict.is_offerable());
        assert!(verdict.supports_tool_calling());
    }

    #[test]
    fn an_embedding_architecture_is_rejected_rather_than_cautioned() {
        // GIVEN a header for an architecture that does not generate text
        let facts = complete_facts("bert", "default");

        // WHEN it is assessed
        let verdict = assess("bge-large-Q8_0.gguf", &facts);

        // THEN it is rejected, because no table update makes it a chat engine
        assert!(!verdict.is_offerable());
        let Verdict::Rejected { blockers, .. } = verdict else {
            panic!("an embedding model must be rejected");
        };
        assert!(blockers
            .iter()
            .any(|b| matches!(b, Blocker::NotGenerative { .. })));
    }

    #[test]
    fn a_missing_pre_tokenizer_matters_only_for_bpe() {
        // GIVEN two headers without a pre-tokenizer, one SentencePiece (as
        // Gemma ships) and one byte-level BPE
        let mut spm = GgufHeaderFacts {
            version: 3,
            architecture: Some("gemma3".to_owned()),
            tokenizer_model: Some("llama".to_owned()),
            has_chat_template: true,
            block_count: Some(34),
            ..GgufHeaderFacts::default()
        };
        let mut bpe = spm.clone();
        bpe.architecture = Some("qwen3".to_owned());
        bpe.tokenizer_model = Some("gpt2".to_owned());
        spm.tokenizer_pre = None;
        bpe.tokenizer_pre = None;

        // WHEN both are assessed
        let spm_caveats = assess("gemma.gguf", &spm);
        let bpe_caveats = assess("qwen.gguf", &bpe);

        // THEN only the BPE one carries the caveat, since the engine never
        // reads the field for a SentencePiece vocabulary
        let has = |v: &Verdict| matches!(v, Verdict::Caveats { caveats } if caveats.contains(&Caveat::NoPreTokenizer));
        assert!(!has(&spm_caveats));
        assert!(has(&bpe_caveats));
    }

    #[test]
    fn an_unknown_pre_tokenizer_on_a_known_architecture_blocks() {
        // GIVEN a known architecture whose pre-tokenizer this build cannot resolve
        let facts = complete_facts("llama", "some-pre-tokenizer-from-next-year");

        // WHEN it is assessed
        let verdict = assess("Model-Q4_K_M.gguf", &facts);

        // THEN it is rejected, since the engine would abort the load after the
        // download rather than before it
        let Verdict::Rejected { blockers, .. } = verdict else {
            panic!("an unresolvable pre-tokenizer must be rejected");
        };
        assert!(blockers
            .iter()
            .any(|b| matches!(b, Blocker::UnknownPreTokenizer { .. })));
    }

    #[test]
    fn an_unknown_pre_tokenizer_on_an_unknown_architecture_only_cautions() {
        // GIVEN an architecture the generated table has never seen, whose
        // pre-tokenizer is therefore equally unseen
        let facts = complete_facts("brandnew5", "brandnew5-bpe");

        // WHEN it is assessed
        let verdict = assess("BrandNew5-Q4_K_M.gguf", &facts);

        // THEN it stays offerable: the tables are stale, which is a fact about
        // the tables and not about the file
        assert!(verdict.is_offerable());
        let Verdict::Caveats { caveats } = verdict else {
            panic!("an unseen architecture must caution, not reject");
        };
        assert!(caveats
            .iter()
            .any(|c| matches!(c, Caveat::UnverifiedArchitecture { .. })));
    }

    #[test]
    fn a_missing_chat_template_cautions_and_withdraws_tool_calling() {
        // GIVEN a complete header for a supported model that carries no template
        let mut facts = complete_facts("qwen3", "qwen2");
        facts.has_chat_template = false;

        // WHEN it is assessed
        let verdict = assess("Qwen3-4B-Q4_K_M.gguf", &facts);

        // THEN it may still be offered, but not as a model that calls tools
        assert!(verdict.is_offerable());
        assert!(!verdict.supports_tool_calling());
        assert_eq!(
            verdict,
            Verdict::Caveats {
                caveats: vec![Caveat::NoChatTemplate]
            }
        );
    }

    #[test]
    fn a_truncated_probe_does_not_report_the_template_as_absent() {
        // GIVEN a screening probe that stopped before the chat template
        let mut facts = complete_facts("qwen3", "qwen2");
        facts.has_chat_template = false;
        facts.truncated = true;

        // WHEN it is assessed
        let verdict = assess("Qwen3-4B-Q4_K_M.gguf", &facts);

        // THEN the caveat names the truncation, not a missing template, so a
        // confirming probe is what resolves it
        let Verdict::Caveats { caveats } = verdict else {
            panic!("a truncated probe must caution");
        };
        assert!(!caveats.contains(&Caveat::NoChatTemplate));
        assert!(caveats
            .iter()
            .any(|c| matches!(c, Caveat::HeaderTruncated { .. })));
    }

    #[test]
    fn a_shard_is_rejected_because_the_downloader_fetches_one_url() {
        // GIVEN the first shard of a split model
        let facts = complete_facts("qwen3moe", "qwen2");

        // WHEN it is assessed
        let verdict = assess("Qwen3-30B-A3B-Q4_K_M-00001-of-00002.gguf", &facts);

        // THEN it is rejected: one URL would leave the set incomplete on disk
        let Verdict::Rejected { blockers, .. } = verdict else {
            panic!("a shard must be rejected while the downloader is single-file");
        };
        assert!(blockers
            .iter()
            .any(|b| matches!(b, Blocker::ShardedDownload { shard_count: 2 })));
    }

    #[test]
    fn a_single_file_name_is_not_mistaken_for_a_shard() {
        // GIVEN names that resemble the shard pattern without matching it
        let cases = [
            "Qwen3-4B-Q4_K_M.gguf",
            "model-00001-of-00001.gguf",
            "-00001-of-00002.gguf",
            "model-1-of-2.gguf",
        ];

        // WHEN each is parsed for a shard total
        // THEN none yields one, so no ordinary file is refused as a shard
        for name in cases {
            assert_eq!(shard_count_of(name), None, "{name} was read as a shard");
        }
    }
}
