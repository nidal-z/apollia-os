# shellcheck shell=bash
# tests/cli/tracks/track1_offline.sh - deterministic OFFLINE track.
#
# Runs under a freshly seeded $HOME (see lib/seed.sh), so read commands assert
# KNOWN seeded content (fixed ids, fixed 2026-07-01 data) instead of empty
# states. Also asserts the exit-code contract (daemon-off → 2, validation → 1,
# clap → 2). Every command here is runnable without the runtime daemon.
#
# Sourced by cli-e2e.sh; uses $BIN, the seeded $HOME, $RUN_TMP and the assert
# helpers. Reads run before mutations; mutations target throwaway rows/dirs so
# the seeded fixtures a later assertion depends on are never destroyed.

DATA="$HOME/.apollia"
MEM="$DATA/memory"
SCRATCH="$RUN_TMP/scratch1"; /bin/mkdir -p "$SCRATCH"

# ── A.1 always-on ───────────────────────────────────────────────────────────
section "A.1 always-on"
check      "version"            "$BIN" version
check_json "version --json"     "$BIN" version --json
check      "--help"             "$BIN" --help
check      "doctor"             "$BIN" doctor
check_json "doctor --json"      "$BIN" doctor --json
check      "completions bash"   "$BIN" completions bash
check      "completions zsh"    "$BIN" completions zsh
check_grep "guide lists topics" "chat|agents" "$BIN" guide
# `inspect` loads the module statically, no runtime involved. A plain Python
# file is not an agent, and the refusal is the deterministic path.
printf 'x = 1\n' > "$SCRATCH/not-an-agent.py"
check_exit "inspect (non-agent file) → 1"  1  "$BIN" inspect "$SCRATCH/not-an-agent.py"
# `logs` tails ~/.apollia/logs/runtime.log; the seed carries no run, so the
# file is written here to exercise the read path rather than the absence path.
/bin/mkdir -p "$DATA/logs"
printf '2026-07-01T00:00:00Z INFO seeded log line\n' > "$DATA/logs/runtime.log"
check      "logs --last (seeded log file)"    "$BIN" logs --last 5
# `update` acquires /tmp/apollia-update.lock before any network call, so a held
# lock is the one deterministic, offline path through the command.
check_exit "update refuses while the lock is held" 1 bash -c "/usr/bin/touch /tmp/apollia-update.lock; '$BIN' update --yes; rc=\$?; /bin/rm -f /tmp/apollia-update.lock; exit \$rc"

# ── A.2 config (seeded apollia.toml) ────────────────────────────────────────
section "A.2 config"
check_grep   "config show has seeded [chat] workspace" "default_workspace" "$BIN" config show
check_json   "config show --json"                       "$BIN" config show --json
check        "config get chat.default_workspace"        "$BIN" config get chat.default_workspace
check_json   "config get --json"                        "$BIN" config get --json
# Mutations on a throwaway config file (never touch the seeded one).
SCFG="$SCRATCH/apollia.toml"
check        "config validate (absent file ok)"         "$BIN" config validate --file "$SCFG"
check        "config set llm.default local"             "$BIN" config set llm.default local --file "$SCFG"
check_grep   "config get llm.default = local" "local"   "$BIN" config get llm.default --file "$SCFG"
check_exit   "config edit refuses non-TTY"  1           "$BIN" config edit --file "$SCFG"
check_exit   "config reset (no --confirm)"  1           "$BIN" config reset
# reset dry-run/confirm on a disposable home dir (config reset wipes ~/.apollia).
RH="$SCRATCH/reset-home"; /bin/mkdir -p "$RH/nested"; /usr/bin/touch "$RH/dummy" "$RH/nested/inner"
check        "config reset --dry-run"                   "$BIN" config reset --dry-run --home "$RH"
[[ -f "$RH/dummy" ]] && _pass "config reset --dry-run preserved files" || _fail "config reset --dry-run preserved files" "deleted"
check        "config reset --confirm"                   "$BIN" config reset --confirm --home "$RH"
check        "hooks list --dry-run"                     "$BIN" hooks list --dry-run
check_json   "hooks list --dry-run --json"              "$BIN" --json hooks list --dry-run

