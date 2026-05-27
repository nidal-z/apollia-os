# DOCS-WRITING

> Documentation conventions per corpus. Read this before writing or editing
> any documentation file.

Apollia has five distinct documentation corpora, each with a single owner,
a single audience, and a single mode (in the Diátaxis sense). The hardest
rule is that the corpora do not duplicate each other.

---

## 1. Corpus map

| Corpus | Path | Mode | Audience | Language |
|---|---|---|---|---|
| Book | `docs/book/` | Tutorial + explanation | Developer onboarding | French |
| Wiki | `docs/wiki/` | Reference | Developer / maintainer | English (post-L2b) |
| Help | `docs/help/` | How-to | Desktop end-user (operator) | French |
| ADR | `docs/adr/` | Explanation, decision | Maintainer | English |
| Agents | `docs/agents/` | LLM rulebook | LLM coding assistants + dev | English |

Each corpus does its job and only its job.

- The Book teaches a concept in 1-2 worked examples then links to the Wiki
  for the exhaustive reference. The Book never duplicates a reference
  table.
- The Wiki holds the full table : every parameter, every error code, every
  signature. No tutorials, no opinions.
- The Help is operator-focused : one article per task the user wants to
  accomplish, no internal vocabulary.
- The ADR holds the decision, the alternatives, the consequences. No
  tutorial content, no reference content.
- The Agents corpus encodes rules an LLM must follow. No tutorial, no
  reference table (cite the Wiki instead).

When in doubt about where new content belongs, the question is : "what
mode is this in?" Tutorial -> Book. Reference -> Wiki. How-to -> Help.
Decision -> ADR. Rule -> Agents.

---

## 2. Rustdoc conventions

Source : RFC 1574 (more API documentation), RFC 505 (API comment conventions).

```rust
/// Returns the next available agent identifier.
///
/// The identifier is monotonically increasing within a single process and
/// resets across restarts. Persistence requires snapshotting through the
/// `AgentRegistry`.
///
/// # Examples
///
/// ```
/// # use apollia_core::AgentId;
/// let id = AgentId::next();
/// assert!(id.as_ref().starts_with("agent-"));
/// ```
///
/// # Errors
///
/// Returns `RegistryError::Exhausted` if the internal counter overflows.
/// In practice this requires more than `u64::MAX` agents in one process.
pub fn next_agent_id() -> Result<AgentId, RegistryError> { /* ... */ }
```

Rules :

- `///` outer comment on every `pub` item. `//!` inner at crate or module
  top.
- First line : short, third-person present indicative. "Returns the X",
  "Computes the Y", "Builds the Z". Never imperative ("Return", "Compute"),
  never first person.
- Section headings, always plural : `# Examples`, `# Errors`, `# Panics`,
  `# Safety`.
- `# Examples` : always at least one. Compiles in CI as a doctest.
- `# Errors` : required when the return type is `Result<_, _>`.
- `# Panics` : required when the function can panic on caller-controllable
  input.
- `# Safety` : required for every `unsafe fn`.
- Code blocks default to `rust`. Mark `no_run`, `ignore`, `compile_fail`,
  `should_panic` when needed.
- Link to other items with `[`backticks`]` and Rust paths :
  `[`AgentRegistry`](crate::registry::AgentRegistry)`.

---

## 3. Prose style

Applies to every Markdown file in the repo (book, wiki, help, adr, agents,
PR descriptions, ADRs, READMEs).

**Never em-dash `—`.** It is the strongest fingerprint of AI-generated
text. Use comma, parenthesis, colon, period, or hyphen `-` instead.

**Never mix French and English in the same file.** Each file is one
language. The corpus assignment in §1 governs which language each file
uses.

**Never AI stock phrases.** Forbidden tokens (non-exhaustive) :

- "as an AI", "as a language model", "I'm here to help", "Certainly!",
  "I'd be happy to"
- "it's important to note that", "it's worth noting that", "il convient
  de noter", "il est important de noter", "comme mentionné précédemment"
- "in conclusion", "in summary" as standalone section headers
- "let's", "we will see", "let me explain" in technical documentation

Use neutral, descriptive prose. Imperative verbs are fine for rules.
Declarative verbs are fine for explanations.

**Cross-reference, do not duplicate.** When you have already explained a
concept elsewhere, link to it. Inline summaries are tolerated only when
the linked content is heavy and the inline summary is one or two
sentences.

**Liens internes** (book to wiki) follow the canonical pattern :

```markdown
> **Référence technique :** [Nom-Page](https://github.com/nidal-z/apollia-os/wiki/Nom-Page)
```

