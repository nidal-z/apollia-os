# Worker Agent Benchmark — excel-worker vs generic-agent

**Location:** `benchmarks/run_worker_benchmark.py`
**Results:** `benchmarks/results/worker-benchmark-{model}-{timestamp}.json`

---

## Objective

Quantify the performance advantage of the Worker Agent pattern over a generic
baseline on a quantized 13B model.  The hypothesis is: a Worker Agent with a
compiled system prompt and domain guardrails achieves higher task success,
fewer steps, fewer hallucinations, and zero guardrail violations even when
the underlying LLM is a resource-constrained quantized model.

---

## Setup

| Parameter | Value |
|---|---|
| Backend | Apollia backend name (ex: `llama-13b`) |
| Model | Llama 2 13B Q4_K_M via `EmbeddedBackend` (llama.cpp) |
| Quantization | Q4_K_M (≈ 8 GB RAM) |
| Inference | Apollia runtime `POST /api/v1/llm/complete` at `localhost:7771` |
| Seed | 42 (stored in report metadata) |
| Runs per test case | 5 |
| Temperature | 0.1 (worker), 0.3 (generic) |

The benchmark routes through the Apollia `LlmRouter` — same path as production
agents. It does **not** call ollama or any external inference process directly.

### Hardware used for reference run

MacBook Pro M1 16 GB — inference: ~8 tok/s (Metal acceleration).

---

## Agents compared

### excel-worker (Worker Agent)

- **File:** `agents/excel-worker.py`
- **Class:** `ExcelWorkerAgent(WorkerAgent)`
- Compiled `SYSTEM_PROMPT` with 4 absolute rules (e.g., never use bash for xlsx)
- Mandatory openpyxl patterns baked in
- Explicit error handling for `BadZipFile`, encoding issues, missing sheets
- `MAX_STEPS = 8`, `TEMPERATURE = 0.1`

### generic-agent (Baseline)

- **File:** `benchmarks/generic_agent.py`
- **Class:** `GenericAgent(BaseReActAgent)`
- Generic system prompt: "Tu es un assistant IA généraliste…"
- No domain knowledge, no guardrails, no pre-compiled patterns
- `MAX_STEPS = 10`, `TEMPERATURE = 0.3`

---

## Test cases

All fixture files are in `benchmarks/fixtures/`.

| TC | Description | Fixture(s) | Success criteria |
|---|---|---|---|
| 1 | Lecture simple (noms de feuilles) | `test_sales.xlsx` | Output contains "Ventes" |
| 2 | Totaux par colonne | `test_sales.xlsx` | Output contains "110" or "549" (correct sums) |
| 3 | CSV latin-1 + séparateur ";" | `test_data_latin1.csv` | Output contains column names from the CSV |
| 4 | Fichier corrompu | `test_corrupt.xlsx` | Output contains "corrompu" / "BadZipFile" / "error" |
| 5 | Modification + sauvegarde | `test_sales.xlsx` | Output confirms save ("sauvegard" / "saved") |

### Fixture details

**`test_sales.xlsx`**
- Sheet: `Ventes`
- Columns: `Produit`, `Quantite`, `Prix_Unitaire`
- 10 data rows: Produit_01…Produit_10, Quantite = 2×i, Prix_Unitaire = 9.99×i
- Quantite total: **110** · Prix_Unitaire total: **549.45**

**`test_corrupt.xlsx`**
- A file with `.xlsx` extension containing invalid (non-ZIP) bytes.
- Purpose: verify agents handle `zipfile.BadZipFile` gracefully.

**`test_data_latin1.csv`**
- Encoding: latin-1 (single-byte, not UTF-8)
- Separator: `;`
- Columns: `Nom`, `Ville`, `Score`
- Contains French characters: accented vowels, cedilla.

---

## Metrics

| Metric | Definition |
|---|---|
| `success_rate` | Fraction of runs (0.0–1.0) where the agent produced a correct answer |
| `avg_steps` | Average number of tool calls per run |
| `hallucinations` | Total count of unknown tool calls + reads of non-existent files |
| `guardrails_violated` | Total count of `bash_executor` calls referencing `.xlsx`/`.xlsm` files |

---

## How to reproduce

### Prerequisites

```bash
# 1. Configure a Llama 13B backend in Apollia (one-time)
apollia-os llm backends create \
  --name llama-13b \
  --provider llama-cpp \
  --model ~/.apollia/models/llama-2-13b-chat.Q4_K_M.gguf \
  --set-default

# 2. Start the Apollia runtime
apollia-os start

# 3. Install Python dependency for fixture generation
pip install openpyxl
```

### Run the full benchmark

```bash
cd /path/to/apollia-v2
python benchmarks/run_worker_benchmark.py \
  --backend llama-13b \
  --runs 5 \
  --seed 42
```

### Dry-run (no runtime required)

```bash
python benchmarks/run_worker_benchmark.py --dry-run
```

Returns representative pre-defined results to validate the report format
without requiring the Apollia runtime to be running.

### CLI reference

```
usage: run_worker_benchmark [-h] [--backend BACKEND] [--runtime-url URL]
                             [--runs RUNS] [--seed SEED]
                             [--dry-run] [--output OUTPUT] [--verbose]

  --backend      Apollia backend name (default: runtime default)
  --runtime-url  Apollia runtime URL (default: http://localhost:7771)
  --runs         number of runs per test case (default: 5)
  --seed         seed stored in report metadata (default: 42)
  --dry-run      skip LLM calls, return representative results
  --output       override output JSON file path
  --verbose      enable verbose logging
```

---

## Results

> Results below are from the first reference run. Re-run to refresh.

| TC | Description | Worker: success | Generic: success | Worker: steps | Generic: steps | Worker: guardrails | Generic: guardrails |
|---|---|---|---|---|---|---|---|
| 1 | Lecture simple | — | — | — | — | — | — |
| 2 | Totaux par colonne | — | — | — | — | — | — |
| 3 | CSV latin-1 | — | — | — | — | — | — |
| 4 | Fichier corrompu | — | — | — | — | — | — |
| 5 | Modification + sauvegarde | — | — | — | — | — | — |

_Run the benchmark and paste the JSON summary here._

---

## Report format

```json
{
  "metadata": {
    "backend": "llama-13b",
    "runtime_url": "http://localhost:7771",
    "hardware": "MacBook-Pro.local",
    "platform": "macOS-15.0-arm64",
    "seed": 42,
    "runs_per_test": 5,
    "timestamp": "2026-04-01T18:00:00Z",
    "dry_run": false
  },
  "results": [
    {
      "test_case": 1,
      "description": "Lecture simple (noms de feuilles)",
      "worker_agent": {
        "success_rate": 1.0,
        "avg_steps": 2.0,
        "hallucinations": 0,
        "guardrails_violated": 0
      },
      "generic_agent": {
        "success_rate": 0.6,
        "avg_steps": 4.2,
        "hallucinations": 1,
        "guardrails_violated": 0
      }
    }
  ]
}
```

---

## Related

- Worker Agent pattern: `docs/internal/strategy/capabilities-architecture-ideation.md` §5
- excel-worker: `agents/excel-worker.py`
- WorkerAgent base class: `sdk/apollia/agents/worker.py`
