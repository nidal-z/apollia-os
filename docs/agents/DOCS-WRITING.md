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
| Notion `PORTAIL TECH` | Apollia's Space (Notion) | Digest concept atlas | Maintainer (R&D), future dev | French |

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
- The Notion `PORTAIL TECH` atlas is a parallel digest layer for R&D and
  onboarding. One page per concept, with analogies, schemas, and "R&D
  surfaces ouvertes" sections. Mirrors the technical corpora without being
  a source of truth. See §13 for the writing rules.

When in doubt about where new content belongs, the question is : "what
mode is this in?" Tutorial -> Book. Reference -> Wiki. How-to -> Help.
Decision -> ADR. Rule -> Agents. R&D digest -> Notion atlas.

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
> **Référence technique :** [Nom-Page](https://github.com/Apollia-OS/apollia-os/wiki/Nom-Page)
```

**Day-one disclaimer in the Book.** Until the public repo and the wiki are
fully linked, the Book does not emit `docs/adr/...` or `wiki/...` URLs.
ADRs are cited by bare identifier (`ADR-018`) with a global warning in
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

---

## 13. Notion atlas `PORTAIL TECH`

Parallel Notion space that mirrors the technical content in a digest
format. One Notion page per concept, designed for R&D consultation and
junior-dev onboarding. Not a source of truth ; the code, the ADRs, and
the LLM corpora remain authoritative.

### 13.1 Where it lives

- **Workspace** : Apollia's Space (the user's primary Notion).
- **Root page** : `PORTAIL TECH` (page ID `36de4a87-ed71-812f-98ff-c078ae0d5f4a`,
  URL [PORTAIL TECH](https://www.notion.so/36de4a87ed71812f98ffc078ae0d5f4a)).
- **Sections** : 12 thematic groups, structured by question (Comment ça
  vit / communique / persiste / sécurise / observe) and by named brick
  (ORIA, LLM, SDK, Desktop, Pipelines, Fondations, R&D ouvertes).

### 13.2 Tool to use

Always create and update via the Notion MCP server :

- `mcp__claude_ai_Notion__notion-create-pages` for new pages.
- `mcp__claude_ai_Notion__notion-update-page` for edits
  (`update_content` for targeted edits, `replace_content` for full
  rewrites).
- `mcp__claude_ai_Notion__notion-fetch` to read current state before
  edit.

Never edit Notion pages out-of-band (copy-paste from a markdown file)
unless the MCP is unavailable. The MCP keeps the page IDs and
cross-references stable.

### 13.3 Page template (canonical 7-section structure)

Every concept page follows the same skeleton, in this order :

- **A. Mental model** : the central idea + useful vocabulary (acronym
  glossary inline).
- **B. Place dans l'archi** : Mermaid diagram or structural table.
- **C. Comment c'est implémenté** : technical facts, parameters table.
- **D. Les décisions structurantes** : the why of non-obvious choices.
- **E. Ce qu'on n'altère pas sans ADR** : protected invariants table.
- **F. Surfaces de R&D ouvertes** : open questions, exploration paths.
  Includes a "Notes personnelles" subsection for the user's R&D notes.
- **G. Pour creuser** : code source path, ADRs, external doc links.

This template is documented at the bottom of the `PORTAIL TECH` page.
Do not deviate without consultation.

### 13.4 Writing style

- **Language** : French.
- **Tone** : impersonal. No `tu`, no `imagine que`, no naive analogies
  ("la radio interne du runtime" type). Use the named pattern instead
  (`pub/sub broadcast`, `actor model`, `RPC oneshot`).
- **Density** : aerated, around 1 screen of Notion per page when
  possible. Section headers H2, sub-sections H3.
- **Vocabulary** : inline glossary in section A, in a bulleted list
  prefixed by `📖 Vocabulaire utile`. Each acronym explained on first
  use (`pub/sub`, `fanout`, `backpressure`, `ADR`, etc.).
- **Blockquotes** in italic for explanatory side-notes :
  `> 💡 *Tokio est la bibliothèque async de référence en Rust...*`.
- **No em-dash** (`—`). Use comma, period, parenthesis, colon, or
  hyphen `-`.
- **Emoji** : 1 per H2 section header (matches existing space
  convention), 1 per H3 sub-section optionally, sparingly inline for
  signal terms (🚫 anti-pattern, 💡 reminder, 🦀 Rust note).

### 13.5 Diagrams

- Use **Mermaid** code blocks (`` ```mermaid ``). Notion renders them
  natively.
- ASCII diagrams break Notion's monospace rendering. Avoid.
- Keep the diagram under 8 nodes when possible. Multiple smaller
  diagrams beat one wide one.

### 13.6 Cross-page links

- **Embedded child page** (when a sub-page should appear as a card
  inline) : `<page url="https://www.notion.so/PAGE_ID">Title</page>`.
- **Inline link** (mentioning another page without embedding) :
  standard markdown `[Title](https://www.notion.so/PAGE_ID)`.
- Embedding a child page in the parent's navigation section avoids the
  default Notion behavior of appending the child as a block at the
  bottom of the parent. Use the embed in the relevant navigation section
  to keep the parent clean.

### 13.7 References from Notion to the repo

Mention paths as inline code without a link, since the repo is private
until launch :

- `crates/apollia-runtime/src/eventbus.rs` (code source)
- `docs/agents/ARCHITECTURE.md` §C (LLM rules)
- `docs/adr/ADR-012` (decision)

Once the repo is public, these mentions can be promoted to links in a
post-launch pass.

**Never reference `docs/wiki/`** in Notion atlas pages. The wiki is being
fully reworked (L2b sprint), all its current content will be obsoleted.
Use `docs/agents/`, `docs/adr/`, `docs/book/`, or direct code paths
instead.

### 13.8 R&D surfaces (section F)

This is the section that turns the atlas from a static mirror into an
R&D tool. Every concept page must have it, even if half-empty at
creation. Structure :

- 3-5 named open questions, one paragraph each.
- A final `📝 Notes personnelles` sub-section, intentionally empty,
  reserved for the user's free-form notes during R&D sessions.

When a question gets answered or transitions into an ADR, remove it
from this section and reference the ADR in section G.

### 13.9 Factual accuracy

The atlas is consulted by the user during R&D and may seed strategic
decisions. **No hallucination tolerance.** Before writing a page :

1. Read the corresponding LLM corpus page (`docs/agents/*.md`).
2. Cross-reference the actual code in `crates/...`.
3. Check the ADR map (`docs/agents/ARCHITECTURE.md` §F).
4. If a fact is uncertain, omit it. Better incomplete than wrong.

### 13.10 Update cadence

The atlas decays slower than `docs/wiki/` because it sits at the concept
level. Update triggers :

- A concept changes shape (e.g. the EventBus moves to a different
  primitive).
- A new concept is introduced (new brick, new pattern).
- An R&D surface gets resolved (close it, reference the resolution).

Day-to-day code changes do not require atlas updates. Reserve atlas
work for sprint endings or R&D sessions.
