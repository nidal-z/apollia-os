# Yumni ESRS classification (Apollia PoC)

A director/worker multi-agent app on the **Apollia SDK** that classifies Yumni
**actions** against a closed list of **ESRS** referential criteria.

The AI does **classification / suggestion, never a decision**. Justifications are
**descriptive** ("contribue a ...") and never **affirmative** ("conforme a ...").
Output is **constrained to the closed list of real criteria codes** provided by
Yumni (anti-hallucination). Conformity remains a human / audit judgement.

## Architecture

```
                       {"actionId": "..."}
                              │
                  ┌───────────▼────────────┐
                  │  rse-classification-    │   deterministic pipeline
                  │  director  (@on_message)│   (precise + auditable)
                  └───────────┬────────────┘
        MCP (yumni)           │            A2A (ctx.a2a.invoke)
   get_action_context ◄───────┤
   list_criteria (closed) ◄───┤
                              ├──► esrs.propose_mappings   (esrs-mapper, worker)
                              │        ctx.llm -> candidates, validated vs closed list
                              ├──► esrs.verify_mapping     (esrs-verifier, worker)
                              │        ctx.llm adversarial keep/confidence/reason
                              │
                threshold (0.6) + dedupe by code
                              │
   create_mapping (APPROVAL-GATED) ◄── only if WRITE_BACK on
                              │
                    structured report:
        {actionId, accepted[], rejected[], wrote}
```

- **esrs-mapper** (worker) - `esrs.propose_mappings(entity, criteria)`: proposes
  candidate criteria via `ctx.llm`, then **rejects any code outside the closed
  list** (`DomainError("UNKNOWN_CRITERION")`).
- **esrs-verifier** (worker) - `esrs.verify_mapping(entity, candidate, criterion)`:
  adversarial second opinion, returns `{keep, confidence, reason}`.
- **rse-classification-director** - deterministic pipeline (not free-form ReAct).
  Business rules (threshold, dedupe, "jamais decision", approval gate) live here.

## Run

```sh
# 1. Install the SDK (pure-Python, zero deps)
pip install -e ../../sdk

# 2. Validate each agent manifest without running it
python -m apollia inspect esrs_mapper.py --json
python -m apollia inspect esrs_verifier.py --json
python -m apollia inspect director.py --json

# 3. Register the Yumni MCP server (connector built separately in the Yumni repo).
#    Merge mcp.toml into your Apollia OS MCP config, or:
export YUMNI_AI_TOKEN=...   # tenant-scoped AI token, never committed
#    (points at yumni/integrations/apollia/mcp-server/dist/index.js, stdio)

# 4. Start the runtime + agents
apollia-os start
apollia-os agent start esrs-mapper
apollia-os agent start esrs-verifier
apollia-os agent start rse-classification-director

# 5. Classify one action (dry-run: no write-back)
apollia-os run rse-classification-director '{"actionId":"act-001"}'

# 6. Classify + persist accepted mappings (write-back on -> approval-gated)
YUMNI_WRITE_BACK=true apollia-os run rse-classification-director '{"actionId":"act-001"}'
```

### Director -> workers -> MCP flow

1. Director parses the message (`{"actionId"}` or `{"entityType":"action","entityId"}`).
2. Reads the action context + the **closed criteria list** via `ctx.tools.call`
   on the `mcp:yumni/*` tools.
3. `ctx.a2a.invoke("esrs.propose_mappings", ...)` -> candidate mappings.
4. For each candidate, `ctx.a2a.invoke("esrs.verify_mapping", ...)`; keep if
   `keep and confidence >= 0.6`; dedupe by `criterionCode`.
5. If `WRITE_BACK` (arg or `YUMNI_WRITE_BACK` env) is on, call
   `mcp:yumni/create_mapping` per accepted mapping - otherwise just report.

### Approval gate on `create_mapping`

`create_mapping` writes into Yumni, so it is marked `approvals."create_mapping" =
"required"` in `mcp.toml`. The AI proposes; a human approves the write. This keeps
the "AI suggests, human decides" rule enforceable at the runtime boundary.

### Sovereign-local vs cloud

`apollia.toml` routes `ctx.llm` to the **embedded llama.cpp engine** (the
`apollia-runner` sidecar) via a `provider = "llama-cpp"` backend whose `model` is
the absolute path to a local `.gguf` — here
`~/.apollia/models/Qwen3-30B-A3B.Q6_K.gguf`. Inference runs in-process, on-machine;
no Ollama, no external server, RSE data never leaves the host. Check the file is
present with `apollia-os model list`. A commented **Anthropic** cloud fallback is
available for stronger reasoning; enabling it means data leaves the machine, so it
is off by default. Agents never hardcode a model — the router decides.

### Audit

Every step (parse, tool calls, A2A invocations, verdicts, write-back) is traced by
the runtime. Inspect a run with:

```sh
apollia-os trace rse-classification-director   # step-by-step execution
apollia-os audit                                # decisions + approval-gated writes
```

## Evaluation

`eval/` is standalone (no Apollia runtime needed to score):

- `eval/dataset.json` - ~10 labeled RSE actions -> plausible ESRS codes.
- `eval/fixtures/criteria.sample.json` - a handful of ESRS criteria so the mapper
  can be reasoned about offline.
- `eval/score.py` - precision / recall / F1 + a confidence-calibration note.

```sh
# After generating predictions by running the director per sample:
python eval/score.py --pred eval/predictions.json
```

See the docstring in `eval/score.py` for the predictions file format and how to
build it from director output.