# ── A.3 project (2 seeded: alpha, beta) ─────────────────────────────────────
section "A.3 project"
check_content "project list shows Seed Project Alpha" "Seed Project Alpha"  "$BIN" project list
check_content "project list shows Seed Project Beta"  "Seed Project Beta"   "$BIN" project list
check         "project show alpha"                     "$BIN" project show seed-project-alpha
check_json_field "project show alpha id"    "d['id']"          "seed-project-alpha" "$BIN" project show seed-project-alpha --json
check_json_field "project show alpha name"  "d['name']"        "Seed Project Alpha" "$BIN" project show seed-project-alpha --json
check_content "project show alpha: 3 providers" "Providers +3|Providers[[:space:]]+3" "$BIN" project show seed-project-alpha
# CRUD on a fresh project (leaves the seeded rows intact).
PID=$("$BIN" project create e2e-tmp --description demo --json 2>/dev/null | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin)['id'])" 2>/dev/null)
[[ -n "$PID" ]] && _pass "project create + JSON id parse" || { _fail "project create" ""; PID=ghost; }
check         "project update --name e2e-tmp2"         "$BIN" project update "$PID" --name e2e-tmp2
check         "project agents add"                     "$BIN" project agents add "$PID" my-agent
check_content "project agents list has my-agent" "my-agent" "$BIN" project agents list "$PID"
check         "project agents remove"                  "$BIN" project agents remove "$PID" my-agent
check         "project templates list"                 "$BIN" project templates list
check         "project templates seed-builtins"        "$BIN" project templates seed-builtins
# Link a seeded chat session to the seeded project, list, unlink.
check         "project link (seed session)"            "$BIN" project link seed-project-alpha --session seed-session-1
check         "project chats"                          "$BIN" project chats seed-project-alpha
check         "project link --unlink"                  "$BIN" project link seed-project-alpha --session seed-session-1 --unlink
check_exit    "project link empty session → 1"  1      "$BIN" project link seed-project-alpha --session ""
check         "project delete --confirm"               "$BIN" project delete "$PID" --confirm

# ── A.4 profile (3 seeded __user__ keys) ────────────────────────────────────
section "A.4 profile"
check_content "profile show has seeded language=fr" "preferences.language" "$BIN" profile show
check_content "profile show has monthly_budget"     "monthly_budget"       "$BIN" profile show
check_json    "profile show --json"                  "$BIN" profile show --json
check         "profile schema"                       "$BIN" profile schema
check         "profile export"                       "$BIN" profile export --output "$SCRATCH/u.json"
[[ -f "$SCRATCH/u.json" ]] && _pass "profile export produced file" || _fail "profile export produced file" ""
# Mutations on a throwaway profile db.
PDB="$SCRATCH/profile.db"
check        "profile set name Alice"                "$BIN" profile set name Alice --db "$PDB"
check_content "profile show throwaway has Alice" "Alice" "$BIN" profile show --db "$PDB"
check        "profile forget name"                   "$BIN" profile forget name --db "$PDB"
check        "profile import --overwrite"            "$BIN" profile import --db "$SCRATCH/prof-imported.db" --input "$SCRATCH/u.json" --overwrite
check_exit   "profile reset (no --confirm)"  1       "$BIN" profile reset --db "$PDB"
check        "profile reset --confirm"               "$BIN" profile reset --confirm --db "$PDB"

# ── A.5 chat config ─────────────────────────────────────────────────────────
section "A.5 chat config"
GDB="$SCRATCH/governance.db"
check        "chat config get (default)"             "$BIN" chat config get --db "$GDB"
check        "chat config set system-prompt"         "$BIN" chat config set system-prompt "You are helpful." --db "$GDB"
check        "chat config set allowed-tools"         "$BIN" chat config set allowed-tools "file_read,bash_executor" --db "$GDB"
check_json   "chat config get --json"                "$BIN" chat config get --db "$GDB" --json
check_exit   "chat config set bogus-key"  1          "$BIN" chat config set bogus-key x --db "$GDB"
check        "chat config reset --confirm"           "$BIN" chat config reset --confirm --db "$GDB"
check        "chat config permissions list (empty)"  "$BIN" chat config permissions list --db "$GDB"
check_json   "chat config permissions list --json"   "$BIN" chat config permissions list --db "$GDB" --json
check_exit   "chat config permissions delete (no id) → 1" 1 "$BIN" chat config permissions delete 9999 --confirm --db "$GDB"
# Session authorizations live only in the daemon's memory and the HTTP route is
# not wired for v0.1.0: both leaves refuse with that explanation, exit 1. The
# assertion pins the documented refusal, so wiring the route flips it loudly.
check_exit   "chat config authorizations list → 1 (route not wired)"   1 "$BIN" chat config authorizations list
check_exit   "chat config authorizations revoke → 1 (route not wired)" 1 "$BIN" chat config authorizations revoke seed-session-1 bash_executor

