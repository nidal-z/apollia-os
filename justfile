# Apollia OS task recipes. `just` lists them by group, `just doctor` checks the
# toolchain. Needs just 1.27 or later. Why a recipe does what it does is told
# in docs/agents/CI-TOOLING.md; personal recipes live in an untracked
# `justfile.local` (see justfile.local.example).

set shell := ["bash", "-euo", "pipefail", "-c"]
# Git for Windows ships the bash these recipes need; `just --shell <path>` overrides.
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-euo", "pipefail", "-c"]
set dotenv-load

import? 'justfile.local'

# Settings. Each reads one environment variable, from the shell or from `.env`
# (template: .env.example), and falls back on the default written here.
root := justfile_directory()
lib := root / "scripts/lib"
# GGUF model, required by every recipe that loads one. No default.
llama_model := env("APOLLIA_LLAMA_MODEL", "")
# llama-server binary. Empty: the one on PATH.
llama_bin := env("APOLLIA_LLAMA_BIN", "")
llama_port := env("APOLLIA_LLAMA_PORT", "8899")
llama_slots := env("APOLLIA_LLAMA_SLOTS", "1")
# Context, temperature and seed. Empty: 32768 tokens and the server's sampling
# for the dev recipes; 131072, 0 and 42 for the automation recipes.
llama_ctx := env("APOLLIA_LLAMA_CTX", "")
llama_temp := env("APOLLIA_LLAMA_TEMP", "")
llama_seed := env("APOLLIA_LLAMA_SEED", "")
llama_extra_args := env("APOLLIA_LLAMA_EXTRA_ARGS", "")
llama_log := env("APOLLIA_LLAMA_LOG", "/tmp/apollia-dev-llama-server.log")
# GGUF model for Track 3 of the CLI end-to-end suite. No default.
test_model := env("APOLLIA_TEST_MODEL_GGUF", "")
automation_out := env("APOLLIA_AUTOMATION_OUT", root / ".apollia-automation")
seed_home := env("APOLLIA_SEED_HOME", root / ".apollia-seed-home")
seed_overlay := env("APOLLIA_SEED_OVERLAY", home_directory() / ".apollia-seed-overlay")

# The settings above, as the shell variables scripts/lib reads.
llama_settings := (
    "LLAMA_BIN=" + quote(llama_bin) + " LLAMA_PORT=" + quote(llama_port)
    + " LLAMA_SLOTS=" + quote(llama_slots) + " LLAMA_CTX=" + quote(llama_ctx)
    + " LLAMA_TEMP=" + quote(llama_temp) + " LLAMA_SEED=" + quote(llama_seed)
    + " LLAMA_EXTRA_ARGS=" + quote(llama_extra_args) + " LLAMA_LOG=" + quote(llama_log)
)
automation_settings := llama_settings + " AUTOMATION_OUT=" + quote(automation_out) + " SEED_HOME=" + quote(seed_home)

# List the recipes by group
[private]
default:
    @just --justfile "{{justfile()}}" --list --unsorted

# Refuse early when the Tauri CLI is absent, before a runner is built for nothing
[private]
_require-tauri:
    @cargo tauri --version >/dev/null 2>&1 || { echo "error: cargo-tauri is not installed, and every desktop recipe needs it. Install the version the release uses: cargo install tauri-cli --version '^2'" >&2; exit 2; }

# Print the Python bundle of this tree and lay down its dev link (exit 1 when absent)
[private]
_bundle-python:
    @source "{{lib}}/python-bundle.sh"; python_bundle_resolve

[private]
_fmt-check:
    cargo fmt --all --check

# List the tools the recipes need, present and absent (exit 2 when one is missing)
[group('build')]
doctor:
    @LLAMA_BIN={{quote(llama_bin)}} bash "{{lib}}/doctor.sh"

# Build the workspace (debug)
[group('build')]
build:
    cargo build --workspace

# Build the workspace (release)
[group('build')]
build-release:
    cargo build --workspace --release

# Build the CLI against the bundled interpreter, not the machine's
[group('build')]
build-cli:
    source "{{lib}}/python-bundle.sh"; python_bundle_activate; cargo build -p apollia-cli

# Build one debug STT runner, plus the suffixed copy the daemon detects
[group('build')]
build-runner backend:
    #!/usr/bin/env bash
    set -euo pipefail
    source "{{root}}/scripts/host_desktop_defaults.sh"; host_runner_cmake_env
    cargo build -p apollia-runner --features local-{{backend}}
    cp "target/debug/apollia-runner" "target/debug/apollia-runner-{{backend}}"
    chmod +x "target/debug/apollia-runner-{{backend}}" || true
    # A copy loses the linker's ad hoc signature on macOS and is killed at launch.
    if [ "$(uname)" = "Darwin" ]; then codesign --force --sign - "target/debug/apollia-runner-{{backend}}"; fi

