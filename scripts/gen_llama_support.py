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

Three modes:

    python3 scripts/gen_llama_support.py            rewrite the generated block
    python3 scripts/gen_llama_support.py --check    fail if it is out of date
    python3 scripts/gen_llama_support.py --selftest run its own cases, offline

`--check` needs the network only when the cache under `.cache/llama-support/`
has no copy of the pinned tag. It runs in two places:

- the pre-commit hook, plain `--check`: when upstream is unreachable and
  uncached it says so and exits 0, so an offline commit is not blocked;
- the `deny` job of `.github/workflows/ci.yml`, as
  `--check --require-upstream`, on a runner with the network and no cache.
  There, the same unreachable upstream exits 2, because a check that read
  nothing has measured nothing, and that is never a pass. This run is where
  the guard bites.

Requests to the GitHub API carry `Authorization` when `GH_TOKEN` is set, so the
CI run is not throttled by the anonymous rate limit; without it they go out
anonymous, as before.

Exit codes: 0 in step, 1 out of step, 2 on a usage error or, under
`--require-upstream`, when nothing could be read.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile
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
    # A scoring head over a Qwen3 trunk: one score per token, no text out.
    "vllm-decision2",
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


def request_for(url: str) -> urllib.request.Request:
    """A request to upstream, authenticated to the GitHub API when it can be.

    The CI runner shares its address with every other job on the pool, so the
    anonymous API limit is spent before this script runs often enough to turn
    the check red for a reason that has nothing to do with the tables.
    """
    headers = {"User-Agent": "apollia-gen"}
    token = os.environ.get("GH_TOKEN", "")
    if token and url.startswith(API_BASE + "/"):
        headers["Authorization"] = f"Bearer {token}"
    return urllib.request.Request(url, headers=headers)


def fetch(tag: str, path: str) -> str:
    """Read an upstream source file, from the cache when it is already there."""
    cached = CACHE_DIR / tag / path.replace("/", "_")
    if cached.exists():
        return cached.read_text(encoding="utf-8", errors="replace")

    url = f"{RAW_BASE}/{tag}/{path}"
    try:
        with urllib.request.urlopen(request_for(url), timeout=60) as response:  # noqa: S310
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
        with urllib.request.urlopen(request_for(url), timeout=60) as response:  # noqa: S310
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


def attention_of(source: str):
    """Read one loader file's attention shape.

    Returns `(window, interval)`: `window` is `(n_pattern, dense_first,
    fixed_n_swa)` or `None`, `interval` is the hybrid full-attention interval or
    `None`.

    The whole file is read. An earlier version kept only the first 6000
    characters, on the assumption that the hyperparameter loader comes first;
    `dflash.cpp` sets its sliding window past that point and silently dropped
    out of the table.
    """
    window = None
    kinds = set(re.findall(r"swa_type\s*=\s*LLAMA_SWA_TYPE_(\w+)", source))
    kinds.discard("NONE")
    if kinds:
        pattern = None
        dense_first = False
        # Upstream sets the pattern either directly (`set_swa_pattern(6)`)
        # or through `load_swa_pattern(ml, 6)`, which takes the same value
        # as a default the file may override with its own
        # `attention.sliding_window_pattern`.
        call = r"(?:set_swa_pattern\(|load_swa_pattern\(\s*ml\s*,)"
        dense = r"(?:,\s*(?:/\*.*?\*/\s*)?(true))?\s*\)"
        literal = re.search(call + r"\s*(\d+)\s*" + dense, source)
        via_var = re.search(call + r"\s*(\w+)\s*" + dense, source)
        if literal:
            pattern, dense_first = int(literal.group(1)), bool(literal.group(2))
        elif via_var:
            default = re.search(via_var.group(1) + r"\s*=\s*(\d+)", source)
            if default:
                pattern, dense_first = int(default.group(1)), bool(via_var.group(2))
        fixed = re.search(r"hparams\.n_swa\s*=\s*(\d+)", source)
        if pattern is not None:
            window = (pattern, dense_first, int(fixed.group(1)) if fixed else 0)

    interval = None
    found = re.search(r"full_attn_interval\s*=\s*(\d+)", source)
    if found and "is_recr_impl" in source:
        interval = int(found.group(1))
    return window, interval


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
        window, interval = attention_of(fetch(tag, f"{MODELS_DIR}/{name}"))
        arch = name[:-4]
        if window is not None:
            swa.append((arch, *window))
        if interval is not None:
            hybrid.append((arch, interval))

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


# ── The check's own exit codes, driven against a simulated upstream ───────────