# ── A.6 permissions (4 seeded rules) ────────────────────────────────────────
section "A.6 permissions"
check_content "permissions list has bash_executor" "bash_executor" "$BIN" permissions list
check_content "permissions list has http_fetch deny" "http_fetch"  "$BIN" permissions list
check_json    "permissions list --json"             "$BIN" permissions list --json
check         "permissions audit (seeded rows)"     "$BIN" permissions audit --limit 10
check         "permissions add web_search global"   "$BIN" permissions add --tool web_search_e2e --scope global
check_content "permissions list shows the new rule" "web_search_e2e" "$BIN" permissions list
check_exit    "permissions add project w/o path"  1 "$BIN" permissions add --tool file_write --scope project
check         "permissions add deny"                "$BIN" permissions add --tool risky_tool --action deny --scope global
check         "permissions revoke 1 --yes"          "$BIN" permissions revoke 1 --yes
check_exit    "permissions revoke session-prefix → 1" 1 "$BIN" permissions revoke s42 --yes

# ── A.7 memory (5 seeded namespaces) ────────────────────────────────────────
section "A.7 memory"
check_content "memory list shows __user__ ns"       "__user__"      "$BIN" memory list --data-dir "$MEM"
check_content "memory list shows legacy-notes ns"   "legacy-notes"  "$BIN" memory list --data-dir "$MEM"
check         "memory list --json"                  "$BIN" memory list --data-dir "$MEM" --json
check         "memory inspect __user__"             "$BIN" memory inspect __user__ --data-dir "$MEM"
check_content "memory search __user__ onboarding hit" "ep-user-01|onboarding" "$BIN" memory search __user__ onboarding --data-dir "$MEM"
check_json    "memory search --json"                "$BIN" memory search __user__ onboarding --data-dir "$MEM" --json
check_exit    "memory search bad --source"  2       "$BIN" memory search __user__ x --source procedural --data-dir "$MEM"
check_exit    "memory forget unknown uuid → 1"  1   "$BIN" memory forget __user__ 00000000-0000-0000-0000-000000000000 --data-dir "$MEM"
# Mutations on a throwaway namespace (bootstrap its db first, like a first run:
# learn-procedure refuses a namespace whose <ns>.db does not exist yet).
/usr/bin/touch "$MEM/e2e-scratch.db"
"$BIN" memory inspect e2e-scratch --data-dir "$MEM" >/dev/null 2>&1 || true
check         "memory learn-procedure (scratch ns)" "$BIN" memory learn-procedure --namespace e2e-scratch --trigger "Test" --steps "1,2,3" --data-dir "$MEM"
check         "memory export scratch ns"            "$BIN" memory export --namespace e2e-scratch --output "$SCRATCH/m.apollia-memory" --data-dir "$MEM"
check         "memory import scratch2 ns"           "$BIN" memory import --namespace e2e-scratch2 --input "$SCRATCH/m.apollia-memory" --replace --data-dir "$MEM"
check         "memory clear scratch ns --confirm"   "$BIN" memory clear --agent e2e-scratch --confirm --data-dir "$MEM"
check         "memory purge scratch2 ns"            "$BIN" memory purge --namespace e2e-scratch2 --older-than 0 --data-dir "$MEM"