# Build the STT runners the desktop dev recipes expect (cpu, plus metal on macOS)
[group('build')]
build-runners:
    just build-runner cpu
    if [ "$(uname -s)" = "Darwin" ]; then just build-runner metal; fi

# Build one release STT runner, for this host or for a named triple
[group('build')]
build-runner-release backend target="":
    #!/usr/bin/env bash
    set -euo pipefail
    source "{{root}}/scripts/host_desktop_defaults.sh"; host_runner_cmake_env "{{target}}"
    if [ -n "{{target}}" ]; then host_runner_cmake_clean "{{target}}" --release --target "{{target}}"; cargo build -p apollia-runner --release --target "{{target}}" --features local-{{backend}}; else host_runner_cmake_clean "" --release; cargo build -p apollia-runner --release --features local-{{backend}}; fi

# Build the desktop bundle, for this host (target/release) or for a named triple
[group('build')]
build-desktop target="" runners="" backend="": _require-tauri
    #!/usr/bin/env bash
    set -euo pipefail
    source "{{root}}/scripts/host_desktop_defaults.sh"
    RUNNERS="{{runners}}"; [ -n "$RUNNERS" ] || RUNNERS="$(host_desktop_runners "{{target}}")"
    BACKEND="{{backend}}"; [ -n "$BACKEND" ] || BACKEND="$(host_desktop_llama_backend "{{target}}")"
    PATCH="$(desktop_updater_patch)"
    ARGS=()
    if [ -n "{{target}}" ]; then ARGS+=(--target "{{target}}"); fi
    if [ -n "$PATCH" ]; then ARGS+=(--config "$PATCH"); fi
    echo "==> build-desktop target={{ if target == '' { 'host' } else { target } }} runners=$RUNNERS engine=${BACKEND:-derived from runners}"
    cd "{{root}}/crates/apollia-desktop"
    export APOLLIA_DESKTOP_RUNNERS="$RUNNERS" APOLLIA_DESKTOP_LLAMA_BACKEND="$BACKEND" CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
    cargo tauri build ${ARGS[@]+"${ARGS[@]}"}

# Remove the build artifacts
[group('build')]
clean:
    cargo clean

# Run the workspace tests under a sentinel HOME, proving ~/.apollia stays untouched
[group('test')]
test:
    python3 scripts/check_test_home_isolation.py --wrap cargo test --workspace --no-fail-fast

# Run the workspace tests with the Python features
[group('test')]
test-python:
    PYO3_PYTHON="$(command -v python3)" cargo test --workspace --features python-tests

# Prove the engine manifest script writes what the engine store accepts
[group('test')]
test-engine-manifest:
    cargo test -p apollia-runtime --test engine_manifest_publication -- --include-ignored --skip test_published_directory_is_signed_by_the_compiled_key

# CLI end-to-end suite, Tracks 1 and 2: the daemon on a throwaway seeded HOME, no model
[group('test')]
test-cli-runtime:
    APOLLIA_REQUIRE_RUNTIME=1 APOLLIA_TEST_MODEL_GGUF=/nonexistent-apollia-e2e.gguf bash tests/cli/cli-e2e.sh

# CLI end-to-end suite, all three tracks: Track 3 loads the GGUF read-only
[group('test')]
test-cli-model gguf=test_model:
    @source "{{lib}}/llama.sh"; llama_require_model "{{gguf}}" APOLLIA_TEST_MODEL_GGUF
    APOLLIA_REQUIRE_RUNTIME=1 APOLLIA_TEST_MODEL_GGUF="{{gguf}}" bash tests/cli/cli-e2e.sh

# One real invocation of every CLI leaf on a throwaway HOME (`--daemon` records the API routes crossed)
[group('test')]
test-cli-sweep *args: build-cli
    python3 tests/cli/smoke/sweep.py {{args}}

# A workflow pausing on a question, end to end on a real daemon, no model
[group('test')]
test-hitl: build-cli
    PYTHONPATH=sdk python3 -m pytest tests/hitl/test_hitl_e2e.py -q -p no:cacheprovider

# Load the two agent evaluation suites and hold their invariants
[group('test')]
test-evals-load:
    bash evals/oria/check-suite.sh
    python3 evals/tools/check_suite.py

