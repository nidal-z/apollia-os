# crates/apollia-core/AGENTS.md

> Local rules for `apollia-core`. Read after the root `AGENTS.md` and before
> editing this crate. Pair with `docs/agents/RUST-PATTERNS.md`.

Every crate of the workspace depends on this one, and this one depends on no
other crate of the workspace. That is the whole reason it has its own rulebook:
the global corpus says "ask first before modifying a public API in
`apollia-core`" and stops there, while five of the rules below exist because a
change here broke something two crates away.

Written because the root rule asks for one when a subtree passes 500 lines and
carries patterns the global rulebook does not cover. This crate is 14 594 lines
under `src/`, and the four sections below are its own.

---

## 1. What belongs here, and what does not

A type belongs to `apollia-core` when two crates need it and neither should
depend on the other. `AgentManifest`, `AIPTask`, `AIPResult`, `RuntimeEvent`,
`StepBudgetConfig`, `RuntimeConfig` are the shape.

A type does not belong here because it is "shared-looking". Behaviour belongs
to the crate that owns the actor: the engine is `apollia-oria`, the tool
dispatch is `apollia-tools`, the mesh is `apollia-runtime`. This crate carries
types, their serialization, their validation and the pure helpers that go with
them.

`src/net.rs` is behind the `net` feature. Everything else is unconditional; a
new optional module states why in the same commit.

---

## 2. `~/.apollia` has one catalogue

`paths::DataFile` is the single source for the name of every database at the
root of the data directory, through `file_name()` and `path(data_dir)`. Adding
a store means adding a variant, never joining a literal onto the data
directory: `scripts/check_data_layout.py` refuses the literal, and it exists
because the layout was in eight places at once.

The same applies to the directories: `paths::data_dir`, `paths::socket_path`
and their `_or_temp` fallbacks are the only ways to reach the user's home from
production code. A fallback into `std::env::temp_dir` is a place to fail
visibly, not a place to keep state, and the doc-comment on each of them says
which case it serves.

---

## 3. A schema is versioned, and a persisted type is total

**SQLite.** `schema::open_versioned(conn, name, version, migrations)` is how a
store opens. It reads `PRAGMA user_version`, refuses a file written by a newer
binary (`SchemaError::NewerThanBinary`), refuses a declared version that does
not match the number of migrations supplied
(`SchemaError::MigrationCountMismatch`), and applies what is missing. A schema
change appends a `Migration` and bumps the version; it never edits one that
shipped. `scripts/check_sqlite_schema_versioning.py` holds the rule across
`crates/`.

**serde.** A type that is written to disk must read back from a file an older
binary wrote. Every field added after the first release carries
`#[serde(default)]` or a `default_*` function, and
`scripts/check_serde_persisted_defaults.py` refuses one that does not. A
rename carries `#[serde(alias = "...")]` for the old spelling;
`StepBudgetConfig::wall_clock_secs` keeps `wall_clock_timeout_secs` that way.

---

## 4. `RuntimeEvent` is a wire format

Adding a variant is a wire-format change, and so is removing one. The rules
that follow the variant live in `crates/apollia-runtime/AGENTS.md` section 2;
what belongs here is the shape:

- Past-tense variant names, typed fields, never a blob `String`.
- `events::subscribe_resilient` (and `resilient`) is where the `Lagged(n)`
  policy lives. Do not restate it at a call site.
- `scripts/check_eventbus_variants.py` crosses every variant against the code
  that emits it and the code that reads it, so a variant nothing publishes and
  a variant nothing consumes are both reported.

---

## 5. Forbidden in this crate

- A dependency on another crate of the workspace. This one is the floor.
- `unwrap()`, `expect()`, `panic!()` outside tests, as everywhere.
- A public item no other crate uses. `scripts/check_pub_unreferenced.py`
  reports it, and the answer is to make it private, not to add a caller.
- Building an HTTP client by hand. `net::` owns the client, the redirect
  policy and the body caps; `scripts/check_http_clients.py` refuses a second
  one.
- Reaching the home directory by string composition. Use `paths::` and
  `PathBuf::join`.

---

## 6. When the rules block you

- A type only one crate uses : it belongs in that crate. Moving it out of here
  is always cheaper than the reverse.
- A breaking change to a public type : the root `AGENTS.md` puts it under ASK
  FIRST, and the decisions chapter of `docs/site/` records the outcome before
  the code lands.
- A new persisted store : write the `DataFile` variant, the versioned schema
  and the `#[serde(default)]` fields in the same commit as the first write.