# ── A.8 connector ───────────────────────────────────────────────────────────
section "A.8 connector"
check        "connector list"                        "$BIN" connector list
check_json   "connector list --json"                 "$BIN" connector list --json
check        "connector accounts"                    "$BIN" connector accounts
check_exit   "connector accounts --provider notion"  1  "$BIN" connector accounts --provider notion
check_exit   "connector test (absent account)"  1    "$BIN" connector test google alice@example.invalid
check        "connector client-id list"              "$BIN" connector client-id list
check        "connector client-id set google"        "$BIN" connector client-id set google "stub-id-e2e.apps.googleusercontent.com"
check        "connector client-id clear"             "$BIN" connector client-id set google ""
check        "connector client-secret set"           "$BIN" connector client-secret set google "stub-secret-e2e"
check        "connector api-key set"                 "$BIN" connector api-key set google "stub-api-key-e2e"
check        "connector drive folder list"           "$BIN" connector drive folder list
check        "connector drive folder set"            "$BIN" connector drive folder set alice@example.invalid "Apollia/Workspace"
check        "connector drive folder reset"          "$BIN" connector drive folder reset alice@example.invalid
check        "connector drive folder picked list"    "$BIN" connector drive folder picked list alice@example.invalid
check        "connector drive folder picked remove"  "$BIN" connector drive folder picked remove alice@example.invalid ghost-folder-id
# Revoking an absent account exits 0: the storage contract is idempotent
# ("Returns Ok(()) even if the token was already gone",
# crates/apollia-auth/src/multi_account.rs, delete). The same contract is
# pinned by a unit test on run_revoke (commands/connector.rs), so a platform
# whose keyring cannot answer under a swapped HOME (macOS: no default
# keychain) records a justified SKIP, never a verdict about the environment.
# NoEntry surfacing as an error is deliberately NOT skipped: on a platform
# that answers, it is a product regression against the contract and fails.
revoke_out=$("$BIN" connector revoke google alice@example.invalid --confirm 2>&1); revoke_rc=$?
if [[ $revoke_rc -eq 0 ]]; then
    _pass "connector revoke (absent account) → 0 (idempotent)"
elif printf '%s' "$revoke_out" | /usr/bin/grep -qE "Platform secure storage failure|Couldn't access platform secure storage"; then
    skip "connector revoke (absent account) → 0 (idempotent)" "platform keyring unreachable under swapped HOME - contract pinned by the run_revoke unit test"
else
    _fail "connector revoke (absent account) → 0 (idempotent)" "expected 0 got $revoke_rc | out: ${revoke_out:0:300}" "$revoke_rc"
fi

# ── A.9 mcp (offline: list + approvals/secret/oauth; add/remove need runtime) ─
section "A.9 mcp"
MCFG="$SCRATCH/mcp.toml"
check        "mcp list (empty mcp.toml)"             "$BIN" mcp list --config "$MCFG"
check_json   "mcp list --json"                       "$BIN" mcp list --config "$MCFG" --json
MAPPR="$SCRATCH/mcp_approvals.db"
check        "mcp set-approval"                      "$BIN" mcp set-approval code-tools bash_exec --db "$MAPPR" --ttl-hours 1
check        "mcp list-pending --db"                 "$BIN" mcp list-pending --db "$MAPPR"
check        "mcp revoke-approval"                   "$BIN" mcp revoke-approval code-tools bash_exec --db "$MAPPR"
check        "mcp secret set"                        "$BIN" mcp secret set notion NOTION_API_KEY "stub-value-e2e"
check_exit   "mcp secret set empty value"  1         "$BIN" mcp secret set notion NOTION_API_KEY ""
check        "mcp secret delete"                     "$BIN" mcp secret delete notion NOTION_API_KEY
check        "mcp oauth client-id set"               "$BIN" mcp oauth client-id set APOLLIA_E2E_CLIENT_ID "stub-client-id"
check        "mcp oauth client-id clear"             "$BIN" mcp oauth client-id clear APOLLIA_E2E_CLIENT_ID
check        "mcp oauth status"                      "$BIN" mcp oauth status --db "$SCRATCH/mcp.db"
check        "mcp oauth logout --confirm (no token)" "$BIN" mcp oauth logout some-server --confirm
check_exit   "mcp oauth logout (no --confirm)"  1    "$BIN" mcp oauth logout some-server
# Both leaves resolve the server in mcp.db before any browser or network step,
# so an unconfigured name is a deterministic, offline refusal.
check_exit   "mcp oauth login (unconfigured server) → 1"    1 "$BIN" mcp oauth login e2e-unconfigured
check_exit   "mcp oauth discover (unconfigured server) → 1" 1 "$BIN" mcp oauth discover e2e-unconfigured
# `mcp server` speaks JSON-RPC over stdio and exits cleanly on stdin EOF; the
# explicit /dev/null keeps it from blocking on a terminal when run by hand.
check        "mcp server exits on stdin EOF"         bash -c "'$BIN' mcp server </dev/null"

