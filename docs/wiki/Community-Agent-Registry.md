# Community Agent Registry

The community agent registry allows third-party developers to distribute Worker
Agents that extend Apollia OS beyond the four built-in agents.

V1 of the registry is a **local directory** (`agents/community/`).  A future
version (V2) will support installation directly from a Git repository URL.

---

## Table of contents

1. [Format of a community agent](#format)
2. [Installation and validation](#installation)
3. [Contribution guide](#contributing)
4. [Reference agents](#reference-agents)

---

## Format of a community agent {#format}

A community agent consists of a single Python file placed in `agents/community/`.
The file must satisfy the Apollia AIP (Agent Interface Protocol) duck-typing
contract and carry a conformant `manifest()`.

### File layout

```
agents/community/
├── my-agent.py          ← Agent source — mandatory
└── README.md            ← Registry index and contribution guide
```

The test suite lives in the shared `agents/tests/` directory:

```
agents/tests/
└── test_my_agent.py     ← Pytest tests — at least one error-case test required
```

### AIP contract

The Python file must expose a module-level variable named `agent` whose class
implements:

| Method | Signature | Required |
|---|---|---|
| `manifest()` | `() → dict` (synchronous) | Yes |
| `run()` | `async (task, ctx) → dict` | Yes |
| `on_start()` | `async () → None` | No |
| `on_stop()` | `async () → None` | No |

### Manifest fields

The dict returned by `manifest()` must include at minimum:

```python
{
    "name":           "my-agent",       # unique, kebab-case
    "version":        "0.1.0",          # semver
    "description":    "...",
    "tools_required": ["bash_executor"],
    # Optional — declare explicitly if needed:
    "dangerous_tools_allowed": False,
}
```

If `dangerous_tools_allowed` is `True`, the installer displays a security
warning and asks for operator approval before proceeding.

---

## Installation and validation {#installation}

### Install from a local path

```bash
apollia-os agent install agents/community/sql-worker.py
apollia-os agent install ./path/to/my-agent.py
```

### Validation steps

The installer performs the following checks in order:

1. **File exists** — the path must point to an existing `.py` file.
2. **AIP contract** — `manifest()` is callable and `run()` is an async
   coroutine function.
3. **Manifest conformance** — required fields (`name`, `version`,
   `tools_required`) must be present.
4. **Security check** — if `dangerous_tools_allowed: True`, a warning is
   displayed.  Installation is not blocked, but operator acknowledgement is
   expected.
5. **Test suite** — `python3 -m pytest agents/tests/test_<name>.py` is
   executed.  A non-zero exit code blocks the installation.

### Skip the test suite

Pass `--skip-tests` to bypass step 5 (not recommended):

```bash
apollia-os agent install ./my-agent.py --skip-tests
```

A warning is displayed when this flag is used.

### Uninstall

```bash
apollia-os agent uninstall my-agent
```

---

## Contribution guide {#contributing}

### Criteria for acceptance

A community agent must satisfy **all three** of the following criteria:

1. **Non-trivial sequence** — the agent performs a domain-specific multi-step
   workflow (connect → inspect → query → format, for example).  A wrapper
   around a single tool call is not a Worker Agent.

2. **Hardcoded domain guardrails** — at least one safety rule must be encoded
   in the agent source code (not only in `SYSTEM_PROMPT`).  Examples:
   - SQL injection prevention via parameterised queries
   - Blocking of destructive Git commands

3. **Test suite** — a `agents/tests/test_<name>.py` file that covers at least
   one error case (invalid input, missing file, permission denied, etc.).

### Validation checklist

Before submitting a community agent, verify:

- [ ] `manifest()` returns a conformant dict (`name`, `version`,
  `tools_required`, `description`).
- [ ] `dangerous_tools_allowed` is declared **explicitly** when needed (do not
  omit it and rely on the default).
- [ ] At least one domain guardrail is present in the source code.
- [ ] `pytest agents/tests/test_<name>.py` exits with status 0.
- [ ] `apollia-os agent install agents/community/<name>.py` succeeds on a
  clean install.

See `docs/adr/ADR-050.md` for the full distribution strategy and V2 roadmap.

---

## Reference agents {#reference-agents}

The following agents ship in `agents/community/` as canonical examples:

### sql-worker

| Field | Value |
|---|---|
| File | `agents/community/sql-worker.py` |
| Skills | `query-sql`, `schema-inspect`, `data-export` |
| Required tools | `python_executor`, `file_read` |
| External packages | none (Python stdlib `sqlite3`) |
| `dangerous_tools_allowed` | `False` (mutations require explicit opt-in) |

Guardrails coded in the agent:

- SELECT-only by default — INSERT/UPDATE/DELETE/DROP are blocked unless
  `dangerous_tools_allowed: True`.
- Parameterised queries only — f-string interpolation in SQL is forbidden.
- 30-second query timeout.
- File existence and `PRAGMA integrity_check` on first connection.
- Connection closed via context manager `with`.

Install:

```bash
apollia-os agent install agents/community/sql-worker.py
```

### git-worker

| Field | Value |
|---|---|
| File | `agents/community/git-worker.py` |
| Skills | `git-status`, `git-diff`, `git-commit` |
| Required tools | `bash_executor`, `file_read` |
| External packages | none (delegates to system `git`) |
| `dangerous_tools_allowed` | `False` |

Guardrails coded in the agent:

- Destructive commands are refused: `git push --force`, `git reset --hard`,
  `git clean -fd`, `git branch -D`, `git checkout -- .`
- Commit messages must follow the Apollia conventional format:
  `type(scope): description`.
- `git status` is always executed before any `git add` or `git commit`.
- Remote operations (`push`, `pull`, `fetch`) require explicit user approval.

Install:

```bash
apollia-os agent install agents/community/git-worker.py
```
