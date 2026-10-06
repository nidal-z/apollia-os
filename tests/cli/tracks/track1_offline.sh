# shellcheck shell=bash
# tests/cli/tracks/track1_offline.sh - deterministic OFFLINE track.
#
# Runs under a freshly seeded $HOME (see lib/seed.sh), so read commands assert
# KNOWN seeded content (fixed ids, fixed 2026-07-01 data) instead of empty
# states. Also asserts the exit-code contract (daemon-off → 2, validation → 1,
# clap usage refusal → 1, the published contract). Every command here is
# runnable without the runtime daemon.
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
# `update` acquires apollia-update.lock in the platform temp directory
# (std::env::temp_dir: $TMPDIR on Unix, /tmp as the fallback) before any
# network call, so a held lock is the one deterministic, offline path
# through the command.
check_exit "update refuses while the lock is held" 1 bash -c "/usr/bin/touch \"\${TMPDIR:-/tmp}/apollia-update.lock\"; '$BIN' update --yes; rc=\$?; /bin/rm -f \"\${TMPDIR:-/tmp}/apollia-update.lock\"; exit \$rc"

# ── A.2 config (seeded apollia.toml) ────────────────────────────────────────
section "A.2 config"
check_grep   "config show has seeded [chat] temperature" "tool_turn_temperature" "$BIN" config show
check_json   "config show --json"                       "$BIN" config show --json
check        "config get chat.tool_turn_temperature"    "$BIN" config get chat.tool_turn_temperature
check_json   "config get --json"                        "$BIN" config get --json
# Mutations on a throwaway config file (never touch the seeded one).
SCFG="$SCRATCH/apollia.toml"
check        "config validate (absent file ok)"         "$BIN" config validate --file "$SCFG"
check        "config set llm.default local"             "$BIN" config set llm.default local --file "$SCFG"
check_grep   "config get llm.default = local" "local"   "$BIN" config get llm.default --file "$SCFG"
# One schema, strict at every level: a typo inside a known section is refused
# and nothing is written; an invalid file is named at its line, by
# `config validate` and by a daemon that refuses to start on it.
cp "$SCFG" "$SCRATCH/apollia.toml.before"
check_exit   "config set api.typo refused → 1"  1      "$BIN" config set api.typo 1 --file "$SCFG"
if cmp -s "$SCFG" "$SCRATCH/apollia.toml.before"; then
    _pass "config set api.typo left the file unchanged"
else
    _fail "config set api.typo left the file unchanged" "the file changed"
fi
BADCFG="$SCRATCH/invalid.toml"
printf '[api]\nbind = "127.0.0.1"\ntypo = 1\n' > "$BADCFG"
check_exit   "config validate (unknown key) → 1"  1    "$BIN" config validate --file "$BADCFG"
bad_out=$("$BIN" --socket "$SCRATCH/invalid.sock" start --port 47917 --config "$BADCFG" 2>&1 >/dev/null); bad_rc=$?
if [[ $bad_rc -eq 1 ]] && /usr/bin/grep -qF "invalid.toml:3:1: api.typo" <<<"$bad_out"; then
    _pass "start refuses an invalid config, fault at its line on stderr"
else
    _fail "start refuses an invalid config, fault at its line on stderr" "rc=$bad_rc | out: ${bad_out:0:300}"
fi
check_exit   "config edit refuses non-TTY"  1           "$BIN" config edit --file "$SCFG"
check_exit   "config reset (no --confirm)"  1           "$BIN" config reset
# reset dry-run/confirm on a disposable home dir (config reset wipes ~/.apollia).
RH="$SCRATCH/reset-home"; /bin/mkdir -p "$RH/nested"; /usr/bin/touch "$RH/dummy" "$RH/nested/inner"
check        "config reset --dry-run"                   "$BIN" config reset --dry-run --home "$RH"
[[ -f "$RH/dummy" ]] && _pass "config reset --dry-run preserved files" || _fail "config reset --dry-run preserved files" "deleted"
check        "config reset --confirm"                   "$BIN" config reset --confirm --home "$RH"
# A first `config` command under an empty home writes the documented default
# file at ~/.apollia/apollia.toml; a present file is never written over.
FH="$SCRATCH/first-home"; /bin/mkdir -p "$FH"
check        "config show (empty home)"                 env HOME="$FH" "$BIN" config show
if [[ -f "$FH/.apollia/apollia.toml" ]] && /usr/bin/head -1 "$FH/.apollia/apollia.toml" | /usr/bin/grep -q "^# apollia.toml, the configuration of Apollia OS"; then
    _pass "config show wrote the default file with its header"
else
    _fail "config show wrote the default file with its header" "absent or without its header"
fi
check_json   "config keys --json"                       "$BIN" config keys --json
check_grep   "config keys lists api.port" "api.port"    "$BIN" config keys
check        "hooks list --dry-run"                     "$BIN" hooks list --dry-run
check_json   "hooks list --dry-run --json"              "$BIN" --json hooks list --dry-run

# ── A.3 project (retired) ───────────────────────────────────────────────────
# Projects became sources: the noun is gone, refused as a usage error (1).
section "A.3 project"
check_exit    "project list (retired) → 1"       1     "$BIN" project list
check_exit    "project link (retired) → 1"       1     "$BIN" project link seed-project-alpha --session seed-session-1

# ── A.4 profile (3 seeded __user__ keys) ────────────────────────────────────
# The seed is a profile written before profiles existed: the first open migrates
# `preferences.language` into the `preferences` field of the default profile.
section "A.4 profile"
check_json_field "profile show: language migrated into preferences" \
  "[e['value'] for e in d['entries'] if e['key']=='preferences'][0]" "en" "$BIN" profile show --json
check_content "profile show has monthly_budget"     "monthly_budget"       "$BIN" profile show
check_json    "profile show --json"                  "$BIN" profile show --json
check_json_field "profile list: the migrated profile is the one default" \
  "[p['name'] for p in d if p['is_default']]" "['Mon profil']" "$BIN" profile list --json
