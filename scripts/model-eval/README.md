# Model evaluation harness

Probes that measure a local model served by `llama-server`: speed, tool calling,
extraction quality, and the decomposition of a full agentic turn.

The first half of this file is the **measurement contract**. It fixes the
vocabulary, the units, and the record shape that every probe, analysis script,
and report in this directory obeys. It is normative. The second half describes
the scripts that implement it.

---

# Part 1: the measurement contract

## 1.1 Why this exists

The failure mode being guarded against is not a bug. It is vocabulary drift: two
producers naming the same quantity differently, or naming two different
quantities the same. Drift is invisible in every individual diff, passes every
test, and invalidates the aggregate. So the vocabulary is fixed here, before the
code that uses it exists.

No artefact invents a field name. If a measurement needs a quantity this
document does not define, the document is amended first, and the amendment is
what the new code reads.

## 1.2 What this contract was built from

- The engine's own reporting, read from the llama.cpp source rather than from
  documentation: `result_timings::to_json` in `tools/server/server-task.cpp`, and
  the emission conditions in `tools/server/server-context.cpp`.
- The launch configuration fields and `build_args` signature introduced by the
  pilotable-parameters work in `crates/apollia-runtime/src/llama_server/config.rs`.
- Section 4 of `docs/agents/OBSERVABILITY.md`, the existing tracing field table.
  Conflicts with it are resolved explicitly in 1.6, not silently.
- A live `llama-server` run on the target machine, used to fill a record by hand
  and confirm every field is obtainable. Reported in 1.11.

No roofline or harness-repair drafts existed when this was written, so there
were no prior implicit definitions to adopt or override. Those operations
inherit this document as given.

## 1.3 Notation and the three standing rules

Every dictionary row is marked in its "How obtained" column with exactly one of:

- **Observed.** Read directly from a response, an endpoint, or a clock. The
  source is named.
- **Derived.** Computed from other fields. The formula is given in full. A
  derived field is always recomputable from the record itself.
- **Approximate.** Carries error that is not measurement noise. The nature of
  the approximation is stated. There are no silent estimates in this schema.

**Rule A, authoritative source.** Where a quantity is obtainable two ways, one
source is named authoritative and the other is kept as a separate cross-check
field. They are never merged, never averaged, and never used interchangeably.
Token counts come from the engine's counters, not from counting streamed events;
the event count survives under its own name so the discrepancy stays visible.

**Rule B, units in names.** Field names are `snake_case` and carry their unit as
a suffix: `_ms`, `_s`, `_tps`, `_tok`, `_chars`, `_bytes`, `_ratio` (0.0 to
1.0), `_pct` (0.0 to 100.0), `_at` (RFC 3339 UTC). A name without a unit suffix
is a count of things, never a measured quantity.

**Rule C, absence is not zero.** A quantity that could not be determined is
`null`. Never `0`, never `-1`, never omitted. Nothing is rounded before storage.

## 1.4 The measurement dictionary

### 1.4.1 Prompt token accounting

The most dangerous ambiguity in this program. `llama-server` reports `prompt_n`
as the tokens **it actually evaluated**, excluding those served from cache. A
reader who takes `prompt_n` as "the size of the prompt" computes a prefill rate
that is right cold and badly wrong warm.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `prompt_tok_total` | tokens | Every token submitted with the request, cached or recomputed. The prompt size a user would mean. | **Derived.** `timings.prompt_n + timings.cache_n`. Cross-check: `usage.prompt_tokens`. | Not `timings.prompt_n`. Not the number of tokens the engine did work on. Not the context occupancy. |
| `prompt_tok_computed` | tokens | Prompt tokens actually run through the model on this request. The numerator of prefill work. | **Observed.** `timings.prompt_n` verbatim. | Not the prompt size. On a warm request it can be 1 while the prompt is 4032 tokens. Never the denominator of a cache hit ratio. |
| `prompt_tok_cached` | tokens | Prompt tokens served from the KV cache with no recomputation. | **Observed.** `timings.cache_n` verbatim. Cross-check: `usage.prompt_tokens_details.cached_tokens`. | Not a measure of cache capacity. Not tokens saved in wall-clock terms, since a cache hit still costs a lookup. |
| `prompt_cache_hit_ratio` | ratio | Share of the submitted prompt that avoided recomputation. | **Derived.** `prompt_tok_cached / prompt_tok_total`. | Not a hit rate over requests. Not `cache_reuse`, which is a launch flag naming a chunk-size threshold. |

### 1.4.2 Generated token accounting

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `decode_tok` | tokens | Tokens generated by the model, reasoning and content together. | **Observed.** `timings.predicted_n` verbatim. Cross-check: `usage.completion_tokens`. | Not the number of streamed events. Not content tokens only. Not capped at `max_tokens` unless generation actually reached it. |
| `sse_chunks` | count | Streamed events carrying a non-empty content delta. A cross-check, never a token count. | **Observed.** Counted client-side while streaming. | **Not a token count.** Measured on this machine at 33 against `decode_tok` 34: tokens that carry no content delta, such as the stop token, are invisible to it. |
| `token_count_discrepancy_ratio` | ratio | Distance between the event count and the authoritative token count. | **Derived.** `abs(sse_chunks - decode_tok) / decode_tok`. | Not a quality gate on the run. It is systematically non-zero and exists to keep the gap visible, so nobody substitutes one source for the other. |
| `decode_tok_reasoning` | tokens | Of `decode_tok`, those inside `<think>` blocks. | **Observed** when `reasoning_split_method` is `"tokenized"`. **Approximate** when it is `"chars_apportioned"`: `decode_tok * reasoning_chars / (reasoning_chars + content_chars)`, which assumes a uniform characters-per-token ratio across the two regions and is wrong to the extent that reasoning and prose tokenize differently. | Not available from the engine. Apollia launches with `--reasoning-format none`, so thoughts arrive inline and the split is a client-side parse, never an engine figure. |
| `decode_tok_content` | tokens | Of `decode_tok`, those outside `<think>` blocks. | **Observed** or **Approximate** by the same method as `decode_tok_reasoning`, of which it is the complement: `decode_tok - decode_tok_reasoning`. | Not the visible answer length in characters, which is `content_chars`. Not `decode_tok` when the model emitted no think block: in that case the split method is `"none"` and the two are equal by definition, not by measurement. |
| `reasoning_chars` | characters | Characters inside `<think>` blocks, tags excluded. | **Observed.** Client-side parse of the content stream. | Not tokens. Exact, unlike the token split derived from it. |
| `content_chars` | characters | Characters outside `<think>` blocks. | **Observed.** Same parse. | Same. |
| `reasoning_split_method` | enum | Which of the three above applies: `"tokenized"`, `"chars_apportioned"`, `"none"`. | **Observed.** Set by the producer. | Not optional. A consumer that needs an exact split checks this before trusting `decode_tok_reasoning`. |

### 1.4.3 Durations

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `ttft_ms` | ms | Time to first token. From the instant the request body is fully written to the instant the first non-empty content delta arrives. Client-observed. | **Observed.** Monotonic clock around a **streamed** request, with its origin taken at dispatch. A client that awaits the first chunk before yielding its stream has already spent the prefill, so an origin taken where consumption starts measures the wrong interval; I13 fires when it does. | Not `prompt_ms`. It includes transport, queueing, slot assignment, prefill and the first sampling step. **Undefined for a non-streaming request**, where it is `null`. |
| `prefill_ms` | ms | Engine-internal prefill duration, covering `prompt_tok_computed` only. | **Observed.** `timings.prompt_ms` verbatim. | Not time to first token. Not proportional to prompt size on a warm request, where it is dominated by fixed cost. |
| `decode_ms` | ms | Engine-internal generation duration for `decode_tok`. | **Observed.** `timings.predicted_ms` verbatim. | Not `request_wall_ms - ttft_ms`, which additionally carries transport and client parsing. |
| `ttft_overhead_ms` | ms | Everything between the client and the engine's own prefill accounting. The share of first-token latency this project owns rather than the engine. | **Derived.** `ttft_ms - prefill_ms`. | Not network latency alone. Not an error term. Measured at 7.65 ms on loopback with a warm slot. |
| `request_wall_ms` | ms | One HTTP request end to end, first byte written to last byte read. | **Observed.** Monotonic clock around the request. | Not a turn. One agentic turn contains several of these. |

### 1.4.4 Cache state, and what makes a measurement cold

Cold and warm are **observed properties of the request**, asserted from the
response. A probe does not get to call a measurement cold because it intended it
to be. This is the difference between a rule and a habit.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `cache_state` | enum | `"cold"` when `prompt_tok_cached == 0`, `"warm"` otherwise. | **Derived** from `timings.cache_n`, after the request completes. | Not a property of the probe's intent, the run order, or whether the server was restarted. A freshly started server still serves a warm request if the prompt shares a prefix with an earlier one. |
| `ttft_cold_ms` | ms | `ttft_ms` of a request whose `cache_state` is `"cold"`. | **Derived.** `ttft_ms`, admissible only when `cache_state == "cold"`. | Not the first run of a loop. Not a warm figure with a cold label. |
| `ttft_warm_ms` | ms | `ttft_ms` of a request whose `cache_state` is `"warm"`, always reported with the `prompt_cache_hit_ratio` that produced it. | **Derived.** `ttft_ms`, admissible only when `cache_state == "warm"`. | Not meaningful alone. The same number can come from a 13 percent or a 99.98 percent hit, which are different measurements. |

Warming a slot with the prompt that will then be measured produces a warm
request. That is a valid measurement of the warm path and an invalid one of
prefill. To measure cold: warm with a **different** prompt, or reset the slot,
then assert `cache_state` on the record rather than trusting the procedure.

### 1.4.5 Rates

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `prefill_tps` | tok/s | Rate at which the engine consumes **new** prompt tokens. | **Derived.** `prompt_tok_computed / (prefill_ms / 1000)`. `null` when `prompt_tok_computed == 0`. Cross-check: `engine_prefill_tps`. | Not comparable between cold and warm requests. Measured cold at 906 tok/s and warm at 63 tok/s on the same model, because with 1 token computed the figure is fixed overhead, not throughput. A warm `prefill_tps` is not a prefill rate. |
| `decode_tps` | tok/s | Generation rate, excluding prefill. | **Derived.** `decode_tok / (decode_ms / 1000)`. Cross-check: `engine_decode_tps`. | Not the user-visible rate. Not tokens divided by total wall-clock. |
| `decode_tps_wall` | tok/s | User-visible generation rate after the first token. | **Derived.** `decode_tok / ((request_wall_ms - ttft_ms) / 1000)`. | Not the engine's rate. Always lower than `decode_tps`. |
| `engine_prefill_tps` | tok/s | The engine's own prefill figure, kept for cross-check. | **Observed.** `timings.prompt_per_second` verbatim. | Not the authoritative field. Present so a divergence from `prefill_tps` is visible rather than absorbed. |
| `engine_decode_tps` | tok/s | The engine's own decode figure, kept for cross-check. | **Observed.** `timings.predicted_per_second` verbatim. | Same. |
| `aggregate_decode_tps_wall` | tok/s | Tokens generated by every slot in one concurrent round, over that round's wall-clock. The server-wide rate. | **Derived.** `sum(decode_tok) / (round_wall_ms / 1000)` across the round's requests. One value per round, so its aggregate has `n == rounds`. | Not a per-slot rate, and not comparable to `decode_tps`: the denominator is wall-clock and includes prefill, which is why it carries the `_wall` suffix I5 requires. Not inflated by a slot that finished early, since the denominator is the whole round. |

A correctly separated measurement shows `prefill_tps` and `decode_tps` differing
by roughly an order of magnitude on a GPU-resident model: 906 against 66 on the
run in 1.11. If they are close, the separation is broken and the record is not
usable.

### 1.4.6 Context

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `n_ctx_slot_tok` | tokens | Context window available to **one slot**, which is what a single conversation gets. | **Observed.** `GET /props`, `default_generation_settings.n_ctx`, the engine's per-slot figure after any capping to the model's training context. The same response carries `total_slots`, so slot count is verified rather than assumed. | Not the `-c` launch value when `-np > 1`. Not the model's trained context length, which can be far larger. |
| `context_occupancy_at_call_ratio` | ratio | How full the window was when the request was issued, before generation. The quantity that governs whether the next iteration will fit. | **Derived.** `prompt_tok_total / n_ctx_slot_tok`. | Not measured after generation. Not against `-c`. |
| `context_occupancy_ratio` | ratio | How full the window was when the request completed. | **Derived.** `(prompt_tok_total + decode_tok) / n_ctx_slot_tok`. | Not the same as the at-call figure, and the two are not interchangeable in a compaction decision. |
| `kv_cache_bytes` | bytes | Memory the KV cache occupies for the whole slot allocation at `n_ctx_slot_tok`. | **Observed** at raised verbosity: `llama_kv_cache: <dev> KV buffer size` in the server log, which requires `-v` and is absent at default verbosity. **Derived** otherwise: `cells * n_layer_attn * n_head_kv * (key_length + value_length) * bytes_per_element`, with GGUF header values and the element size of `cache_type_k` / `cache_type_v`. `cells` is `n_ctx` padded up to a multiple of 256. `n_layer_attn` is the number of blocks that actually hold a KV cache, which is **not** `block_count` on a hybrid architecture. Validated exactly on four model and context pairs in 1.11. | Not the memory the model occupies. Not proportional to the prompt: it is allocated for the full window regardless of how much is used. Not halved by a cache quantisation without also changing `bytes_per_element` in the formula. Not scaled by `-np`: the cache is unified across slots. |

### 1.4.7 The agentic turn

