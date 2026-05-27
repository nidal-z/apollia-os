# CI-TOOLING

> Editor configs, lint configs, MSRV, pre-commit, and CI pipeline. Read this
> when setting up a new machine or modifying the build infrastructure.

This file is the reference. The actual config files in the repo are the
source of truth at runtime.

---

## 1. Files at a glance

| File | Purpose |
|---|---|
| `.editorconfig` | per-language indentation, line endings, EOL whitespace |
| `rustfmt.toml` | Rust formatter config (edition, max width, import grouping) |
| `clippy.toml` | Clippy config (MSRV, complexity thresholds) |
| `rust-toolchain.toml` | pinned Rust toolchain (channel, components) |
| `pyproject.toml` | Python project metadata, Ruff config, pytest config |
| `.pre-commit-config.yaml` | pre-commit hook definitions |
| `Cargo.toml` (root) | `[workspace.dependencies]`, `[workspace.lints]`, `[workspace.package]` |
| `package.json` (desktop UI) | Node tooling, scripts |
| `.github/workflows/*.yml` | CI pipelines |

---

## 2. `.editorconfig`

```ini
root = true

[*]
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
charset = utf-8
indent_style = space

[*.{rs,toml}]
indent_size = 4

[*.{py,pyi}]
indent_size = 4

[*.{ts,tsx,js,svelte,json,jsonc,html,css}]
indent_size = 2

[*.md]
trim_trailing_whitespace = false  # markdown line breaks need trailing spaces
indent_size = 2

[Makefile]
indent_style = tab
```

---

## 3. `rustfmt.toml`

```toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
reorder_imports = true
use_field_init_shorthand = true
use_try_shorthand = true
newline_style = "Unix"
```

Run : `cargo fmt --check` in CI, `cargo fmt` to apply.

Edition 2021 reflects the current workspace state. Migration to edition
2024 is a planned follow-up (separate ADR + dedicated PR).

---

## 4. `clippy.toml`

```toml
msrv = "1.85"
cognitive-complexity-threshold = 30
type-complexity-threshold = 250
too-many-arguments-threshold = 5
```

Run : `cargo clippy --workspace --all-targets -- -D warnings`.

---

## 5. `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.85"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
profile = "minimal"
```

Committed for reproducibility. CI installs the toolchain via
`actions-rs/toolchain` or `dtolnay/rust-toolchain@stable`.

MSRV is `1.85`. Tested explicitly in the CI matrix.

---

## 6. `[workspace.lints]` in root `Cargo.toml` (target state)

Not yet applied to the workspace. Promotion path :

1. Add the block in a dedicated PR.
2. Start with everything at `warn`.
3. Run `cargo clippy --workspace --all-targets` and inventory current
   warnings.
4. Per warning category, either fix the sites or downgrade the lint to
   `warn` permanently with a documented reason.
5. Promote to `deny` once the workspace is clean for that category.

Target block once the audit is done :

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unreachable_pub = "warn"
unused_must_use = "deny"

[workspace.lints.clippy]
all = "deny"
correctness = "deny"
suspicious = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
missing_errors_doc = "warn"
missing_panics_doc = "warn"
pedantic = "warn"
```

Each crate inherits via `[lints] workspace = true` in its own
`Cargo.toml`. Pedantic allow-list is per-crate, justified inline.

`unsafe_code = "forbid"` is workspace-wide. A crate that genuinely needs
unsafe overrides with `[lints.rust] unsafe_code = "deny"` plus a
top-of-crate explanation.

Until promotion : the rules in `docs/agents/FORBIDDEN.md` and
`docs/agents/RUST-PATTERNS.md` are the policy. Reviewers enforce them.

---

## 7. Ruff config (in `sdk/pyproject.toml`)

```toml
[tool.ruff]
target-version = "py312"
line-length = 100
src = ["apollia", "tests"]

[tool.ruff.lint]
select = [
    "E", "F", "W",       # pycodestyle + pyflakes
    "I",                  # isort
    "B",                  # bugbear
    "UP",                 # pyupgrade
    "RUF",                # ruff-specific
    "S",                  # bandit (security)
    "SIM",                # simplify
    "ANN",                # annotations
    "ASYNC",              # async-specific
    "TCH",                # type-checking
    "PT",                 # pytest
    "D",                  # pydocstyle (google)
]
ignore = [
    "D100", "D104",       # missing module/package docstring
    "ANN101", "ANN102",   # self/cls annotation
    "COM812",             # trailing comma (conflicts with formatter)
    "ISC001",             # implicit string concat (conflicts with formatter)
]

[tool.ruff.lint.per-file-ignores]
"tests/**/*.py" = ["S101", "D", "ANN"]
"apollia/stubs/*.pyi" = ["D", "ANN"]

[tool.ruff.lint.pydocstyle]
convention = "google"

[tool.ruff.format]
quote-style = "double"
indent-style = "space"
line-ending = "lf"
```

