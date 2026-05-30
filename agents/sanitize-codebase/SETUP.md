# sanitize-codebase setup

Operator guide for the Apollia OS mirror sanitize agent (Phase 2 of the
v0.1.0-preview release). Assumes a working Apollia OS install and a
local Docker daemon (for SonarQube + the MCP bridge).

## 1. Local SonarQube + MCP bridge

The sonar-cleanup-worker calls SonarQube through MCP. Re-use the
existing setup from `docs/internal/release/scripts/SANITIZE-SETUP.md` :

```sh
bash docs/internal/release/scripts/sonar-up.sh
# Open http://localhost:9100, login admin/admin, change the password.
# Create a User token: My Account > Security > Generate Token
export SONARQUBE_TOKEN=<token>

# First full scan (baseline).
bash docs/internal/release/scripts/sonar-scan.sh
```

Wire the MCP server in `~/.apollia/apollia.toml` :

```toml
[[mcp.servers]]
name      = "sonarqube"
transport = "stdio"
command   = "docker"
args      = ["run", "--init", "-i", "--rm",
             "-e", "SONARQUBE_URL", "-e", "SONARQUBE_TOKEN",
             "mcp/sonarqube"]

[mcp.servers.env]
SONARQUBE_URL   = "http://host.docker.internal:9100"
SONARQUBE_TOKEN = "<token>"
```

## 2. Local LLM backend (llama-cpp-2)

The mirror runs on a local quantized LLM. Configure
`~/.apollia/apollia.toml` :

```toml
[llm]
default_backend = "local"

[llm.backends.local]
engine        = "llama-cpp-2"
model_id      = "Qwen2.5-Coder-14B-Instruct-Q5_K_M.gguf"
n_ctx         = 16384
n_gpu_layers  = -1            # auto detect; set to a positive number on macOS Metal
temperature   = 0.2           # sanitize is deterministic, low temperature
```

Notes :
- The backend is `llama-cpp-2` (not `mistral-rs`).
- Pick a coding-tuned model. Qwen2.5-Coder-14B Q5_K_M is a good default
  for an Apple Silicon laptop; Qwen2.5-Coder-32B Q4_K_M works on a
  workstation with 32+ GB unified memory or a discrete GPU.
- `MessageContent` is text-only end to end. No vision is involved.
- Cold-start of the GGUF model can take 30-60 seconds. Subsequent
  invocations reuse the loaded weights for the daemon lifetime.

Quick check :

```sh
apollia llm probe --backend local --prompt "Reply with OK."
```

## 3. Install the agent package

From the repo root :

```sh
apollia agent install agents/sanitize-codebase
apollia agent list | grep sanitize-codebase
```

The director and the two workers should show up with `status: ready`.

## 4. First batch (manual, dry-run friendly)

Try one batch synchronously before enabling the interval trigger :

```sh
apollia agent run sanitize-codebase-director \
  --skill sanitize.run_batch \
  --input '{"prompt":"Run the next sanitize batch","batch_size":2}'
```

The output is a JSON envelope with `counts`, `progress`, `metrics`, and
a rendered summary. Inspect the per-file results in the namespace
memory:

```sh
apollia memory list --namespace sanitize-codebase --prefix file: | head
```

## 5. Enable the interval trigger

Once the manual run looks healthy, enable the 10-minute trigger :

```sh
apollia trigger enable sanitize-interval
apollia trigger logs sanitize-interval --follow
```

Disable with :

```sh
apollia trigger disable sanitize-interval
```

## 6. Replay-and-compare

Cf. `eval/replay.md` for the full procedure. Short version :

```sh
git checkout -b sanitize-apollia pre-sanitize-baseline
apollia trigger enable sanitize-interval
# leave it running until counts.pending == 0
git diff sanitize-claude sanitize-apollia
```

## Troubleshooting

| Symptom | Likely cause | Action |
|---|---|---|
| `mcp:sonarqube/search_issues` unavailable | Docker not running, token expired | `docker ps`, regen token, restart daemon |
| Every batch yields residues with `harness_red_reverted` | Local LLM produces non-compiling edits | Lower `max_issues`, switch to a larger model, raise `temperature` slightly |
| `code_lines_changed` residue from comment worker | LLM emitted a token outside the comment block | The agent reverts automatically; investigate the prompt rendering in `templates/comment-translate-prompt.md.j2` |
| `edit_anchor_ambiguous` | Two identical substrings in the file | Expected; the file will be retried with a different issue ranking next tick |
| Interval trigger does nothing | `enabled = false` in `manifest.toml`, or daemon not refreshed | Re-install the agent : `apollia agent install --force` |

## Cost monitoring

The point of this agent is the zero-euro inference promise. Track it :

```sh
apollia llm usage --backend local --since 1h
```

The `cost_usd` field should remain 0.00 for the duration of the run. If
it does not, something accidentally fell back to a cloud backend ; check
`apollia.toml`.