Produced by the runtime, not by a probe hitting `llama-server` directly. One
user-visible turn contains one or more engine completions and zero or more tool
invocations.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `turn_wall_ms` | ms | From accepting the user message to emitting the final assistant message. What the user experiences as "one turn". | **Observed.** Monotonic clock in the runtime. | Not `request_wall_ms`. Not the sum of the completions: a turn contains tool time and orchestration too. |
| `iterations` | count | Completions issued to the engine within the turn. | **Observed.** Counted in the loop. | Not the number of tool calls. Not the number of streamed messages the UI rendered. |
| `tool_calls` | count | Tool invocations executed within the turn. | **Observed.** Counted in the loop. | Not tool calls the model requested but that were denied or never dispatched. Those are counted separately or not at all. |
| `tool_ms` | ms | Wall-clock of one tool invocation, dispatch to result available, including any approval wait. | **Observed.** Monotonic clock around the invocation. | Not the tool's own compute time when a human was in the loop. See `tool_approval_ms`. |
| `tool_approval_ms` | ms | Of `tool_ms`, the part spent blocked on a human decision. `null` when no approval was required. | **Observed.** Monotonic clock around the approval wait. | Not a system cost. A turn that waited on a human is not a slow turn, and mixing the two makes the residual meaningless. |
| `tool_ms_total` | ms | Tool time across the turn. | **Derived.** Sum of `tool_ms` over `tool_records`. | Not exclusive of approval waits. Subtract `tool_approval_ms` explicitly if that is what is meant. |
| `approvals` | count | Human approvals the turn waited on, whatever the answer was. | **Observed.** Counted where the wait happens, upstream of the invoker. | Not `tool_calls`. A refused or timed-out approval waits and runs nothing, so the two counts differ precisely on the turns where the wait matters most. |
| `approval_ms` | ms | Wall-clock of one approval wait, from the request being emitted to a decision arriving. | **Observed.** Monotonic clock around the wait. | Not part of `tool_ms`: the wait is upstream of the invoker, not inside it. Not attributable to any tool that ran, because a refusal runs none. |
| `approved` | bool | Whether the decision let the call proceed. | **Observed.** From the decision itself. | Not a distinction between a typed refusal and a timeout. Both arrive identically at the recording site and the contract does not invent the difference. |
| `approval_ms_total` | ms | Human wait across the turn. | **Derived.** Sum of `approval_ms` over `approval_records`. | Not a system cost, and not a component of `tool_ms_total`. |
| `engine_ms_total` | ms | Engine time across the turn. | **Derived.** Sum of `prefill_ms + decode_ms` over `iteration_records`. | Not the sum of `request_wall_ms`, which double-counts transport into the residual. |
| `orchestration_residual_ms` | ms | Turn wall-clock minus every attributed term. Prompt assembly, serialisation, channel hops, persistence, scheduling. The term this project can act on without touching the model or the engine. | **Derived.** `turn_wall_ms - engine_ms_total - tool_ms_total - approval_ms_total`. | Not an error bar. Not necessarily positive: a negative value means something ran concurrently and the additive model does not hold for that turn. Not human wait: finding 3 records what it cost to have left it in. |
| `orchestration_residual_ratio` | ratio | The residual as a share of the turn. | **Derived.** `orchestration_residual_ms / turn_wall_ms`. | Not comparable across turns of very different lengths without also reporting `turn_wall_ms`. |

A negative residual is reported as measured. It is never clamped to zero, and a
record carrying one is excluded from residual aggregates and listed in
`records_excluded`. Clamping would convert a broken model into a plausible
number, which is the exact outcome this contract exists to prevent.

### 1.4.8 Dispersion

A single sample on Apple Silicon is dominated by power state. Every reported
quantity is an aggregate over repetitions, minimum 5.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `n` | count | Repetitions contributing to the aggregate. | **Observed.** | Not the number of tokens or requests, but of complete repetitions of one measurement. |
| `median` | source unit | 50th percentile. | **Derived.** Linear interpolation between order statistics. | Not the mean. Reported instead of the mean because the distribution is right-skewed by scheduling. |
| `p95` | source unit | 95th percentile, nearest-rank: the value at index `ceil(0.95 * n)` of the sorted samples, 1-indexed. | **Derived.** | **At `n = 5` this is the maximum**, not a tail estimate. A `p95` carrying information needs `n >= 20`. |
| `cv` | ratio | Coefficient of variation, `stdev / mean`, sample standard deviation with an `n - 1` denominator. | **Derived.** | Not a confidence interval. Above 0.10 the measurement is unstable and the median is not reported as fact. |
| `samples` | array | Every raw observation, in execution order. | **Observed.** | Not optional. Retained so a consumer can re-estimate with a different estimator without rerunning anything. |

### 1.4.9 Provenance

A number without provenance does not exist. The block is carried **inline in
every record**, not hoisted to file level and referenced, because measurement
records travel alone and must stay interpretable when they do.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `git_sha` | string | Apollia commit the measurement ran against. | **Observed.** `git rev-parse HEAD`. | Not sufficient alone. See `git_dirty`. |
| `git_dirty` | bool | Whether the working tree carried uncommitted changes. | **Observed.** `git status --porcelain` non-empty. | Not a defect. It is common during a measurement campaign, and it means `git_sha` does not fully identify the code. |
| `llama_server_version` | string | Engine build. | **Observed.** First line of `llama-server --version`, verbatim. | Not the release tag in the packaging script. The binary that ran may be a PATH fallback rather than the pinned bundle. |
| `llama_server_path` | string | Binary actually executed. | **Observed.** Resolved path. | Not the configured path. Required because the bundled binary and a system fallback are both reachable. |
| `model_path` | string | GGUF loaded. | **Observed.** Launch argument. | Not a model identity. Two files with the same name can differ. See `model_sha256`. |
| `model_sha256` | string | Content hash of the GGUF, or of the first shard for a split model. | **Observed.** `shasum -a 256`. | Not a hash of the whole model when `model_sha256_scope` is `"first_shard"`. |
| `model_sha256_scope` | enum | `"whole_file"`, `"first_shard"`, `"none"`. | **Observed.** | Not optional. Without it the hash is uninterpretable for split models. |
| `launch_args` | array | The **full** argument vector, as strings. | **Observed.** The `args` field of the `llama.server.spawn.config` tracing event, or the vector the probe passed. | Not a summary and not the flags someone believes are in effect. This is the field that makes two campaigns comparable or not. |
| `machine_id` | string | Hardware model identifier. | **Observed.** `sysctl -n hw.model`. | Not the marketing name. |
| `machine_chip` | string | Processor. | **Observed.** `sysctl -n machdep.cpu.brand_string`. | Not a performance figure. Bandwidth and peak throughput are roofline inputs with cited sources. |
| `machine_memory_bytes` | bytes | Physical memory. | **Observed.** `sysctl -n hw.memsize`. | Not memory available to the model. |
| `os_version` | string | Operating system version. | **Observed.** `sw_vers -productVersion`. | Not the kernel version. |
| `measured_at` | RFC 3339 | When the measurement ran. | **Observed.** | Not when the record was written or aggregated. |

### 1.4.10 Run conditions

Confounds that cannot be eliminated are recorded so they can be reasoned about
afterwards.

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `run_index` | count | Position in the campaign's execution order, 0-based. | **Observed.** | Not the repetition index within one measurement. |
| `run_order` | enum | `"sequential"` or `"randomised"`. | **Observed.** | Not cosmetic. Randomised order is what makes thermal drift appear as noise instead of as a fake effect. |
| `page_cache` | enum | `"cold"`, `"warm"`, `"unknown"`, for the mmap'd model file. | **Observed** when controlled, `"unknown"` otherwise. | Not the KV cache state, which is `cache_state`. Two different caches, and conflating them is a naming trap this column exists to close. |
| `server_restarted_before` | bool | Whether the server was restarted immediately before this run. | **Observed.** | Not a guarantee of a cold KV cache. A restarted server still serves a warm request on a shared prefix. |
| `slot_reset_before` | bool | Whether the slot's KV state was cleared before this run. | **Observed.** | Same. Assert `cache_state`, do not infer it from this. |
| `sampling_seed` | integer | Fixed seed, so token counts are reproducible. | **Observed.** | Not optional for speed work. `-1` disqualifies the record from speed comparison. |
| `sampling_temperature` | float | `0.0` for all speed measurement. | **Observed.** | Not a quality setting here. A non-zero value changes the token count between runs and destroys reproducibility. |
| `notes` | string | Anything uncontrolled and worth knowing. | **Observed.** | Not a substitute for a field. If it recurs, it becomes a field. |

### 1.4.11 Roofline

| Canonical name | Unit | Definition | How obtained | What it is **not** |
|---|---|---|---|---|
| `params_total` | count | Total parameters. | **Observed.** GGUF header. | Not what governs decode speed on a mixture of experts model. |
| `params_active_per_token` | count | Parameters read per generated token: experts used per token times expert size, plus shared parameters. Equals `params_total` for a dense model. | **Derived** from GGUF header fields. | Not `params_total`. For the models in this shortlist the two differ by an order of magnitude, and using the wrong one makes the ceiling meaningless. |
| `bits_per_weight` | bits | Effective bits per weight of the quantisation. | **Derived exactly.** Sum of every tensor's storage size, from the ggml block layout of that tensor's own type, divided by the parameter count. Not inferred from `general.file_type`, which names only the dominant quantisation. | Not the nominal bit width of the quantisation name: a `Q5_K_M` file is 5.70 bits per weight, not 5. |
| `bytes_per_token_read` | bytes | Bytes moved per generated token: active parameter bytes, plus KV traffic at the stated context length, plus `recurrent_state_bytes`. | **Derived.** Formula stated in the record's `assumptions`. | Not the model file size. Not the active parameter bytes alone, which understates the ceiling at long context. |
| `recurrent_state_bytes` | bytes | Recurrent state of a hybrid model, for one sequence. `0` on a pure attention model. | **Derived.** `n_layer_recurrent * ((d_inner + 2 * n_group * d_state) * (d_conv - 1) + d_inner * d_state) * 4`, from the `ssm.*` GGUF keys, f32 throughout. Validated in 1.11. | Not part of `kv_cache_bytes`, whose definition is context-dependent while this is constant. Not excluded from decode traffic: it is read on every token. |
| `bandwidth_bytes_per_s` | bytes/s | Memory bandwidth of the machine. | **Observed** from a cited source, recorded in `sources`. | Not measured by this harness. A vendor figure, and the ceiling inherits its optimism. |
| `peak_flops` | FLOP/s | Peak throughput of the machine. | **Observed** from a cited source. | Same. |
| `decode_ceiling_tps` | tok/s | Memory-bandwidth-bound decode ceiling. | **Derived.** `bandwidth_bytes_per_s / bytes_per_token_read`. | Not achievable in practice. It ignores every inefficiency below the bandwidth limit. |
| `prefill_ceiling_tps` | tok/s | Compute-bound prefill ceiling. | **Derived** from FLOPs per token and `peak_flops`. | Not valid at long contexts: attention cost beyond the linear KV term is not modelled, and the record says so. |
| `decode_efficiency_pct` | percent | Share of the decode ceiling reached. | **Derived.** `100 * decode_tps / decode_ceiling_tps`. | Not a grade. A low value can mean the ceiling is wrong. |
| `prefill_efficiency_pct` | percent | Share of the prefill ceiling reached. | **Derived.** `100 * prefill_tps / prefill_ceiling_tps`. | Same, and invalid if `prefill_tps` came from a warm request. |
| `assumptions` | array | Every assumption behind the figures above. | **Observed.** Written by the producer. | Not optional. A roofline whose assumptions are hidden is worse than no roofline. |

## 1.5 Invariants

Checkable assertions. A producer asserts them before writing a record; a
consumer asserts them before trusting one. A violation is reported in the
record's `invalid` array, never silently repaired.

- **I1.** `prompt_tok_total == prompt_tok_computed + prompt_tok_cached`.
- **I2.** `cache_state == "cold"` if and only if `prompt_tok_cached == 0`.
- **I3.** `ttft_cold_ms` appears only on a record whose `cache_state` is
  `"cold"`; `ttft_warm_ms` only on `"warm"`, and never without
  `prompt_cache_hit_ratio`.
- **I4.** `prefill_tps` is `null` when `prompt_tok_computed == 0`, never `0.0`.
- **I5.** `decode_tps` is computed over `decode_ms`, never over wall-clock. The
  wall-clock figure is `decode_tps_wall`, a different field.
- **I6.** `turn_wall_ms == engine_ms_total + tool_ms_total + orchestration_residual_ms`,
  exactly, by construction. Check tolerance 1 ms.
- **I7.** Every record carries a complete `provenance` block. A field that could
  not be determined is `null` with a reason in `conditions.notes`, never omitted.
- **I8.** `n >= 5` for any aggregate. Below that the aggregate is marked
  provisional and excluded from comparisons.
- **I9.** `sampling_temperature == 0.0` and `sampling_seed != -1` for any record
  used in a speed comparison.
- **I10.** A campaign comparing configurations has `run_order == "randomised"`.
- **I11.** A delta between configurations smaller than the baseline's own `cv`
  is reported as no detectable effect, never as a gain.
- **I12.** `ttft_ms` is `null` on a non-streaming record.
- **I13.** `ttft_ms >= prefill_ms` on any streamed record carrying both, so
  `ttft_overhead_ms` is never negative. A first-token time below the engine's own
  prefill means the client clock started after prefill had begun, and the figure
  excludes the wait it claims to measure. The failure is invisible in the number
  alone: only the comparison against `prompt_ms` exposes it.

## 1.6 Naming coherence between Rust and JSON

**The rule.** A tracing field emitted by the Rust runtime and a JSON key in a
result record that denote the same concept carry the same name. There is no
translation layer, no per-producer alias table, and no abbreviation on one side
only. A reader who has seen `orchestration_residual_ms` in a log knows it is the
same quantity as `orchestration_residual_ms` in a campaign file, without
checking.

**The obligation.** Any new tracing field is added to the table in section 4 of
`docs/agents/OBSERVABILITY.md` **in the change that introduces it**, with the
same name and meaning it carries here. A field that exists in code but not in
that table is a contract break, exactly as a field invented in a probe would be.

**Conflicts with the existing table, resolved.** Section 4 already defines two
fields that overlap this dictionary. Neither is renamed, and neither is silently
reused:

| Existing field | Its meaning there | Resolution |
|---|---|---|
| `tokens_in` | LLM input tokens | Retained for coarse per-call accounting where it is already used. **Not admissible in a measurement record or in `llm.completion.timings`**, because it does not say whether cached tokens are included, which is the entire distinction 1.4.1 exists to make. Use the `prompt_tok_*` triplet. |
| `tokens_out` | LLM output tokens | Same. Use `decode_tok`, which additionally has a defined relationship to `sse_chunks` and to the reasoning split. |
| `duration_ms` | Elapsed milliseconds | Retained as the generic span duration. Never used for any quantity in 1.4.3, each of which has a specific name and a specific exclusion. |

The launch configuration fields in a record's `engine` block carry the names of
the Rust struct fields they mirror: `n_ctx`, `n_gpu_layers`, `n_batch`,
`n_ubatch`, `n_parallel`, `cont_batching`, `cache_type_k`, `cache_type_v`,
`flash_attn`, `cache_reuse`. Those are the fields the runtime already logs under
`llama.server.spawn.config`, so a record's `engine` block and a spawn log line
are directly comparable without mapping.

**Where the engine's timings enter the Rust side.** They are parsed in
`apollia-llm`, at the OpenAI-compatible backend, and surface as
`CompletionResponse::engine_timings` on the non-streaming path and as
`StreamChunk::Timings` on the streaming one. The engine speaks that protocol, so
the boundary that already owns its wire format is the boundary that owns this
object; parsing it anywhere else would mean re-reading server-sent events
outside the backend that produced them. `apollia-runtime` consumes it and never
re-parses it.

The object crosses as raw JSON deliberately. The `timings` schema belongs to one
engine, and a typed cross-crate mirror of it would make `apollia-llm` depend on
the shape of a single backend. Consumers that understand the shape read it;
every cloud backend yields `None` and nothing downstream changes. This is why
`num()` and `int()` in the runtime's recorder read keys defensively rather than
deserialising a struct: an engine that renames a key degrades one field to
`null` instead of failing a completion.

The keys read from that object are `prompt_n`, `cache_n`, `prompt_ms`,
`predicted_n`, `predicted_ms`, `prompt_per_second` and `predicted_per_second`.
They are the engine's names, not this dictionary's, and 1.4 states the mapping
for each. Renaming them at ingestion would hide an engine change behind a
translation layer, which is the one thing 1.6 exists to prevent.

## 1.7 The result record schema

JSON Schema shaped, 2020-12 vocabulary. Normative content is the field semantics
in 1.4; this fixes the structure.

```jsonc
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "apollia:model-eval/measurement-record/1",
  "title": "Measurement record",
  "type": "object",
  "required": ["schema_version", "record_id", "probe", "measured_at",
               "provenance", "conditions", "engine"],
  "properties": {
    "schema_version": { "const": 1 },
    "record_id":      { "type": "string" },
    "campaign_id":    { "type": ["string", "null"] },
    "probe": {
      "enum": ["speed", "prefill_curve", "cache_reuse", "concurrency", "agentic",
               "toolcall", "esrs", "batch", "roofline"]
    },
    "label":       { "type": "string" },
    "measured_at": { "type": "string", "format": "date-time" },

    "provenance": {
      "type": "object",
      "required": ["git_sha", "git_dirty", "llama_server_version", "llama_server_path",
                   "model_path", "model_sha256", "model_sha256_scope", "launch_args",
                   "machine_id", "machine_chip", "machine_memory_bytes", "os_version"],
      "properties": {
        "git_sha":              { "type": "string" },
        "git_dirty":            { "type": "boolean" },
        "llama_server_version": { "type": "string" },
        "llama_server_path":    { "type": "string" },
        "model_path":           { "type": "string" },
        "model_sha256":         { "type": ["string", "null"] },
        "model_sha256_scope":   { "enum": ["whole_file", "first_shard", "none"] },
        "launch_args":          { "type": "array", "items": { "type": "string" } },
        "machine_id":           { "type": "string" },
        "machine_chip":         { "type": "string" },
        "machine_memory_bytes": { "type": "integer" },
        "os_version":           { "type": "string" }
      }
    },

    "conditions": {
      "type": "object",
      "required": ["run_index", "run_order", "page_cache", "server_restarted_before",
                   "slot_reset_before", "sampling_seed", "sampling_temperature"],
      "properties": {
        "run_index":               { "type": "integer", "minimum": 0 },
        "run_order":               { "enum": ["sequential", "randomised"] },
        "page_cache":              { "enum": ["cold", "warm", "unknown"] },
        "server_restarted_before": { "type": "boolean" },
        "slot_reset_before":       { "type": "boolean" },
        "sampling_seed":           { "type": "integer" },
        "sampling_temperature":    { "type": "number" },
        "notes":                   { "type": ["string", "null"] }
      }
    },

    // Mirrors the Rust launch configuration, field for field. See 1.6.
    "engine": {
      "type": "object",
      "required": ["n_ctx", "n_ctx_slot_tok"],
      "properties": {
        "n_ctx":          { "type": "integer" },
        "n_ctx_slot_tok": { "type": "integer" },
        "kv_cache_bytes": { "type": ["integer", "null"] },
        "n_gpu_layers":   { "type": "integer" },
        "n_batch":        { "type": ["integer", "null"] },
        "n_ubatch":       { "type": ["integer", "null"] },
        "n_parallel":     { "type": ["integer", "null"] },
        "cont_batching":  { "type": ["boolean", "null"] },
        "cache_type_k":   { "type": ["string", "null"] },
        "cache_type_v":   { "type": ["string", "null"] },
        "flash_attn":     { "enum": ["on", "off", "auto", null] },
        "cache_reuse":    { "type": ["integer", "null"] }
      }
    },

    // One entry per repetition, in execution order. Raw, unrounded.
    "samples": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["cache_state", "prompt_tok_total", "prompt_tok_computed",
                     "prompt_tok_cached", "decode_tok", "prefill_ms", "decode_ms",
                     "request_wall_ms", "streamed"],
        "properties": {
          "streamed":               { "type": "boolean" },
          "cache_state":            { "enum": ["cold", "warm"] },
          "prompt_cache_hit_ratio": { "type": "number" },
          "prompt_tok_total":       { "type": "integer" },
          "prompt_tok_computed":    { "type": "integer" },
          "prompt_tok_cached":      { "type": "integer" },
          "decode_tok":             { "type": "integer" },
          "decode_tok_reasoning":   { "type": ["integer", "null"] },
          "decode_tok_content":     { "type": ["integer", "null"] },
          "reasoning_split_method": { "enum": ["tokenized", "chars_apportioned", "none"] },
          "reasoning_chars":        { "type": ["integer", "null"] },
          "content_chars":          { "type": ["integer", "null"] },
          "sse_chunks":             { "type": ["integer", "null"] },
          "token_count_discrepancy_ratio": { "type": ["number", "null"] },
          "ttft_ms":                { "type": ["number", "null"] },
          "ttft_cold_ms":           { "type": ["number", "null"] },
          "ttft_warm_ms":           { "type": ["number", "null"] },
          "prefill_ms":             { "type": "number" },
          "decode_ms":              { "type": "number" },
          "ttft_overhead_ms":       { "type": ["number", "null"] },
          "request_wall_ms":        { "type": "number" },
          "prefill_tps":            { "type": ["number", "null"] },
          "decode_tps":             { "type": ["number", "null"] },
          "decode_tps_wall":        { "type": ["number", "null"] },
          "engine_prefill_tps":     { "type": ["number", "null"] },
          "engine_decode_tps":      { "type": ["number", "null"] },
          "context_occupancy_at_call_ratio": { "type": ["number", "null"] },
          "context_occupancy_ratio":         { "type": ["number", "null"] },
          "degenerate":             { "type": "boolean" },
          "empty":                  { "type": "boolean" }
        }
      }
    },

    // Aggregates over `samples`, keyed by the exact dictionary field name.
    "stats": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "required": ["n", "median", "p95", "min", "max", "cv", "samples"],
        "properties": {
          "n":       { "type": "integer", "minimum": 1 },
          "median":  { "type": "number" },
          "p95":     { "type": "number" },
          "min":     { "type": "number" },
          "max":     { "type": "number" },
          "mean":    { "type": "number" },
          "cv":      { "type": "number" },
          "samples": { "type": "array", "items": { "type": "number" } }
        }
      }
    },

    // probe == "prefill_curve"
    "prefill_curve": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["prompt_tok_total", "stats"],
        "properties": {
          "prompt_tok_total": { "type": "integer" },
          "stats":            { "type": "object" }
        }
      }
    },

    // probe == "agentic"
    "turn": {
      "type": "object",
      "required": ["turn_wall_ms", "iterations", "tool_calls", "engine_ms_total",
                   "tool_ms_total", "orchestration_residual_ms"],
      "properties": {
        "session_id":                   { "type": ["string", "null"] },
        "turn_wall_ms":                 { "type": "number" },
        "iterations":                   { "type": "integer" },
        "tool_calls":                   { "type": "integer" },
        "engine_ms_total":              { "type": "number" },
        "tool_ms_total":                { "type": "number" },
        "orchestration_residual_ms":    { "type": "number" },
        "orchestration_residual_ratio": { "type": "number" },
        "iteration_records": {
          "type": "array",
          "items": { "type": "object" }
        },
        "tool_records": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["tool_name", "tool_ms"],
            "properties": {
              "tool_name":        { "type": "string" },
              "tool_ms":          { "type": "number" },
              "tool_approval_ms": { "type": ["number", "null"] },
              "iteration_index":  { "type": "integer" }
            }
          }
        }
      }
    },

    // probe == "roofline"
    "roofline": {
      "type": "object",
      "required": ["params_total", "params_active_per_token", "bits_per_weight",
                   "bytes_per_token_read", "decode_ceiling_tps", "assumptions"],
      "properties": {
        "params_total":            { "type": "integer" },
        "params_active_per_token": { "type": "integer" },
        "bits_per_weight":         { "type": "number" },
        "bytes_per_token_read":    { "type": "integer" },
        "kv_cache_bytes":          { "type": "integer" },
        "recurrent_state_bytes":   { "type": "integer" },
        "bandwidth_bytes_per_s":   { "type": "number" },
        "peak_flops":              { "type": ["number", "null"] },
        "decode_ceiling_tps":      { "type": "number" },
        "prefill_ceiling_tps":     { "type": ["number", "null"] },
        "decode_efficiency_pct":   { "type": ["number", "null"] },
        "prefill_efficiency_pct":  { "type": ["number", "null"] },
        "sources":                 { "type": "object" },
        "assumptions":             { "type": "array", "items": { "type": "string" } }
      }
    },

    // probe == "concurrency". The record's `samples` and `stats` are the
    // per-slot view, identical in shape to any other timed probe. This block is
    // the across-slot view, which per-slot aggregation cannot express.
    "concurrency": {
      "type": "object",
      "required": ["n_requests", "rounds", "aggregate_decode_tps_wall"],
      "properties": {
        "n_requests": { "type": "integer" },
        "rounds":     { "type": "integer" },
        // Generated tokens of the whole round over the round's wall-clock,
        // prefill included. The `_wall` suffix is required by I5: this is not
        // comparable to `decode_tps`, which excludes prefill, and the two appear
        // on the same record. One aggregate per round, so `n` equals `rounds`.
        "aggregate_decode_tps_wall": { "type": ["object", "null"] },
        "round_wall_ms":             { "type": ["object", "null"] }
      }
    },

    // Quality probes keep their existing payloads. They are scored, not timed.
    "toolcall": { "type": "object" },
    "esrs":     { "type": "object" },
    "batch":    { "type": "object" },

    "invalid": {
      "type": "array",
      "items": { "type": "string" }
    }
  }
}
```

## 1.8 Worked example

Real values, from the run reported in 1.11. Single sample shown for readability;
a conforming record carries at least 5.

```json
{
  "schema_version": 1,
  "record_id": "speed-ministral-3-8b-000",
  "campaign_id": "op0-fill-in-test",
  "probe": "speed",
  "label": "ministral-3-8b",
  "measured_at": "2026-07-29T10:52:00Z",

  "provenance": {
    "git_sha": "ae88a4e445ca9d5b7ff90f48cb11011b3983f72c",
    "git_dirty": true,
    "llama_server_version": "version: 9870 (2d973636e)",
    "llama_server_path": "/opt/homebrew/bin/llama-server",
    "model_path": "/Users/nidalzoumita/.apollia_old/models/Ministral-3-8B-Instruct-2512-Q5_K_M.gguf",
    "model_sha256": "7a5454127ec772e2389f0e71a77fedb88b83d4366d8a69facd0cfd0898f04d35",
    "model_sha256_scope": "whole_file",
    "launch_args": ["-m", "/Users/nidalzoumita/.apollia_old/models/Ministral-3-8B-Instruct-2512-Q5_K_M.gguf",
                    "-ngl", "999", "-c", "32768", "-np", "1", "-cb",
                    "--flash-attn", "on", "--jinja",
                    "--reasoning-format", "none",
                    "--host", "127.0.0.1", "--port", "8099"],
    "machine_id": "Mac15,14",
    "machine_chip": "Apple M3 Ultra",
    "machine_memory_bytes": 274877906944,
    "os_version": "26.5"
  },

  "conditions": {
    "run_index": 3,
    "run_order": "sequential",
    "page_cache": "warm",
    "server_restarted_before": false,
    "slot_reset_before": false,
    "sampling_seed": 7,
    "sampling_temperature": 0.0,
    "notes": "warm-path measurement, prefix fully resident in the slot"
  },

  "engine": {
    "n_ctx": 32768,
    "n_ctx_slot_tok": 32768,
    "kv_cache_bytes": null,
    "n_gpu_layers": 999,
    "n_batch": null,
    "n_ubatch": null,
    "n_parallel": 1,
    "cont_batching": true,
    "cache_type_k": null,
    "cache_type_v": null,
    "flash_attn": "on",
    "cache_reuse": null
  },

  "samples": [
    {
      "streamed": true,
      "cache_state": "warm",
      "prompt_cache_hit_ratio": 0.9997519841269841,
      "prompt_tok_total": 4032,
      "prompt_tok_computed": 1,
      "prompt_tok_cached": 4031,
      "decode_tok": 34,
      "decode_tok_reasoning": null,
      "decode_tok_content": 34,
      "reasoning_split_method": "none",
      "reasoning_chars": 0,
      "content_chars": 148,
      "sse_chunks": 33,
      "token_count_discrepancy_ratio": 0.029411764705882353,
      "ttft_ms": 23.505,
      "ttft_cold_ms": null,
      "ttft_warm_ms": 23.505,
      "prefill_ms": 15.855,
      "decode_ms": 512.659,
      "ttft_overhead_ms": 7.65,
      "request_wall_ms": 536.257,
      "prefill_tps": 63.07158625039420,
      "decode_tps": 66.32088776360115,
      "decode_tps_wall": 66.30885886354417,
      "engine_prefill_tps": 63.07158625039420,
      "engine_decode_tps": 66.32088776360115,
      "context_occupancy_at_call_ratio": 0.123046875,
      "context_occupancy_ratio": 0.1240844726562500,
      "degenerate": false,
      "empty": false
    }
  ],

  "invalid": []
}
```