check         "profile schema"                       "$BIN" profile schema
check         "profile export"                       "$BIN" profile export --output "$SCRATCH/u.json"
[[ -f "$SCRATCH/u.json" ]] && _pass "profile export produced file" || _fail "profile export produced file" ""
# Mutations on a throwaway profile db.
PDB="$SCRATCH/profile.db"
check        "profile set default name Alice"        "$BIN" profile set default name Alice --db "$PDB"
check_content "profile show throwaway has Alice" "Alice" "$BIN" profile show --db "$PDB"
check        "profile forget name"                   "$BIN" profile forget name --confirm --db "$PDB"
check        "profile import --overwrite"            "$BIN" profile import --db "$SCRATCH/prof-imported.db" --input "$SCRATCH/u.json" --overwrite
# Named profiles: create, fill, rename, make default, delete; the default is never deleted.
FIRST=$("$BIN" profile show --json --db "$PDB" 2>/dev/null | "${E2E_PYTHON:-python3}" -c "import sys,json;print(json.load(sys.stdin)['profile']['id'])" 2>/dev/null)
SECOND=$("$BIN" profile create Formateur --json --db "$PDB" 2>/dev/null | "${E2E_PYTHON:-python3}" -c "import sys,json;print(json.load(sys.stdin)['id'])" 2>/dev/null)
[[ -n "$FIRST" && -n "$SECOND" ]] && _pass "profile create returns an identifier" || _fail "profile create returns an identifier" "first='$FIRST' second='$SECOND'"
check_json_field "profile list: two profiles"        "len(d)" "2"           "$BIN" profile list --json --db "$PDB"
check         "profile set <id> role"                "$BIN" profile set "$SECOND" role Formatrice --db "$PDB"
check_json_field "profile show <id>: role written on that profile" \
  "d['profile']['role']" "Formatrice" "$BIN" profile show "$SECOND" --json --db "$PDB"
check_json_field "profile show default: role not on the default" \
  "d['profile']['role']" "None" "$BIN" profile show default --json --db "$PDB"
check_json_field "profile rename" "d['name']" "Formation" "$BIN" profile rename "$SECOND" Formation --json --db "$PDB"
check_exit   "profile delete default (refused)" 1    "$BIN" profile delete "$FIRST" --confirm --db "$PDB"
check_grep   "profile delete default says why"  "default profile and cannot be deleted" \
  bash -c '"$0" profile delete "$1" --confirm --db "$2" 2>&1; true' "$BIN" "$FIRST" "$PDB"
check_json_field "profile delete default --json envelope" "d['error']['code']" "general_error" \
  bash -c '"$0" profile delete "$1" --confirm --json --db "$2"; true' "$BIN" "$FIRST" "$PDB"
check_exit   "profile show unknown id"          1    "$BIN" profile show no-such-profile --db "$PDB"
check_exit   "profile delete (no --confirm)"    1    "$BIN" profile delete "$SECOND" --db "$PDB"
check_json_field "profile set-default <id>" "d['is_default']" "True" "$BIN" profile set-default "$SECOND" --json --db "$PDB"
check        "profile delete former default"         "$BIN" profile delete "$FIRST" --confirm --db "$PDB"
check_json_field "profile list: one profile left, the default" \
  "[(p['name'], p['is_default']) for p in d]" "[('Formation', True)]" "$BIN" profile list --json --db "$PDB"
check_exit   "profile reset (no --confirm)"  1       "$BIN" profile reset --db "$PDB"
check        "profile reset --confirm"               "$BIN" profile reset --confirm --db "$PDB"

# ── A.5 chat config ─────────────────────────────────────────────────────────
section "A.5 chat config"
GDB="$SCRATCH/governance.db"
# The settings of the chat are those of the Apollia agent, read and written
# through the runtime's routes: with no runtime, get / set / reset exit 2. The
# real reads and writes are in Track 2. A bad key is refused before the runtime.
CHAT_OFF="$RUN_TMP/chat-off.sock"
check_exit   "chat config get (runtime absent) → 2"  2 "$BIN" --socket "$CHAT_OFF" chat config get
check_exit   "chat config get --json (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" chat config get --json
check_exit   "chat config set system-prompt (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" chat config set system-prompt "You are helpful."
check_exit   "chat config set bogus-key"  1          "$BIN" --socket "$CHAT_OFF" chat config set bogus-key x
check_exit   "chat config reset --confirm (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" chat config reset --confirm
# What the model of a session can do is read from the runtime: with no
# runtime, exit 2. The real read is covered by the runtime's route tests.
check_exit   "chat capabilities (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" chat capabilities s-1
check_exit   "chat capabilities --json (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" chat capabilities s-1 --json
# Apollia is built in: uninstalling it is refused, exit 1.
check_exit   "agent uninstall apollia:chat (built in) → 1" 1 "$BIN" agent uninstall apollia:chat --confirm
# Every other uninstall goes through the runtime, which unregisters the agents
# and journals the removal: with no runtime nothing is removed, exit 2. The
# real removal is in Track 2.
check_exit   "agent uninstall (no --confirm) → 1" 1 "$BIN" --socket "$CHAT_OFF" agent uninstall e2e-notes
check_exit   "agent uninstall --clear-memory (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" agent uninstall e2e-notes --clear-memory --confirm
check_exit   "agent package uninstall --clear-history --json (runtime absent) → 2" 2 "$BIN" --socket "$CHAT_OFF" agent package uninstall e2e-pack --clear-memory --clear-history --confirm --json
check        "chat config permissions list (empty)"  "$BIN" chat config permissions list --db "$GDB"
check_json   "chat config permissions list --json"   "$BIN" chat config permissions list --db "$GDB" --json
check_exit   "chat config permissions delete (no id) → 1" 1 "$BIN" chat config permissions delete 9999 --confirm --db "$GDB"
# Session authorizations live only in the daemon's memory and the HTTP route is
# not wired for v0.1.0: both leaves refuse with that explanation, exit 1. The
# assertion pins the documented refusal, so wiring the route flips it loudly.
check_exit   "chat config authorizations list → 1 (route not wired)"   1 "$BIN" chat config authorizations list
check_exit   "chat config authorizations revoke → 1 (route not wired)" 1 "$BIN" chat config authorizations revoke seed-session-1 bash_executor --confirm

