# crates/apollia-llm/AGENTS.md

> Local rules for the LLM layer. Read after the root `AGENTS.md` and before
> editing this crate. Pair with `docs/agents/OBSERVABILITY.md` for what a call
> has to publish.

`apollia-llm` holds the cloud clients, the router that picks a backend per
call, the retry policy every backend shares, the JSON-Schema sanitizer that
keeps tool calling working against a local server, and the meta routines. It is
16 496 lines under `src/`, and the four rules below are its own.

Local inference does not live here. The llama-server sidecar is
`apollia-runner`, and the daemon reaches it through `RunnerLlmBackend` in
`apollia-runtime`. A change to local inference belongs in one of those two.

---

## 1. A backend is reached through the router, never directly

`LlmRouter` (`src/router.rs`, with `src/router/` for the parts) resolves which
backend answers a call, from the agent's declared routing and the runtime
configuration. It is a plain struct, not an actor: there is no channel and no
mailbox, and it is called synchronously from whoever holds it.

Callers use `complete`, or `complete_with_observability` when an `EventBus` is
available, so the call is published rather than silent. A caller that reaches a
`backends::` client by hand loses the routing, the retry and the record at
once.

---

## 2. One retry policy, and it is cancellable

`RetryPolicy` (`src/retry.rs`) is shared by every backend: exponential delay
`base_delay_ms * 2^attempt`, the exponent clamped, and a `CancellationToken`
that ends the wait. A backend that grows its own retry loop makes the total
latency of a call unknowable, because the two loops multiply.

`ResilienceLayer` in `apollia-oria` is a different thing on a different axis
(circuit breakers per tool). The two are not alternatives and neither replaces
the other.

---

## 3. Tool schemas are sanitized before they reach a grammar

`src/schema_sanitize.rs` normalizes the JSON Schema of a tool's parameters
before an OpenAI-compatible request carries it. A llama.cpp server builds a
constraining grammar from that schema, and several ordinary constructs drive
the converter into an expansion it then refuses with `400 failed to parse
grammar`: a large `maxLength` on a string nested in an array item is the
observed trigger, and `oneOf` / `anyOf` / `allOf` / `$ref` are unsupported.

A new tool parameter shape goes through the sanitizer, and the rejection is
covered by a test that carries the offending schema. Debugging this from the
outside costs hours: the failure is a 400 from the server with no mention of
which tool caused it.

---

## 4. A meta prompt is a resource, not a document

`prompts/meta/*.md` are pulled in by `include_str!` from
`src/meta_orchestrator.rs`. `include_str!` resolves at compile time, so
deleting one is a compilation error, and rewording one changes model behaviour
with no test to catch it. `docs/agents/FORBIDDEN.md` states the rule for the
whole tree; this is the crate where it bites.

Adding a routine means the variant, the `include_str!` line and the prompt
file, in one commit.

---

## 5. A call is recorded

`LlmCallRepository` (`src/repository.rs`) persists every call into `llm_calls`,
fed by the subscriber `spawn_llm_subscriber`. Prompt and completion text are
not stored; what is stored is the shape of the call, its cost and its timing.
Do not add the text: the local-first principle is that a transcript stays where
the user put it.

`src/pricing.rs` carries the per-model cost table used to turn tokens into a
number the operator sees. A model added without a pricing entry reports zero
cost, which reads as free rather than as unknown.

---

## 6. Forbidden in this crate

- A second HTTP client. `http_client.rs` wraps `apollia_core::net`, which owns
  the redirect policy and the body caps;
  `scripts/check_http_clients.py` refuses another one.
- A retry loop inside a backend.
- A tool schema handed to a request without passing the sanitizer.
- `unwrap()`, `expect()`, `panic!()` outside tests.
- Local inference code. It belongs to `apollia-runner`.

---

## 7. When the rules block you

- A backend needs a parameter the others do not have : it goes on the request
  type with a default, so the routing stays uniform.
- A prompt needs to change : change the `.md` file, and say in the commit what
  behaviour you expect to move. Nothing else will tell the next reader.
- A model is missing : `src/model_defaults/` and `src/pricing.rs` are the two
  places, and both are read at runtime.