An `agentic` record, hand-constructed, showing the residual identity:

```json
{
  "schema_version": 1,
  "record_id": "agentic-000",
  "probe": "agentic",
  "label": "ministral-3-8b",
  "measured_at": "2026-07-29T11:04:00Z",
  "provenance": { "...": "as above" },
  "conditions": { "...": "as above" },
  "engine": { "n_ctx": 32768, "n_ctx_slot_tok": 32768 },
  "turn": {
    "session_id": "sess-op0-example",
    "turn_wall_ms": 8420.0,
    "iterations": 3,
    "tool_calls": 2,
    "engine_ms_total": 4079.9,
    "tool_ms_total": 2359.1,
    "orchestration_residual_ms": 1981.0,
    "orchestration_residual_ratio": 0.23527315914489311,
    "iteration_records": [
      { "prefill_ms": 1180.4, "decode_ms": 640.2, "prompt_tok_total": 1904,
        "prompt_tok_computed": 1904, "prompt_tok_cached": 0, "cache_state": "cold",
        "decode_tok": 42, "context_occupancy_at_call_ratio": 0.05810546875 },
      { "prefill_ms": 95.3, "decode_ms": 810.7, "prompt_tok_total": 2338,
        "prompt_tok_computed": 434, "prompt_tok_cached": 1904, "cache_state": "warm",
        "decode_tok": 55, "context_occupancy_at_call_ratio": 0.0713500976562500 },
      { "prefill_ms": 62.8, "decode_ms": 1290.5, "prompt_tok_total": 2601,
        "prompt_tok_computed": 263, "prompt_tok_cached": 2338, "cache_state": "warm",
        "decode_tok": 88, "context_occupancy_at_call_ratio": 0.07937622070312500 }
    ],
    "tool_records": [
      { "tool_name": "bash_executor", "tool_ms": 2210.5, "tool_approval_ms": 1800.0,
        "iteration_index": 0 },
      { "tool_name": "file_io", "tool_ms": 148.6, "tool_approval_ms": null,
        "iteration_index": 1 }
    ]
  },
  "invalid": []
}
```

## 1.9 Containers

One record type, two containers.

**Campaign file**, `results/<campaign_id>.json`, what a probe run produces and
what `aggregate.py`, `roofline.py` and `sweep.py` all read:

```jsonc
{
  "schema_version": 1,
  "campaign_id": "baseline-2026-07-29",
  "started_at": "2026-07-29T08:00:00Z",
  "finished_at": "2026-07-29T09:12:31Z",
  "records": [ /* measurement records */ ],
  "records_excluded": [
    { "record_id": "...", "reason": "negative orchestration residual" }
  ]
}
```

**Trace file**, JSON Lines, one record per line, appended live by the runtime
under an environment opt-in. Each line is complete and self-contained, so a
truncated file stays readable up to its last whole line.

`records_excluded` is not optional. A campaign that drops a run without saying
so reports a cleaner result than it measured.

## 1.10 Names retired by this contract

The pre-contract harness used these. They are not reintroduced, and the six
files in `results/` that carry them predate this document.

| Retired name | Why | Replacement |
|---|---|---|
| `ttft_ms` at record level | Silent about cache state, and the producer measured a warm request while presenting it as a speed figure | `ttft_cold_ms` or `ttft_warm_ms`, with `cache_state` asserted |
| `decode_tps` from chunk counting | Counted streamed events, not tokens | `decode_tps` from `timings.predicted_n` and `timings.predicted_ms`; the event count survives as `sse_chunks` |
| `decode_tok` from chunk counting | Same | `decode_tok` from `timings.predicted_n` |
| `total_ms` | Ambiguous scope, could be a request or a turn | `request_wall_ms` or `turn_wall_ms` |
| `chars` | Did not distinguish reasoning from content | `reasoning_chars` and `content_chars` |
| `prompt_ms` as a field name | Matches the engine's key but reads as "prompt duration" rather than "prefill duration" | `prefill_ms`, paired with `prefill_tps` |
| `predicted_ms` as a field name | Same, and "predicted" has no meaning outside llama.cpp | `decode_ms`, paired with `decode_tps` |

## 1.11 Fill-in test and its outcome

The acceptance test for this document: take one real `llama-server` response and
one hand-written agentic turn, and populate a record by hand. Every field is
either fillable or the schema is wrong.

**Setup.** `llama-server` b9870 launched with the exact frozen baseline argument
vector, serving `Ministral-3-8B-Instruct-2512-Q5_K_M.gguf` on an M3 Ultra.
Four requests: one non-streaming cold, one non-streaming warm, one streamed with
`timings_per_token` off, one streamed with it on.

**Outcome: the schema holds.** Every field in 1.4 was fillable from the run, the
GGUF header, or the machine, and the worked example in 1.8 is that record. Five
findings changed the document.

1. **`usage` is a second source for two fields, and it agrees.** The
   OpenAI-compatible `usage.prompt_tokens` matched `prompt_n + cache_n` on every
   request (3456 cold, 3472 warm), and `usage.prompt_tokens_details.cached_tokens`
   matched `cache_n` exactly. I1 is therefore verified against an independent
   counter, and both are recorded as cross-checks under Rule A rather than being
   collapsed into one field.

2. **A request intended as cold came back warm, and the schema caught it.** The
   streamed run used a fresh prompt on a server that had served two earlier
   requests, and returned `cache_n` 534 against `prompt_n` 3498: a 13.2 percent
   hit from a shared prefix. Under I2 that record is `warm` and cannot carry
   `ttft_cold_ms`. This is exactly the failure the metrology rule describes, and
   it is why `cache_state` is observed from the response rather than asserted by
   the probe. A procedural definition would have labelled it cold.

3. **Warm `prefill_tps` is not a rate.** Cold: 3456 tokens in 3815.3 ms, 906
   tok/s. Warm: 1 token in 15.9 ms, 63 tok/s. The warm figure is fixed
   per-request cost, not throughput, and comparing the two as if both were
   prefill rates would report a 14x regression where none exists. The "what it
   is not" entry for `prefill_tps` now says so with these numbers.

4. **Chunk counting under-counts, measurably.** 33 content-bearing events against
   `predicted_n` 34, a 2.9 percent discrepancy on a 34-token generation. The gap
   is structural, not noise: tokens carrying no content delta are invisible to
   the client. `token_count_discrepancy_ratio` is therefore documented as
   expected-non-zero evidence, not as a run quality gate.

5. **`kv_cache_bytes` needs raised verbosity, and the formula is now validated.**
   The allocation line is absent from the default server log. At `-v`,
   `llama_kv_cache: MTL0 KV buffer size = 544.00 MiB` appears, with an
   independent `common_memory_breakdown_print` table reporting 544 in its
   context column. Against the formula, using GGUF header values from the same
   log (34 layers, 8 KV heads, key and value length 128) at `n_ctx` 4096 with
   f16 cache:

   `4096 * 34 * 8 * (128 + 128) * 2 = 570425344 bytes = 544.00 MiB`

   Exact agreement. The formula and its verbosity requirement are recorded in
   1.4.6, so the roofline work inherits a validated source instead of deriving
   one.

6. **That formula is wrong by a factor of 4 on a hybrid model, and the roofline
   work found it.** Finding 5 validated against one dense model, where every
   block holds a KV cache, so the `n_layer` term was indistinguishable from
   `block_count`. Extending the check to `Qwen3.6-35B-A3B-MXFP4_MOE` predicted
   320 MiB at `n_ctx` 4096 against 80 MiB reported. The model declares
   `qwen35moe.full_attention_interval = 4`: only 10 of its 40 blocks hold a KV
   cache, the other 30 carry a recurrent state in a separate
   `llama_memory_recurrent` pool, and the server log shows them as `filtered`.
   The `n_layer` term is therefore `n_layer_attn`, counted from the tensor
   inventory (blocks owning an `attn_k` tensor) rather than from a metadata key,
   so a future hybrid needs no change. 1.4.6 now says so.

   The recurrent pool is a second allocation the contract had no field for, and
   it is read on every decoded token, so omitting it understates decode traffic.
   `recurrent_state_bytes` was added to 1.4.11 and validated on the same run:
   `30 blocks * 4 seqs * ((4096 + 2*16*128) * 3 + 4096 * 128) * 4 bytes =
   263454720 bytes = 251.25 MiB`, against `llama_memory_recurrent: size =
   251.25 MiB`. Exact.

   Full result after the correction, `roofline.py --validate-kv --matrix --ctx
   4096,16384`, two models and two context lengths, six pool comparisons:

   | model | pool | n_ctx | cells | predicted | reported | delta |
   |---|---|---|---|---|---|---|
   | Ministral-3-8B Q5_K_M | kv cache | 4096 | 4096 | 544.0 MiB | 544.0 MiB | 0.000 % |
   | Ministral-3-8B Q5_K_M | kv cache | 16384 | 16384 | 2.12 GiB | 2.12 GiB | 0.000 % |
   | Qwen3.6-35B-A3B MXFP4 | kv cache | 4096 | 4096 | 80.0 MiB | 80.0 MiB | 0.000 % |
   | Qwen3.6-35B-A3B MXFP4 | recurrent | 4096 | 4 | 251.2 MiB | 251.2 MiB | 0.000 % |
   | Qwen3.6-35B-A3B MXFP4 | kv cache | 16384 | 16384 | 320.0 MiB | 320.0 MiB | 0.000 % |
   | Qwen3.6-35B-A3B MXFP4 | recurrent | 16384 | 4 | 251.2 MiB | 251.2 MiB | 0.000 % |

   The general lesson is the one finding 2 already made in another register: a
   formula validated on a single instance is validated against that instance,
   not against the quantity. The check is now a mode of the script, so it reruns
   on any model rather than being a paragraph.

7. **I11's "the baseline's own cv" is ambiguous, and the narrow reading cannot
   support a comparison between configurations. Proposed clarification, not
   applied.** Measured within-run cv on the two frozen baselines, five requests
   each:

   | model | cold decode | cold prefill | cold ttft | warm decode |
   |---|---|---|---|---|
   | ministral-3-8b | 0.0056 | 0.0019 | 0.0025 | 0.0119 |
   | qwen3.6-35b-a3b | 0.0146 | 0.0233 | 0.0193 | 0.0155 |

   Every one of those figures comes from five requests served consecutively by a
   single server process. No restart, no model reload, no thermal excursion
   occurs inside that window, so none of that variance is in the number, by
   construction rather than by luck. A sweep compares configurations that each
   required a fresh process and a fresh load, and it may run for hours. Using a
   0.0146 floor to declare a two percent difference between two such runs an
   effect asserts that restart and drift contribute nothing, which the
   measurement never tested.

   `sweep.py` therefore repeats the baseline at random positions and measures
   its dispersion across those runs.

   **Second half, and the more important one: a cv is not a threshold, whichever
   cv it is.** I11 as written compares a delta against the cv itself, which is a
   one-sigma test. The level's own median carries the same between-run error the
   cv describes, so the standard error of the difference is larger than the cv,
   and under a true null a large fraction of comparisons clear one sigma by
   chance. Measured by simulation through the harness's own aggregation, drawing
   baseline and level from an identical distribution so that no effect exists by
   construction, 4000 trials per cell:

   | threshold | true cv 0.005 | true cv 0.02 | true cv 0.05 |
   |---|---|---|---|
   | the bare cv, one sigma | 43.2 % | 43.1 % | 43.2 % |
   | t(0.975, n-1) * cv * sqrt(1 + 1/n) | 5.9 % | 5.8 % | 5.3 % |

   The false-positive rate of the one-sigma rule is scale-free, so no amount of
   care in measuring the cv improves it, and more baseline repeats do not help:
   it asymptotes near 32 percent. On a grid of 15 levels across 6 metrics that is
   roughly 39 invented effects in a report where nothing changed at all.

   The threshold `sweep.py` uses is the half-width of a two-sided 95 percent
   interval on the difference between an `n`-run baseline and a single level run,
   the level assumed to carry the baseline's dispersion because one run cannot
   estimate its own. At `n = 5` it is 3.04 times the cv. The report prints the
   dispersion and the threshold in adjacent columns so neither is mistaken for
   the other.

   **Proposed clarification of I11, not applied.** "A delta smaller than the
   baseline's own cv" becomes "a delta within the detection threshold implied by
   the baseline's between-run dispersion", with the threshold stated. Per
   amendment rule 4 this touches 1.5, so 1.5 is unchanged pending R&D scope
   sign-off; `sweep.py` states the threshold it used and how it was derived in
   every report it renders.

   **Evidence base.** The within-run figures above are measured from the two
   frozen baselines. The false-positive rates are measured by simulation through
   the production code path, not argued. What is not yet measured is the
   between-run cv itself, which needs a campaign with repeated baselines; that is
   the first thing a full sweep produces. What would falsify the first half: if
   the between-run cv comes back at or below the within-run cv, the extra
   baseline runs cost wall-clock and buy nothing. Nothing falsifies the second
   half short of a different definition of a detection threshold, since the
   one-sigma rule's error rate is arithmetic rather than empirical.

