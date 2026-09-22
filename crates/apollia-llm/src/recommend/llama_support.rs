//! What the embedded `llama-server` build can load.
//!
//! # This file is maintained by a generator
//!
//! `scripts/gen-llama-support.py` reads the llama.cpp sources at the tag pinned
//! in `packaging/fetch-llama-server.sh` and rewrites everything between the
//! generated markers below. Run it when that pin moves:
//!
//! ```sh
//! python scripts/gen-llama-support.py
//! ```
//!
//! `scripts/gen-llama-support.py --check` re-derives the tables and fails if
//! they differ from what is checked in, which is how the pin and the tables are
//! kept from drifting apart. Before this existed the architecture list in
//! `hf_registry/compat.rs` carried a comment promising it would be "updated on
//! each crate bump", and nothing enforced it.
//!
//! # Why an unknown entry is not a refusal
//!
//! These tables lag upstream by however long it has been since the generator
//! last ran, and a name absent from them is far more often a name the generator
//! has not seen than a model the engine will refuse. Refusing on absence would
//! make every newly released model invisible until someone re-ran a script,
//! which is precisely the staleness this whole effort is meant to remove.
//!
//! So absence produces a caveat and presence produces a confirmation. Only the
//! things that are wrong by construction, an embedding architecture asked to
//! generate text, block outright.

/// The llama.cpp release the tables below were derived from.
///
/// Kept in step with `LLAMA_CPP_TAG` in `packaging/fetch-llama-server.sh` by
/// `scripts/gen-llama-support.py --check`.
pub const LLAMA_CPP_TAG: &str = "b10092";

// ── generated:architectures ─────────────────────────

/// `general.architecture` values the engine loads for text generation.
///
/// Sourced from `LLM_ARCH_NAMES` in `src/llama-arch.cpp`, less the entries that
/// [`NON_GENERATIVE_ARCHITECTURES`] claims.
pub static GENERATIVE_ARCHITECTURES: &[&str] = &[
    "afmoe",
    "apertus",
    "arcee",
    "arctic",
    "arwkv7",
    "baichuan",
    "bailingmoe",
    "bailingmoe2",
    "bitnet",
    "bloom",
    "chameleon",
    "chatglm",
    "codeshell",
    "cogvlm",
    "cohere2",
    "cohere2moe",
    "command-r",
    "dbrx",
    "deci",
    "deepseek",
    "deepseek2",
    "deepseek32",
    "deepseek4",
    "dflash",
    "dots1",
    "dream",
    "eagle3",
    "ernie4_5",
    "ernie4_5-moe",
    "exaone",
    "exaone-moe",
    "exaone4",
    "falcon",
    "falcon-h1",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gemma4",
    "gemma4-assistant",
    "glm-dsa",
    "glm4",
    "glm4moe",
    "gpt-oss",
    "gpt2",
    "gptj",
    "gptneox",
    "granite",
    "granitehybrid",
    "granitemoe",
    "grok",
    "grovemoe",
    "hunyuan-dense",
    "hunyuan-moe",
    "hunyuan_vl",
    "hy_v3",
    "internlm2",
    "jais",
    "jais2",
    "jamba",
    "kimi-linear",
    "laguna",
    "lfm2",
    "lfm2moe",
    "llada",
    "llada-moe",
    "llama",
    "llama4",
    "maincoder",
    "mamba",
    "mamba2",
    "mellum",
    "mimo2",
    "minicpm",
    "minicpm3",
    "minimax-m2",
    "mistral3",
    "mistral4",
    "mpt",
    "nemotron",
    "nemotron_h",
    "nemotron_h_moe",
    "olmo",
    "olmo2",
    "olmoe",
    "openelm",
    "orion",
    "pangu-embedded",
    "phi2",
    "phi3",
    "phimoe",
    "plamo",
    "plamo2",
    "plamo3",
    "plm",
    "qwen",
    "qwen2",
    "qwen2moe",
    "qwen2vl",
    "qwen3",
    "qwen35",
    "qwen35moe",
    "qwen3moe",
    "qwen3next",
    "qwen3vl",
    "qwen3vlmoe",
    "refact",
    "rnd1",
    "rwkv6",
    "rwkv6qwen2",
    "rwkv7",
    "seed_oss",
    "smallthinker",
    "smollm3",
    "stablelm",
    "starcoder",
    "starcoder2",
    "step35",
    "t5",
    "talkie",
    "xverse",
];