# Check the formatting and run clippy on the workspace
[group('quality')]
lint: _fmt-check
    cargo clippy --workspace --all-targets -- -D warnings

# Format the Rust code
[group('quality')]
fmt:
    cargo fmt --all

# Build the three configurations the default features hide
[group('quality')]
check-features:
    cargo check -p apollia-cli --no-default-features --all-targets
    cargo check -p apollia-llm --no-default-features --all-targets
    cargo check -p apollia-tools --no-default-features --all-targets

# Audit the dependency surfaces the release ships (exit 2 offline: nothing measured)
[group('quality')]
audit-deps:
    #!/usr/bin/env bash
    set -uo pipefail
    if ! curl -fsI --max-time 10 https://pypi.org >/dev/null 2>&1; then
      echo "audit-deps: network unreachable, nothing was audited" >&2
      exit 2
    fi
    reds=0
    pip-audit -r packaging/requirements-bundled.txt --no-deps --progress-spinner off || reds=1
    ( cd crates/apollia-desktop/ui && npm audit --omit=dev ) || reds=1
    cargo audit || reds=1
    if [ "$reds" -ne 0 ]; then
      echo "audit-deps: at least one surface carries an open advisory" >&2
      exit 1
    fi
    echo "audit-deps: three surfaces audited, zero advisory"

# The capability inventory: what the product does and what executes it (`--list-holes` names the gaps)
[group('quality')]
coverage *args:
    python3 scripts/capability_inventory.py {{args}}