8. **1.4.8's instability threshold has to apply to the baseline runs, not only
   to what they are compared against.** The rule as written says an aggregate
   with a cv above 0.10 is unstable and its median is not reported as fact. In a
   comparison the baseline does more than report a median: its dispersion across
   runs is what every detection threshold is derived from. One unstable baseline
   run among five therefore does not produce one unreliable number, it raises the
   bar for every level of every factor at once, and the report says "no
   detectable effect" with more confidence than a stable baseline would have
   allowed. That failure is silent in a way an unstable level is not, because
   nothing in the level's own figures looks wrong.

   `sweep.py` checks the rule on both sides and withholds every verdict for a
   metric whose baseline contains an unstable run. It names the run rather than
   dropping it: choosing which runs to exclude after seeing the answer is how a
   sweep talks itself into a result.

9. **The first full grid run was invalidated by machine contention, and the
   numbers say exactly when.** CPU work was started on the same machine shortly
   after the campaign began. Runs 0 and 1, both baseline runs, recorded
   within-run cv of 0.42 and 0.46 on decode against 0.005 to 0.013 on every
   later run, and one concurrency round returned 33.0 tok/s where its neighbours
   in the same run returned 77.7. Every record measured from 13:54 onward is
   clean; every unstable one falls in the 13:45 to 13:53 window.

   Two things follow. The obvious one is operational and is now in 2.6: a sweep
   owns the machine for its duration. The second is that the contamination was
   invisible in the headline figures. The baseline decode median across the five
   runs was 78.55 tok/s, entirely plausible, and the campaign reported 20 of 20
   runs completed with nothing excluded. What exposed it was the dispersion,
   which is the whole reason 1.4.8 exists and the reason 1.4.9 asks for the
   samples to be retained rather than only their summary. A campaign reporting
   medians alone would have published the contaminated result.

10. **A record selected by observed cache state is the wrong record whenever a
    configuration changes the cache.** The sweep read the speed probe's cold and
    warm records by asking which state their samples came back in. At
    `-ctxcp 0` this model serves nothing from any cache, so the resend of an
    identical prompt comes back cold like everything else, and the warm record
    was silently filed as a second cold record: duplicated into the cold metrics,
    deleted from the warm ones. Visible in the report as two rows for
    `ctx_checkpoints = 0` under `prefill_tps cold` where one run existed.

    Records are now selected by the producer's own name, and the state a metric
    presupposes is a separate declared gate. A metric whose gate is not met is
    withheld and named, rather than reclassified. The continuation metrics carry
    no gate on purpose: there the cache state is the thing being measured, and
    gating a hit ratio on the cache being warm would suppress exactly the result
    that matters.

    The general form is worth stating, because it will recur. Any rule that
    infers which measurement a record *is* from a quantity the experiment is
    *varying* will misfile the record at the most interesting level of the
    factor. Identity comes from the producer; state is an observation about it.

**Two fields were `null` and correctly so.** `ttft_ms` on the non-streaming
requests, which is what I12 now states explicitly, and `kv_cache_bytes` on the
default-verbosity run. Neither is a schema gap; both are the absence-is-not-zero
rule doing its job.

**Streaming timings confirmed on the real build.** Without
`timings_per_token`, exactly one chunk carried timings, the final one. With it,
all 34 did. Both paths therefore yield authoritative token counts, and the
opt-in is needed only for per-token granularity.

## 1.12 What the repaired harness found

Findings from campaigns run under this contract. Each is here
because it contradicts something a reader would otherwise assume.

**1. Prefix reuse on the hybrid model is served by context checkpoints, and one
request of one specific length disables it for the life of the process.**
`cache_reuse` measures the shape of a ReAct step: a long prompt, then the same
prompt with a short result appended. Ministral serves 98.50 percent of the
continuation from cache, recomputing 71 tokens of 4746. Qwen3.6-35B-A3B against
a fresh server serves 86.06 percent, recomputing 587 of 4214. Run **inside the
campaign**, after the speed and prefill curve probes, the same probe on the same
model served **0.02 percent**: 1 token of 4214, recomputing 4213.

The mechanism is established below by intervention rather than inference. Every
figure is n = 5 with cv at or below 0.0012, records in
`results/prefix-collapse-*.json` with `invalid: []`, engine `9870 (2d973636e)`,
product launch vector, one server per condition and one factor changed at a
time. Each claim names the sequence and the launch delta that produced it, so
any of them can be replayed with `prefix-collapse.sh <condition>`.

| condition | launch delta | sequence | cached of total | hit |
|---|---|---|---|---|
| l1-control | none | pair | 3626 / 4214 | 86.06 % |
| l2a-pre-short | none | 64-token request, pair | 3626 / 4214 | 86.06 % |
| l2b-pre-long | none | 16384-token request, pair | 3626 / 4214 | 86.06 % |
| l7-curve-preamble | none | the whole prefill curve, pair | 3626 / 4214 | 86.06 % |
| l8-campaign-replay | none | the campaign's own two probes, pair | 1 / 4214 | **0.02 %** |
| l9-trap | none | one 517-token request, pair | 1 / 4214 | **0.02 %** |
| l9b-trap-cms0 | `-cms 0` | one 517-token request, pair | 3626 / 4214 | 86.06 % |
| l4-ctxcp0 | `-ctxcp 0` | pair | 0 / 4214 | **0.00 %** |
| l5a-ubatch128 | `-ub 128` | pair | 4010 / 4213 | 95.18 % |
| l5-ubatch256 | `-ub 256` | pair | 3882 / 4214 | 92.14 % |
| l5b-ubatch1024 | `-ub 1024` | pair | 3114 / 4213 | 73.90 % |
| l5c-ubatch2048 | `-ub 2048` | pair | 2094 / 4213 | 49.71 % |
| l6-dense | none, dense model | 64-token request, pair | 4675 / 4746 | 98.50 % |

**Reuse on this model is a checkpoint restore, not a cache lookup.** 1.4.6
records that 10 of its 40 blocks hold a KV cache; the rest hold recurrent state
that cannot be rolled back to an arbitrary position, so the engine serves a warm
prefix by restoring a *context checkpoint* and by nothing else. Launched with
`--ctx-checkpoints 0`, the isolated pair serves 0.00 percent and the engine log
carries five `forcing full prompt re-processing due to lack of cache data` where
the baseline carries five `restored context checkpoint`. Prefill goes from 461
to 2541 ms. Nothing else changed.

**The restore point sits one micro-batch behind the end of the previous prompt.**
Checkpoints are laid down `4 + n_ubatch` and `4` tokens before a prompt ends, and
the engine clamps that distance to `n_batch`: `n_last = std::min(n_batch,
offset)`. The later of the two is useless to a continuation, since it sits past
the point where the two prompts diverge and the search rejects it by position,
`checking checkpoint with [4143, 4143] against 4141`. A warm continuation
therefore recomputes `min(n_batch, 4 + n_ubatch)` tokens, plus whatever it
appended after the divergence, 71 on this probe's shape. Measured across a
sixteenfold range of `-ub`, one server per level, n = 5, cv at or below 0.0004:

| `-ub` | recomputed | warm hit | `min(n_batch, 4 + n_ubatch) + 71` |
|---|---|---|---|
| 128 | 203 | 95.18 % | 203 |
| 256 | 331 | 92.14 % | 331 |
| 512, default | 587 | 86.06 % | 587 |
| 1024 | 1099 | 73.90 % | 1099 |
| 2048 | 2119 | 49.71 % | 2119 |

The clamp was found by the measurement rather than assumed, and it is the reason
the table carries the longer expression. The simpler `4 + n_ubatch + 71` fits the
first four levels exactly and overshoots the fifth by 4 tokens, which is
precisely the amount by which `4 + 2048` exceeds the default `n_batch` of 2048.
Predicted before it was run, that fifth row would have been wrong; it is stated
here as measured.

Neither direction of the flag is free. At 256 the restore point moves 256 tokens
closer and reuse reaches 92.14 percent, at a cost of 45 percent of prefill
throughput, 1638 to 893 tok/s, and 42 percent of decode, 78 to 45 tok/s. At 2048
prefill improves, which is what finding 4 measures, and a ReAct continuation
recomputes half its prompt on every iteration. The two halves of `-ub` are
weighed against each other in finding 8, and neither half is a recommendation on
its own.

**The collapse is one poisoned checkpoint, not an accumulation of history.** A
checkpoint created while the slot holds exactly one token has
`pos_min = pos_max = 0`. The invalidation loop erases only checkpoints past the
reuse point, so that one is never erased and the slot's checkpoint list is never
empty again. The minimum-spacing rule then refuses every new checkpoint that does
not begin `--checkpoint-min-step` tokens past the last one, 8192 by default,
which a 4k prompt never clears. Every continuation from then on finds only the
one-token checkpoint, restores it, and recomputes the rest: `restored context
checkpoint (pos_min = 0, pos_max = 0, n_tokens = 1, n_past = 1)`, five times out
of five, 1 token of 4214.

Such a checkpoint is created by a prompt of exactly `4 + n_ubatch + 1` tokens,
517 at the default `-ub 512`. That is why history in general does not reproduce
the collapse and one request does. A 64-token request before the pair leaves
reuse at 86.06 percent, so does a 16384-token one, so does replaying the whole
prefill curve, 30 requests from 512 to 16384 tokens. Replaying the campaign's own
two probes reproduces the collapse exactly, and the engine log of that run
contains exactly one prompt of 517 tokens, the prefill curve's 512-token point,
where the run that did not collapse contains none. Issued deliberately, that one
request is the whole reproduction.

**The condition that avoids it, and its price.** `--checkpoint-min-step 0`, env
`LLAMA_ARG_CHECKPOINT_MIN_SPACING_NT`, lets the seed lay down its own checkpoint
even when the poisoned one is present, and the same sequence that collapses under
the default holds at 86.06 percent. The price is measured, not assumed: 36
checkpoints created instead of 5 over the same five repetitions, at 62.813 MiB
each against the 8192 MiB `-cram` budget, and on the warm path 521 ms of prefill
instead of 461. The alternative reading, that `--ctx-checkpoints` should be
raised, is not supported: the ring never filled in any of these runs, and
`-ctxcp 0` is the collapse on demand. Apollia passes neither flag today and has
no path to them except `APOLLIA_LLAMA_EXTRA_ARGS`, which is where a decision to
act on this would land.

For an agentic loop the practical figure is 461 ms against 2563 ms of prefill on
every iteration of every turn, and nothing in the response distinguishes the two
cases except `cache_n`, which is exactly why 1.4.1 insists it be recorded. A
campaign that reported only a decode rate would show nothing.

**2. Efficiency against the roofline separates the two models sharply, in the
opposite direction to the intuition about mixture of experts.** At n_ctx 32768,
measured median over ceiling:

| model | prefill | decode |
|---|---|---|
| ministral-3-8b | 894 / 2665 tok/s, 33.5 % | 63.5 / 79.9 tok/s, **79.4 %** |
| qwen3.6-35b-a3b | 1705 / 7194 tok/s, 23.7 % | 74.9 / 236.1 tok/s, **31.7 %** |

The dense model is at 79 percent of its memory-bandwidth ceiling, which on
Apple Silicon is effectively the ceiling: 1.4.11 records that achievable
bandwidth is 70 to 85 percent of the theoretical peak. There is nothing to win
there without changing the model. The mixture of experts model reaches 32
percent, so it is **not** bandwidth-bound, and roughly three times its measured
decode rate is available to whatever removes the actual constraint. Its low
active-parameter count buys a high ceiling that it does not currently reach.

**3. An unanswered tool approval is invisible in the turn decomposition.**
Before the agentic probe answered approvals, a turn spent 300 seconds blocked on
one and the record reported `tool_calls: 0`, `tool_ms_total: 0.0`, and
`orchestration_residual_ms: 300059`, a 98.7 percent residual. The wait was
attributed to orchestration, which 1.4.7 defines as the term this project can
act on directly, when in fact nothing in the runtime was running. `tool_approval_ms`
exists to keep human wait out of the residual, but it is only recorded once the
tool executes, so an approval that is never answered bypasses it entirely. A
turn whose residual exceeds its engine time by an order of magnitude should be
read as a blocked approval until proven otherwise.

With approvals answered, the same scenario decomposes as it should: an 8
iteration, 7 tool call turn spent 12984.9 ms of 13102.6 ms in the engine, 2.5 ms
in tools, and 115.2 ms in orchestration. Across three turns the orchestration
residual was 0.5 to 1.8 percent of wall-clock, and I6 held to 0.000000 ms on
every one. Apollia's own overhead is not where the time goes.

**Closed.** The wait is now recorded where it happens, which is upstream of the
invoker rather than inside it: an approval is requested, the turn blocks on the
answer, and only then does a tool run, if it runs at all. `approvals`,
`approval_ms`, `approved` and `approval_ms_total` are defined in 1.4.7, human
wait is a term of the residual identity rather than a component of it, and I6
still closes exactly. A refused or timed-out approval now appears as an approval
record with `approved: false` and no tool call, which is the shape that
previously showed as pure orchestration. The comment in the tool dispatcher
asserting that the approval wait sat inside the invoker's span was wrong in both
halves and is corrected. Two tests cover it, one of them the 300-second
unanswered case that produced the figure above.

What is still not distinguished is a refusal an operator typed from one a
timeout produced: both reach the recording site as the same decision, and the
contract records `approved: false` for both rather than guessing.

**4. The headroom in finding 2 rests on a ceiling that may not hold for the
model it flatters, and the two candidate explanations are not separable in this
shortlist.** `sweep.py --ceiling-check` multiplies each model's measured cold
decode median by its `bytes_per_token_read` to get the bandwidth the model
actually achieved on the bus, which is the same arithmetic as
`decode_efficiency_pct` read the other way round:

