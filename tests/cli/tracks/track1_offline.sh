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
check_exit   "connector revoke (absent account) → 1" 1 "$BIN" connector revoke google alice@example.invalid --confirm

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

# ── A.12 tools / model / plan cache / workspace / rollback (offline) ─────────
section "A.12 local services"
check        "tools list"                            "$BIN" tools list
check_content "tools list has bash_executor" "bash_executor" "$BIN" tools list
check        "tools config get bash_executor"        "$BIN" tools config get bash_executor
check_content "model list shows 2 seeded gguf" "Qwen3.6|Phi-3" "$BIN" model list
# model delete round-trip on a throwaway stub dropped into the seed models dir.
STUB_GGUF="$DATA/models/e2e-stub-delete.gguf"; /usr/bin/touch "$STUB_GGUF"
check         "model delete --confirm"                "$BIN" model delete "e2e-stub-delete.gguf" --confirm
[[ ! -f "$STUB_GGUF" ]] && _pass "model delete removed the file" || _fail "model delete removed the file" "still present"
check        "plan cache stats"                      "$BIN" plan cache stats
check        "plan cache clear --force"              "$BIN" plan cache clear --force
check        "plan cache evict --max-age-days 7"     "$BIN" plan cache evict --max-age-days 7
check        "workspace status"                      "$BIN" workspace status
check        "workspace init --force (scratch cwd)"  bash -c "cd '$SCRATCH' && '$BIN' workspace init --force"
check        "rollback --list"                       "$BIN" rollback --list
check        "rollback --last-n 1 --dry-run"         "$BIN" rollback --last-n 1 --dry-run
check_exit   "rollback --dry-run (no target) → 1"  1 "$BIN" rollback --dry-run

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
check        "agent list (off, local fallback)"      "$BIN" --socket "$OFFSOCK" agent list
check_content "agent list shows 4 seeded agents" "apollia-chat" "$BIN" --socket "$OFFSOCK" agent list
check_exit   "agent install missing file → 1"  1     "$BIN" --socket "$OFFSOCK" agent install /tmp/never-exists.py

# ── A.14 justified offline skips (network / UI / browser) ────────────────────
section "A.14 justified skips"
skip "chat (REPL)"            "interactive rustyline REPL - covered in Track 3 via pty"
skip "update / update --check" "outbound HTTPS to api.github.com"
skip "model search / model show" "outbound HTTPS to huggingface.co"
skip "stt model download"     "whisper model HF download (~1 GB)"
skip "agent install <git-url>" "git clone over network"
skip "mcp oauth login"        "AS authorize URL + browser callback"
skip "mcp oauth discover"     "live network discovery against a remote MCP endpoint"
skip "onboard"                "interactive chat-based onboarding agent"
skip "notify test"            "dispatches a real desktop notification / webhook POST"
skip "stt transcribe"         "needs an audio file + a loaded Whisper model"
# Exact leaf paths below so the coverage report buckets them as justified-skip.
skip "tools credentials set" "masked stdin passphrase prompt (no tty in the suite)"
skip "tools credentials list" "reads the OS keychain; no seeded credentials"
skip "tools credentials delete" "mutates the OS keychain"
skip "tools credentials test" "live call to the credentialed backend"
skip "chat config authorizations list" "deferred v0.1.1 - in-memory runtime state, no HTTP route yet"
skip "chat config authorizations revoke" "deferred v0.1.1 - in-memory runtime state, no HTTP route yet"