/// Architectures the engine knows but which do not generate text.
///
/// A repository of these is not a lagging table entry but a category error: an
/// embedding or audio model offered as a chat engine. This list therefore
/// blocks, where an unknown name only cautions.
pub static NON_GENERATIVE_ARCHITECTURES: &[&str] = &[
    "bert",
    "clip",
    "deepseek2-ocr",
    "eurobert",
    "gemma-embedding",
    "jina-bert-v2",
    "jina-bert-v3",
    "llama-embed",
    "modern-bert",
    "neo-bert",
    "nomic-bert",
    "nomic-bert-moe",
    "paddleocr",
    "t5encoder",
    "wavtokenizer-dec",
];

// ── generated:pre-tokenizers ────────────────────────────

/// `tokenizer.ggml.pre` values this build resolves.
///
/// Sourced from the `tokenizer_pre` comparison chain in
/// `src/llama-vocab.cpp`. A GGUF whose pre-tokenizer is missing from the
/// engine's chain aborts the load outright, with `unknown pre-tokenizer type`,
/// after the download has already been paid for. That failure is the reason
/// the probe reads this field before the download rather than after.
pub static KNOWN_PRE_TOKENIZERS: &[&str] = &[
    "a.x-4.0",
    "afmoe",
    "bailingmoe",
    "bailingmoe2",
    "bloom",
    "chameleon",
    "chatglm-bpe",
    "codeshell",
    "cohere2moe",
    "command-r",
    "dbrx",
    "deepseek-coder",
    "deepseek-llm",
    "deepseek-r1-qwen",
    "deepseek-v3",
    "default",
    "exaone",
    "exaone-moe",
    "exaone4",
    "f2llmv2",
    "falcon",
    "falcon-h1",
    "falcon3",
    "gemma4",
    "gigachat",
    "glm4",
    "gpt-2",
    "gpt-4o",
    "gpt3-finnish",
    "granite-docling",
    "granite-embed-multi-311m",
    "granite-embed-multi-97m",
    "grok-2",
    "hunyuan",
    "hunyuan-dense",
    "jais",
    "jais-2",
    "jina-de",
    "jina-es",
    "jina-v1-en",
    "jina-v2-code",
    "jina-v2-de",
    "jina-v2-es",
    "jina-v5-nano",
    "joyai-llm",
    "kanana2",
    "kimi-k2",
    "kormo",
    "laguna",
    "lfm2",
    "llada-moe",
    "llama-bpe",
    "llama-v3",
    "llama3",
    "llama4",
    "megrez",
    "mellum",
    "mellum2",
    "midm-2.0",
    "minerva-7b",
    "minicpm5",
    "minimax-m2",
    "modern-bert",
    "mpt",
    "olmo",
    "phi-2",
    "pixtral",
    "poro-chat",
    "qwen2",
    "qwen35",
    "refact",
    "roberta-bpe",
    "sarvam-moe",
    "seed-coder",
    "smaug-bpe",
    "smollm",
    "solar-open",
    "stablelm2",
    "starcoder",
    "superbpe",
    "talkie",
    "tekken",
    "tiny_aya",
    "trillion",
    "viking",
    "whitespace",
    "youtu",
];

// ── generated:attention ──────────────────────────────────

/// One architecture's sliding-window layout.
///
/// `n_pattern` follows `llama_hparams::set_swa_pattern`: layer `il` slides when
/// `n_pattern == 0`, or when `il % n_pattern < n_pattern - 1`, or, with
/// `dense_first`, when `il % n_pattern != 0`. `n_pattern == 1` means no layer
/// slides at all.
///
/// `fixed_window` is a window the loader hardcodes rather than reading from the
/// GGUF; `0` means the file's own `attention.sliding_window` decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlidingWindow {
    /// The `general.architecture` this applies to.
    pub architecture: &'static str,
    /// Period of the dense layers.
    pub n_pattern: u32,
    /// Whether the pattern starts on a dense layer.
    pub dense_first: bool,
    /// A window size fixed in the loader, or `0` to read the file's own.
    pub fixed_window: u32,
}

/// Architectures whose layers mostly attend to a short window.
///
/// Sourced from the `set_swa_pattern` calls in `src/models/`. This is the
/// largest single correction to a cache estimate: Gemma 3 holds 29 of its 34
/// layers at 1536 cells rather than 32768, which is 0.8 GB of cache where the
/// naive formula predicts 5.3 GB.
pub static SLIDING_WINDOWS: &[SlidingWindow] = &[
    SlidingWindow {
        architecture: "afmoe",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "cohere2",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "cohere2moe",
        n_pattern: 4,
        dense_first: true,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "deepseek4",
        n_pattern: 0,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "exaone-moe",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 128,
    },
    SlidingWindow {
        architecture: "exaone4",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 4096,
    },
    SlidingWindow {
        architecture: "gemma-embedding",
        n_pattern: 6,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "gemma2",
        n_pattern: 2,
        dense_first: false,
        fixed_window: 4096,
    },
    SlidingWindow {
        architecture: "gemma3",
        n_pattern: 6,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "gemma3n",
        n_pattern: 5,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "llama4",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 8192,
    },
    SlidingWindow {
        architecture: "mellum",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "modern-bert",
        n_pattern: 3,
        dense_first: true,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "olmo2",
        n_pattern: 4,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "openai-moe",
        n_pattern: 2,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "plamo3",
        n_pattern: 8,
        dense_first: false,
        fixed_window: 0,
    },
    SlidingWindow {
        architecture: "smallthinker",
        n_pattern: 4,
        dense_first: true,
        fixed_window: 4096,
    },
];