# ── A.6 permissions (4 seeded rules) ────────────────────────────────────────
section "A.6 permissions"
check_content "permissions list has bash_executor" "bash_executor" "$BIN" permissions list
check_content "permissions list has http_fetch deny" "http_fetch"  "$BIN" permissions list
check_json    "permissions list --json"             "$BIN" permissions list --json
check         "permissions audit (seeded rows)"     "$BIN" permissions audit --limit 10
check         "permissions add web_search global"   "$BIN" permissions add --tool web_search_e2e --scope global
check_content "permissions list shows the new rule" "web_search_e2e" "$BIN" permissions list
check_exit    "permissions add --scope project refused → 1" 1 "$BIN" permissions add --tool file_write --scope project
check_exit    "permissions add --scope agent w/o --agent → 1" 1 "$BIN" permissions add --tool file_write --scope agent
check         "permissions add agent rule"          "$BIN" permissions add --tool http_fetch_e2e --scope agent --agent e2e-agent
check_content "permissions list --scope agent has it" "http_fetch_e2e" "$BIN" permissions list --scope agent
check_exit    "permissions list --scope session → 1" 1 "$BIN" permissions list --scope session
check         "permissions add deny"                "$BIN" permissions add --tool risky_tool --action deny --scope global
check         "permissions revoke 1 --yes"          "$BIN" permissions revoke 1 --yes
check_exit    "permissions revoke non-numeric id → 1" 1 "$BIN" permissions revoke s42 --yes

# ── A.7 memory (5 seeded namespaces) ────────────────────────────────────────
section "A.7 memory"
check_content "memory list shows __user__ ns"       "__user__"      "$BIN" memory list --data-dir "$MEM"
check_content "memory list shows legacy-notes ns"   "legacy-notes"  "$BIN" memory list --data-dir "$MEM"
check         "memory list --json"                  "$BIN" memory list --data-dir "$MEM" --json
check         "memory inspect __user__"             "$BIN" memory inspect __user__ --data-dir "$MEM"
check_content "memory search __user__ onboarding hit" "ep-user-01|onboarding" "$BIN" memory search __user__ onboarding --data-dir "$MEM"
check_json    "memory search --json"                "$BIN" memory search __user__ onboarding --data-dir "$MEM" --json
check_exit    "memory search bad --source"  1       "$BIN" memory search __user__ x --source procedural --data-dir "$MEM"
check_exit    "memory forget unknown uuid → 1"  1   "$BIN" memory forget __user__ 00000000-0000-0000-0000-000000000000 --confirm --data-dir "$MEM"
# Mutations on a throwaway namespace (bootstrap its db first, like a first run:
# learn-procedure refuses a namespace whose <ns>.db does not exist yet).
/usr/bin/touch "$MEM/e2e-scratch.db"
"$BIN" memory inspect e2e-scratch --data-dir "$MEM" >/dev/null 2>&1 || true
check         "memory learn-procedure (scratch ns)" "$BIN" memory learn-procedure --namespace e2e-scratch --trigger "Test" --steps "1,2,3" --data-dir "$MEM"
check         "memory export scratch ns"            "$BIN" memory export --namespace e2e-scratch --output "$SCRATCH/m.apollia-memory" --data-dir "$MEM"
check         "memory import scratch2 ns"           "$BIN" memory import --namespace e2e-scratch2 --input "$SCRATCH/m.apollia-memory" --replace --data-dir "$MEM"
check         "memory clear scratch ns --confirm"   "$BIN" memory clear --agent e2e-scratch --confirm --data-dir "$MEM"
check         "memory purge scratch2 ns"            "$BIN" memory purge --namespace e2e-scratch2 --older-than 0 --confirm --data-dir "$MEM"

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
check        "connector drive folder reset"          "$BIN" connector drive folder reset alice@example.invalid --confirm
check        "connector drive folder picked list"    "$BIN" connector drive folder picked list alice@example.invalid
check        "connector drive folder picked remove"  "$BIN" connector drive folder picked remove alice@example.invalid ghost-folder-id --confirm
# IMAP mailbox: offline, no keychain. `list` reads the seeded data dir (no
# mailbox); `add` without a password stops before the secret store; `test`
# refuses an unknown domain before any connection, and reports a closed
# loopback port as unreachable. Each failure carries its typed code.
check        "connector imap list"                   "$BIN" connector imap list
check_json   "connector imap list --json"            "$BIN" connector imap list --json
imap_case() {
    local label=$1 code=$2; shift 2
    local out rc
    out=$("$@" 2>&1); rc=$?
    if [[ $rc -eq 1 ]] && printf '%s' "$out" | /usr/bin/grep -q "\"$code: "; then
        _pass "$label → 1 ($code)"
    else
        _fail "$label → 1 ($code)" "got $rc | out: ${out:0:300}" "$rc"
    fi
}
imap_case "connector imap add (no password)" missing_password \
    env -u APOLLIA_IMAP_PASSWORD "$BIN" connector imap add ana@gmail.com --json
imap_case "connector imap test (unknown domain)" unknown_provider \
    env APOLLIA_IMAP_PASSWORD=e2e "$BIN" connector imap test ana@example.invalid --json
