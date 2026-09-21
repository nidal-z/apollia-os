#!/usr/bin/env python3
"""Derive the llama.cpp support and attention tables from the pinned release.

`crates/apollia-llm/src/recommend/llama_support.rs` describes what the embedded
`llama-server` can load and how it lays out a key/value cache. Both are
properties of one upstream build: the tag pinned as `LLAMA_CPP_TAG` in
`packaging/fetch-llama-server.sh`.

Four tables come out of here:

- **architectures**, from `LLM_ARCH_NAMES`: what the engine loads at all.
- **pre-tokenizers**, from the comparison chain in `llama-vocab.cpp`: a value
  missing from it aborts the load after the download has been paid for.
- **sliding windows**, from the per-architecture loaders in `src/models/`: which
  layers hold a short window rather than the whole context. Ignoring this
  overstated Gemma 3's cache by a factor of six.
- **recurrent and hybrid layers**: architectures whose layers carry a
  fixed-size state instead of a cache that grows with the context. Qwen3.5 and
  Qwen3-Next keep a real cache on one layer in four.

Nothing here is written by hand, because a table that silently lags the binary
it describes refuses models that work and accepts models that do not, with
equal confidence. The first generated run found the hand-seeded architecture
list short by 64 entries.

Two modes:

    python3 scripts/gen_llama_support.py            rewrite the generated block
    python3 scripts/gen_llama_support.py --check    fail if it is out of date

`--check` runs in the hook. It needs the network only when the cache under
`.cache/llama-support/` has no copy of the pinned tag, and it skips rather than
failing when upstream is unreachable and uncached, so an offline commit is not
blocked. CI runs it with the network, which is where the guard bites.

Exit codes: 0 in step, 1 out of step, 2 on a usage error.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PIN_FILE = REPO_ROOT / "packaging" / "fetch-llama-server.sh"
TARGET = REPO_ROOT / "crates" / "apollia-llm" / "src" / "recommend" / "llama_support.rs"
CACHE_DIR = REPO_ROOT / ".cache" / "llama-support"

RAW_BASE = "https://raw.githubusercontent.com/ggml-org/llama.cpp"
API_BASE = "https://api.github.com/repos/ggml-org/llama.cpp"

ARCH_SOURCE = "src/llama-arch.cpp"
VOCAB_SOURCE = "src/llama-vocab.cpp"

# Per-architecture hyperparameter loaders. Upstream moved these out of
# `llama-model.cpp` into one file per architecture, so a model's attention
# shape is only discoverable by reading all of them.
MODELS_DIR = "src/models"

# Architectures llama.cpp loads but which do not generate text. Upstream does
# not flag these in a machine-readable way, so this is the one hand-kept list
# here, and it is what turns "the engine can load it" into "the engine can chat
# with it".
#
# Two classes of near-miss are deliberately NOT on this list:
#
#   - vision-language models (`qwen3vl`, `hunyuan_vl`, `cogvlm`, ...). They
#     generate text; the image tower is an extra input, not a different job.
#   - `pangu-embedded`, whose name says "embedded" in the sense of a small
#     model for embedded hardware, not "embedding". It is a chat model.
#
# Re-read this list whenever the pin moves: a new encoder-only or OCR
# architecture upstream lands in the generative table until it is named here.
NON_GENERATIVE = [
    # Encoder-only embedding models.
    "bert",
    "eurobert",
    "gemma-embedding",
    "jina-bert-v2",
    "jina-bert-v3",
    "llama-embed",
    "modern-bert",
    "neo-bert",
    "nomic-bert",
    "nomic-bert-moe",
    "t5encoder",
    # Document recognition, not conversation.
    "deepseek2-ocr",
    "paddleocr",
    # A vision encoder on its own, with no language head to chat with.
    "clip",
    # Audio codec.
    "wavtokenizer-dec",
]

RULE = "─"
BEGIN_ARCH = "// " + RULE * 2 + " generated:architectures " + RULE
BEGIN_PRE = "// " + RULE * 2 + " generated:pre-tokenizers " + RULE
BEGIN_ATTN = "// " + RULE * 2 + " generated:attention " + RULE
END = "// " + RULE * 2 + " generated:end " + RULE


def fail(message: str) -> None:
    print(f"gen_llama_support: {message}", file=sys.stderr)


def read_pinned_tag() -> str:
    """The llama.cpp release the binary actually ships."""
    text = PIN_FILE.read_text(encoding="utf-8")
    match = re.search(r'LLAMA_CPP_TAG="\$\{LLAMA_CPP_TAG:-([^}"]+)\}"', text)
    if not match:
        raise SystemExit(f"LLAMA_CPP_TAG not found in {PIN_FILE}")
    return match.group(1)


def fetch(tag: str, path: str) -> str:
    """Read an upstream source file, from the cache when it is already there."""
    cached = CACHE_DIR / tag / path.replace("/", "_")
    if cached.exists():
        return cached.read_text(encoding="utf-8", errors="replace")

    url = f"{RAW_BASE}/{tag}/{path}"
    try:
        with urllib.request.urlopen(url, timeout=60) as response:  # noqa: S310
            body = response.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, TimeoutError) as exc:
        raise SystemExit(f"could not fetch {url}: {exc}") from exc

    cached.parent.mkdir(parents=True, exist_ok=True)
    cached.write_text(body, encoding="utf-8")
    return body


def list_model_files(tag: str) -> list[str]:
    """Names of the per-architecture loader files at `tag`."""
    cached = CACHE_DIR / tag / "_models_index.txt"
    if cached.exists():
        return cached.read_text(encoding="utf-8").split()

    url = f"{API_BASE}/contents/{MODELS_DIR}?ref={tag}"
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "apollia-gen"})
        with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
            entries = json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise SystemExit(f"could not fetch {url}: {exc}") from exc

    names = sorted(e["name"] for e in entries if e["name"].endswith(".cpp"))
    cached.parent.mkdir(parents=True, exist_ok=True)
    cached.write_text("\n".join(names), encoding="utf-8")
    return names


def enum_to_name(arch_source: str) -> dict[str, str]:
    """Map `LLM_ARCH_QWEN3` to the GGUF string `qwen3`."""
    block = re.search(r"LLM_ARCH_NAMES\s*=\s*\{(.*?)\n\};", arch_source, re.S)
    if not block:
        return {}
    pairs = re.findall(
        r'\{\s*LLM_ARCH_([A-Z0-9_]+)\s*,\s*"([^"]+)"\s*\}', block.group(1)
    )
    return dict(pairs)


def extract_architectures(arch_source: str) -> list[str]:
    """Pull the GGUF architecture names out of `LLM_ARCH_NAMES`."""
    names = list(enum_to_name(arch_source).values())
    if not names:
        raise SystemExit("LLM_ARCH_NAMES not found; upstream layout changed")
    return sorted({n for n in names if n not in {"(unknown)", "unknown"}})


def extract_pre_tokenizers(source: str) -> list[str]:
    """Pull the pre-tokenizer identifiers out of the vocabulary loader.

    Upstream resolves `tokenizer.ggml.pre` through a chain of comparisons
    against string literals. Every literal in that chain is a value the engine
    accepts; anything else aborts the load with `unknown pre-tokenizer type`.
    """
    names = set(re.findall(r'tokenizer_pre\s*==\s*"([^"]+)"', source))
    if not names:
        raise SystemExit("no pre-tokenizer comparisons found; upstream layout changed")
    return sorted(names)


def extract_attention(tag: str, arch_source: str):
    """Read each architecture's attention shape out of its own loader.

    Returns three tables, each a term in the key/value cache size:

    - sliding window: `(arch, n_pattern, dense_first, fixed_n_swa)`
    - recurrent: architectures whose state does not grow with the context
    - hybrid: `(arch, full_attention_interval)`
    """
    swa: list[tuple] = []
    hybrid: list[tuple] = []

    for name in list_model_files(tag):
        head = fetch(tag, f"{MODELS_DIR}/{name}")[:6000]
        arch = name[:-4]

        kinds = set(re.findall(r"swa_type\s*=\s*LLAMA_SWA_TYPE_(\w+)", head))
        kinds.discard("NONE")
        if kinds:
            pattern = None
            dense_first = False
            literal = re.search(
                r"set_swa_pattern\(\s*(\d+)\s*(?:,\s*(true))?\s*\)", head
            )
            via_var = re.search(
                r"set_swa_pattern\(\s*(\w+)\s*(?:,\s*(true))?\s*\)", head
            )
            if literal:
                pattern, dense_first = int(literal.group(1)), bool(literal.group(2))
            elif via_var:
                default = re.search(via_var.group(1) + r"\s*=\s*(\d+)", head)
                if default:
                    pattern, dense_first = int(default.group(1)), bool(via_var.group(2))
            fixed = re.search(r"hparams\.n_swa\s*=\s*(\d+)", head)
            if pattern is not None:
                swa.append(
                    (arch, pattern, dense_first, int(fixed.group(1)) if fixed else 0)
                )

        interval = re.search(r"full_attn_interval\s*=\s*(\d+)", head)
        if interval and "is_recr_impl" in head:
            hybrid.append((arch, int(interval.group(1))))

    by_enum = enum_to_name(arch_source)
    recurrent: list[str] = []
    block = re.search(r"llm_arch_is_recurrent\(.*?\)\s*\{(.*?)\n\}", arch_source, re.S)
    if block:
        names = re.findall(r"LLM_ARCH_([A-Z0-9_]+)", block.group(1))
        recurrent = sorted({by_enum[n] for n in names if n in by_enum})

    return sorted(swa), recurrent, sorted(hybrid)


def render_table(name: str, doc: list[str], values: list[str]) -> str:
    lines = [*doc, f"pub static {name}: &[&str] = &["]
    lines.extend(f'    "{value}",' for value in values)
    lines.append("];")
    return "\n".join(lines)


ARCH_DOC = [
    "/// `general.architecture` values the engine loads for text generation.",
    "///",
    "/// Sourced from `LLM_ARCH_NAMES` in `src/llama-arch.cpp`, less the entries that",
    "/// [`NON_GENERATIVE_ARCHITECTURES`] claims.",
]

NON_GEN_DOC = [
    "/// Architectures the engine knows but which do not generate text.",
    "///",
    "/// A repository of these is not a lagging table entry but a category error: an",
    "/// embedding or audio model offered as a chat engine. This list therefore",
    "/// blocks, where an unknown name only cautions.",
]

PRE_DOC = [
    "/// `tokenizer.ggml.pre` values this build resolves.",
    "///",
    "/// Sourced from the `tokenizer_pre` comparison chain in",
    "/// `src/llama-vocab.cpp`. A GGUF whose pre-tokenizer is missing from the",
    "/// engine's chain aborts the load outright, with `unknown pre-tokenizer type`,",
    "/// after the download has already been paid for. That failure is the reason",
    "/// the probe reads this field before the download rather than after.",
]

RECURRENT_DOC = [
    "/// Architectures whose state does not grow with the context at all.",
    "///",
    "/// Sourced from `llm_arch_is_recurrent`. Sizing one of these with the",
    "/// attention formula would predict gigabytes where the real cost is",
    "/// megabytes, and flat in the context length.",
]

ATTENTION_PREAMBLE = """/// One architecture's sliding-window layout.
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
"""

HYBRID_PREAMBLE = """/// One architecture's hybrid attention layout.
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
"""


def build_block(
    architectures: list[str],
    pre_tokenizers: list[str],
    swa: list[tuple],
    recurrent: list[str],
    hybrid: list[tuple],
) -> str:
    generative = [a for a in architectures if a not in NON_GENERATIVE]

    # Emitted in the shape `cargo fmt` produces, so that generating,
    # formatting and then checking is a fixed point. A one-line struct
    # literal reads fine here but rustfmt expands it, and the guard would
    # then report drift on every commit that ran the formatter.
    swa_rows = "\n".join(
        "    SlidingWindow {\n"
        f'        architecture: "{a}",\n'
        f"        n_pattern: {p},\n"
        f"        dense_first: {str(d).lower()},\n"
        f"        fixed_window: {f},\n"
        "    },"
        for a, p, d, f in swa
    )
    hybrid_rows = "\n".join(
        "    HybridAttention {\n"
        f'        architecture: "{a}",\n'
        f"        full_attention_interval: {n},\n"
        "    },"
        for a, n in hybrid
    )

    attention = "\n".join(
        [
            ATTENTION_PREAMBLE + swa_rows,
            "];",
            "",
            HYBRID_PREAMBLE + hybrid_rows,
            "];",
            "",
            render_table("RECURRENT_ARCHITECTURES", RECURRENT_DOC, recurrent),
        ]
    )

    parts = [
        BEGIN_ARCH + RULE * 24,
        "",
        render_table("GENERATIVE_ARCHITECTURES", ARCH_DOC, generative),
        "",
        render_table(
            "NON_GENERATIVE_ARCHITECTURES", NON_GEN_DOC, sorted(NON_GENERATIVE)
        ),
        "",
        BEGIN_PRE + RULE * 27,
        "",
        render_table("KNOWN_PRE_TOKENIZERS", PRE_DOC, pre_tokenizers),
        "",
        BEGIN_ATTN + RULE * 33,
        "",
        attention,
        "",
        END + RULE * 34,
    ]
    return "\n".join(parts)


def current_tables(text: str) -> dict:
    """Read the tables back out of the checked-in file.

    Compared against the freshly derived data instead of comparing the rendered
    text, because `cargo fmt` reflows the generated block: it collapses a short
    list onto one line and expands a struct literal across several. Matching on
    the text would report drift after every run of the formatter, and a guard
    that cries wolf is one people start passing with `--no-verify`.
    """

    # Only the generated region counts. The hand-written tests below it build
    # SlidingWindow literals of their own, and counting those as table entries
    # made the guard report three architectures upstream has never heard of.
    start = text.find(BEGIN_ARCH)
    end = text.find(END)
    region = text[start:end] if start != -1 and end != -1 else text

    def string_table(name: str) -> list[str]:
        block = re.search(name + r":\s*&\[&str\]\s*=\s*&\[(.*?)\n?\];", text, re.S)
        return sorted(re.findall(r'"([^"]+)"', block.group(1))) if block else []

    swa = sorted(
        (a, int(n), d == "true", int(f))
        for a, n, d, f in re.findall(
            r'SlidingWindow\s*\{\s*architecture:\s*"([^"]+)",\s*'
            r"n_pattern:\s*(\d+),\s*dense_first:\s*(true|false),\s*"
            r"fixed_window:\s*(\d+),?\s*\}",
            region,
            re.S,
        )
    )
    hybrid = sorted(
        (a, int(n))
        for a, n in re.findall(
            r'HybridAttention\s*\{\s*architecture:\s*"([^"]+)",\s*'
            r"full_attention_interval:\s*(\d+),?\s*\}",
            region,
            re.S,
        )
    )
    tag = re.search(r'LLAMA_CPP_TAG: &str = "([^"]*)";', text)

    return {
        "tag": tag.group(1) if tag else "",
        "generative": string_table("GENERATIVE_ARCHITECTURES"),
        "non_generative": string_table("NON_GENERATIVE_ARCHITECTURES"),
        "pre_tokenizers": string_table("KNOWN_PRE_TOKENIZERS"),
        "recurrent": string_table("RECURRENT_ARCHITECTURES"),
        "swa": swa,
        "hybrid": hybrid,
    }


def derived_tables(tag, architectures, pre_tokenizers, swa, recurrent, hybrid) -> dict:
    """The same shape, from what was just read out of the llama.cpp sources."""
    return {
        "tag": tag,
        "generative": sorted(a for a in architectures if a not in NON_GENERATIVE),
        "non_generative": sorted(NON_GENERATIVE),
        "pre_tokenizers": sorted(pre_tokenizers),
        "recurrent": sorted(recurrent),
        "swa": sorted((a, p, d, f) for a, p, d, f in swa),
        "hybrid": sorted((a, n) for a, n in hybrid),
    }


def splice(current: str, tag: str, block: str) -> str:
    """Replace the generated region and the recorded tag, leave the rest."""
    start = current.find(BEGIN_ARCH)
    end_marker = current.find(END)
    if start == -1 or end_marker == -1:
        raise SystemExit(f"generated markers not found in {TARGET}")
    end = current.find("\n", end_marker)
    end = len(current) if end == -1 else end

    updated = current[:start] + block + current[end:]
    return re.sub(
        r'pub const LLAMA_CPP_TAG: &str = "[^"]*";',
        f'pub const LLAMA_CPP_TAG: &str = "{tag}";',
        updated,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of writing when the tables are out of date",
    )
    args = parser.parse_args()

    tag = read_pinned_tag()
    try:
        arch_source = fetch(tag, ARCH_SOURCE)
        architectures = extract_architectures(arch_source)
        pre_tokenizers = extract_pre_tokenizers(fetch(tag, VOCAB_SOURCE))
        swa, recurrent, hybrid = extract_attention(tag, arch_source)
    except SystemExit as exc:
        if args.check and "could not fetch" in str(exc):
            print(
                "gen_llama_support: upstream sources unreachable and not cached, "
                "skipping the check (CI runs it with the network)"
            )
            return 0
        raise

    current = TARGET.read_text(encoding="utf-8")
    want = derived_tables(tag, architectures, pre_tokenizers, swa, recurrent, hybrid)

    if current_tables(current) == want:
        print(f"gen_llama_support: tables are in step with llama.cpp {tag}")
        return 0

    block = build_block(architectures, pre_tokenizers, swa, recurrent, hybrid)
    updated = splice(current, tag, block)

    if args.check:
        fail(
            f"the support tables do not match llama.cpp {tag}.\n"
            f"  Run: python3 scripts/gen_llama_support.py\n"
            f"  Then review the diff: a model that stops being offered, or starts,\n"
            f"  is the point of the change and belongs in the commit message."
        )
        return 1

    TARGET.write_text(updated, encoding="utf-8")
    print(
        f"gen_llama_support: rewrote {TARGET.relative_to(REPO_ROOT)} "
        f"for llama.cpp {tag} ({len(architectures)} architectures, "
        f"{len(pre_tokenizers)} pre-tokenizers, {len(swa)} sliding-window, "
        f"{len(hybrid)} hybrid, {len(recurrent)} recurrent)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