/// One architecture's hybrid attention layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridAttention {
    /// The `general.architecture` this applies to.
    pub architecture: &'static str,
    /// Every nth layer keeps a real cache; the rest carry recurrent state.
    pub full_attention_interval: u32,
}

/// Architectures that mix linear-attention layers with full-attention ones.
///
/// Only the full-attention layers hold a cache that grows with the context.
/// Qwen3.5 defaults to one layer in four, so its cache is a quarter of what a
/// dense model of the same depth would reserve.
pub static HYBRID_ATTENTION: &[HybridAttention] = &[
    HybridAttention {
        architecture: "qwen35",
        full_attention_interval: 4,
    },
    HybridAttention {
        architecture: "qwen35moe",
        full_attention_interval: 4,
    },
    HybridAttention {
        architecture: "qwen3next",
        full_attention_interval: 4,
    },
];

/// Architectures whose state does not grow with the context at all.
///
/// Sourced from `llm_arch_is_recurrent`. Sizing one of these with the
/// attention formula would predict gigabytes where the real cost is
/// megabytes, and flat in the context length.
pub static RECURRENT_ARCHITECTURES: &[&str] =
    &["arwkv7", "mamba", "mamba2", "rwkv6", "rwkv6qwen2", "rwkv7"];

// ── generated:end ───────────────────────────────────

/// Whether the engine generates text with this architecture.
#[must_use]
pub fn architecture_is_generative(arch: &str) -> bool {
    GENERATIVE_ARCHITECTURES.contains(&arch)
}

/// Whether this architecture is known and known not to generate text.
#[must_use]
pub fn architecture_is_non_generative(arch: &str) -> bool {
    NON_GENERATIVE_ARCHITECTURES.contains(&arch)
}

/// Whether the engine's pre-tokenizer chain resolves this identifier.
#[must_use]
pub fn pre_tokenizer_is_known(pre: &str) -> bool {
    KNOWN_PRE_TOKENIZERS.contains(&pre)
}

/// The sliding-window layout of this architecture, when it has one.
#[must_use]
pub fn sliding_window_for(arch: &str) -> Option<SlidingWindow> {
    SLIDING_WINDOWS
        .iter()
        .find(|w| w.architecture == arch)
        .copied()
}

/// The hybrid attention layout of this architecture, when it has one.
#[must_use]
pub fn hybrid_attention_for(arch: &str) -> Option<HybridAttention> {
    HYBRID_ATTENTION
        .iter()
        .find(|h| h.architecture == arch)
        .copied()
}

/// Whether every layer carries recurrent state rather than a growing cache.
#[must_use]
pub fn architecture_is_recurrent(arch: &str) -> bool {
    RECURRENT_ARCHITECTURES.contains(&arch)
}

impl SlidingWindow {
    /// Whether layer `il` attends to a short window.
    ///
    /// Reproduces `llama_hparams::set_swa_pattern` exactly, the two modulo
    /// tests spelled as `is_multiple_of`. Getting this wrong by one layer is a
    /// few percent of the cache; getting the branch wrong inverts the whole
    /// estimate, which is why it follows upstream rather than being re-derived.
    #[must_use]
    pub fn layer_slides(&self, il: u32) -> bool {
        if self.n_pattern == 0 {
            return true;
        }
        if self.dense_first {
            !il.is_multiple_of(self.n_pattern)
        } else {
            il % self.n_pattern < self.n_pattern - 1
        }
    }
}

