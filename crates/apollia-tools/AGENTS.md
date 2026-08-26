# crates/apollia-tools/AGENTS.md

> Local rules for the tool layer. Read after the root `AGENTS.md` and before
> editing this crate. Pair with `docs/agents/SECURITY.md`: most of what follows
> is a trust boundary, not a style preference.

`apollia-tools` is where an agent's intention becomes an effect on the machine:
a shell command, a file written, an HTTP call, a Python process. It also owns
the registry that catalogues those tools, the audit trail that records every
invocation, and the SQLite repositories for agents, packages, projects and
tasks. It is 24 439 lines under `src/`, and every rule below was paid for.

---

## 1. A tool is a descriptor plus an executor

A native tool is a module under `src/tools/` exposing a struct with a
`descriptor()` returning a `ToolDescriptor`, and an implementation of the
`ToolExecutor` trait. `ToolRegistry` catalogues them, `ToolResolver` validates
at `INITIALIZING` that an agent's declared tools exist, and `ToolDispatcher`
gives one JSON dispatch path for native and MCP tools alike.

The descriptor is aligned with the MCP tool schema on purpose: a native tool
and an MCP tool reach the model through the same shape, so a caller never
branches on which one it is.

---

## 2. Arbitrary-code executors are never blanket-authorized

`bash_executor` and `python_executor` execute what the model wrote. They are
named in `apollia_permissions::CODE_EXECUTOR_TOOLS`, and the invariant is that
neither can be authorized by name for a whole session: each invocation keeps
its own human decision. The matching side lives in `apollia-permissions`
(`is_code_executor`, the prefix rule engine), the persisting side in
`apollia-runtime`'s chat manager, and the crossing between the two is a test in
this crate: `code_executor_descriptor_names_stay_in_sync` asserts that both
descriptor names are still members, so renaming a descriptor cannot silently
drop it out of the guard.

Adding an arbitrary-code executor means adding its name to
`CODE_EXECUTOR_TOOLS` and extending that test in the same commit.

---

## 3. A path is validated against the sandbox, every time

`sandbox_path` is the single validator: it canonicalizes and re-checks that the
result is still under the agent's root, which is what closes the symlink escape
that a prefix comparison alone does not. Every filesystem tool goes through it,
and a new one that composes a path by string is a traversal waiting to be
found.

`SandboxProfile` carries the isolation the platform can actually provide.
Namespace isolation through `unshare(1)` is Linux-only; on macOS and Windows
there is no OS sandbox behind the shell tool, and the rule that stands in its
place is the per-invocation human decision of section 2. Do not write a comment
or a document that implies otherwise.

---

## 4. A mutation is journalled before it happens

`journal` is a per-session Tokio mpsc actor that writes a JSONL entry and
fsyncs it *before* the filesystem mutation proceeds. The order is the whole
point: a mutation recorded after the fact is unrecoverable if the process dies
between the two. A new mutating tool takes the `JournalWriterHandle` and
follows the same order.

`audit` is the other ledger and answers a different question: what the agent
invoked, with what, and what came back. It is a mpsc actor over SQLite with a
channel of 1024. Both are append-only.

---

## 5. `unsafe` is allowed here, and only here, with a reason

This crate's manifest carries `unsafe_code = "allow"`, which the workspace
denies everywhere else. Two production sites justify it, both in
`src/tools/rlimits.rs`: the `libc::setrlimit` calls that bound a child
process, and the pre-exec closure that runs between `fork()` and `execve()`
where only async-signal-safe calls are legal. Every site carries its
`// SAFETY:` comment, and `scripts/check_rust_rules.py` refuses one that does
not.

A third site needs a reason written before the code, not after.

---

## 6. Forbidden in this crate

- A filesystem tool that does not go through `sandbox_path`.
- A mutation written before its journal entry is durable.
- Blanket-authorizing an arbitrary-code executor by name.
- A second HTTP client. `http_fetch` goes through `apollia_core::net`, which
  owns the redirect policy, the private-address check on every hop and the
  body caps.
- `unwrap()`, `expect()`, `panic!()` outside tests, and `unsafe` without a
  `// SAFETY:` comment.

---

## 7. Testing

- A tool that touches the shell is skipped gracefully where the platform
  cannot run it: on a Linux runner without `CAP_SYS_ADMIN`, `unshare --pid
  --mount` fails with `EPERM`, and a test that asserts through it fails for a
  reason that has nothing to do with the code.
- A tool that touches the filesystem uses `tempfile`, never the real home.
- Every rejection has a test in both directions: the traversal refused, and the
  legitimate path still allowed.
- GIVEN / WHEN / THEN, as everywhere.

---

## 8. When the rules block you

- A tool needs a capability the sandbox refuses : the answer is a narrower
  capability, not a bypass. Widening the sandbox is a decision recorded in the
  decisions chapter of `docs/site/`.
- A tool needs to run longer than the budget allows : the budget belongs to
  `apollia-oria` and is not negotiable from here.
- A repository needs a new table : the versioned schema layer of
  `apollia-core` is how it opens, and
  `scripts/check_sqlite_schema_versioning.py` holds it.