**Day-one disclaimer in the Book.** Until the public repo and the wiki are
fully linked, the Book does not emit `docs/adr/...` or `wiki/...` URLs.
ADRs are cited by bare identifier (`ADR-095`) with a global warning in
the introduction : "Detailed cross-references will be activated when the
public wiki is online."

---

## 4. Book conventions (`docs/book/`)

- Built with mdBook.
- French.
- Pedagogical : one chapter introduces one concept, in 1-2 examples.
- Never duplicates the Wiki. Each concept links to the Wiki reference
  page once the wiki refactor (L2b) is done.
- Code samples are runnable when possible. If not, mark `text` and
  explain why.
- Chapter length : aim for 5-15 minutes of reading.

---

## 5. Wiki conventions (`docs/wiki/`)

- English (post-L2b). The Book and Help remain French.
- Reference mode : exhaustive, no opinion, no tutorial.
- One page per subject. Tables for parameter lists, error codes, type
  signatures.
- No `STORY-NNN`, `sprint-N`, `[Lot N]`, `[Bloc X]` tokens. Internal
  references stay in `docs/internal/`.
- Status banner during the L2b refactor : "This corpus is being rebuilt
  in English. Some pages may contain stale references."

---

## 6. Help conventions (`docs/help/`)

- French.
- Operator audience (desktop end-user, no developer assumption).
- One article = one task the user wants to accomplish.
- No internal vocabulary (no "acteur", no "EventBus", no "mpsc"). Use the
  vocabulary the UI uses.
- Screenshots when they clarify (sparingly).

---

## 7. ADR conventions (`docs/adr/`)

- English.
- Filename : `ADR-NNN-kebab-title.md`, numbered globally, never reused.
- Skeleton : skill `apollia-adr` generates it. Sections : Context,
  Decision, Consequences, Alternatives Considered.
- Status : Proposed -> Accepted -> (Deprecated | Superseded by ADR-NNN).
  Status line at the top of the file.
- Append-only history : once Accepted, an ADR is amended by a new ADR
  that supersedes it, not by editing in place.
- When to open one : see `docs/agents/ARCHITECTURE.md` §E.

---

## 8. Designer briefs

When you write a brief for the site designer :

- Describe structure (what blocks, in what order, with what nesting).
- Describe wording (the actual copy, or the intent).
- Describe intent (what the visitor should understand or feel).

Never CSS values. Never colors. Never sizes. The designer knows the
charter. Briefs are content artifacts, not implementation specs.

---

## 9. README conventions

- Repo root `README.md` : pitch (1 paragraph), install quickstart (3
  commands max), link to the Book and to the Wiki landing page.
- Crate-level READMEs : minimal. One paragraph, link to the Wiki spec for
  the crate. No tutorial.
- No README in subdirectories unless there is a real reason.

---

## 10. LLM-friendly structure

These conventions make the corpora retrievable by LLM agents and embedding
search :

- Strict H2 / H3 hierarchy. Avoid H4 unless necessary, never H5+.
- Sections are self-contained : a reader who lands on H2 §5 must
  understand it without reading §1-4.
- Naming is consistent across pages. The same concept uses the same name
  everywhere.
- Repetitive structure across similar pages. LLMs read patterns.
- A `llms.txt` at the doc site root once published (Howard 2024 proposal,
  growing adoption).

---

## 11. Updating documentation after a change

When you make a change that crosses a corpus boundary :

| Change | Updates required |
|---|---|
| New public Rust API | Wiki page, doc-comment on the item, possibly Book chapter |
| New CLI sub-command | `crates/apollia-cli/AGENTS.md`, Wiki `Briques-CLI.md`, Book if user-facing |
| New ADR | Add to `docs/wiki/Decisions-Log.md`, optionally cited from the relevant ADR map (`docs/agents/ARCHITECTURE.md` §F) |
| New tracing field | `docs/agents/OBSERVABILITY.md` table |
| New design token | `app.css`, `tailwind.config.js`, `wiki/DESIGN-SYSTEM.md` |
| Behavior change visible in operator UI | `docs/help/` article + Book chapter |

Skill `apollia-doc-sync` automates parts of this for sprint closure. Skill
`apollia-doc-sync-diff` synchronizes from a git commit range.

---

## 12. When the rules block you

- New corpus needed : open an ADR. Adding a corpus is a real
  architectural choice.
- Cross-corpus duplication tempting : cite, do not copy. If the citation
  is too thin, the source page is too dense. Refactor the source.
- Inline reference table tempting in the Book : refactor that section into
  a Wiki page, then link to it.