FAKE_TAG = "b0"
FAKE_ARCH = (
    "static const std::map<llm_arch, const char *> LLM_ARCH_NAMES = {\n"
    '    { LLM_ARCH_QWEN3, "qwen3" },\n'
    '    { LLM_ARCH_MAMBA, "mamba" },\n'
    "};\n"
    "bool llm_arch_is_recurrent(const llm_arch & arch) {\n"
    "    switch (arch) { case LLM_ARCH_MAMBA: return true; }\n"
    "}\n"
)
FAKE_VOCAB = 'if (tokenizer_pre == "qwen2") {}\n'
FAKE_MODEL = (
    "hparams.swa_type = LLAMA_SWA_TYPE_STANDARD;\nhparams.set_swa_pattern(4);\n"
)
FAKE_TARGET = (
    "//! fixture\n"
    'pub const LLAMA_CPP_TAG: &str = "";\n\n'
    + BEGIN_ARCH
    + "\n"
    + END
    + "\n"
)


class _FakeResponse:
    def __init__(self, body: str) -> None:
        self._body = body.encode("utf-8")

    def __enter__(self) -> "_FakeResponse":
        return self

    def __exit__(self, *exc: object) -> None:
        return None

    def read(self) -> bytes:
        return self._body


def selftest_modes() -> list[str]:
    """Hook and CI modes, stale and fresh tables, and the API token, offline.

    Upstream is a dictionary, the cache and the tables live in a temporary
    directory, and the module's paths are swapped for the duration, so nothing
    under the repository is read or written.
    """
    global CACHE_DIR, TARGET, PIN_FILE
    failures: list[str] = []
    saved = (CACHE_DIR, TARGET, PIN_FILE, urllib.request.urlopen)
    saved_token = os.environ.pop("GH_TOKEN", None)
    upstream = {
        f"{RAW_BASE}/{FAKE_TAG}/{ARCH_SOURCE}": FAKE_ARCH,
        f"{RAW_BASE}/{FAKE_TAG}/{VOCAB_SOURCE}": FAKE_VOCAB,
        f"{RAW_BASE}/{FAKE_TAG}/{MODELS_DIR}/gemma3.cpp": FAKE_MODEL,
        f"{API_BASE}/contents/{MODELS_DIR}?ref={FAKE_TAG}": json.dumps(
            [{"name": "gemma3.cpp"}, {"name": "README.md"}]
        ),
    }
    online = {"up": False}
    seen: list[urllib.request.Request] = []

    def fake_urlopen(request, timeout=None):  # noqa: ARG001
        seen.append(request)
        url = request.full_url if hasattr(request, "full_url") else str(request)
        if not online["up"] or url not in upstream:
            raise urllib.error.URLError("simulated: no network")
        return _FakeResponse(upstream[url])

    def quiet(check: bool, require: bool) -> int:
        """`run`, its output kept off the selftest's own report."""
        stdout, stderr = sys.stdout, sys.stderr
        sink = open(os.devnull, "w", encoding="utf-8")  # noqa: SIM115
        sys.stdout = sys.stderr = sink
        try:
            return run(check, require)
        except SystemExit as exc:
            return 1 if exc.code is None or isinstance(exc.code, str) else exc.code
        finally:
            sys.stdout, sys.stderr = stdout, stderr
            sink.close()

    try:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            CACHE_DIR = root / "cache"
            TARGET = root / "llama_support.rs"
            PIN_FILE = root / "fetch-llama-server.sh"
            PIN_FILE.write_text(
                f'LLAMA_CPP_TAG="${{LLAMA_CPP_TAG:-{FAKE_TAG}}}"\n', encoding="utf-8"
            )
            TARGET.write_text(FAKE_TARGET, encoding="utf-8")
            urllib.request.urlopen = fake_urlopen

            # GIVEN upstream unreachable and an empty cache
            # WHEN the hook runs the check
            code = quiet(check=True, require=False)
            # THEN an offline commit is not blocked
            if code != 0:
                failures.append(f"hook mode offline and uncached exited {code}, not 0")

            # GIVEN the same unreachable upstream and empty cache
            # WHEN CI runs the check with --require-upstream
            code = quiet(check=True, require=True)
            # THEN nothing measured is a 2, never a pass
            if code != 2:
                failures.append(f"CI mode offline and uncached exited {code}, not 2")

            # GIVEN upstream reachable and checked-in tables that lag it
            online["up"] = True
            # WHEN CI runs the check
            code = quiet(check=True, require=True)
            # THEN the lag is a 1, and the tables are left untouched
            if code != 1:
                failures.append(f"stale tables exited {code}, not 1")
            if TARGET.read_text(encoding="utf-8") != FAKE_TARGET:
                failures.append("--check wrote the tables it was only asked to compare")

            # GIVEN the tables rewritten from that same upstream
            quiet(check=False, require=False)
            # WHEN CI runs the check again
            code = quiet(check=True, require=True)
            # THEN tables in step are a 0
            if code != 0:
                failures.append(f"tables in step exited {code}, not 0")

            # GIVEN a token in GH_TOKEN and nothing cached
            shutil.rmtree(CACHE_DIR)
            seen.clear()
            os.environ["GH_TOKEN"] = "selftest-token"
            # WHEN the check reads upstream
            quiet(check=True, require=True)
            # THEN the API request carries it, and the raw downloads do not
            api = [r for r in seen if r.full_url.startswith(API_BASE)]
            raw = [r for r in seen if r.full_url.startswith(RAW_BASE)]
            if not api or any(
                r.get_header("Authorization") != "Bearer selftest-token" for r in api
            ):
                failures.append("the GitHub API request did not carry GH_TOKEN")
            if any(r.get_header("Authorization") for r in raw):
                failures.append("a raw download carried the API token")

            # GIVEN no token
            shutil.rmtree(CACHE_DIR)
            seen.clear()
            del os.environ["GH_TOKEN"]
            # WHEN the check reads upstream
            quiet(check=True, require=True)
            # THEN every request goes out anonymous, as before
            if not seen or any(r.get_header("Authorization") for r in seen):
                failures.append("a request carried Authorization without GH_TOKEN")
    finally:
        CACHE_DIR, TARGET, PIN_FILE, urllib.request.urlopen = saved
        os.environ.pop("GH_TOKEN", None)
        if saved_token is not None:
            os.environ["GH_TOKEN"] = saved_token
    return failures


