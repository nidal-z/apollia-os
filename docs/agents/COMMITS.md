# COMMITS

> Conventional commit conventions for Apollia. Read this before every commit.

We follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/)
with a strict scope policy and a small set of repo-specific rules.

---

## 1. Format

```
<type>(<scope>): <subject>

<body, optional, wrap at 72>

<footer, optional>
```

Example :

```
feat(apollia-runtime): add EventBus capacity validation

Capacity outside [64, 65536] now returns ConfigError::InvalidCapacity at
construction time instead of silently truncating in the broadcast channel.

```

---

## 2. Types

| Type | Use |
|---|---|
| `feat` | new user-facing behavior |
| `fix` | bug fix |
| `refactor` | restructuring without behavior change |
| `test` | adding or fixing tests only |
| `docs` | documentation only |
| `chore` | tooling, dependencies, build, CI |
| `perf` | measurable performance change |
| `build` | build system, packaging |
| `ci` | CI pipeline |
| `revert` | reverting a previous commit |
| `style` | formatting, no semantic change |

Pick exactly one. If you need two, split the commit.

---

## 3. Scopes

The scope is the workspace member or domain that owns the change.

**Crate scopes** : the full crate name without abbreviation.

```
feat(apollia-core): add StepBudget type
fix(apollia-runtime): propagate skill_id in AIPTask
refactor(apollia-permissions): inline SafeList check
test(apollia-cli): add parsing tests for agent logs
```

**Documentation scopes** :

```
docs(book): chapter 8 on declarative orchestration
docs(wiki): update Architecture-Principes
docs(site): document the multi-runner sidecar
docs(help): operator article for OAuth setup
```

**SDK scope** :

```
feat(sdk): add @on_message decorator
fix(sdk): TypedDict required_keys handling
```

**Cross-cutting / repo-level** :

```
chore: bump Tokio to 1.40
ci: pin ubuntu-22.04 for Linux x86 builds
build: enable cargo workspace.lints
```

A commit that legitimately spans multiple crates carries no scope (just
`feat:` / `fix:` / etc.). This should be rare. Most multi-crate commits
are actually two commits.

---

## 4. Subject line

- Imperative, present tense : `add`, `fix`, `update`, never `added`,
  `fixed`, `updated`.
- No trailing period.
- Max 72 characters.
- No emoji, no decoration.

Bad : `Added new feature for the user.`
Good : `add user-facing flag for --json output`

---

## 5. Body

Optional. When present :

- Wrap at 72 characters.
- Explain the **why**, not the what. The diff shows the what.
- Reference the constraint, incident, or ADR that motivates the change.

Example body :

```
The previous mpsc::channel size of 64 was hitting backpressure under
sustained CLI batch invocations (apollia agent list --json piped through
jq). 256 absorbs the burst without measurable memory cost.

Reproduced with `tests/cli/cli-e2e.sh --stress`.
```

---

## 6. Footers

Used for cross-references and breaking changes.

```
Refs #142
Closes #237
BREAKING CHANGE: SecretStore::read now returns Option<Secret>
```

Multiple footers allowed. One per line.

Breaking changes : either `!` after the scope (`feat(apollia-core)!: drop
AgentId from String`) or a `BREAKING CHANGE:` footer. Both are equivalent.
Choose one and be consistent within a release cycle.

---

## 7. NEVER : the `Co-Authored-By: Claude` trailer

Apollia commits are authored by the human contributor. AI assistance is
implicit and never co-credited. This is a hard rule.

If a commit was drafted by an LLM, the human still owns the commit and the
review. The trailer is forbidden.

---

## 8. One commit, one logical unit

A commit is :
- One user story or one logical sub-task.
- Either a single feature, a single fix, or a single refactor. Not a
  combination.

If you find yourself writing `feat(...) and update tests`, split into two
commits.

---

## 9. Pre-commit gate

The pre-commit hook runs :

1. `ruff format` (Python)
2. `ruff check` (Python)
3. `rustfmt --check` (Rust)
4. `cargo clippy --workspace -- -D warnings`
5. `cargo check --workspace`
6. `commitlint` on the commit message

A failing hook means the commit did not happen. Fix the issue and re-stage.
Never `--no-verify`.

---

## 10. Branches

| Prefix | Use |
|---|---|
| `feat/` | new feature |
| `fix/` | bug fix |
| `refactor/` | restructure |
| `docs/` | documentation only |
| `chore/` | tooling, deps |
| `test/` | tests only |
| `perf/` | performance |
| `release/` | release branch |

Branch body : kebab-case (`feat/agents-md-spec`, `fix/oauth-refresh-leak`).
One topic per branch.

Branch from `main`. Rebase onto `main` before opening the PR. Never merge
`main` into a feature branch (creates merge noise).

---

## 11. Tags and versioning

Strict SemVer : `vMAJOR.MINOR.PATCH`. Pre-release suffix when needed :
`v0.1.0-preview`, `v0.1.0-rc1`.

Tag on the commit that ships the version. Push the tag explicitly :
`git push origin v0.1.0`.

Workspace versions are aligned : every `Cargo.toml` in `crates/` uses
`version.workspace = true`. Bump once in the root `Cargo.toml`.

---

## 12. CHANGELOG

`CHANGELOG.md` at the repo root. Format : Keep a Changelog (sectioned by
release). Entries are derived from conventional commits but written for
human consumption, not auto-generated.

A new release entry contains :
- Date.
- Highlights (3-7 bullets, what changed for the user).
- Breaking changes section if any.
- Compatibility notes.
- Link to the GitHub release.

---

## 13. PR description template

When opening a PR :

```markdown
## What

One paragraph, what changes and why.

## Test plan

- [ ] cargo test --workspace --no-fail-fast
- [ ] cargo clippy --workspace -- -D warnings
- [ ] pytest
- [ ] manual : ...

## Breaking changes

None.  // or describe

## Related

Refs ADR-NNN
Closes #N
```

PR title follows the same rules as commit subject.

PRs over 800 lines of diff require an explicit split rationale in the
description.

---

## 14. When the rules block you

- Stuck on which scope to pick : choose the crate where the user-visible
  effect lands. If the change is purely internal to one crate but visible
  through another, scope to the visible one.
- Multiple legitimate scopes : open multiple commits. One per scope. This
  is friction by design.
- Need to amend a published commit : create a new commit. Never `git push
  --force` on `main`.