# Run every tracked guard script and the external gates, and report every red one
[group('quality')]
guards:
    #!/usr/bin/env bash
    set -uo pipefail
    # Five guards judge the built binary, so it is rebuilt first (docs/agents/CI-TOOLING.md).
    printf '== build target/debug/apollia-os\n'
    if ! cargo build -p apollia-cli --bin apollia-os; then
      printf '== FAIL the binary five guards judge could not be built;\n'
      printf '        their verdicts below say nothing about this tree\n'
      exit 1
    fi
    # Named one by one: scripts/check_selftest.py looks each basename up in this file.
    guards=(
      "scripts/automation/tools/validate.py"
      "scripts/check_agents_md.py"
      "scripts/check_attached_coverage.py"
      "scripts/check_attached_coverage.py --selftest"
      "scripts/check-model-families.py"
      "scripts/check-model-families.py --selftest"
      "scripts/check_automation_derived.py"
      "scripts/check_ci_workflows.py"
      "scripts/check_claim_anchors.py"
      "scripts/check_claims.py"
      "scripts/check_cli_e2e_coverage.py"
      "scripts/check_cli_json_contract.py"
      "scripts/check_cli_json_contract.py taxonomy"
      "scripts/check_cli_parse_tests.py"
      "scripts/check_compat_routes.py"
      "scripts/check_compat_routes.py --selftest"
      "scripts/check_config_reference.py"
      "scripts/check_config_reference.py --selftest"
      "scripts/check_corpus_claims.py"
      "scripts/check_crate_lints.py"
      "scripts/check_ctx_contract.py"
      "scripts/check_data_layout.py"
      "scripts/check_design_tokens.py"
      "scripts/check_desktop_csp.py"
      "scripts/check_doc_generators.py"
      "scripts/check_docs_anchors.py"
      "scripts/check_docs_frontmatter.py"
      "scripts/check_docs_lang.py"
      "scripts/check_docs_mirror.py"
      "scripts/check_docs_routes.py"
      "scripts/check_file_language.py"
      "scripts/check_eventbus_variants.py"
      "scripts/check_embedded_docs_size.py"
      "scripts/check_embedded_docs_size.py --selftest"
      "scripts/check_download_sums.py"
      "scripts/check_guard_verdicts.py"
      "scripts/check_http_clients.py"
      "scripts/check_i18n_catalogue.py"
      "scripts/check_module_size.py"
      "scripts/check_instrument_verdicts.py"
      "scripts/check_no_font_cdn.py"
      "scripts/check_openapi_routes.py"
      "scripts/check_optional_builders.py --strict"
      "scripts/check_panic_free.py"
      "scripts/check_prose.py"
      "scripts/check_pub_unreferenced.py"
      "scripts/check_python_rules.py"
      "scripts/check_swizzled_theme.py"
      "scripts/check_release_artifacts.py"
      "scripts/check_generated_clients.py"
      "scripts/check_connector_fixtures.py"
      "scripts/check_rust_rules.py"
      "scripts/check_rust_tests.py"
      "scripts/check_selftest.py"
      "scripts/check_serde_persisted_defaults.py"
      "scripts/check_sqlite_schema_versioning.py"
      "scripts/check_subprocess_window.py"
      "scripts/check_tauri_ipc_args.py"
      "scripts/check_tauri_ipc_callers.py"
      "scripts/check_tracing_messages.py"
      "scripts/check_tracked_inventory.py"
      "scripts/check_tracked_inventory.py --selftest"
      "scripts/check_unimported_files.py"
      "scripts/check_ui_vocabulary.py"
      "scripts/check_windows_python_dll.py"
      "scripts/check_windows_python_dll.py --selftest"
      "scripts/check_testid_anchors.py"
      "scripts/check_testing_mocks.py"
      "scripts/check_testing_mocks.py --selftest"
      "scripts/check_custom_event_listeners.py"
      "scripts/check_entry_doc_commands.py"
      "scripts/check_playwright_specs.py"
    )
    # External gates of the same corpus. clippy leads: it is the one that can hide the others.
    externals=(
      "cargo clippy --workspace --all-targets -- -D warnings"
      "cargo machete"
      "cargo audit"
      "cargo deny check advisories"
      "cd sdk && mypy apollia"
      "cd crates/apollia-desktop/ui && npm run audit:i18n"
      "cd crates/apollia-desktop/ui && npm run audit:a11y"
      "ruff check scripts"
      "bash scripts/lib/selftest.sh"
    )
    # Reds accumulate. Exit 0 all green, 1 one red, 2 no red but one guard measured nothing.
    reds=()
    skips=()
    for guard in "${guards[@]}"; do
      # shellcheck disable=SC2086
      python3 $guard
      code=$?
      if [ "$code" -eq 0 ]; then
        echo "== ok   $guard"
      elif [ "$code" -eq 2 ]; then
        echo "== skip $guard (nothing measured)"
        skips+=("$guard")
      else
        echo "== RED  $guard"
        reds+=("$guard")
      fi
    done
    # 127 is the shell's answer for a tool that is not installed: a skip, not a red.
    for gate in "${externals[@]}"; do
      bash -c "$gate"
      code=$?
      if [ "$code" -eq 0 ]; then
        echo "== ok   $gate"
      elif [ "$code" -eq 127 ]; then
        echo "== skip $gate (nothing measured: the tool is not installed)"
        skips+=("$gate")
      else
        echo "== RED  $gate"
        reds+=("$gate")
      fi
    done
    echo
    if [ "${#skips[@]}" -ne 0 ]; then
      echo "${#skips[@]} guard(s) measured nothing:"
      for guard in "${skips[@]}"; do echo "  $guard"; done
      echo
    fi
    if [ "${#reds[@]}" -ne 0 ]; then
      echo "${#reds[@]} guard(s) red:" >&2
      for guard in "${reds[@]}"; do echo "  $guard" >&2; done
      echo >&2
    fi
    total=$(( ${#guards[@]} + ${#externals[@]} ))
    green=$(( total - ${#reds[@]} - ${#skips[@]} ))
    echo "$green guards green, ${#reds[@]} red, ${#skips[@]} measured nothing, $total in the recipe"
    if [ "${#reds[@]}" -ne 0 ]; then exit 1; fi
    if [ "${#skips[@]}" -ne 0 ]; then exit 2; fi

# Local CI: the guards (clippy among them), the format check, then the tests
[group('quality')]
ci: guards _fmt-check test
    @echo "OK local CI passed"

# Build the public documentation site (Docusaurus, en + fr)
[group('docs')]
docs:
    cd docs/site && npm ci && npm run build

# Regenerate the site's machine references (CLI, API, SDK) from the code
[group('docs')]
docs-regen:
    bash docs/site/regen.sh

# Generate the Rust API documentation into target/doc
[group('docs')]
docs-rustdoc:
    cargo doc --no-deps --workspace --document-private-items

# Install the desktop frontend dependencies
[group('desktop')]
desktop-ui-install:
    cd crates/apollia-desktop/ui && npm ci

# Run the desktop in dev mode, linked against the bundled interpreter (STT runners: `just build-runners` first)
[group('desktop')]
desktop-dev: _require-tauri
    source "{{lib}}/python-bundle.sh"; python_bundle_activate; cd crates/apollia-desktop; cargo tauri dev

# Run the desktop in dev mode beside a background llama-server on the given model
[group('desktop')]
desktop-dev-llama model=llama_model: _require-tauri build-runners
    #!/usr/bin/env bash
    set -euo pipefail
    {{llama_settings}}
    source "{{lib}}/llama.sh"; llama_require_model "{{model}}"
    source "{{lib}}/python-bundle.sh"; python_bundle_activate
    llama_start "{{model}}" dev
    cd crates/apollia-desktop && RUST_LOG="${RUST_LOG:-debug}" cargo tauri dev

# Start a local llama-server (--jinja) as an external OpenAI-compatible backend
[group('llm')]
llama-server model=llama_model:
    @{{llama_settings}}; source "{{lib}}/llama.sh"; llama_serve "{{model}}"

# Drive the desktop with a script on a throwaway seeded HOME (the deterministic books)
[group('automation')]
automation-seeded script: _require-tauri build-runners
    @{{automation_settings}}; source "{{lib}}/automation.sh"; automation_run seeded "{{script}}"

# Same seeded HOME, plus a background llama-server for the model books
[group('automation')]
automation-seeded-llama script model=llama_model: _require-tauri build-runners
    @{{automation_settings}}; source "{{lib}}/automation.sh"; automation_run seeded "{{script}}" "{{model}}"

# First launch: a blank HOME holding only the language model and the embeddings model
[group('automation')]
automation-first-run script model=llama_model embeddings="": _require-tauri build-runners
    @{{automation_settings}}; source "{{lib}}/automation.sh"; SEED_HOME="$(automation_blank_home "{{model}}" "{{embeddings}}")"; automation_run blank "{{script}}"

# Screenshot run: the seeded HOME with the narrative overlay kept
[group('automation')]
automation-screenshots script: _require-tauri build-runners
    @{{automation_settings}}; export APOLLIA_SEED_OVERLAY={{quote(seed_overlay)}}; source "{{lib}}/automation.sh"; automation_run shooting "{{script}}"

# Drive the desktop with a script on your real profile, which the script may modify
[group('automation')]
[confirm("This runs the script against your real ~/.apollia profile, not a throwaway one. Continue?")]
automation-real script: _require-tauri build-runners
    @{{automation_settings}}; source "{{lib}}/automation.sh"; automation_run real "{{script}}"

# Same real profile, plus a background llama-server for the model books
[group('automation')]
[confirm("This runs the script against your real ~/.apollia profile, not a throwaway one. Continue?")]
automation-real-llama script model=llama_model: _require-tauri build-runners
    @{{automation_settings}}; source "{{lib}}/automation.sh"; automation_run real "{{script}}" "{{model}}"

# Read the verdict of the last seeded master run (exit 2 when the report is absent or stale)
[group('automation')]
automation-verdict:
    python3 scripts/check_automation_report.py

# Build the release CLI, for this host or for a named triple
[group('release')]
release-cli target="":
    cargo build -p apollia-cli --release {{ if target == "" { "" } else { "--target " + quote(target) } }}

# Build the CLI and the desktop bundle for any triple and runner set
[group('release')]
release-desktop target runners="":
    just release-cli {{target}}
    just build-desktop {{target}} "{{runners}}"

# Release preset for macOS on Apple Silicon (runners: cpu metal)
[group('release')]
release-macos runners="cpu metal":
    just release-desktop aarch64-apple-darwin "{{runners}}"

# Release preset for Linux x86-64 (runners: cpu)
[group('release')]
release-linux runners="cpu":
    just release-desktop x86_64-unknown-linux-gnu "{{runners}}"

# Release preset for Windows x86-64 (runners: cpu)
[group('release')]
release-windows runners="cpu":
    just release-desktop x86_64-pc-windows-msvc "{{runners}}"

# Check that this tree compiles on Linux, in a container (x86 or arm; exit 2 without Docker)
[group('linux')]
linux-check arch="x86":
    bash scripts/linux-check.sh {{arch}} compile

# Run the workspace suites on Linux, in a container (arm or x86, then an optional crate and filter)
[group('linux')]
linux-test arch="arm" *scope="":
    bash scripts/linux-check.sh {{arch}} test {{scope}}

# Prepare a linked worktree so its guards measure what the main tree measures (groups: rust ui docs full)
[group('maintainer')]
worktree-prep *GROUPS:
    bash scripts/worktree-prep.sh {{GROUPS}}

# Record the verdict of the expensive guards of this tree into a file
[group('maintainer')]
worktree-verdicts OUT:
    python3 scripts/worktree_verdicts.py --record {{OUT}}

# Compare two verdict records made on the same commit
[group('maintainer')]
worktree-compare MAIN WORKTREE:
    python3 scripts/worktree_verdicts.py --compare {{MAIN}} {{WORKTREE}}