def selftest() -> int:
    """Pin the two ways the generator has drifted from the sources it reads."""
    failures: list[str] = []

    # GIVEN a loader whose sliding window is set past its first 6000 characters,
    # the way `dflash.cpp` sets it at b12064
    late = (
        "// " + "x" * 7000 + "\n"
        "hparams.swa_type = LLAMA_SWA_TYPE_STANDARD;\n"
        "hparams.set_swa_pattern(0);\n"
    )
    # WHEN its attention shape is read
    window, _ = attention_of(late)
    # THEN the window is found
    if window != (0, False, 0):
        failures.append(f"a window set after 6000 characters was read as {window}")

    # GIVEN a hybrid loader whose interval sits far into the file, like `qwen4exp.cpp`
    hybrid = "// " + "x" * 8000 + "\nuint32_t full_attn_interval = 4;\nis_recr_impl\n"
    # WHEN its attention shape is read
    _, interval = attention_of(hybrid)
    # THEN the interval is found
    if interval != 4:
        failures.append(f"a hybrid interval set late was read as {interval}")

    # GIVEN the architecture list of a build that knows `vllm-decision2`
    architectures = ["qwen3", "vllm-decision2"]
    # WHEN the generative table is derived
    tables = derived_tables("tag", architectures, [], [], [], [])
    # THEN the scoring head is not offered as a chat engine
    if tables["generative"] != ["qwen3"]:
        failures.append(f"generative table was {tables['generative']}")

    failures.extend(selftest_modes())

    for failure in failures:
        fail(f"selftest: {failure}")
    if failures:
        return 1
    print("gen_llama_support: selftest holds")
    return 0


def _shown(path: Path) -> Path:
    """A path relative to the repository when it sits inside it."""
    try:
        return path.relative_to(REPO_ROOT)
    except ValueError:
        return path


def run(check: bool, require_upstream: bool) -> int:
    """Derive the tables and compare, or rewrite; the exit code of the script."""
    tag = read_pinned_tag()
    try:
        arch_source = fetch(tag, ARCH_SOURCE)
        architectures = extract_architectures(arch_source)
        pre_tokenizers = extract_pre_tokenizers(fetch(tag, VOCAB_SOURCE))
        swa, recurrent, hybrid = extract_attention(tag, arch_source)
    except SystemExit as exc:
        if check and "could not fetch" in str(exc):
            if require_upstream:
                fail(
                    f"{exc}\n  upstream sources unreachable and not cached under "
                    f"{CACHE_DIR}: nothing was measured, which is not a pass"
                )
                return 2
            print(
                "gen_llama_support: upstream sources unreachable and not cached, "
                "skipping the check; the deny job of ci.yml runs it with "
                "--require-upstream, where this case is a red"
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

    if check:
        fail(
            f"the support tables do not match llama.cpp {tag}.\n"
            f"  Run: python3 scripts/gen_llama_support.py\n"
            f"  Then review the diff: a model that stops being offered, or starts,\n"
            f"  is the point of the change and belongs in the commit message."
        )
        return 1

    TARGET.write_text(updated, encoding="utf-8")
    print(
        f"gen_llama_support: rewrote {_shown(TARGET)} "
        f"for llama.cpp {tag} ({len(architectures)} architectures, "
        f"{len(pre_tokenizers)} pre-tokenizers, {len(swa)} sliding-window, "
        f"{len(hybrid)} hybrid, {len(recurrent)} recurrent)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of writing when the tables are out of date",
    )
    parser.add_argument(
        "--require-upstream",
        action="store_true",
        help="with --check: exit 2 instead of 0 when upstream is unreachable "
        "and uncached (the CI mode; the hook leaves it off)",
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="run the generator's own cases against synthetic sources",
    )
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    if args.require_upstream and not args.check:
        parser.error("--require-upstream only applies to --check")
    return run(args.check, args.require_upstream)


if __name__ == "__main__":
    sys.exit(main())