# ── A.10 chat hygiene (4 seeded sessions) ───────────────────────────────────
section "A.10 chat hygiene"
check_exit   "chat delete (no --confirm)"  1         "$BIN" chat delete seed-session-1
check        "chat rename seed-session-4"            "$BIN" chat rename seed-session-4 "Renamed via E2E"
check_exit   "chat rename empty title"  1            "$BIN" chat rename seed-session-4 "   "
check        "chat export seed-session-1 md"         "$BIN" chat export seed-session-1 --output "$SCRATCH/c.md"
[[ -f "$SCRATCH/c.md" ]] && _pass "chat export produced .md" || _fail "chat export produced .md" ""
check        "chat export seed-session-1 json"       "$BIN" chat export seed-session-1 --output "$SCRATCH/c.json" --format json
check_json   "chat export .json is valid JSON"       /bin/cat "$SCRATCH/c.json"
check_exit   "chat export unknown session"  1        "$BIN" chat export ghost-sess-id
check        "chat delete seed-session-4 --confirm"  "$BIN" chat delete seed-session-4 --confirm

# ── A.11 llm (threshold + setup validation, no daemon) ──────────────────────
section "A.11 llm (offline)"
LCFG="$SCRATCH/llm.toml"
check        "llm costs --get-threshold (absent)"    "$BIN" llm costs --get-threshold --config "$LCFG"
check        "llm costs --threshold 0.5"             "$BIN" llm costs --threshold 0.5 --config "$LCFG"
check_grep   "llm costs threshold = 0.5" "0.5"       "$BIN" llm costs --get-threshold --config "$LCFG"
check_exit   "llm setup without --local"  1          "$BIN" llm setup --model /tmp/never.gguf
check_exit   "llm setup --local missing model"  1    "$BIN" llm setup --local --model /definitely/missing.gguf

# ── A.12 tools / model / plan cache / workspace (offline) ───────────────────
section "A.12 local services"
check        "tools list"                            "$BIN" tools list
check_content "tools list has bash_executor" "bash_executor" "$BIN" tools list
check        "tools config get bash_executor"        "$BIN" tools config get bash_executor
# Tool credentials run against the file backend here (APOLLIA_TOKEN_STORAGE=file
# is exported by the orchestrator), so nothing reaches the OS keychain. The
# seed ships one row for web_search, with a deliberately non-decryptable blob.
check_content "tools credentials list has seeded web_search" "web_search" "$BIN" tools credentials list
check_exit   "tools credentials set refuses non-TTY"  1 bash -c "'$BIN' tools credentials set web_search api_key </dev/null"
check        "tools credentials delete (absent key, idempotent)" "$BIN" tools credentials delete web_search e2e-absent-key
check_exit   "tools credentials test (seeded undecryptable blob) → 1" 1 "$BIN" tools credentials test web_search
check_content "model list shows 2 seeded gguf" "Qwen3.6|Phi-3" "$BIN" model list
# `model show` validates the org/repo shape before any network request.
check_exit   "model show (invalid spec) → 1"          1 "$BIN" model show not-an-org-repo
# `stt transcribe` checks the audio file before loading any model.
check_exit   "stt transcribe (missing file) → 1"      1 "$BIN" stt transcribe "$SCRATCH/never-recorded.wav"
# `stt model download` answers "already exists" before any network request
# when the destination file is present; the stub makes that path real.
/usr/bin/touch "$DATA/models/e2e-stt-present.bin"
check        "stt model download (already present)"   "$BIN" stt model download e2e-stt-present
# model delete round-trip on a throwaway stub dropped into the seed models dir.
STUB_GGUF="$DATA/models/e2e-stub-delete.gguf"; /usr/bin/touch "$STUB_GGUF"
check         "model delete --confirm"                "$BIN" model delete "e2e-stub-delete.gguf" --confirm
[[ ! -f "$STUB_GGUF" ]] && _pass "model delete removed the file" || _fail "model delete removed the file" "still present"
check        "plan cache stats"                      "$BIN" plan cache stats
check        "plan cache clear --force"              "$BIN" plan cache clear --force
check        "plan cache evict --max-age-days 7"     "$BIN" plan cache evict --max-age-days 7
check        "workspace status"                      "$BIN" workspace status
check        "workspace init --force (scratch cwd)"  bash -c "cd '$SCRATCH' && '$BIN' workspace init --force"

