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

# _redact <string> -> replaces the three machine-specific roots with stable
# tokens, then drops a root fragment left at the end by truncation.
#
# The report directory is git-ignored, so scripts/check_prose.py never sees it,
# and CI uploads it as an artifact from a public repository. A raw failure detail
# carries two machine-specific roots: the repository path through $BIN, and the
# macOS user id that mktemp puts inside $RUN_TMP.
#
# Order is imposed, not cosmetic: REPO_ROOT lives under the real HOME both here
# and on the Linux runner, so substituting HOME first would leave nothing for the
# REPO_ROOT pass to match.
#
# The trailing-fragment pass closes a hole the substitutions cannot: the detail
# is truncated where it is built and redacted here, so a cut landing inside a
# root leaves a prefix no substitution recognises any more. A cut is always at
# the end of the string, so the fragment can only be a trailing proper prefix of
# a root, and dropping it loses nothing the cut had not already destroyed.
_redact() {
    local s=$1 root frag n best=""
    [[ -n "${RUN_TMP:-}" ]]   && s=${s//"$RUN_TMP"/\$RUN_TMP}
    [[ -n "${REPO_ROOT:-}" ]] && s=${s//"$REPO_ROOT"/\$REPO}
    [[ -n "${REAL_HOME:-}" ]] && s=${s//"$REAL_HOME"/\$HOME}
    for root in "${RUN_TMP:-}" "${REPO_ROOT:-}" "${REAL_HOME:-}"; do
        n=$(( ${#root} - 1 ))
        while [[ $n -gt ${#best} ]]; do
            frag=${root:0:n}
            if [[ "$s" == *"$frag" ]]; then
                best=$frag
                break
            fi
            n=$(( n - 1 ))
        done
    done
    [[ -n "$best" ]] && s=${s%"$best"}
    printf '%s' "$s"
}

# _report_row <track> <label> <verdict> <exit> <dur_ms> [detail]
#
# The detail column is written only when the caller has one, which today means
# only a failure. A passing row carries no detail, so render_report.py leaves the
# key out entirely rather than emitting 154 empty strings into a green report.
_report_row() {
    [[ -n "${REPORT_ROWS:-}" ]] || return 0
    local detail=${6:-}
    if [[ -n "$detail" ]]; then
        detail=$(_sanitize "$(_redact "$detail")")
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$(_sanitize "$1")" "$(_sanitize "$2")" "$3" "$4" "${5:-0}" "$detail" >>"$REPORT_ROWS"
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
