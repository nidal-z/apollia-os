# Community Agents

This directory contains Worker Agents contributed by the community or serving
as reference implementations. Unlike bundled agents, community agents are not
installed automatically — they must be installed explicitly:

```bash
apollia-os agent install agents/community/sql-worker.py
apollia-os agent install agents/community/git-worker.py
```

## Agents in this directory

| Agent | Domain | Packages |
|---|---|---|
| `sql-worker.py` | SQLite queries (SELECT-only by default) | none (stdlib) |
| `git-worker.py` | Git operations (status, diff, commit) | none (uses bash_executor) |

## Contributing a community agent

A community agent must satisfy three criteria to be accepted:

1. **Non-trivial sequence**: the agent performs a domain-specific multi-step workflow
2. **Domain guardrails**: at minimum one safety rule hardcoded in the source (not only in the system prompt)
3. **Tests**: a `tests/test_<name>.py` file with at least one error-case test

### Required structure

```
agents/community/
└── my-agent.py          # Agent source — manifest() + run() async
```

### Validation checklist

- [ ] `manifest()` returns a conformant dict (name, version, tools_required, skills)
- [ ] `dangerous_tools_allowed` is declared explicitly if needed
- [ ] At least one domain guardrail in code (not only in system_prompt)
- [ ] `pytest agents/tests/test_<name>.py` passes

See `docs/adr/ADR-050.md` for the full distribution strategy.