Run : `ruff format --check` + `ruff check` in CI. `ruff format` + `ruff
check --fix` to apply.

This is a curated subset rather than `select = ["ALL"]` to keep
warnings actionable. Expand when the codebase is clean for the current
set.

---

## 8. Pyright config (in `sdk/pyproject.toml`)

```toml
[tool.pyright]
include = ["apollia"]
pythonVersion = "3.12"
strict = ["apollia/**"]
reportMissingImports = "error"
reportUnusedImport = "warning"
reportMissingTypeStubs = "warning"
```

Run : `pyright sdk/apollia`.

`mypy` is acceptable as a secondary checker but `pyright` is the gate.

---

## 9. pytest config (in `pyproject.toml`)

```toml
[tool.pytest.ini_options]
asyncio_mode = "strict"
testpaths = ["sdk/tests", "agents"]
addopts = [
  "--strict-markers",
  "--strict-config",
  "-ra",
]
markers = [
  "unit: fast unit test",
  "integration: integration test, may touch the filesystem",
  "slow: skipped unless --run-slow is passed",
]
```

---

## 10. Pre-commit hooks (`.pre-commit-config.yaml`)

```yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.6.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
      - id: check-merge-conflict
      - id: check-yaml
      - id: check-toml
      - id: check-added-large-files
        args: ["--maxkb=500"]
      - id: detect-private-key

  - repo: https://github.com/astral-sh/ruff-pre-commit
    rev: v0.5.0
    hooks:
      - id: ruff
        args: ["--fix"]
      - id: ruff-format

  - repo: local
    hooks:
      - id: rustfmt
        name: rustfmt
        entry: cargo fmt --
        language: system
        types: [rust]

      - id: clippy
        name: clippy
        entry: cargo clippy --workspace --all-targets -- -D warnings
        language: system
        types: [rust]
        pass_filenames: false

      - id: cargo-check
        name: cargo check
        entry: cargo check --workspace
        language: system
        types: [rust]
        pass_filenames: false

  - repo: https://github.com/compilerla/conventional-pre-commit
    rev: v3.4.0
    hooks:
      - id: conventional-pre-commit
        stages: [commit-msg]
        args: []
```

Install : `pre-commit install && pre-commit install --hook-type
commit-msg`.

Run on demand : `pre-commit run --all-files`.

Never `--no-verify`. If a hook fails, fix the underlying issue.

---

## 11. CI pipeline (GitHub Actions)

Target structure (one file per concern) :

```
.github/workflows/
├── ci.yml              # PR gate: fmt, clippy, test, doctest, pyright, pytest
├── release.yml         # tag-triggered: build binaries, attach to release
├── docs.yml            # build mdBook, deploy to Cloudflare Pages
└── desktop-release.yml # Tauri release builds, .dmg/.exe/.AppImage
```

`ci.yml` job sequence :

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo nextest run --workspace`
4. `cargo test --doc`
5. `ruff format --check && ruff check`
6. `pyright`
7. `pytest`
8. `pnpm test` (desktop)
9. `bash tests/cli/cli-e2e.sh` (Phase A)

Caching :
- `Swatinem/rust-cache@v2` (keyed on `Cargo.lock`).
- `actions/setup-uv` + `uv sync --frozen` for Python.

Matrix :
- `os: [ubuntu-22.04, macos-latest, windows-latest]`
- `rust: [1.85, stable]` (MSRV + stable, never beta unless tracking a
  specific feature).

Linux jobs pin `ubuntu-22.04` to keep glibc compatibility for shipped
binaries (commit `dc5957ee`).

---

## 12. Coverage

```sh
cargo llvm-cov nextest --lcov --output-path lcov.info
codecov-cli upload --file lcov.info
```

Target : > 80% lines / > 70% branches on core crates. Aspirational
workspace-wide.

---

## 13. Local setup quickstart

```sh
# Clone
git clone git@github.com:nidal-z/apollia-os.git
cd apollia-os

# Toolchain (rust-toolchain.toml handles the Rust side)
rustup show

# Python
uv venv .venv
source .venv/bin/activate
uv pip install -e ./sdk

# Pre-commit
pre-commit install
pre-commit install --hook-type commit-msg

# First build
cargo build --workspace
cargo test --workspace
```

---

## 14. When the rules block you

- A lint flags legitimate code : add `#[allow(clippy::<lint>)]` with an
  inline justification. Never `--allow` blanket on the command line.
- A pre-commit hook is too slow : split the hook (formatter fast, clippy
  pushed to pre-push). Document in this file.
- CI matrix grows past 6 jobs : reorganize into parallel workflows
  before adding more.
