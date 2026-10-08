# shellcheck shell=bash
# tests/cli/tracks/track2_packaged.sh - deterministic RUNTIME track, packaged
# agent and workflow.
#
# A package is written here in the shape a package takes today: an agent that
# is data in `apollia.toml`, and a Python workflow whose steps are declared
# ahead. It is built into an `.apo`, installed through the runtime, run, and
# uninstalled. No model is involved: the workflow counts words.
#
# Sourced twice by cli-e2e.sh, on a daemon of its own and a freshly seeded
# $HOME, after the daemon of Tracks 2 and 3 has stopped:
#
#   PACKAGED_PHASE=install  the package is built, previewed and installed, and
#                           the agent it declares is read back and given a task
#   PACKAGED_PHASE=run      the daemon has been restarted on the same $HOME;
#                           the workflow is described, run, followed, reached
#                           through A2A, and the package is uninstalled
#
# The restart between the two is part of the fixture, not of the product
# contract under test: the runtime hands the workflows of the installed
# packages to its engine when it starts, so the suite starts it once more
# rather than assert on a run it knows cannot be found yet.
#
# Uses $BIN, $SOCK, the seeded $HOME, $RUN_TMP and the assert helpers.

Q=("$BIN" --socket "$SOCK")   # convenience prefix
DESK="$RUN_TMP/desk-pkg"
DESK_APO="$RUN_TMP/e2e-desk-1.0.0.apo"

case "${PACKAGED_PHASE:-}" in
install)
    section "packaged agent and workflow (install)"
    /bin/mkdir -p "$DESK/flows"
    # The `[[workflows]]` table is what `python -m apollia describe` writes for
    # the module below; `package build` refuses a module the manifest does not
    # describe.
    /bin/cat >"$DESK/apollia.toml" <<'TOML'
[package]
name = "e2e-desk"
version = "1.0.0"
publisher = "E2E Publisher"
description = "A clerk and the workflow it files with."
sdk = ">=0.3.0"

[[agents]]
id = "e2e-clerk"
name = "Clerk"
mission = "File what you are given."
examples = ["File the March invoices."]

[[workflows]]
name = "file-note"
module = "flows/filing.py"
description = "Count the words of a note and label it."
action = "read"
inputs = { type = "object", properties = { note = { type = "string" }, label = { type = "string" } }, additionalProperties = false, required = ["note"], x-order = ["note", "label"] }
steps = ["count", "label"]
TOML
    /bin/cat >"$DESK/flows/filing.py" <<'PYEOF'
"""Workflow of the end-to-end package: counts the words of a note, then labels it."""

from typing import Any, NotRequired, TypedDict

from apollia import WorkflowCtx, workflow


class Filing(TypedDict):
    """Inputs of the file-note workflow."""

    note: str
    label: NotRequired[str]


@workflow(
    "file-note",
    "Count the words of a note and label it.",
    inputs=Filing,
    steps=["count", "label"],
    action="read",
)
async def file_note(ctx: WorkflowCtx, inputs: Filing) -> dict[str, Any]:
    async with ctx.step("count"):
        words = len(inputs["note"].split())
    async with ctx.step("label"):
        label = inputs.get("label", "inbox")
    return {"words": words, "label": label}
PYEOF
    check            "package build (agent + workflow)"  "$BIN" package build "$DESK" -o "$DESK_APO"
    check_json_field "package preview: the agent"        "d['agents'][0]['id']+'/'+d['agents'][0]['mission']" "e2e-clerk/File what you are given." "${Q[@]}" package preview "$DESK_APO" --json
    check_json_field "package preview: the workflow and its steps" "d['workflows'][0]['name']+':'+','.join(d['workflows'][0]['steps'])" "file-note:count,label" "${Q[@]}" package preview "$DESK_APO" --json
    check_json_field "package install: the agent and the workflow" "','.join(d['agents'])+'/'+','.join(d['workflows'])" "e2e-clerk/file-note" "${Q[@]}" package install "$DESK_APO" --json
    # The agent is a definition and nothing else: no file to import, no process
    # to start. It is read back with its package, and its mission is the one of
    # the manifest.
    check_content    "agent list --definitions names the agent and its package" "e2e-clerk +package +e2e-desk" "${Q[@]}" agent list --definitions
    check_content    "agent show carries the mission of the manifest" "File what you are given." "${Q[@]}" agent show e2e-clerk
    check_content    "agent package show lists the agent" "e2e-clerk" "${Q[@]}" agent package show e2e-desk
    check_content    "a2a skills exposes the packaged agent" "agent:e2e-clerk" "${Q[@]}" a2a skills
    # With no model wired the task cannot answer; what is asserted is the path:
    # `run` accepts it for that agent and the store records it under its name.
    CRUN=$("${Q[@]}" run e2e-clerk "File the March invoices." --detach --json 2>/dev/null)
    CTID=$(printf '%s' "$CRUN" | "${E2E_PYTHON:-python3}" -c "import sys,json;d=json.load(sys.stdin);print(d.get('task_id') or d.get('id',''))" 2>/dev/null)
    if [[ -n "$CTID" ]]; then
        _pass "run packaged agent --detach" "$CTID"
        CROW=$(e2e_sqlite "$HOME/.apollia/hitl.db" "SELECT agent_name || '|' || (input_json IS NOT NULL) FROM tasks WHERE task_id = '$CTID';" 2>/dev/null)
        [[ "$CROW" == "e2e-clerk|1" ]] \
            && _pass "hitl.db records the packaged agent and the whole input" "$CROW" \
            || _fail "hitl.db records the packaged agent and the whole input" "row: ${CROW:-none}"
        "${Q[@]}" task cancel "$CTID" --confirm >/dev/null 2>&1 || true
    else
        _fail "run packaged agent --detach" "no task_id (out: ${CRUN:0:200})"
    fi
    ;;