| model | decode | bytes per token | achieved | of peak | architecture |
|---|---|---|---|---|---|
| ministral-3-8b | 63.5 tok/s | 10.25 GB | 650 GB/s | 79.4 % | dense, full attention |
| qwen3.6-35b-a3b | 74.9 tok/s | 3.47 GB | 260 GB/s | 31.7 % | MoE 8 of 256 experts, hybrid attention |

Both models read from the same 819 GB/s bus, and the dense one demonstrates that
650 GB/s of it is reachable. Two readings survive that: the hybrid model is
genuinely not bandwidth-bound and roughly 2.5x of its decode rate is available,
or its access pattern cannot reach a contiguous read's rate and
`decode_ceiling_tps` is optimistic for exactly the architecture that appears to
have room. `bandwidth_bytes_per_s` is a vendor figure for sequential access, and
a mixture of experts model gathers 8 of 256 expert tensors per token.

**The shortlist cannot separate them.** Every mixture of experts model here is
also a hybrid attention model, and the only dense model is also full attention.
The 260 GB/s deficit is equally consistent with expert gather and with the
recurrent path, and nothing in this data distinguishes them. What would settle
it is a third model breaking the pair: dense with hybrid attention, or mixture of
experts with full attention. Until then the 2.5x is an upper bound carrying a
known modelling weakness, and 1.4.11's `decode_efficiency_pct` note that "a low
value can mean the ceiling is wrong" is doing real work rather than hedging.

**One discriminator is available without a third model,** and it is already a
factor in the sweep grid rather than a separate experiment. Micro-batch width
sets how many tokens amortise a single expert gather during prefill, so if the
deficit is a gather problem the mixture of experts model's prefill responds to
`-ub` materially more than the dense model's does.

**It fell toward the ceiling being wrong.** Both sweeps, five baseline runs each,
randomised order, every figure past its detection threshold:

| prefill response to `-ub`, against the engine default | ub 128 | ub 256 | ub 1024 | ub 2048 |
|---|---|---|---|---|
| qwen3.6-35b-a3b, MoE 8 of 256, hybrid | -43.8 % | -21.5 % | +13.8 % | +21.1 % |
| ministral-3-8b, dense, full attention | not run | -5.7 % | +4.5 % | +5.4 % |

The larger response exceeds the smaller by 3.1 to 3.9 times at every level both
models ran, in both directions. The mixture of experts model carries a cost that
a wide micro-batch amortises and that the dense model largely does not, which is
the signature the gather reading predicts.

This table is the prefill half of `-ub` and must not be read as a
recommendation. The same flag sets how much of a warm prompt the hybrid model is
obliged to recompute, measured in finding 8, and the two halves point opposite
ways. Neither is the whole result.

**The consequence for the ceiling is the part that matters, and it is
structural.** Decode showed no detectable effect from `-ub` on either model, at
any level, and it could not have: a decoded token is a batch of one by
construction, so a cost that batch width amortises during prefill cannot be
amortised at decode at all. A model carrying a large batch-amortisable cost
therefore pays it in full on every generated token, and that is precisely the
traffic `decode_ceiling_tps` models as a contiguous sequential read at the
vendor's peak bandwidth. The 2.5x apparent headroom is more likely to be a
ceiling that does not fit this architecture than throughput waiting to be
collected.

**What this still does not establish.** The confound above is untouched: the
batch sensitivity could belong to the recurrent path rather than to expert
gather, since the recurrent blocks also process the prompt in wide batches and
also decode one token at a time. The measurement shows that the hybrid mixture of
experts model carries a large per-batch cost the dense model does not; it does
not name which of its two distinguishing features carries it. Both readings point
the same way for the ceiling, which is why the conclusion above is stated while
the attribution is not. A third model breaking the pair remains what would settle
the attribution, and `1.4.11` should not gain a scatter term until something
establishes the mechanism rather than the correlation.

**Power, from the adversarial pass.** The decode half of this finding is
underpowered and must not be read as a measured absence. On the hybrid model the
observed `-ub` decode deltas run -1.6 to +1.2 percent and an effect that size is
detected 8 to 18 percent of the time at this sample size; 80 percent power needs
6.5 percent. On the dense model, +0.2 to +1.3 percent observed against a 7.0
percent requirement. Warm decode on both models sits at the same thresholds.
The sentence "decode showed no detectable effect from `-ub` on either model, at
any level, and it could not have" therefore rests entirely on its second clause,
which is an argument from batch-of-one decoding and is untouched. The conclusion
may well be right. The decode measurement is not what makes it right. The prefill
differential of 3.1 to 3.9 times is unaffected and was reproduced by hand.

**5. Quantising the KV cache costs speed on both models, and the cost grows with
the context it exists to make affordable.** The intuition is that a smaller cache
moves fewer bytes and therefore decodes faster. Both grids say the opposite, past
every threshold:

| model | level | decode cold | prefill cold |
|---|---|---|---|
| qwen3.6-35b-a3b | `q4_0/q4_0` | 70.99 tok/s, -8.0 % | 1688.18 tok/s, -1.6 % |
| qwen3.6-35b-a3b | `q8_0/q8_0` | 71.88 tok/s, -6.8 % | 1688.62 tok/s, -1.6 % |
| ministral-3-8b | `q8_0/q8_0` | 58.07 tok/s, -11.0 % | 861.94 tok/s, -4.4 % |

against detection thresholds of 4.2 and 0.4 percent on the hybrid grid and 4.7
and 0.2 on the dense one. The per-length curve gives the shape: on the hybrid
model at `q4_0` the prefill loss is 1.3 percent at 519 tokens and 7.8 percent at
16372, and on the dense model at `q8_0` it is 2.1 percent at 1043 tokens and 18.2
percent at 16893. The flag is worst exactly where a smaller cache would be worth
having.

The quality side is not answered here, and the plan asked for it. `kv_cache_type`
declares a `toolcall` quality gate, the scored probe reports one observation, and
its 0.92 is therefore provisional under I8 in both grids with no verdict rendered
against it. The speed result alone is enough to leave the flag off, which is what
the frozen baseline already does; the quality comparison would only have mattered
had the speed side been favourable. Datasets:
`results/sweep-qwen-full-grid-20260729T150416Z.json`,
`results/sweep-ministral-confirmation-20260729T162235Z.json`.

**6. The slot count buys aggregate throughput by taking it from every
conversation on the machine, and nothing else in the grid moves anything.** `-np`
is the one factor that improves a headline number: aggregate decode rises 27.2
percent at 2 slots, 40.6 at 4 and 48.1 at 8 on the hybrid model, and 17.0 percent
at 4 slots on the dense one, all past thresholds of 1.3 and 2.3 percent. The same
runs record what pays for it. Per-slot decode falls 25.8 percent at 2 slots and
55.1 at 4, and at 8 slots the within-run cv reaches 0.194, above the 0.10 of
1.4.8, so that median is reported as unstable rather than as fact; the dense
model's 4-slot run behaves the same way at cv 0.233. Both columns come from the
same records, and reporting either alone describes a machine that was not
measured. `-c` scales with `-np` in every one of these runs so each slot keeps
32768 tokens, which is what makes this a comparison of slot count rather than of
slot capacity.

Every other flag is inert at the sizes one conversation uses. `-b` shows no
detectable effect at 1024 or 4096 and costs 1.2 percent of prefill at 512. `-c`
is the useful negative: raising it from 32768 to 131072, a fourfold larger
window, costs 0.5 percent of prefill and 0.7 percent of cold first token, and
8192 is indistinguishable from the baseline. A context window is close to free
until it is filled, which is the opposite of the usual assumption and part of why
1.4.6 measures occupancy against the slot rather than against `-c`. Datasets: the
two grids above.

**Power, from the adversarial pass.** The `-c` and `-np` results survive intact
and are the best-supported claims in this section: prefill and cold first token
reach 80 percent power below 0.5 percent, and the `-np` gains of 27, 41 and 48
percent sit against a 1.8 percent bar. The `-b` decode result does not. Observed
deltas of -0.7 to -1.2 percent on the hybrid model sit against a 4.2 percent
threshold, so "no detectable effect" there means the experiment could not have
seen an effect of that size, not that none exists.

**7. Proposed clarification to I11: the bar is a detection threshold, not the
dispersion itself.** This is a proposal, not an amendment. 1.5 is unchanged, and
stays unchanged until the R&D scope adopts or rejects what follows. It is
recorded here with the measurement that motivated it, as 1.11's amendment
protocol requires.

Read literally, I11 reports a delta against the dispersion, which compares a
difference to a one-sigma spread and declares roughly a third of null comparisons
an effect. `sweep.py` resolves a level against
`t(0.975, runs-1) * cv * sqrt(1 + 1/runs)` instead, the half-width of a two-sided
95 percent interval on the difference between the baseline and one level run,
which at five runs lands near three times the cv. The measured baselines show the
size of the gap: on the hybrid grid the between-run cv is 0.0012 for prefill and
0.0139 for cold decode, and the thresholds derived from them are 0.4 and 4.2
percent.

The proposal carries a second half, a caution rather than a formula. The prefill
baseline is reproducible enough that its threshold is 0.4 percent on one model
and 0.2 on the other, which makes a 0.5 percent difference detectable and says
nothing about whether it matters. Detectable and important are different
questions: the verdict answers the first, the delta column answers the second,
and a report printing one without the other invites the reader to conflate them.
Both are printed for that reason. Datasets: the two grids above.

**8. The wide micro-batch that finding 4 credits for prefill is paid back on the
warm path, and only on the model that needs checkpoints.** Finding 4 reads `-ub`
through prefill, where 2048 buys 21.1 percent on the hybrid model. The same runs
measure the warm request, and it moves the other way. The recomputed column is
`prompt_tok_total - prompt_tok_cached` from the speed probe's warm record:

| `-ub` | hybrid, recomputed of 2050 | hybrid warm TTFT | dense, recomputed of 2573 | dense warm TTFT |
|---|---|---|---|---|
| 128 | 132 | 182.70 ms, -46.9 % | not run | not run |
| 256 | 260 | 240.77 ms, -30.0 % | 1 | no detectable effect |
| default 512 | 516 | 343.99 ms | 1 | 20.98 ms |
| 1024 | 1028 | 559.76 ms, +62.7 % | 1 | no detectable effect |
| 2048 | 2049 | 998.26 ms, +190.2 % | 1 | no detectable effect |

against thresholds of 1.4 percent on the hybrid grid and 11.2 on the dense one.
The recomputed column is `4 + n_ubatch` exactly, at every level, until it exceeds
the prompt: at 2048 the mandatory recompute of 2052 tokens is larger than the
2050-token prompt, so one token is cached and the request is cold in all but
name. The dense model recomputes one token whatever the flag says.

Finding 1 measures the same law on a different shape, a ReAct continuation of
4214 tokens rather than a repeat of an identical 2050-token prompt, and lands on
`min(n_batch, 4 + n_ubatch) + 71` across the same range of levels. Two probes,
two prompt shapes and two operations agree on the rule, which is what makes the
warm cost of `-ub` a property of the engine rather than of either measurement.
The clamp shows up there and not here because this shape's prompt is shorter than
`n_batch` and runs out first.

This is finding 1's mechanism measured by a different operation with a different
probe: a warm continuation on the hybrid model restores a checkpoint laid
`4 + n_ubatch` tokens back and recomputes the remainder, so widening the
micro-batch widens the mandatory recompute by the same amount, while the dense
model reuses through its KV cache and pays nothing. Neither verdict changes; the
prefill gain is real and so is the warm loss. What changes is the recommendation
a reader would draw from either in isolation. An agentic loop pays cold prefill
once per turn and the warm path on every iteration after the first, so the flag
finding 4 credits is the flag this finding warns about, and the two are read
together or not at all. Datasets: the two grids above, and
`results/prefix-collapse-*.json` for the mechanism.

**Power, from the adversarial pass.** The dense model's warm TTFT row is
underpowered: -4.6 to +4.5 percent observed against an 11.2 percent threshold, at
which a 4 percent effect is caught 12 percent of the time. Its "no detectable
effect" cells say the experiment could not have seen a change of that size. The
conclusion survives on other evidence, and on stronger evidence than a threshold
comparison: the dense model's recomputed column is 1 at every level, which is a
direct observation. The hybrid rows are far past their 1.4 percent bar and are
unaffected.

**9. A sweep owns the machine, and a contaminated baseline is invisible in every
headline figure.** The first full grid under this harness ran with other work on
the same machine. Its dataset is kept at
`results/contaminated/sweep-qwen-full-grid-20260729T134337Z.json`, because it is
the evidence for this finding rather than a mess to delete.

Nothing in the summary looked wrong. It reported 20 runs planned, 20 completed, 0
excluded, and a baseline cold decode median of 78.55 tok/s, a plausible figure
within 2 percent of the clean run's 77.15. The damage was entirely in the
dispersion: baseline runs 0 and 1 carry within-run cv of 0.42, 0.39, 0.32, 0.38
and 0.46 across the metrics, where the clean run gives 0.01. Because both were
baseline runs, that dispersion set every threshold derived from it. Aggregate
decode's threshold became 76.5 percent and per-slot decode's 90.1, against 1.3
and 2.0 in the clean run, and the consequence was a real effect reported as none:
8 slots raised aggregate throughput 45.6 percent, which the contaminated grid
could not tell from zero and the clean grid records as +48.1 percent past a 1.3
percent threshold.

The gap in the code was real rather than bad luck. 1.4.8's instability rule was
applied to level runs and not to baseline runs, and an unstable baseline fails
silently in a way an unstable level does not, because nothing in the level's own
figures looks wrong. `sweep.py` now checks both sides and withholds the verdicts
for a metric whose baseline contains an unstable run, naming the run rather than
dropping it: choosing which runs to exclude after seeing the answer is how a
sweep talks itself into a result.