# ── A.13 exit-code contract (runtime-bound commands, daemon OFF → 2) ─────────
section "A.13 exit-code contract (daemon off)"
OFFSOCK="$RUN_TMP/off.sock"
check_exit   "status (off) → 2"        2  "$BIN" --socket "$OFFSOCK" status
check_exit   "audit list (off) → 2"    2  "$BIN" --socket "$OFFSOCK" audit list
check_exit   "audit stats (off) → 2"   2  "$BIN" --socket "$OFFSOCK" audit stats
check_exit   "llm status (off) → 2"    2  "$BIN" --socket "$OFFSOCK" llm status
check_exit   "llm backends list (off) → 2" 2 "$BIN" --socket "$OFFSOCK" llm backends list
check_exit   "task list (off) → 2"     2  "$BIN" --socket "$OFFSOCK" task list
check_exit   "trigger list (off) → 2"  2  "$BIN" --socket "$OFFSOCK" trigger list
check_exit   "notify list (off) → 2"   2  "$BIN" --socket "$OFFSOCK" notify list
check_exit   "stt status (off) → 2"    2  "$BIN" --socket "$OFFSOCK" stt status
check_exit   "resilience list (off) → 2" 2 "$BIN" --socket "$OFFSOCK" resilience list
check_exit   "a2a skills (off) → 2"    2  "$BIN" --socket "$OFFSOCK" a2a skills
check_exit   "digest (off) → 2"        2  "$BIN" --socket "$OFFSOCK" digest --since 24h
check_exit   "chat --list (off) → 1"   1  "$BIN" --socket "$OFFSOCK" chat --list
check_exit   "stop (off) → 2"          2  "$BIN" --socket "$OFFSOCK" stop
check_exit   "onboard (off) → 2"       2  "$BIN" --socket "$OFFSOCK" onboard
# `model search` reaches the runtime before HuggingFace, and its daemon-off
# refusal exits 1 where the other runtime-bound leaves exit 2.
check_exit   "model search (off) → 1"  1  "$BIN" --socket "$OFFSOCK" model search whisper
check        "agent list (off, local fallback)"      "$BIN" --socket "$OFFSOCK" agent list
check_content "agent list shows 4 seeded agents" "apollia-chat" "$BIN" --socket "$OFFSOCK" agent list
check_exit   "agent install missing file → 1"  1     "$BIN" --socket "$OFFSOCK" agent install /tmp/never-exists.py

# ── A.14 un-exercised VARIANTS (every leaf itself has a real invocation) ─────
# Coverage counts real invocations only (scripts/check_cli_e2e_coverage.py), so
# a skip line justifies nothing anymore. The lines below record the variants
# the suite still does not drive, labelled as variants so no leaf appears both
# exercised and skipped.
section "A.14 un-exercised variants"
skip "chat (interactive REPL turn)"   "covered in Track 3 via pty capture"
skip "update (live GitHub check)"     "outbound HTTPS to api.github.com; the lock refusal is asserted above"
skip "model search (live HF query)"   "outbound HTTPS to huggingface.co; the daemon-off refusal is asserted above"
skip "stt model download (real download)" "whisper model HF download (~1 GB); the already-present path is asserted above"
skip "agent install <git-url>"        "git clone over network; file install is asserted in Track 2"
skip "mcp oauth login (browser flow)" "AS authorize URL + browser callback; the unconfigured refusal is asserted above"
skip "mcp oauth discover (live endpoint)" "network discovery; the unconfigured refusal is asserted above"
skip "onboard (interactive session)"  "chat-based onboarding agent; the daemon-off refusal is asserted above"
skip "stt transcribe (real audio)"    "needs an audio file + a loaded Whisper model; the missing-file refusal is asserted above"
skip "tools credentials set (with a TTY)" "masked stdin passphrase prompt; the non-TTY refusal is asserted above"
skip "tools credentials test (decryptable credential)" "needs a live credentialed backend; the seeded undecryptable blob is asserted above"