run)
    section "packaged agent and workflow (run)"
    check_content    "workflow show lists the declared steps" "count, label" "${Q[@]}" workflow show e2e-desk/file-note
    check_json_field "workflow show --json: the required input" "','.join(i['name'] for i in d['inputs'] if i['required'])" "note" "${Q[@]}" workflow show e2e-desk/file-note --json
    # A run to its end: the Python body counted the words, the default label
    # applied, and each declared step was reported as completed, in order.
    check_json_field "workflow run --wait: output and steps" \
        "d['run']['status']+'/'+str(d['run']['output']['words'])+'/'+d['run']['output']['label']+'/'+','.join(s['name']+'='+s['status'] for s in d['steps'])" \
        "completed/4/inbox/count=completed,label=completed" \
        "${Q[@]}" workflow run e2e-desk/file-note --input "note=pay the March invoice" --wait --json
    check_json_field "workflow run --wait: the optional input is read" "d['run']['output']['label']" "paid" "${Q[@]}" workflow run e2e-desk/file-note --input "note=done" --input label=paid --wait --json
    # A run that is not waited for answers with its identifier, which `status`
    # follows to the same end.
    WRID=$("${Q[@]}" -q workflow run e2e-desk/file-note --input "note=one two" 2>/dev/null | /usr/bin/head -1)
    if [[ -n "$WRID" ]]; then
        _pass "workflow run (not waited) answers a run id" "$WRID"
        WSTATUS=""
        for _ in 1 2 3 4 5 6 7 8 9 10; do
            WSTATUS=$("${Q[@]}" workflow status "$WRID" --json 2>/dev/null | "${E2E_PYTHON:-python3}" -c "import sys,json;d=json.load(sys.stdin);print(d['run']['status']+'/'+str((d['run']['output'] or {}).get('words')))" 2>/dev/null)
            [[ "$WSTATUS" == completed/* ]] && break
            /bin/sleep 1
        done
        [[ "$WSTATUS" == "completed/2" ]] \
            && _pass "workflow status follows the run to its end" "$WSTATUS" \
            || _fail "workflow status follows the run to its end" "got: ${WSTATUS:-nothing}"
        check_content "workflow status (human) lists the steps" "count +completed" "${Q[@]}" workflow status "$WRID"
    else
        _fail "workflow run (not waited) answers a run id" "no run id on stdout"
    fi
    check_json_field "workflow logs --json holds the three runs" "len(d['runs'])" 3 "${Q[@]}" workflow logs e2e-desk/file-note --json
    check_exit       "workflow run without its required input → 1" 1 "${Q[@]}" workflow run e2e-desk/file-note --input label=paid
    check_json_field "workflow logs --json: the refused launch recorded nothing" "len(d['runs'])" 3 "${Q[@]}" workflow logs e2e-desk/file-note --json
    # The same workflow is a target of A2A, under its typed identifier.
    check_content    "a2a skills exposes the workflow" "workflow:e2e-desk/file-note" "${Q[@]}" a2a skills
    check_json_field "a2a invoke workflow: completed with its output" "d['result']['status']+'/'+str(d['result']['output'][0]['data']['words'])" "completed/3" "${Q[@]}" a2a invoke workflow:e2e-desk/file-note --args '{"note":"a b c"}' --timeout 30 --json
    # Uninstalled, the package takes its agent and the history of its runs.
    check_json_field "agent package uninstall: the agent goes, history erased" "','.join(d['agents'])+'/'+str(d['historyCleared'])" "e2e-clerk/True" "${Q[@]}" agent package uninstall e2e-desk --clear-history --confirm --json
    check_exit       "agent show (uninstalled with its package) → 1" 1 "${Q[@]}" agent show e2e-clerk
    check_json_field "workflow logs --json holds no run after the uninstall" "len(d['runs'])" 0 "${Q[@]}" workflow logs e2e-desk/file-note --json
    check            "audit verify (install, runs and uninstall journaled)" "${Q[@]}" audit verify
    ;;
*)
    _fail "packaged agent and workflow" "PACKAGED_PHASE is '${PACKAGED_PHASE:-}', expected install or run"
    ;;
esac