impl HybridAttention {
    /// Whether layer `il` keeps a real key/value cache.
    ///
    /// Mirrors the default in `llama_model_qwen35::load_arch_hparams`: a layer
    /// is recurrent when `(il + 1) % interval != 0`, so the full-attention
    /// layers are the last of each group.
    #[must_use]
    pub fn layer_has_cache(&self, il: u32) -> bool {
        self.full_attention_interval != 0 && (il + 1).is_multiple_of(self.full_attention_interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many of `n_layer` layers attend to the short window.
    fn sliding_layers(window: &SlidingWindow, n_layer: u32) -> u32 {
        (0..n_layer).filter(|il| window.layer_slides(*il)).count() as u32
    }

    /// How many of `n_layer` layers keep a real cache.
    fn cached_layers(hybrid: &HybridAttention, n_layer: u32) -> u32 {
        (0..n_layer)
            .filter(|il| hybrid.layer_has_cache(*il))
            .count() as u32
    }

    #[test]
    fn the_two_architecture_lists_do_not_overlap() {
        // GIVEN the generative and non-generative architecture tables
        // WHEN every generative entry is looked for among the non-generative
        // THEN none appears in both, so no architecture both loads and blocks
        for arch in GENERATIVE_ARCHITECTURES {
            assert!(
                !NON_GENERATIVE_ARCHITECTURES.contains(arch),
                "{arch} is in both architecture tables"
            );
        }
    }

    #[test]
    fn the_tables_are_sorted_so_the_generator_produces_a_stable_diff() {
        // GIVEN the four generated string tables
        let tables: [(&str, &[&str]); 4] = [
            ("GENERATIVE_ARCHITECTURES", GENERATIVE_ARCHITECTURES),
            ("NON_GENERATIVE_ARCHITECTURES", NON_GENERATIVE_ARCHITECTURES),
            ("KNOWN_PRE_TOKENIZERS", KNOWN_PRE_TOKENIZERS),
            ("RECURRENT_ARCHITECTURES", RECURRENT_ARCHITECTURES),
        ];

        // WHEN each is compared against its own sorted, deduplicated form
        // THEN they match, so a regenerated table diffs only where upstream moved
        for (name, table) in tables {
            let mut sorted: Vec<&str> = table.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                table,
                sorted.as_slice(),
                "{name} is unsorted or has repeats"
            );
        }
    }

    #[test]
    fn the_gemma3_window_pattern_matches_the_engine() {
        // GIVEN Gemma 3, whose loader sets a period of six
        let window = sliding_window_for("gemma3").expect("gemma3 slides");

        // WHEN its 34 layers are classified
        // THEN five are dense and twenty-nine slide, which is what turns a
        // 5.3 GB cache estimate into a 0.8 GB one
        assert_eq!(window.n_pattern, 6);
        assert_eq!(sliding_layers(&window, 34), 29);
        assert!(window.layer_slides(0));
        assert!(!window.layer_slides(5));
        assert!(window.layer_slides(6));
    }

    #[test]
    fn a_pattern_of_zero_slides_every_layer_and_one_slides_none() {
        // GIVEN the two degenerate periods upstream documents
        let all = SlidingWindow {
            architecture: "x",
            n_pattern: 0,
            dense_first: false,
            fixed_window: 0,
        };
        let none = SlidingWindow {
            architecture: "x",
            n_pattern: 1,
            dense_first: false,
            fixed_window: 0,
        };

        // WHEN a stack of layers is classified
        // THEN zero means every layer slides and one means no layer does
        assert_eq!(sliding_layers(&all, 32), 32);
        assert_eq!(sliding_layers(&none, 32), 0);
    }

    #[test]
    fn a_dense_first_pattern_puts_the_full_layer_at_the_front() {
        // GIVEN the upstream example: period two, dense first
        let window = SlidingWindow {
            architecture: "x",
            n_pattern: 2,
            dense_first: true,
            fixed_window: 0,
        };

        // WHEN the first layers are classified
        // THEN layer zero is dense and layer one slides, the opposite of the
        // default ordering
        assert!(!window.layer_slides(0));
        assert!(window.layer_slides(1));
        assert!(!window.layer_slides(2));
    }

    #[test]
    fn qwen35_keeps_a_cache_on_one_layer_in_four() {
        // GIVEN Qwen3.5, whose loader defaults to a full-attention interval of four
        let hybrid = hybrid_attention_for("qwen35").expect("qwen35 is hybrid");

        // WHEN its 32 layers are classified
        // THEN eight keep a cache, so its context costs a quarter of what a
        // dense model of the same depth would reserve
        assert_eq!(hybrid.full_attention_interval, 4);
        assert_eq!(cached_layers(&hybrid, 32), 8);
        assert!(hybrid.layer_has_cache(3));
        assert!(!hybrid.layer_has_cache(0));
    }

    #[test]
    fn a_recurrent_architecture_is_recognised_and_a_transformer_is_not() {
        // GIVEN one recurrent architecture and one ordinary transformer
        // WHEN each is looked up
        // THEN only the recurrent one is claimed, so the attention formula is
        // never applied to a model whose state is flat in the context
        assert!(architecture_is_recurrent("mamba2"));
        assert!(!architecture_is_recurrent("qwen3"));
    }
}