imap_case "connector imap test (closed port)" unreachable \
    env APOLLIA_IMAP_PASSWORD=e2e "$BIN" connector imap test ana@example.invalid --host 127.0.0.1 --port 9 --json
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
check        "mcp secret delete"                     "$BIN" mcp secret delete notion NOTION_API_KEY --confirm
check        "mcp oauth client-id set"               "$BIN" mcp oauth client-id set APOLLIA_E2E_CLIENT_ID "stub-client-id"
check        "mcp oauth client-id clear"             "$BIN" mcp oauth client-id clear APOLLIA_E2E_CLIENT_ID --confirm
check        "mcp oauth status"                      "$BIN" mcp oauth status --db "$SCRATCH/mcp.db"
check        "mcp oauth logout --confirm (no token)" "$BIN" mcp oauth logout some-server --confirm
check_exit   "mcp oauth logout (no --confirm)"  1    "$BIN" mcp oauth logout some-server
# Both leaves resolve the server in mcp.db before any browser or network step,
# so an unconfigured name is a deterministic, offline refusal.
check_exit   "mcp oauth login (unconfigured server) → 1"    1 "$BIN" mcp oauth login e2e-unconfigured
check_exit   "mcp oauth discover (unconfigured server) → 1" 1 "$BIN" mcp oauth discover e2e-unconfigured
# `mcp server` is a stdio bridge to the daemon and serves no tool of its own:
# with no daemon on its socket it stops at once, exit 2, naming the command
# that starts one. The native-tool flags it used to take are gone.
MCP_OFF="$RUN_TMP/mcp-off.sock"
check_exit   "mcp server (no daemon) → 2"  2         bash -c "'$BIN' --socket '$MCP_OFF' mcp server </dev/null"
check        "mcp server (no daemon) names apollia-os start" bash -c "out=\$('$BIN' --socket '$MCP_OFF' mcp server </dev/null 2>&1); [[ \$out == *'apollia-os start'* ]] || { echo \"\$out\"; false; }"
check_exit   "mcp server --with-runtime (retired) → 1"  1 "$BIN" mcp server --with-runtime
check_exit   "mcp server --sandbox-root (retired) → 1"  1 "$BIN" mcp server --sandbox-root /tmp

# ── A.10 chat hygiene (4 seeded sessions) ───────────────────────────────────
section "A.10 chat hygiene"
check_exit   "chat delete (no --confirm)"  1         "$BIN" chat delete seed-session-1
# `chat list` reads chat.db with no runtime: the four seeded sessions, each
# with the folder it is in. The first call opens and migrates chat.db, so the
# folder tables exist for the rows written next the way the desktop writes
# them; the folder is removed again so the seed counts stay what they were.
check_content "chat list names a seeded session"     "seed-session-1 .*Discovering the sovereign runtime" "$BIN" chat list
check_json_field "chat list --json lists the four sessions" "len(d)" "4" "$BIN" chat list --json
check_json_field "chat list --json: no folder yet"   "[s['folder'] for s in d if s['id']=='seed-session-1'][0]" "None" "$BIN" chat list --json
e2e_sqlite "$HOME/.apollia/chat.db" "INSERT INTO chat_folders (id, name, position) VALUES ('e2e-folder', 'E2E clients', 0); UPDATE chat_sessions SET folder_id = 'e2e-folder' WHERE id = 'seed-session-1';"
check_content "chat list shows the folder of a session" "seed-session-1 +E2E clients" "$BIN" chat list
check_json_field "chat list --json carries the folder" "[s['folder'] for s in d if s['id']=='seed-session-1'][0]" "E2E clients" "$BIN" chat list --json
check_json_field "chat list --json: the others have none" "sorted({str(s['folder']) for s in d})" "['E2E clients', 'None']" "$BIN" chat list --json
e2e_sqlite "$HOME/.apollia/chat.db" "UPDATE chat_sessions SET folder_id = NULL WHERE folder_id = 'e2e-folder'; DELETE FROM chat_folders WHERE id = 'e2e-folder';"
check_exit   "chat list (no database) -> 1"  1       "$BIN" chat list --db "$SCRATCH/absent-chat.db"
check        "chat rename seed-session-4"            "$BIN" chat rename seed-session-4 "Renamed via E2E"
check_exit   "chat rename empty title"  1            "$BIN" chat rename seed-session-4 "   "
check        "chat export seed-session-1 md"         "$BIN" chat export seed-session-1 --output "$SCRATCH/c.md"
[[ -f "$SCRATCH/c.md" ]] && _pass "chat export produced .md" || _fail "chat export produced .md" ""
check        "chat export seed-session-1 json"       "$BIN" chat export seed-session-1 --output "$SCRATCH/c.json" --format json
check_json   "chat export .json is valid JSON"       /bin/cat "$SCRATCH/c.json"
check_exit   "chat export unknown session"  1        "$BIN" chat export ghost-sess-id
check_content "chat delete --help says it is for good" "cannot be restored" "$BIN" chat delete --help
# A session the desktop sent to the trash (`deleted_at` set) is erased by
# `chat delete` like any other: the row is gone, not kept for a restore. The
# column exists by now, since `chat rename` above opened and migrated chat.db.
e2e_sqlite "$HOME/.apollia/chat.db" "UPDATE chat_sessions SET deleted_at = '2026-07-02T00:00:00Z' WHERE id = 'seed-session-4';"
check        "chat delete seed-session-4 --confirm"  "$BIN" chat delete seed-session-4 --confirm
TRASH_LEFT=$(e2e_sqlite "$HOME/.apollia/chat.db" "SELECT COUNT(*) FROM chat_sessions WHERE id = 'seed-session-4';")
[[ "$TRASH_LEFT" == "0" ]] && _pass "chat delete erased the trashed session for good" || _fail "chat delete erased the trashed session for good" "rows left: $TRASH_LEFT"

# ── A.11 llm (setup validation, no daemon) ──────────────────────────────────
section "A.11 llm (offline)"
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
check_exit   "tools credentials set refuses an empty piped value" 1 bash -c "'$BIN' tools credentials set web_search api_key </dev/null"
check        "tools credentials delete (absent key, idempotent)" "$BIN" tools credentials delete web_search e2e-absent-key --confirm
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
check        "plan cache evict --max-age-days 7"     "$BIN" plan cache evict --max-age-days 7 --confirm
check        "workspace status"                      "$BIN" workspace status
check        "workspace init --force (scratch cwd)"  bash -c "cd '$SCRATCH' && '$BIN' workspace init --force"

# ── A.13a package archives (.apo, files only) ───────────────────────────────
# A package folder is written here, built into an archive, and read back. The
# signed build stores its author key through the file secret store the suite
# forces (APOLLIA_TOKEN_STORAGE=file), under the seeded HOME.
section "A.13a package archives"
PKG="$SCRATCH/pkg"; /bin/mkdir -p "$PKG/tools"
cat > "$PKG/apollia.toml" <<'TOML'
[package]
name = "e2e-pkg"
version = "0.1.0"
publisher = "E2E Publisher"
sdk = ">=0.3"