**The guard was written into one of the two comparison paths, and the sentence
above stood as a general claim until something rendered the fixture and read the
output rather than the code.** It covered the metric table. The per-length curve
table is a second path with its
own baselines, its own thresholds and, until this operation, no instability check
of any kind: `curve_points` read each point's median and discarded its `cv`, and
`curve_analysis` called `verdict()` directly rather than going through
`row_verdict`. On the contaminated fixture the same command that refused 7 of 8
metrics went on to print **70 per-length verdicts** off baselines carrying
within-run cv of 0.19, 0.29 and 0.36, including verdicts of the form "worse,
beyond the 0.8 percent threshold". Two were the reverse of what the clean grid
says at the same delta:

| length, level | contaminated | clean grid |
|---|---|---|
| 16372 tok, `kv_cache_type = q4_0/q4_0` | -7.8 %, no detectable effect | -7.8 %, worse, beyond 0.2 % |
| 16372 tok, `kv_cache_type = q8_0/q8_0` | -9.4 %, no detectable effect | -9.5 %, worse, beyond 0.2 % |

**The guard was extended rather than the sentence corrected a second time.**
`curve_points` now carries each point's within-run cv and the index of the run
that produced it, and `curve_analysis` withholds a length at every level when a
baseline run measured it above the 0.10 of 1.4.8. Re-rendered against
`results/contaminated/sweep-qwen-full-grid-20260729T134337Z.json` today: 7 of 8
metrics refused in the metric table, and 35 of the same 70 curve verdicts
withheld, at lengths 1031, 2095 and 16372, naming baseline runs 1, 0 and 0. Both
inverted verdicts above are among them. The deltas are still printed beside the
withholding, so the size of what is not being claimed stays visible.

The three clean grids re-render **byte-identical** before and after the change,
which is the property that matters second: a guard that alters a clean result is
over-firing, and `ADVERSARIAL-OP6.md` attack 3c refuted over-firing for the
metric table. Case A9 in `sweep_selftest.py` holds the behaviour, and was proven
against a copy with the guard removed rather than only against the passing file.

Findings 1 to 9 come from the first grids and the prefix-collapse series.
Findings 10 to 14 come from the warm grid that followed, which added the warm
continuation axis and the two factors the first grid could not carry. **Numbering
is continuous across the whole section and never restarts.** It restarted once,
when two operations appended to this section independently, and produced two
findings for every number from 5 to 9 while the reference checker reported zero
unresolved. Anything appended here takes the next free number.

**10. Every level of `-ub` improves one axis and worsens the other, and a grid
that reported one number per factor could not have seen it.** Second grid on the
hybrid model, 20 runs, five interspersed baselines, randomised order, no run
above the instability threshold, nothing excluded. Cold is a prompt the engine
has never seen; warm is the ReAct pair of finding 1, a 4214-token continuation.

| `-ub` | cold prefill | warm continuation TTFT | continuation recompute | hit |
|---|---|---|---|---|
| 128 | -43.8 % | **-39.5 %** | 203 tok | 95.2 % |
| 256 | -21.4 % | **-23.7 %** | 331 tok | 92.1 % |
| 512, default | baseline 1714 tok/s | baseline 477 ms | 587 tok | 86.1 % |
| 1024 | +14.0 % | **+47.5 %** | 1099 tok | 73.9 % |
| 2048 | +21.6 % | **+142.1 %** | 2119 tok | 49.7 % |

Every row disagrees with itself. The recompute column is finding 1's
`min(n_batch, 4 + n_ubatch) + 71` reproduced exactly in an independent campaign,
which is why the two axes move in opposite directions: a wider micro-batch
prefills faster and pushes the checkpoint restore point further from the end of
the prompt. Finding 4 reported the left column alone and called `-ub 2048` a 21
percent gain. On the shape an agentic loop actually runs it is a 142 percent
regression. Neither figure is wrong and neither is the answer.

**11. `-ctxcp` is a cliff, not a dial, and one checkpoint is enough.** At 1 and at
4 every warm figure is indistinguishable from the default 32: hit 86.06 percent,
587 tokens recomputed, TTFT within half a percent. At 0 the continuation
recomputes all 4214 tokens and TTFT rises 434.9 percent. Each checkpoint costs
62.8 MiB by finding 1, and 32 of them buy nothing this workload can detect over 1.

The same run answers a question finding 1 left open. At `-ctxcp 0` the speed
probe's resend of an **identical** prompt also came back cold, five samples of
five. Reuse on this model is a context checkpoint restore for every shape, not
only for a continuation with an appended suffix. There is no separate prefix
cache underneath it.

**12. `--cache-reuse` does nothing here, at any level.** 64, 256 and 1024 all
leave the continuation recomputing exactly 587 tokens with hit at 86.06 percent
and no detectable effect on any warm or cold metric. The flag governs reuse via
KV shifting, and finding 1 established that this model's reuse is a checkpoint
restore. It is a lever on a mechanism this architecture does not use for this
shape. The exclusion that kept it out of the first grid is spent, and the answer
it was hiding is a null result.

**13. `-cms 0` is pure cost when no trap is present.** Hit ratio and recompute are
unchanged; continuation prefill is 2.3 percent worse. Finding 1 measured the same
flag as a rescue, restoring 86.06 percent where a poisoned checkpoint had taken
reuse to 0.02. Both are true: it buys nothing when the checkpoint list is healthy
and it buys everything when it is not, so it is insurance whose premium is now
measured.

**14. The `-ub` penalty is entirely mediated by checkpoints, which the interaction
confirms and the arithmetic makes exact.** Predicted in the plan before the run:
with checkpoints disabled the warm cost of a wide micro-batch should disappear,
because the recompute it causes is the distance back to a restore point and there
would be no restore point.

| cell | continuation recompute | continuation TTFT |
|---|---|---|
| `-ub 256`, `-ctxcp 4` | 331 tok | -23.6 % |
| `-ub 2048`, `-ctxcp 4` | 2119 tok | +143.9 % |
| `-ub 256`, `-ctxcp 0` | 4214 tok | +577.7 % |
| `-ub 2048`, `-ctxcp 0` | 4214 tok | +341.3 % |

On recompute the prediction holds exactly: at `-ctxcp 0` both micro-batch widths
recompute 4214 tokens, the whole prompt, and the sixteenfold spread between them
collapses to nothing. On TTFT it does not, and the reason is the point. With no
checkpoint to restore, a continuation is a cold prefill, so `-ub` stops acting on
the restore distance and starts acting on prefill throughput, where wide is
faster. Its effect does not vanish; it **changes sign**, from +341 percent against
+578 percent. A prediction stated only as "the effect disappears" would have been
recorded as confirmed on one column and refuted on the other. It is written here
as measured on both.

## How to read every verdict in this section

Two properties hold across all fourteen findings and are not restated in each.

**Multiplicity is uncorrected.** The grids rendered 205 comparisons at a
per-comparison rate of 5 percent, correctly calibrated, with no family-wise
correction. Five to six of them are expected to be false effects on chance alone.
Two are flagged as the likeliest, neither cited by any finding: on the hybrid
grid, `n_ctx = 65536` warm decode at +2.5 percent against a 2.4 percent
threshold, non-monotone against both neighbouring levels; and on the hybrid
curve, `n_batch = 4096` at 4079 tokens, -0.1 percent against a 0.1 percent bar,
alone among that level's six lengths. A verdict that stands alone at one level of
one factor, without a mechanism, is more likely to be one of the expected false
positives than a discovery.

**"No detectable effect" is not "no effect", and where the difference bites it is
now stated in the finding itself.** Findings 4, 6 and 8 carry a power paragraph
naming which of their absences the experiment was too small to see. An absence
without a power statement beside it should be read as unquantified, not as a
measured zero. This is I11 applied to this section's own conclusions rather than
only to the sweeps that produced them.

Both properties come from `ADVERSARIAL-OP6.md`, which holds the full pass: six
attacks, three refuted as plainly as the three confirmed, and the reproduction
commands for each.

**Both rest on `sweep_nulltest.py`, which did not run when this was written.**
The adversarial pass ran it; the warm grid's rewrite of `METRICS` then gave each
speed metric an `id_contains` selector, the null suite's synthetic records carry
ids that match none of them, and `analyse` returned an empty analysis for every
replicate. It also unpacked two return values from a function that had grown a
third. Neither failure was silent in the output, and both survived anyway,
because nothing re-ran the file: the calibration endpoint printed "INSTRUMENT NOT
CALIBRATED" and no reader was there. Repaired here, it reproduces the figures
above rather than replacing them: false-positive rates of 4.1 to 5.8 percent
against a nominal 5 across fourteen configurations, and 80 percent power at 6.60,
6.97, 15.78 and 1.82 percent for the four thresholds findings 4, 6 and 8 quote as
6.5, 7.0, "caught 12 percent of the time" and 1.8. The numbers were right. The
tool that establishes them was not runnable, which is a different property and
the one this note exists to record.

---

# Part 2: the harness

## 2.1 Scripts

| File | What it does |
|---|---|
| `harness.py` | Shared library: one request into one contract-shaped sample, dispersion, provenance, the I1 to I13 checks, the campaign container |
| `speed_probe.py` | Cold and warm latency and throughput, two records, never one |
| `prefill_curve_probe.py` | Prefill rate as a function of prompt length, cold at every point, with a text plot |
| `cache_reuse_probe.py` | How much of a ReAct-shaped continuation the engine serves from cache |
| `prefix_collapse_probe.py` | The same continuation, with the slot's history as a declared parameter: an arbitrary request sequence, run before the pair, recorded verbatim in every record it produces |
| `prefix-collapse.sh` | Runs the conditions behind 1.12 finding 1, one server and one changed factor each, at `-lv 5` so the engine's own checkpoint decisions can be read against the records |
| `agentic_probe.py` | Drives real turns against the Apollia runtime and collects its turn records |
| `toolcall_probe.py` | Tool-calling correctness across a fixed task set, scored |
| `esrs_probe.py` | Extraction quality on a labelled sample, precision, recall, F1 |
| `quality_record.py` | Wraps the scored probes' payloads in contract-shaped records |
| `eval-model.sh` | Launches a server for one model, runs every probe, writes one campaign |
| `merge_campaign.py` | Joins the per-probe files into one campaign container |
| `run-matrix.sh` | Runs `eval-model.sh` across the shortlist, one model at a time |
| `sweep.py` | Varies one launch parameter at a time against a frozen baseline, in randomised order, and reports an effect or its absence. Runs the probes above as subprocesses; the one probe it owns is `concurrency`, which a sequential client cannot express |
| `sweep_selftest.py` | Regression cases for the sweep analysis, each one a way the report was once willing to state a conclusion the data did not support. No server, no model |
| `sweep_nulltest.py` | Drives `analyse` over synthetic data with no effect in it and tallies what it calls an effect anyway, then injects known effects and tallies what it catches. This is where the 5 percent per-comparison rate and every power figure in 1.12 come from. No server, no model |
| `plans/*.toml` | The experiment plans `sweep.py` reads: baseline, factors, levels, probes, repetitions |
| `aggregate.py` | Renders campaign files as a comparison table |
| `roofline.py` | Theoretical ceilings from the GGUF header and a machine table, and measured over ceiling when given a campaign file |
| `strip_defective_blocks.py` | One-shot: removes the `speed` and `batch` blocks from the pre-contract files |
| `check_references.py` | Asserts that every finding, section, invariant and path this document cites resolves. Run it after any merge into the contract; a concurrent amendment is how the citations broke the first time |

Every probe conforms to Part 1. Each writes records whose `invalid` array is
**computed** against I1 to I13 by `harness.check_invariants`, not assumed empty,
so a record that violates an invariant says so in the file rather than being
caught by a reader who happens to check.

`toolcall_probe.py` and `esrs_probe.py` keep their scoring logic unchanged. They
score rather than time, so neither the warm-cache defect nor the chunk-counting
defect ever applied to them; what they gained is a provenance block and a place
in the campaign container.

**Cold is obtained with `cache_prompt: false`, not by restarting the server.**
Restarting does not work: a fresh server still serves a warm request when the
prompt shares the chat template prefix, measured at 534 cached tokens on this
machine. Neither does varying the prompt, for the same reason. The engine's
`POST /slots/{id}?action=erase` answers 501 unless the server was launched with
`--slot-save-path`, so `conditions.slot_reset_before` records the real outcome
of an attempted reset rather than the probe's intent.

## 2.2 Running one model

```sh
REPS=5 CAMPAIGN_ID=baseline-<label> \
  bash scripts/model-eval/eval-model.sh <label> <path-to-first-shard.gguf>
```

Writes `results/<campaign_id>.json`, one campaign container holding every
probe's records. `REPS` defaults to 5, the contract minimum below which an
aggregate is provisional under I8.

**The probe order is part of the measurement.** `cache_reuse` runs first, against
a server that has answered nothing, for the reason `sweep.py` has enforced the
same order since 1.12 finding 1 landed: the prefill curve's 512-token point is a
request of `4 + n_ubatch + 1` tokens, which leaves a checkpoint the engine never
erases, and every continuation afterwards recomputes its whole prompt. Measured
after the curve, this model's reuse reads 1 cached token of 4214 rather than
3626, and nothing in the record says which of the two was measured except
`cache_n`. `results/baseline-qwen3.6-35b-a3b.json` is that mistake, kept: its
warm continuation is 0.02 percent and its warm first token 2587 ms against a cold
2552 ms. Any campaign file produced before this order was fixed carries the same
defect on a hybrid model, and none of them is comparable to one produced after.

The launch vector is the product's own: `-ngl 999 -c 32768 -np 1 -cb
--flash-attn on --jinja --reasoning-format none`. The pre-contract harness used
`-np 8 -c 16384`, which is a different machine as far as throughput is
concerned, and figures from the two are not comparable. That is why
`launch_args` is recorded verbatim in every record rather than annotated.

## 2.2b The agentic probe

```sh
APOLLIA_PERF_TRACE=/tmp/turns.jsonl apollia-os start     # in another shell
APOLLIA_PERF_TRACE=/tmp/turns.jsonl OUT=results/agentic.json \
  python3 scripts/model-eval/agentic_probe.py
