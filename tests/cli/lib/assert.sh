# shellcheck shell=bash
# tests/cli/lib/assert.sh - assertion helpers shared by every track.
#
# Deterministic helpers (check/check_exit/check_json/check_grep/check_json_field)
# assert an exact, known property and tally PASS/FAIL. `skip` records a justified
# skip. The capture_* helpers drive non-deterministic (LLM) commands: they assert
# only STRUCTURE (exit code, streaming happened, terminated in time) and record
# the full input/output into the report for human review.
#
# Every helper feeds the report accumulator (lib/report.sh) tagged with the
# current track ($CURRENT_TRACK). Counters (PASS/FAIL/SKIP/FAILED_LABELS) live in
# the orchestrator scope; these functions are sourced, not exec'd.

# ── Styling ────────────────────────────────────────────────────────────────
red()    { printf '\033[31m%s\033[0m' "$1"; }
green()  { printf '\033[32m%s\033[0m' "$1"; }
yellow() { printf '\033[33m%s\033[0m' "$1"; }
bold()   { printf '\033[1m%s\033[0m' "$1"; }
section() { printf '\n%s\n' "$(bold "─── $1")"; }

CAP_N=0

# _now_ms - millisecond clock (bash 4.4+ EPOCHREALTIME, else 0).
_now_ms() {
    if [[ -n "${EPOCHREALTIME:-}" ]]; then
        printf '%.0f' "$(awk "BEGIN{print ${EPOCHREALTIME/,/.} * 1000}")"
    else
        echo 0
    fi
}

# _record_fail <label> <detail> [exit] [dur_ms]
#
# Every recorded failure produces a report row, including the ones the
# orchestrator records outside any assertion (a seed that would not build, a
# daemon that never came up). Those used to bump the counter without ever
# reaching _report_row, so the artifact announced a failing summary over an empty
# assertion list. Recording here rather than adding calls next to each of them
# also means the next caller someone writes gets its row without knowing.
_record_fail() {
    local label=$1 detail=$2 rc=${3:-1} dur=${4:-0}
    FAIL=$((FAIL + 1))
    FAILED_LABELS+=("$label")
    if [[ "${VERBOSE:-0}" == "1" ]]; then
        printf '    %s\n' "$detail"
    fi
    _report_row "$CURRENT_TRACK" "$label" FAIL "$rc" "$dur" "$detail"
}

_pass() {
    local label=$1 note=${2:-} dur=${3:-0}
    printf '  %s %s%s\n' "$(green ✔)" "$label" "${note:+ ($note)}"
    PASS=$((PASS + 1))
    _report_row "$CURRENT_TRACK" "$label" PASS 0 "$dur"
}

_fail() {
    local label=$1 detail=$2 rc=${3:-1} dur=${4:-0}
    printf '  %s %s\n' "$(red ✗)" "$label"
    _record_fail "$label" "$detail" "$rc" "$dur"
}

# check "label" cmd args...   → expect exit 0.
check() {
    local label=$1; shift
    local out rc t0 dur
    t0=$(_now_ms); out=$("$@" 2>&1); rc=$?; dur=$(( $(_now_ms) - t0 ))
    if [[ $rc -eq 0 ]]; then
        _pass "$label" "" "$dur"
    else
        _fail "$label" "cmd: $* | out: ${out:0:300}" "$rc" "$dur"
    fi
}

# check_exit "label" expected_rc cmd args...
check_exit() {
    local label=$1 expected=$2; shift 2
    local out rc t0 dur
    t0=$(_now_ms); out=$("$@" 2>&1); rc=$?; dur=$(( $(_now_ms) - t0 ))
    if [[ $rc -eq $expected ]]; then
        _pass "$label" "rc=$rc" "$dur"
    else
        _fail "$label" "expected $expected got $rc | cmd: $* | out: ${out:0:300}" "$rc" "$dur"
    fi
}

# check_json "label" cmd args...   → expect exit 0 + valid JSON on stdout.
check_json() {
    local label=$1; shift
    local out rc t0 dur
    t0=$(_now_ms); out=$("$@" 2>/dev/null); rc=$?; dur=$(( $(_now_ms) - t0 ))
    if [[ $rc -eq 0 ]] && /usr/bin/python3 -c "import sys,json;json.loads(sys.stdin.read())" <<<"$out" 2>/dev/null; then
        _pass "$label" "JSON valid" "$dur"
    else
        _fail "$label" "rc=$rc or non-JSON | cmd: $* | out: ${out:0:300}" "$rc" "$dur"
    fi
}

# check_grep "label" pattern cmd args...   → exit 0 + stdout matches ERE.
check_grep() {
    local label=$1 pattern=$2; shift 2
    local out rc t0 dur
    t0=$(_now_ms); out=$("$@" 2>&1); rc=$?; dur=$(( $(_now_ms) - t0 ))
    if [[ $rc -eq 0 ]] && echo "$out" | /usr/bin/grep -qE "$pattern"; then
        _pass "$label" "" "$dur"
    else
        _fail "$label" "pattern: $pattern | cmd: $* | out: ${out:0:300}" "$rc" "$dur"
    fi
}

# check_content is check_grep with intent: assert a KNOWN seeded value is present.
check_content() { check_grep "$@"; }

# check_json_field "label" py_expr expected cmd args...
#   py_expr accesses the parsed document as `d`, e.g. "d['id']" or "len(d)".
check_json_field() {
    local label=$1 expr=$2 expected=$3; shift 3
    local out rc got t0 dur
    t0=$(_now_ms); out=$("$@" 2>/dev/null); rc=$?
    got=$(printf '%s' "$out" | /usr/bin/python3 -c "import sys,json;d=json.load(sys.stdin);print($expr)" 2>/dev/null)
    dur=$(( $(_now_ms) - t0 ))
    if [[ $rc -eq 0 && "$got" == "$expected" ]]; then
        _pass "$label" "$expr=$got" "$dur"
    else
        _fail "$label" "expected $expr=$expected got '$got' rc=$rc | cmd: $* | out: ${out:0:200}" "$rc" "$dur"
    fi
}