[[tools]]
name = "ping"
module = "tools/ping.py"
description = "Answer pong."
action = "read"
inputs = { type = "object" }
TOML
printf "@tool(name='ping')\nasync def ping(ctx, inputs):\n    return 'pong'\n" > "$PKG/tools/ping.py"
check      "package build (unsigned)"                     "$BIN" package build "$PKG" -o "$SCRATCH/e2e.apo"
check_json_field "package inspect: unsigned"  "d['signature']['state']"  "unsigned"  "$BIN" package inspect "$SCRATCH/e2e.apo" --json
check_json_field "package inspect: manifest name" "d['manifest']['package']['name']" "e2e-pkg" "$BIN" package inspect "$SCRATCH/e2e.apo" --json
check_json_field "package inspect: entries" "' '.join(e['path'] for e in d['entries'])" "apollia.toml tools/ping.py" "$BIN" package inspect "$SCRATCH/e2e.apo" --json
check_content "package inspect (human) lists the module" "tools/ping.py" "$BIN" package inspect "$SCRATCH/e2e.apo"
check_json_field "package build --sign: signed" "d['signature']['state']" "signed" "$BIN" package build "$PKG" --sign -o "$SCRATCH/e2e-signed.apo" --json
check_json_field "package inspect: publisher" "d['signature']['publisher']" "E2E Publisher" "$BIN" package inspect "$SCRATCH/e2e-signed.apo" --json
# Rewriting one entry after signing must turn the archive into a refusal.
"${E2E_PYTHON:-python3}" - "$SCRATCH/e2e-signed.apo" "$SCRATCH/e2e-tampered.apo" <<'PY'
import sys, zipfile
src, dst = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
    for info in zin.infolist():
        data = zin.read(info.filename)
        if info.filename == "tools/ping.py":
            data = b"import os\n"
        zout.writestr(info.filename, data)
PY
check_exit "package inspect (tampered after signing) → 1" 1 "$BIN" package inspect "$SCRATCH/e2e-tampered.apo"
# A module defining a workflow the manifest does not describe: descriptors missing.
printf "@workflow(name='report', steps=['a'])\nasync def report(ctx, inputs):\n    pass\n" > "$PKG/tools/report.py"
check_exit "package build (undescribed module) → 1" 1 "$BIN" package build "$PKG" -o "$SCRATCH/e2e-bad.apo"
check_exit "package inspect (not an archive) → 1"   1 "$BIN" package inspect "$PKG/apollia.toml"
# The preview, the installation and the keyring go through the runtime: with no
# runtime they answer 2. A package file that does not exist is refused before
# the runtime is reached, 1.
PKGOFF="$RUN_TMP/package-off.sock"
E2E_KEY="ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
check_exit "package preview (runtime absent) → 2"       2 "$BIN" --socket "$PKGOFF" package preview "$SCRATCH/e2e.apo"
check_exit "package install (runtime absent) → 2"       2 "$BIN" --socket "$PKGOFF" package install "$SCRATCH/e2e.apo"
check_exit "package preview (missing file) → 1"         1 "$BIN" --socket "$PKGOFF" package preview "$SCRATCH/no-such.apo"
check_exit "package install --json (missing file) → 1"  1 "$BIN" --socket "$PKGOFF" package install "$SCRATCH/no-such.apo" --json
check_exit "package trust import (runtime absent) → 2"  2 "$BIN" --socket "$PKGOFF" package trust import "$E2E_KEY" --label "E2E"
check_exit "package trust list (runtime absent) → 2"    2 "$BIN" --socket "$PKGOFF" package trust list
check_exit "package trust remove (runtime absent) → 2"  2 "$BIN" --socket "$PKGOFF" package trust remove "$E2E_KEY" --confirm
# The decisions waiting for the user live in the runtime: with no runtime the
# list answers 2, in human and in JSON form. Track 2 lists them, empty.
check_exit "approvals pending (runtime absent) → 2"        2 "$BIN" --socket "$PKGOFF" approvals pending
check_exit "approvals pending --json (runtime absent) → 2" 2 "$BIN" --socket "$PKGOFF" approvals pending --json
# Answering one goes through the runtime too. An answer to a question with no
# `--value` is refused before the runtime is reached.
check_exit "approvals decide (runtime absent) → 2"         2 "$BIN" --socket "$PKGOFF" approvals decide workflow-question:none refuse
check_exit "approvals decide answer without --value → 1"   1 "$BIN" --socket "$PKGOFF" approvals decide workflow-question:none answer

# ── A.13b organisation policy (files only) ──────────────────────────────────
# The key pair and the policy folder are prepared under the scratch folder;
# nothing is written to the system policy folder. `policy show` reads the
# system folder, absent on a test machine, so it answers "none".
section "A.13b organisation policy"
POLKEYS="$SCRATCH/policy-keys"; POL="$SCRATCH/policy"
check_content "policy keygen prints the fingerprint" '^[0-9a-f]{64}$' "$BIN" policy keygen --out "$POLKEYS" -q
check_content "policy keygen: org.key is 0600" '^0o600$' "${E2E_PYTHON:-python3}" -c 'import os,sys;print(oct(os.stat(sys.argv[1]).st_mode & 0o777))' "$POLKEYS/org.key"
check_exit "policy keygen (key already there) → 1" 1 "$BIN" policy keygen --out "$POLKEYS"
check_content "policy init writes policy.toml" 'policy\.toml$' "$BIN" policy init "$POL" -q
check_json_field "policy sign: publisher" "d['publisher']" "E2E IT" "$BIN" policy sign --key "$POLKEYS/org.key" --publisher "E2E IT" "$POL" --json
check_json_field "policy verify: valid" "d['verdict']" "valid" "$BIN" policy verify "$POL" --json
check_content "policy verify (human)" '^valid$' "$BIN" policy verify "$POL"
# One byte of the signed policy is altered: the signature no longer holds.
"${E2E_PYTHON:-python3}" - "$POL/policy.toml" <<'PY'
import sys
path = sys.argv[1]
data = bytearray(open(path, "rb").read())
data[0] ^= 0x01
open(path, "wb").write(bytes(data))
PY
check_exit "policy verify (one byte altered) → 1" 1 "$BIN" policy verify "$POL"
check_content "policy verify --json (one byte altered): reason" '"reason": "signature"' bash -c "'$BIN' policy verify '$POL' --json; true"
check_content "policy verify (human, one byte altered): reason" 'refused \(signature\)' bash -c "'$BIN' policy verify '$POL'; true"
check_exit "policy verify (missing folder) → 1" 1 "$BIN" policy verify "$SCRATCH/no-such-policy"
check_content "policy show: none" '^none$' "$BIN" policy show
check_json_field "policy show --json: none" "d['state']" "none" "$BIN" policy show --json

