# NAMING

> Naming conventions across Rust, Python, files, branches, ADRs, events,
> tracing fields, and HTTP routes. Read this before introducing any new name.

Consistent names are the cheapest form of documentation. They survive
refactors, translate cleanly to other languages, and let LLMs predict the
right symbol on first try.

---

## 1. Rust

Source : RFC 430 (naming conventions), RFC 1574 (rustdoc), the
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).

### Cases

| Item | Case | Example |
|---|---|---|
| Types, traits, enums | `UpperCamelCase` | `AgentManifest`, `Iterator` |
| Functions, methods, modules, variables | `snake_case` | `parse_args`, `agent_id` |
| Constants, statics | `SCREAMING_SNAKE_CASE` | `MAX_STEPS`, `DEFAULT_PORT` |
| Type parameters | Single uppercase letter or short `UpperCamelCase` | `T`, `K`, `Out` |
| Lifetimes | Short lowercase | `'a`, `'src` |

Acronyms are one word : `Uuid`, `HttpClient`, `Stdin`. Never `UUID`,
`HTTPClient`, `StdIn`.

### Conversion prefixes

| Prefix | Cost | Ownership |
|---|---|---|
| `as_<T>` | free, ref to ref | borrowed |
| `to_<T>` | non-trivial, owned output | consumes nothing |
| `into_<T>` | free, owned output | consumes self |
| `from_<T>` | constructor | takes ownership |
| `try_from_<T>` | constructor that may fail | returns `Result` |

Examples : `str::as_bytes`, `Path::to_string_lossy`, `Vec::into_iter`,
`PathBuf::from`.

### Getters

No `get_` prefix. The bare noun is the getter.

```rust
// WRONG
fn get_name(&self) -> &str

// RIGHT
fn name(&self) -> &str
```

Exception : `get` is acceptable in indexer-style APIs where lookup may fail
(`HashMap::get`).

### Iterators

| Method | Yields | Receiver |
|---|---|---|
| `iter` | `&T` | `&self` |
| `iter_mut` | `&mut T` | `&mut self` |
| `into_iter` | `T` | `self` |

### Crate names

- Prefix : `apollia-*` for every workspace member.
- Never suffix with `-rs` or `-rust`.
- Kebab-case.

### Apollia-specific type suffixes

| Suffix | Role | Example |
|---|---|---|
| `*Manifest` | declarative spec of an agent or tool | `AgentManifest`, `ToolManifest` |
| `*Engine` | stateful component owning a domain | `MemoryEngine`, `OriaEngine` |
| `*Backend` | swappable implementation behind a trait | `LlmBackend`, `SttBackend` |
| `*Provider` | source of data injected on demand | `ContextProvider` |
| `*Manager` | coordinator over a pool of resources | `McpClientManager` |
| `*Context` | scoped capability bundle | `RuntimeContext` |
| `*Registry` | name-to-entity lookup | `ToolRegistry`, `AgentRegistry` |
| `*Router` | dispatch over multiple backends | `LlmRouter` |
| `*Handle` | clonable handle to a Tokio actor | `EventBusHandle` |
| `*Id` | newtype identifier | `AgentId`, `TaskId` |

### Newtype identifiers

`AgentId`, `TaskId`, `SkillId`, `StepId`, `SessionId`, `RunId`. Each is
`struct Xxx(String)` with `Display`, `From<&str>`, `From<String>`, `AsRef<str>`,
`PartialEq`, `Eq`, `Hash`. Source pattern :
`crates/apollia-core/src/events/`.

### Enum error variants

Descriptive nouns, no `Err` prefix. Prefer specificity over brevity.

```rust
// WRONG
enum MyError { Err1, ErrFoo, ErrNotFound }

// RIGHT
enum MyError { Io(...), ParseFailed { .. }, AgentNotFound { id: AgentId } }
```

---

## 2. Python

Source : PEP 8.

| Item | Case | Example |
|---|---|---|
| Functions, methods, variables, modules | `snake_case` | `send_email`, `email_triage` |
| Classes | `UpperCamelCase` | `EmailTriage`, `AgentError` |
| Constants | `SCREAMING_SNAKE_CASE` | `DEFAULT_TIMEOUT` |
| Type parameters (PEP 695) | `UpperCamelCase`, single letter ok | `T`, `Out` |
| Private | leading underscore | `_internal_helper` |

Module names are short, lowercase, no underscore when possible
(`schemas.py`, not `agent_schemas.py`). Underscore is allowed when omitting
it produces an unreadable run-on.

