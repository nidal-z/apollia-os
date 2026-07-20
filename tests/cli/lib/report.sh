# shellcheck shell=bash
# tests/cli/lib/report.sh - structured result accumulator for the CLI E2E suite.
#
# Two artifacts are produced at the end of a run:
#   * report.json - machine-readable, one entry per assertion, plus the
#     non-deterministic captures (input/output/streaming). For CI + diffing.
#   * report.md   - human-readable, with a dedicated section surfacing the
#     Track 3 (LLM) captures for human review (the content is not asserted,
#     only that it happened / streamed).
#
# Rows are buffered as TAB-separated lines (assertions never contain tabs), and
# captured payloads are stored as separate files referenced by path, so no
# shell-side JSON escaping is ever needed. render_report.py assembles both
# artifacts at finalize time.

# report_init <run_tmp>
report_init() {
    local run_tmp=$1
    REPORT_ROWS="$run_tmp/rows.tsv"
    REPORT_CAPS="$run_tmp/captures.tsv"
    REPORT_CAP_DIR="$run_tmp/captures"
    : >"$REPORT_ROWS"
    : >"$REPORT_CAPS"
    /bin/mkdir -p "$REPORT_CAP_DIR"
}

# _sanitize <string> -> strips tabs and newlines (keeps rows one-per-line).
_sanitize() { printf '%s' "$1" | /usr/bin/tr '\t\n' '  '; }

# _report_row <track> <label> <verdict> <exit> <dur_ms>
_report_row() {
    [[ -n "${REPORT_ROWS:-}" ]] || return 0
    printf '%s\t%s\t%s\t%s\t%s\n' \
        "$(_sanitize "$1")" "$(_sanitize "$2")" "$3" "$4" "${5:-0}" >>"$REPORT_ROWS"
}

# _report_capture <label> <exit> <dur_ms> <chunks> <first_ms> <input> <cap_file>
_report_capture() {
    [[ -n "${REPORT_CAPS:-}" ]] || return 0
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$(_sanitize "$1")" "$2" "${3:-0}" "${4:-0}" "${5:-0}" \
        "$(_sanitize "$6")" "$7" >>"$REPORT_CAPS"
}

# report_finalize <out_json> <out_md> <pass> <fail> <skip> <wall_s>
report_finalize() {
    local out_json=$1 out_md=$2 pass=$3 fail=$4 skip=$5 wall=$6
    /usr/bin/python3 "$LIB_DIR/render_report.py" \
        --rows "$REPORT_ROWS" \
        --captures "$REPORT_CAPS" \
        --out-json "$out_json" \
        --out-md "$out_md" \
        --pass "$pass" --fail "$fail" --skip "$skip" --wall "$wall" \
        2>/dev/null || {
            echo "report render failed (python3 missing?); rows kept at $REPORT_ROWS" >&2
            return 1
        }
}