# skip "label" "reason"
skip() {
    local label=$1 reason=${2:-skipped}
    printf '  %s %s - %s\n' "$(yellow ⊘)" "$label" "$reason"
    SKIP=$((SKIP + 1))
    _report_row "$CURRENT_TRACK" "$label" SKIP 0 0
}

# ── Non-deterministic capture (Track 3) ─────────────────────────────────────
# Common tail: parse a run_capture/pty_run summary, assert structure, record the
# capture + a report row.
#   $1 summary-json  $2 label  $3 min_chunks  $4 input  $5 capfile  $6 mode
# Modes:
#   strict - pass iff exit==0 && !timed_out && chunks>=min (one-shot/SSE cmds).
#   stream - pass iff chunks>=min (a pty REPL we terminate on purpose: the
#            structural signal is that a reply STREAMED, not a clean exit code).
#   soft   - like strict, but if it fails because the runner sidecar is
#            unavailable (503 / connection error), record a SKIP not a FAIL
#            (isolated-HOME runner flakiness is an environment gap).
_capture_assert() {
    local summary=$1 label=$2 min_chunks=$3 input=$4 capfile=$5 mode=${6:-strict}
    local exit_code chunks dur first timed
    exit_code=$(printf '%s' "$summary" | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin).get('exit',-1))" 2>/dev/null)
    chunks=$(printf   '%s' "$summary" | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin).get('chunks',0))" 2>/dev/null)
    dur=$(printf      '%s' "$summary" | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin).get('duration_ms',0))" 2>/dev/null)
    first=$(printf    '%s' "$summary" | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin).get('first_chunk_ms',0))" 2>/dev/null)
    timed=$(printf    '%s' "$summary" | /usr/bin/python3 -c "import sys,json;print(json.load(sys.stdin).get('timed_out',False))" 2>/dev/null)
    _report_capture "$label" "${exit_code:-1}" "${dur:-0}" "${chunks:-0}" "${first:-0}" "$input" "$capfile"

    local ok=1
    case "$mode" in
        stream) [[ "${chunks:-0}" -ge "$min_chunks" ]] || ok=0 ;;
        *)      [[ "${exit_code:-1}" == "0" && "${timed:-True}" == "False" && "${chunks:-0}" -ge "$min_chunks" ]] || ok=0 ;;
    esac

    if [[ "$ok" == "1" ]]; then
        _pass "$label" "${chunks} chunk(s), ${dur}ms" "$dur"
    elif [[ "$mode" == "soft" ]] && /usr/bin/grep -qiE "runner|503|connection (refused|closed)|SendRequest|Connect" "$capfile" 2>/dev/null; then
        skip "$label" "apollia-runner sidecar unavailable mid-run (503/connection) - isolated-HOME flakiness"
    else
        _fail "$label" "exit=$exit_code chunks=$chunks timed_out=$timed" "${exit_code:-1}" "$dur"
    fi
}

# capture_run "label" timeout "input desc" -- cmd args...   (strict)
capture_run() {
    local label=$1 timeout=$2 input=$3; shift 3
    [[ "$1" == "--" ]] && shift
    CAP_N=$((CAP_N + 1))
    local capfile="$REPORT_CAP_DIR/cap-$CAP_N.txt" summary
    summary=$(/usr/bin/python3 "$LIB_DIR/run_capture.py" --timeout "$timeout" --out "$capfile" -- "$@" 2>/dev/null)
    _capture_assert "$summary" "$label" 1 "$input" "$capfile" strict
}

# capture_run_soft "label" timeout "input desc" -- cmd args...   (skip on runner gap)
capture_run_soft() {
    local label=$1 timeout=$2 input=$3; shift 3
    [[ "$1" == "--" ]] && shift
    CAP_N=$((CAP_N + 1))
    local capfile="$REPORT_CAP_DIR/cap-$CAP_N.txt" summary
    summary=$(/usr/bin/python3 "$LIB_DIR/run_capture.py" --timeout "$timeout" --out "$capfile" -- "$@" 2>/dev/null)
    _capture_assert "$summary" "$label" 1 "$input" "$capfile" soft
}

# capture_stream "label" timeout min_chunks "input desc" -- cmd args...   (strict)
capture_stream() {
    local label=$1 timeout=$2 min_chunks=$3 input=$4; shift 4
    [[ "$1" == "--" ]] && shift
    CAP_N=$((CAP_N + 1))
    local capfile="$REPORT_CAP_DIR/cap-$CAP_N.txt" summary
    summary=$(/usr/bin/python3 "$LIB_DIR/run_capture.py" --timeout "$timeout" --out "$capfile" -- "$@" 2>/dev/null)
    _capture_assert "$summary" "$label" "$min_chunks" "$input" "$capfile" strict
}

# capture_pty "label" timeout min_chunks "prompt" -- cmd args...   (stream mode)
capture_pty() {
    local label=$1 timeout=$2 min_chunks=$3 prompt=$4; shift 4
    [[ "$1" == "--" ]] && shift
    CAP_N=$((CAP_N + 1))
    local capfile="$REPORT_CAP_DIR/cap-$CAP_N.txt" summary
    summary=$(/usr/bin/python3 "$LIB_DIR/pty_run.py" --timeout "$timeout" --idle 6 --prompt "$prompt" --out "$capfile" -- "$@" 2>/dev/null)
    _capture_assert "$summary" "$label" "$min_chunks" "$prompt" "$capfile" stream
}
