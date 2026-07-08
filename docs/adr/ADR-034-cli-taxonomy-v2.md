# ADR-034: CLI taxonomy v2 (canonical verbs, git-style top level, clean pre-release rename)

- Status: Accepted
- Date: 2026-07-06
- Supersedes: ADR-004 (noun-verb command shape), on the verb canon and the
  top-level rules

## Context

The `apollia-os` CLI has grown to 41 top-level commands under the noun-verb
contract of ADR-004 (`apollia <noun> <verb>`). An inventory of the actual command
enums in `crates/apollia-cli/src/commands/` (not the docs) shows the contract has
drifted in three concrete ways:

- **Verb synonyms.** The same intent is spelled several ways: reading one item is
  `show` (7 uses), `get` (6), `info` (2), and `describe` (1); destroying is `delete`
  (10) and `remove` (3); creating is `create` (4), `new` (1), and `add` (3). Four
  verbs for "show one thing" is not memorable.
- **Top-level exceptions.** Several top-level commands are not noun-verb at all:
  `run`, `hitl`, `digest`, `doctor`, `inspect`, `onboard`, `logs`, `trace`,
  `version`, `status`, `start`, `stop`.
- **Compound kebab nouns.** `mcp-server`, `chat-config`, `plan-cache`, and
  `user-memory` are compound nouns that duplicate an existing parent noun
  (`mcp`, `chat`, `plan-cache` vs a `plan` domain, `memory`).

The pre-release window (`v0.1.0-preview`, no public repository and therefore no
external script base yet) is the moment to fix the command contract cleanly. Once
the repository is public, the taxonomy becomes a hard contract again and every
change costs a deprecation cycle. Command shape is a long-lived contract
(ADR-004), so refining it is decided here.

## Decision

We adopt CLI taxonomy v2 and apply it as a clean pre-release rename: obsolete
names are removed, with no deprecation aliases.

- **Canonical verb set.** One verb per intent, but only where the intent is
  genuinely the same. Verifying the actual command enums narrowed the
  canonicalization to two cases and confirmed several verbs must stay distinct:
  - `show` is the only entity-detail read verb (absorbs `info`, `describe`, and
    the entity-detail `get`, e.g. `mcp get` / `agent info`).
  - `create` is the only entity-creation verb (absorbs `new`, e.g. `agent new`).
  - **Kept distinct (verification-driven):**
    - `get` / `set` for config key/value pairs (`config`, `chat config`,
      `tools config`, `notify events`): `get` here is the counterpart of `set`,
      not an entity read, so it stays.
    - `add` / `remove` for relationships and registrations, git-style
      (`mcp add`, `project agents add/remove`, connector picked folders): these
      attach or detach an existing thing, they do not create or delete an
      entity, so `create` / `delete` would misdescribe them.
    - `delete` for genuine entity deletion; `remove` stays where it means detach.
    - `clear` and `evict` are distinct in `plan cache`: `clear` wipes all, `evict`
      purges only entries past an age. They coexist and are not merged.
  - We keep the lifecycle verbs that carry a distinct meaning: `install` /
    `uninstall` (agents), `revoke` (auth and approvals), `enable` / `disable`,
    `start` / `stop` / `restart`, `reset`, `reload`.
  - `update` mutates an entity (distinct from `set`).
  - The unambiguous remainder is unchanged: `list`, `status`, `logs`, `test`,
    `fire`, `validate`, `export`, `import`, `forget`.

- **Top level: a git-style bare-verb whitelist.** A fixed, documented set of bare
  commands is allowed at the top level, covering runtime lifecycle and read-only
  diagnostics: `start`, `stop`, `status`, `run`, `doctor`, `inspect`, `logs`,
  `trace`, `version`, `digest`, `onboard`. Every other command is strictly
  noun-verb. The whitelist is a curated exception, documented in
  `crates/apollia-cli/AGENTS.md`; adding to it requires a note there.

- **Compound nouns fold into their parent, or a clearer single noun.**
  `mcp-server` becomes `mcp server`, `chat-config` becomes `chat config`, and
  `plan-cache` becomes `plan cache`. `user-memory` is renamed to the `profile`
  noun, since the user profile is its own domain (`apollia-os profile show`,
  `profile set`, `profile forget`, ...).

- **`hitl` folds into `task`.** It is exactly `task list --pending-approval`, so it
  is removed as a bare command and expressed through `task`.

- **Clean cut, no deprecation.** Because there is no public installed base yet, we
  do not keep deprecated aliases, hidden-alias-with-warning shims, or a removal
  timeline. The old names are removed outright so the public launch surface is
  tidy. Ergonomic convenience aliases (for example `ls`, `rm`, `ps`) are out of
  scope here; they are a discoverability feature decided in ADR-C and are not
  deprecated names.

## Alternatives considered

### Fix only the verb synonyms (rejected)
**For:** the smallest change; leaves the top-level structure untouched.
**Against:** a half-measure. It leaves the non-noun-verb exceptions and the
compound kebab nouns, so the surface stays inconsistent and the pre-release window
is wasted.

### Force everything into strict noun-verb, including lifecycle (rejected)
**For:** maximal purity, zero bare commands.
**Against:** `runtime start`, `runtime status`, `agent run` are more verbose and
break the git/docker muscle memory operators already have (`start`, `status`,
`run`). More churn for no memorability gain.

### Keep deprecated aliases with a removal timeline (rejected for now)
**For:** the correct approach once real users depend on the old names.
**Against:** there are no such users yet. Carrying deprecated names and warning
shims into the first public release pollutes the surface for no benefit.

### Chosen: canonical verbs + git-style bare whitelist + folded nouns + clean rename
**For:** a small, predictable verb vocabulary; fewer top-level nouns; a memorable
surface aligned with common CLI conventions; a clean public launch.
**Trade-offs:** a one-time breaking change to every script that uses an old name
(acceptable while pre-release); the bare-verb whitelist is a maintained exception
list rather than a pure rule.

## Consequences

**Positive:**
- The verb vocabulary shrinks to one verb per intent, which is the main
  memorability win.
- Top-level nouns decrease as compound nouns fold into parents.
- The first public CLI surface is clean, with no legacy or deprecation noise.

**Negative / trade-offs:**
- Breaking change: any existing local script using `agent info`, `mcp-server`,
  `plan-cache clear`, etc. must be updated. Mitigated by the pre-release timing.
- The bare-verb whitelist must be kept in sync with reality in
  `crates/apollia-cli/AGENTS.md`.

**Neutral / to watch:**
- After the public passage the taxonomy is a hard contract again; later additions
  must fit v2 or carry their own ADR.
- The parsing-test suite (target 150+, `crates/apollia-cli/AGENTS.md` section 4)
  must be updated in lockstep with the renames.

## Principles impacted

- Principle #8 (Human CLI, machine API): v2 strengthens the human side
  (memorability, discoverability) while `--json` machine mode and exit codes 0-5
  are untouched.
- ADR-004 (noun-verb): refined and partially superseded. The noun-verb rule holds
  for everything outside the documented bare-verb whitelist; the verb canon and
  the whitelist are the new specifics.

## Links

- Supersedes: ADR-004
- Related: ADR-B (IA-native CLI surface), ADR-C (discoverability: completions,
  palette, convenience aliases)
- Stories: to be created after this ADR is accepted