```

The only probe that measures what a user experiences: it drives real turns
through the runtime rather than hitting `llama-server` directly, so its numbers
include prompt assembly, tool dispatch and persistence. It does not decompose
anything itself. The runtime writes one turn record per turn to the trace file,
already in the contract's shape, and the probe drives the turns and reads it.
A record arriving with a non-empty `invalid` is surfaced and listed in
`records_excluded`, never averaged into an aggregate.

## 2.3 Running the shortlist

```sh
bash scripts/model-eval/run-matrix.sh                   # all models
bash scripts/model-eval/run-matrix.sh qwen3-30b-a3b     # a subset
```

Models are declared in `run-matrix.sh` as `label | gguf glob | slots | context`.
A model whose GGUF is absent is skipped, not failed.

## 2.4 Ceilings and efficiency

```sh
python3 scripts/model-eval/roofline.py --gguf <path.gguf> --ctx 32768
python3 scripts/model-eval/roofline.py --matrix --measured results/<campaign>.json
python3 scripts/model-eval/roofline.py --validate-kv --matrix --ctx 4096,16384
python3 scripts/model-eval/roofline.py --matrix --json > results/roofline.json
```

The header is parsed directly, so nothing is loaded and a 20 GiB model costs a
few MiB of reads. Machine figures come from a table keyed by `sysctl -n
hw.model`, each entry citing its source into the record's `sources`; an unknown
machine is an error, never a silent default. `--validate-kv` launches
`llama-server` per model and context and compares the predicted allocation
against the reported one, exiting non-zero past a 1 percent disagreement.

Efficiency needs a campaign file holding `stats.prefill_tps.median` and
`stats.decode_tps.median` for the same label and `engine.n_ctx`. Records whose
context length differs are skipped rather than divided, and an absent
measurement yields `null`, not `0`.

## 2.5 Known divergences from the product configuration

**The launch vector no longer diverges.** `eval-model.sh` and `sweep.py` both
launch with `-ngl 999 -c 32768 -np 1 -cb --flash-attn on --jinja
--reasoning-format none`, which is what `LlamaServerConfig::default` produces in
`crates/apollia-runtime/src/llama_server/config.rs`. `sweep.py` builds its argv
in the same field order as the Rust `build_args`, so a record's `launch_args`
and a `llama.server.spawn.config` log line compare directly. The pre-contract
harness used `-np 8 -c 16384`, which is a different machine as far as throughput
is concerned; nothing in `results/` produced under that vector is comparable to
anything produced under this one, and `launch_args` is recorded verbatim in
every record rather than annotated for exactly that reason.

**The binary does diverge, and it is not fixed here.** Every campaign in
`results/` ran on the Homebrew `llama-server`, reported as `version: 9870
(2d973636e)` in each record's provenance. `packaging/fetch-llama-server.sh` pins
`LLAMA_CPP_TAG=b10092`, and `packaging/llama-server-checksums.txt` still holds
placeholder hashes, so the pinned build has never been fetched on this machine.
Measurements transfer to what the product ships only insofar as b9870 and b10092
agree, which nothing here has tested. `llama_server_version` is in the record so
the question is answerable later rather than forgotten; 1.4.9 anticipated this
in the "what it is not" column of that field.

**Two swept flags have no field in `LlamaServerConfig`.** `-ctxcp`
(`--ctx-checkpoints`) and `-cms` (`--checkpoint-min-step`) are the two levers on
the mechanism that serves a warm continuation on a hybrid model, per 1.12
finding 1, and neither exists as a named field on the Rust side. The runtime can
still reach them through `extra_args`, which it appends last, so a result here is
actionable; it is not settable as configuration. `sweep.py` carries them in
`NOT_IN_RUST` and every factor block that varies one says so, because a
recommendation to change one is first a recommendation to add the field.

**`--slot-save-path` is passed nowhere,** in the harness or in the runtime.
`POST /slots/{id}?action=erase` therefore answers 501, which is why cold is
obtained from `cache_prompt: false` and why `conditions.slot_reset_before`
records an outcome rather than an intent.

## 2.6 Sweeping a launch parameter

```sh
python3 scripts/model-eval/sweep.py plans/sweep-qwen3.6-35b-a3b.toml --dry-run
python3 scripts/model-eval/sweep.py plans/sweep-qwen3.6-35b-a3b.toml
python3 scripts/model-eval/sweep.py --report results/sweep-<plan>-<stamp>.json
python3 scripts/model-eval/sweep.py --ceiling-check
```

**Run nothing else on the machine.** Not a build, not a test suite, not another
agent. This is not a courtesy to the sweep, it is the difference between a
result and a wasted afternoon: the first full grid run under this harness was
invalidated by CPU work started alongside it, which halved throughput for the
first two runs and left a within-run cv of 0.46 where a clean run gives 0.01.
Both happened to be baseline runs, so the contamination set the dispersion every
threshold was derived from, and one factor's 45 percent gain was reported as no
detectable effect. `sweep.py` now refuses to compare against a baseline run whose
within-run cv exceeds the 0.10 of 1.4.8, so the failure is loud, but the runs
still have to be repeated. See finding 9 in 1.12.

Reads an experiment plan in TOML, varies one launch parameter at a time against
the plan's frozen baseline, and writes one campaign container holding every
run's records. `--dry-run` prints the whole run order with each configuration's
exact argv, the KV footprint per context length checked against the machine's
memory, any level beyond the model's trained context, and an estimated
wall-clock with its source stated. It spawns nothing.

Four properties are what separate this from a table of numbers.

**The probes are not reimplemented.** `speed_probe.py`, `prefill_curve_probe.py`
and `toolcall_probe.py` run as subprocesses, unchanged, through the environment
contract they already read. A level's number is comparable to the baseline's
only if the same code produced both, and a second producer of the same
measurement is the drift Part 1 exists to prevent. What `sweep.py` adds is the
context a probe cannot observe from inside one run, passed through
`harness.env_overlay`: `RUN_ORDER`, `PAGE_CACHE`, `ENGINE_EXTRA` and
`MODEL_SHA256`.

**The order is randomised and recorded.** Configurations are shuffled with a
seed carried in the dataset, so drift on a machine two hours into a run appears
as noise spread across the factors instead of as an effect belonging to whatever
ran last. Every record carries `run_order: "randomised"`, and I10 is checked
before any comparison is rendered: a dataset that was run sequentially is
refused, not annotated.

**Dispersion is measured between runs, and the bar is a threshold rather than
the dispersion itself.** The baseline is repeated several times at random
positions. A cv over five consecutive requests against a server that is already
running holds no restart and no thermal variance, and every level in a sweep
required both. The I11 verdict is decided against a detection threshold derived
from the between-run dispersion, not against the dispersion itself: comparing a
delta to the bare cv is a one-sigma test, which declares roughly a third of null
comparisons an effect. Both the cv and the threshold are printed. See finding 7
in 1.12, which states the change as a proposed clarification with its measured
evidence rather than as an amendment to 1.5.

**Every invariant is honoured, not only I8.** A record carrying any violation is
disqualified rather than compared, and the report names which. I9 is explicit
that a record with a non-zero temperature or an unfixed seed is inadmissible in
a speed comparison. A level whose own within-run cv exceeds the 0.10 of 1.4.8 is
reported as unstable and its median is not stated as fact. A record whose samples
disagree on `cache_state` enters no metric, and is listed rather than allowed to
disappear from a report that still counts its run as completed.

**Every factor is reported on two verdict axes and they are never merged.**
COLD PREFILL is the shape a benchmark measures, a prompt the engine has never
seen. WARM CONTINUATION is the shape an agentic loop runs, the same prefix again
with a tool result appended. They are different machines, and a flag can buy one
while selling the other. Where the two verdicts disagree in sign the report says
so, per level, and derives no single recommendation from them. `-ub` is the known
instance and the reason the rule exists: it buys cold prefill and, by finding 1's
mechanism, makes every continuation recompute `min(n_batch, 4 + n_ubatch)` more
tokens.

`warm_continuation` runs **first** in every run, and the order is load-bearing
rather than tidy. Finding 1: one request of exactly `4 + n_ubatch + 1` tokens
leaves a checkpoint that is never erased, and every continuation afterwards
recomputes its whole prompt. The prefill curve issues such a request at its
512-token point under the default `-ub`. A continuation measured after it would
report whether the trap fired, at a magnitude that swamps any launch flag.

**A continuation is selected by record name, never by cache state.** At
`-ctxcp 0` it serves 0 of 4214 tokens, so `cache_state` reads `"cold"` on both
halves of the pair and a selector keyed on cache state would silently swap the
continuation for the seed at exactly the level under test.

**Interactions are declared and reported separately.** Single factors run against
the frozen baseline first; a combined cell is a `[[interaction]]` in the plan,
shuffled into the same randomised order so it does not get a systematically
different machine, and printed in its own section so a two-flag cell cannot be
read as a one-flag verdict. The report states when one of the combined fields was
never varied alone, because then the cell has nothing to be read against.

**The prefill curve is compared per length.** A record's pooled `prefill_tps`
covers whatever lengths fitted the slot, so a level that changes the slot changes
the pool, and the difference is a change of x-axis wearing the appearance of a
change of rate. Each length gets its own baseline, dispersion and threshold, and
lengths present on only one side are named on both sides.

**A faster configuration that produces worse text is not a result.**
`harness` marks a sample `degenerate` when the output has collapsed into
repetition, and any record carrying one is reported as DEFECTIVE rather than as
a gain. That heuristic does not catch a model that has merely got worse, which
is what KV quantisation does, so a factor may declare `quality_gate = "toolcall"`
and every level of it additionally runs the scored tool-calling probe. A level
whose score fell cannot be reported as an improvement whatever its speed.

**`--cache-reuse` and the continuation probe are back in the sweep,** and the
narrower objection that kept them out has an answer rather than a waiver. That
objection was that a `--cache-reuse` level measured here would carry two effects
the plan cannot separate, the flag's own and whatever checkpoint state the
preceding level left behind, and that randomised order spreads such state across
the factors rather than removing it. The premise is that state survives from one
level to the next. It does not: `sweep.py` spawns one `llama-server` per run and
stops it afterwards, so a checkpoint list cannot outlive the level that created
it. There is no preceding state to spread.

What remains true of the objection is the part inside a run, and it is why
`warm_continuation` runs first. A continuation measured after the prefill curve
would sit behind that curve's 512-token point, which is the trap at the default
`-ub`. Running the continuation against a server that has answered nothing puts
the flag alone in the measurement. `l1-control` is the evidence that this is
enough: a fresh server and the pair repeated five times holds at 86.06 percent
with cv at or below 0.0012, so the probe does not poison itself.

The reuse question keeps its own experiment in `prefix-collapse.sh`, which
remains the right tool for varying history deliberately, a thing a grid should
not do. What the grid adds is the warm axis for every other factor, which is what
finding 8 argued for: `-ub` is a legitimate factor here and moves reuse there,
and until both were measured in one place that seam had to be crossed by hand.

The `-np` factor scales `-c` with the slot
count, because llama-server divides the context across slots and raising `-np`
at a fixed `-c` would conflate slot count with slot capacity; those levels are
measured with concurrent requests, since a single sequential client occupies one
slot and would report the flag as inert.

`--ceiling-check` needs no model load. It multiplies each model's measured cold
decode median by its `bytes_per_token_read` and prints the achieved effective
bandwidth against the machine's peak, which is what tells this programme whether
the efficiency figures it produces are trustworthy. See finding 4 in 1.12.

## 2.7 Reproducing the prefix reuse collapse

```sh
bash scripts/model-eval/prefix-collapse.sh                 # every condition
bash scripts/model-eval/prefix-collapse.sh l9-trap l9b-trap-cms0
```

One `llama-server` per condition, because the thing under test is a launch flag
and the thing being controlled is history. Each server runs at `-lv 5`, where the
engine prints the decisions behind a reuse: `main/do_checkpoint`, `checking
checkpoint with [a, b] against c`, `restored context checkpoint`, `forcing full
prompt re-processing`. The driver prints those counts next to each condition's
records, and a run whose log and records disagree is a defect in the probe rather
than a finding.

The probe takes the history as a parameter. `SEQUENCE` is a comma-separated list
executed once per repetition, `PREAMBLE` the same grammar run once before any
measurement, and both are written verbatim into `conditions.notes` of every
record, so no claim can be read without the requests that produced it:

| step | what it issues |
|---|---|
| `pre:<tokens>:<cold\|warm>` | one intervening request of that many tokens |
| `trap` | one request of exactly `4 + n_ubatch + 1` tokens, the length that leaves an unerasable checkpoint, built by measuring the chat template's own cost rather than assuming it |
| `pair` | the ReAct shape: a cold seed, then the same prefix plus a short appended result |

`trap` is exact where `pre` is approximate. `filler_for_tokens` converges to
within two percent, which is enough for a curve whose x-axis is the measured
length and not enough here: at `pre:512` the prompt lands anywhere from 517 to
521 tokens, and only 517 arms the trap. The step therefore pads a token at a
time to an exact count and asserts the length back off the response, raising
rather than measuring a different experiment. On Ministral it cannot be built at
all, since the chat template alone costs 535 tokens; the probe says so and stops.
That model needs no trap, having created no checkpoint in any run.

The `-ub` levels are `l5a`, `l5`, `l5b` and `l5c`, at 128, 256, 1024 and 2048
against the default 512. They are a line rather than a point because the
recompute a warm continuation pays is a function of `n_ubatch`, and one level
cannot show a function; the clamp at `n_batch` only appears at the top of the
range.

The conditions and their outcomes are in 1.12 finding 1. Three of them are worth
knowing before reading any warm figure from this corpus: `-ctxcp 0` takes reuse
on the hybrid model to zero, one 517-token request takes it to one token in 4214
for the rest of the process, and `-ub 2048` halves it on every iteration.