TypedDict canonical schemas live in `<agent>/schemas.py`.

Test files : `test_<module>.py`. Test functions : `test_<scenario>`.

---

## 3. Events and tracing

### EventBus event types

Past-tense verb in `UpperCamelCase`. The event describes something that has
happened.

```rust
enum RuntimeEvent {
    AgentStarted { id: AgentId, at: SystemTime },
    AgentCrashed { id: AgentId, error: String },
    TaskCompleted { id: TaskId, duration_ms: u64 },
    MemoryRecallFailed { agent: AgentId, query: String },
}
```

### Tracing static messages

Lowercase, dot-separated, in `domain.action[.qualifier]` form.

```
agent.started
agent.crashed
task.completed
memory.recall.failed
tool.invoked
mcp.connect.timeout
```

### Tracing field names

Stable workspace-wide. Adding a new one is a structured-logging schema
change. Document it in `docs/agents/OBSERVABILITY.md`.

| Field | Type | Meaning |
|---|---|---|
| `agent_id` | `String` | `AgentId` value |
| `task_id` | `String` | `TaskId` value |
| `skill_id` | `String` | `SkillId` value |
| `step` | `u64` | step counter inside a run |
| `tool_name` | `&str` | name of the tool invoked |
| `duration_ms` | `u64` | elapsed milliseconds |
| `bytes_read` / `bytes_written` | `u64` | data volume |
| `error_kind` | `&str` | classifier on errors |
| `trace_id` / `span_id` | `String` | for OTLP exports |

---

## 4. HTTP and JSON

- Route style : `resource/verb`, singular, lowercase.
  Examples : `/agent/list`, `/task/read`, `/tool/invoke`.
- JSON field naming : `camelCase` on the wire, achieved with
  `#[serde(rename_all = "camelCase")]`.
- Timestamps : `i64` Unix seconds. Field name ends in `_at`
  (`created_at`, `expires_at`).
- Booleans : `is_*`, `has_*`, `can_*`. Never `flag_*`.

---

## 5. Files and directories

- Documentation files : `Kebab-Case-Or-Title-Case.md`
  (`Architecture-Vue-Ensemble.md`, `DESIGN-SYSTEM.md`).
- Python files : `snake_case.py`.
- Rust files : `snake_case.rs`.
- Config files : their canonical name (`Cargo.toml`, `pyproject.toml`,
  `.editorconfig`, `clippy.toml`).
- Shell scripts : `kebab-case.sh`.

Never accents, spaces, or non-ASCII in filenames.

---

## 6. Git branches and tags

| Prefix | Use |
|---|---|
| `feat/` | new feature |
| `fix/` | bug fix |
| `refactor/` | restructuring without behavior change |
| `docs/` | documentation only |
| `chore/` | tooling, deps, build |
| `test/` | tests only |
| `perf/` | performance |
| `release/` | release branch |

Branch body : kebab-case description (`feat/agents-md-spec`,
`fix/oauth-refresh-leak`).

Tags : `vMAJOR.MINOR.PATCH`, strict SemVer (`v0.1.0`, `v0.1.0-preview`).

---

## 7. ADRs and stories

- ADR filenames : `ADR-NNN-kebab-title.md` in `docs/adr/`.
  Example : `ADR-023-sdk-agentkit-design.md`.
- Numbered globally, never reused.

---

## 8. Cargo features

- Kebab-case (`web-search`, `brave-search`, not `web_search`).
- Always additive : enabling a feature only adds capability, never removes.
- `default = [...]` is documented in the `Cargo.toml` immediately above.

---

## 9. Test naming

### Rust

```rust
#[test]
fn test_<unit>_<scenario>_<expected>() { ... }

// Examples
#[test]
fn test_parse_args_invalid_socket_returns_err() { ... }

#[tokio::test]
fn test_eventbus_publish_blocks_when_capacity_full() { ... }
```

### Python

```python
def test_<unit>_<scenario>() -> None: ...

# Examples
def test_triage_high_priority_marks_urgent() -> None: ...

@pytest.mark.asyncio
async def test_email_send_retries_on_transient_error() -> None: ...
```

---

## 10. When introducing a new name

1. Check this file. If unsure, mirror an existing pattern in the same crate.
2. If you are adding a new tracing field, also add it to the table in
   `docs/agents/OBSERVABILITY.md`.
3. If you are adding a new type suffix not listed in §1, propose it in the
   PR description and update this file.