# ── A.13 exit-code contract (runtime-bound commands, daemon OFF → 2) ─────────
section "A.13 exit-code contract (daemon off)"
OFFSOCK="$RUN_TMP/off.sock"
check_exit   "status (off) → 2"        2  "$BIN" --socket "$OFFSOCK" status
check_exit   "audit list (off) → 2"    2  "$BIN" --socket "$OFFSOCK" audit list
check_exit   "audit stats (off) → 2"   2  "$BIN" --socket "$OFFSOCK" audit stats
# The usage error of an unknown flag is exit 2 as well, so the message is read
# too: every filter is parsed, and the refusal is the runtime being off.
check_exit   "audit journal, every filter (off) → 2" 2 "$BIN" --socket "$OFFSOCK" audit journal --agent flux-quotes --family read --family send_external --nature action --result refused --since 2026-10-01 --until 2026-10-04
check_content "audit journal, every filter (off): refused by the runtime being off" 'runtime not started' bash -c "'$BIN' --socket '$OFFSOCK' audit journal --agent flux-quotes --family read --family send_external --nature action --result refused --since 2026-10-01 --until 2026-10-04; true"
# The export writes its file itself, from what the runtime answers: with the
# runtime off it is refused for that reason and the file is not created. A
# missing --format or --output, and a format the journal is not written in,
# are usage errors, told apart from the runtime being off by their message.
check_exit   "audit export, detail and every filter (off) → 2" 2 "$BIN" --socket "$OFFSOCK" audit export --format csv --detail --agent flux-quotes --family read --nature action --result refused --since 2026-10-01 --until 2026-10-04 --output "$RUN_TMP/journal-off.csv"
check_content "audit export (off): refused by the runtime being off" 'runtime not started' bash -c "'$BIN' --socket '$OFFSOCK' audit export --format json --output '$RUN_TMP/journal-off.json'; true"
check_content "audit export (off): no file is written" '^absent$' bash -c "'$BIN' --socket '$OFFSOCK' audit export --format csv --output '$RUN_TMP/journal-off.csv' >/dev/null 2>&1; test -e '$RUN_TMP/journal-off.csv' && echo present || echo absent"
check_content "audit export without --output: usage error" 'required arguments were not provided' bash -c "'$BIN' --socket '$OFFSOCK' audit export --format csv 2>&1; true"
check_content "audit export --format pdf: usage error naming the formats" 'possible values: csv, json' bash -c "'$BIN' --socket '$OFFSOCK' audit export --format pdf --output '$RUN_TMP/journal-off.pdf' 2>&1; true"
check_exit   "llm status (off) → 2"    2  "$BIN" --socket "$OFFSOCK" llm status
check_exit   "llm backends list (off) → 2" 2 "$BIN" --socket "$OFFSOCK" llm backends list
check_exit   "task list (off) → 2"     2  "$BIN" --socket "$OFFSOCK" task list
check_exit   "trigger list (off) → 2"  2  "$BIN" --socket "$OFFSOCK" trigger list
check_exit   "notify list (off) → 2"   2  "$BIN" --socket "$OFFSOCK" notify list
check_exit   "stt status (off) → 2"    2  "$BIN" --socket "$OFFSOCK" stt status
check_exit   "resilience list (off) → 2" 2 "$BIN" --socket "$OFFSOCK" resilience list
check_exit   "a2a skills (off) → 2"    2  "$BIN" --socket "$OFFSOCK" a2a skills
check_exit   "digest (off) → 2"        2  "$BIN" --socket "$OFFSOCK" digest --since 24h
check_exit   "chat --list (off) → 2"   2  "$BIN" --socket "$OFFSOCK" chat --list
check_exit   "stop (off) → 2"          2  "$BIN" --socket "$OFFSOCK" stop
check_exit   "onboard (off) → 2"       2  "$BIN" --socket "$OFFSOCK" onboard
check_exit   "model search (off) → 2"  2  "$BIN" --socket "$OFFSOCK" model search whisper
# The ranking is computed by the daemon (hardware probe, live HuggingFace
# listing), so with the daemon off the refusal is the deterministic path.
check_exit   "model recommend (off) → 2" 2 "$BIN" --socket "$OFFSOCK" model recommend
# The default embeddings model lives in the daemon's system.db, which checks the
# identifier against the registry: daemon off, both leaves refuse with 2.
check_exit   "model embeddings show (off) → 2" 2 "$BIN" --socket "$OFFSOCK" model embeddings show
check_exit   "model embeddings set-default (off) → 2" 2 "$BIN" --socket "$OFFSOCK" model embeddings set-default multilingual-e5-small
# The registry of sources lives in the daemon's system.db, owned by its actor:
# daemon off, each leaf refuses with 2; a deletion without --confirm outside a
# terminal is refused with 1 before any call.
check_exit   "source list (off) → 2"           2     "$BIN" --socket "$OFFSOCK" source list
check_exit   "source create (off) → 2"         2     "$BIN" --socket "$OFFSOCK" source create --kind folder --label Contracts --reference conn-contracts
check_exit   "source delete (off) → 2"         2     "$BIN" --socket "$OFFSOCK" source delete src-absent --confirm
check_exit   "source delete without --confirm → 1" 1 "$BIN" --socket "$OFFSOCK" source delete src-absent
# The report of the migration of the former projects is read from the daemon:
# daemon off it refuses with 2, in human and in --json mode.
check_exit   "source report (off) → 2"         2     "$BIN" --socket "$OFFSOCK" source report
check_exit   "source report --json (off) → 2"  2     "$BIN" --socket "$OFFSOCK" --json source report
# The preview of a workbook or a CSV file is read by the daemon: daemon off it
# refuses with 2, in human and in --json mode; a file that is not there is
# refused with 1 before any call.
printf 'name,city\nAda,London\n' > "$SCRATCH/inspect.csv"
check_exit   "source inspect (off) → 2"        2     "$BIN" --socket "$OFFSOCK" source inspect "$SCRATCH/inspect.csv"
check_exit   "source inspect --json (off) → 2" 2     "$BIN" --socket "$OFFSOCK" --json source inspect "$SCRATCH/inspect.csv" --sheet inspect --header-row 1
check_exit   "source inspect missing file → 1" 1     "$BIN" --socket "$OFFSOCK" source inspect "$SCRATCH/no-such-file.xlsx"
check        "agent list (off, local fallback)"      "$BIN" --socket "$OFFSOCK" agent list
check_content "agent list shows 4 seeded agents" "apollia-chat" "$BIN" --socket "$OFFSOCK" agent list
check_exit   "agent install missing file → 1"  1     "$BIN" --socket "$OFFSOCK" agent install /tmp/never-exists.py
# The definition and overrides of an agent live behind the runtime, which
# journals every write: daemon off, each leaf refuses with 2, and a field or a
# value the CLI can judge alone is refused with 1 before any call.
check_exit   "agent show (off) → 2"            2     "$BIN" --socket "$OFFSOCK" agent show apollia-chat
check_exit   "agent list --definitions (off) → 2" 2  "$BIN" --socket "$OFFSOCK" agent list --definitions
check_exit   "agent set (off) → 2"             2     "$BIN" --socket "$OFFSOCK" agent set apollia-chat mission "Answer briefly."
check_exit   "agent reset (off) → 2"           2     "$BIN" --socket "$OFFSOCK" agent reset apollia-chat --confirm
check_exit   "agent set unknown field → 1"     1     "$BIN" --socket "$OFFSOCK" agent set apollia-chat model qwen3
check_exit   "agent set two missions → 1"      1     "$BIN" --socket "$OFFSOCK" agent set apollia-chat mission one two
check_exit   "agent reset unknown field → 1"   1     "$BIN" --socket "$OFFSOCK" agent reset apollia-chat --field model --confirm
# The profile source of an agent is its `sources` override: checking it goes
# through the runtime like any override, daemon off it refuses with 2, in human
# and in --json mode.
check_exit   "agent set sources profile (off) → 2" 2 "$BIN" --socket "$OFFSOCK" agent set apollia-chat sources profile
check_exit   "agent set sources profile --json (off) → 2" 2 "$BIN" --socket "$OFFSOCK" --json agent set apollia-chat sources profile
# The export of an agent is built and signed by the runtime: daemon off, it
# refuses with 2, in human and in --json mode; an output in a folder that does
# not exist is refused with 1 before any call, and nothing is written.
check_exit   "agent export (off) → 2"          2     "$BIN" --socket "$OFFSOCK" agent export seed-ledger
check_exit   "agent export --sign --json (off) → 2" 2 "$BIN" --socket "$OFFSOCK" --json agent export seed-ledger --sign --publisher "Seed Author"
check_exit   "agent export into a missing folder → 1" 1 "$BIN" --socket "$OFFSOCK" agent export seed-ledger -o "$SCRATCH/no-such-dir/seed-ledger.apo"
# An agent without code runs in the runtime too, by the same `run` leaf: daemon
# off, it refuses with 2 before any call, whatever the agent.
check_exit   "run agent without code (off) → 2" 2    "$BIN" --socket "$OFFSOCK" run seed-ledger "Reconcile March."
check_exit   "run agent without code --json (off) → 2" 2 "$BIN" --socket "$OFFSOCK" --json run seed-ledger "Reconcile March."
# A batch runs in the runtime: daemon off, each leaf refuses with 2, in human
# and in --json mode; a malformed source option is a usage error, 1, before
# any call.
check_exit   "batch run (off) → 2"             2     "$BIN" --socket "$OFFSOCK" batch run seed-ledger --source-type spreadsheet --source /data/requests.xlsx
check_exit   "batch run --json (off) → 2"      2     "$BIN" --socket "$OFFSOCK" --json batch run seed-ledger --source-type spreadsheet --source /data/requests.xlsx --concurrency 2 --max-failures 3
check_exit   "batch status (off) → 2"          2     "$BIN" --socket "$OFFSOCK" batch status 00000000-0000-0000-0000-000000000000
check_exit   "batch list --json (off) → 2"     2     "$BIN" --socket "$OFFSOCK" --json batch list
check_exit   "batch run bad --option → 1"      1     "$BIN" --socket "$OFFSOCK" batch run seed-ledger --source-type spreadsheet --source /data/requests.xlsx --option sheet
check_exit   "batch run without source → 1"    1     "$BIN" --socket "$OFFSOCK" batch run seed-ledger
check_exit   "batch accesses (off) → 2"        2     "$BIN" --socket "$OFFSOCK" batch accesses seed-ledger
check_exit   "batch pause --json (off) → 2"    2     "$BIN" --socket "$OFFSOCK" --json batch pause 00000000-0000-0000-0000-000000000000
check_exit   "batch resume (off) → 2"          2     "$BIN" --socket "$OFFSOCK" batch resume 00000000-0000-0000-0000-000000000000
check_exit   "batch stop (off) → 2"            2     "$BIN" --socket "$OFFSOCK" batch stop 00000000-0000-0000-0000-000000000000
check_exit   "batch retry --grant (off) → 2"   2     "$BIN" --socket "$OFFSOCK" batch retry 00000000-0000-0000-0000-000000000000 --grant gmail_send
check_exit   "batch pause without id → 1"      1     "$BIN" --socket "$OFFSOCK" batch pause
check_exit   "batch export (off) → 2"          2     "$BIN" --socket "$OFFSOCK" batch export 00000000-0000-0000-0000-000000000000 --format xlsx
check_exit   "batch export --json (off) → 2"   2     "$BIN" --socket "$OFFSOCK" --json batch export 00000000-0000-0000-0000-000000000000 --format csv -o -
check_exit   "batch export bad --format → 1"   1     "$BIN" --socket "$OFFSOCK" batch export 00000000-0000-0000-0000-000000000000 --format pdf
# A workflow runs in the runtime: daemon off, each leaf refuses with 2; an
# identifier without its package, a malformed --input, or a stop without
# --confirm off a terminal is a usage error, 1, before any call.
check_exit   "workflow show (off) → 2"         2     "$BIN" --socket "$OFFSOCK" workflow show invoices/close-month
check_exit   "workflow show bare name → 1"     1     "$BIN" --socket "$OFFSOCK" --json workflow show close-month
check_exit   "workflow run (off) → 2"          2     "$BIN" --socket "$OFFSOCK" workflow run invoices/close-month --input folder=/data/march
check_exit   "workflow run --json (off) → 2"   2     "$BIN" --socket "$OFFSOCK" --json workflow run invoices/close-month --input folder=/data/march --wait
check_exit   "workflow status (off) → 2"       2     "$BIN" --socket "$OFFSOCK" workflow status 00000000-0000-0000-0000-000000000000
check_exit   "workflow logs --json (off) → 2"  2     "$BIN" --socket "$OFFSOCK" --json workflow logs invoices/close-month
check_exit   "workflow cancel (off) → 2"       2     "$BIN" --socket "$OFFSOCK" workflow cancel 00000000-0000-0000-0000-000000000000 --confirm
check_exit   "workflow cancel no confirm → 1"  1     "$BIN" --socket "$OFFSOCK" --json workflow cancel 00000000-0000-0000-0000-000000000000
check_exit   "workflow run bare name → 1"      1     "$BIN" --socket "$OFFSOCK" workflow run close-month
check_exit   "workflow run bad --input → 1"    1     "$BIN" --socket "$OFFSOCK" workflow run invoices/close-month --input folder
# Usage errors are exit 1 by contract (1 = usage, 2 = runtime): a script can
# tell a typo from a stopped daemon.
check_exit   "clap usage refusal → 1"  1  "$BIN" agent list --bogus

# ── A.13b service (definition written, never loaded) ────────────────────────
# `service install --no-load` writes the user-session definition under the
# seeded HOME and stops there: launchd, systemd and Task Scheduler act on the
# operator's real session whatever HOME says, so loading is the manual gesture.
# `status` and `uninstall` match the manager's answer against this definition
# file, so a service another home installed is neither reported nor stopped.
# XDG_CONFIG_HOME is dropped so the Linux unit lands under the seeded HOME.
section "A.13b service"
case "$(uname -s)" in
    Darwin) SVC_DEF="$HOME/Library/LaunchAgents/com.apollia.os.plist" ;;
    Linux)  SVC_DEF="$HOME/.config/systemd/user/apollia-os.service" ;;
    *)      SVC_DEF="$DATA/service/apollia-os.xml" ;;
esac
check_json_field "service status before install: not_installed" "d['state']" "not_installed" env -u XDG_CONFIG_HOME "$BIN" service status --json
check        "service install --no-load"             env -u XDG_CONFIG_HOME "$BIN" service install --no-load
[[ -f "$SVC_DEF" ]] && _pass "service install wrote the definition in place" || _fail "service install wrote the definition in place" "absent: $SVC_DEF"
# The Windows XML is UTF-16, which grep does not read; the Rust tests pin it.
if [[ "$(uname -s)" == Darwin || "$(uname -s)" == Linux ]]; then
    BIN_ABS=$("${E2E_PYTHON:-python3}" -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$BIN")
    if /usr/bin/grep -qF "$BIN_ABS" "$SVC_DEF" && /usr/bin/grep -qE '(<string>start</string>|" start$)' "$SVC_DEF"; then
        _pass "service definition runs the absolute binary with start"
    else
        _fail "service definition runs the absolute binary with start" "binary $BIN_ABS or start missing in $SVC_DEF"
    fi
fi
check_json_field "service status: installed"        "d['installed']" "True"    env -u XDG_CONFIG_HOME "$BIN" service status --json
check_json_field "service status: stopped"          "d['state']"     "stopped" env -u XDG_CONFIG_HOME "$BIN" service status --json
check_exit   "service uninstall (no --confirm) → 1" 1 env -u XDG_CONFIG_HOME "$BIN" service uninstall
check        "service uninstall --confirm"           env -u XDG_CONFIG_HOME "$BIN" service uninstall --confirm
[[ ! -f "$SVC_DEF" ]] && _pass "service uninstall removed the definition" || _fail "service uninstall removed the definition" "still present: $SVC_DEF"
check_exit   "service uninstall (absent) → 1"      1 env -u XDG_CONFIG_HOME "$BIN" service uninstall --confirm

# ── A.14 un-exercised VARIANTS (every leaf itself has a real invocation) ─────
# Coverage counts real invocations only (scripts/check_cli_e2e_coverage.py), so
# a skip line justifies nothing anymore. The lines below record the variants
# the suite still does not drive, labelled as variants so no leaf appears both
# exercised and skipped.
section "A.14 un-exercised variants"
skip "chat (interactive REPL turn)"   "covered in Track 3 via pty capture"
skip "update (live GitHub check)"     "outbound HTTPS to api.github.com; the lock refusal is asserted above"
skip "model search (live HF query)"   "outbound HTTPS to huggingface.co; the daemon-off refusal is asserted above"
skip "model recommend (live ranking)" "outbound HTTPS to huggingface.co; the daemon-off refusal is asserted above"
skip "stt model download (real download)" "whisper model HF download (~1 GB); the already-present path is asserted above"
skip "agent install <git-url>"        "git clone over network; file install is asserted in Track 2"
skip "mcp oauth login (browser flow)" "AS authorize URL + browser callback; the unconfigured refusal is asserted above"
skip "mcp oauth discover (live endpoint)" "network discovery; the unconfigured refusal is asserted above"
skip "onboard (interactive session)"  "chat-based onboarding agent; the daemon-off refusal is asserted above"
skip "stt transcribe (real audio)"    "needs an audio file + a loaded Whisper model; the missing-file refusal is asserted above"
skip "tools credentials set (with a TTY)" "masked console prompt; the piped path is asserted above"
skip "tools credentials test (decryptable credential)" "needs a live credentialed backend; the seeded undecryptable blob is asserted above"
