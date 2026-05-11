# veille-ia — Agent de veille IA/LLM (v3.0.0)

Agent Apollia OS qui produit chaque matin un **rapport de veille quotidienne** sur l'écosystème IA/LLM : nouveaux modèles, frameworks, mouvements des concurrents directs (n8n, Make, Zapier AI, Lindy AI, Dust.tt, Mistral, Cohere North, OpenFANG…), évolutions standards (MCP, A2A), et signaux régulation (EU AI Act).

Conçu comme **cas d'usage de référence** de la plateforme Apollia OS : il applique les **4 piliers obligatoires** issus de l'état de l'art 2025-2026.

## Les 4 piliers appliqués

| Pilier | Implémentation |
|---|---|
| **1 — Templates** | Pydantic schemas (`schemas.py`) + Jinja2 templates (`templates/*.md.j2`) + auto-repair max 3 retries. |
| **2 — Steps fonctionnels** | State machine 15 étapes (`state.py`). LLM appelé chirurgicalement sur 3 steps. |
| **3 — Datasources** | YAML externalisés (`datasources/*.yaml`). Lecture priorité : `ctx.workspace` > local > defaults. |
| **4 — Mémoire** | Entités `entity:{type}:{id}` avec timeline + dédup `seen:{hash}` + procédurale. |

---

## Architecture

```
veille-ia-agent  (Director — state machine 15 steps)
│
│  Délègue via A2A
├── web-search-worker         skill: search-and-extract
│      → web_search + web_read → articles filtrés + dédupliqués
│
├── entity-extraction-worker  skill: extract-entities          [NOUVEAU v3.0.0]
│      → LLM extrait entités (companies/products/events/topics)
│
└── synthesis-worker          skill: synthesize-report
       → LLM scoring + Pydantic VeilleReport JSON (pas de Markdown)
```

Le director (state machine déterministe) :
1. Charge datasources YAML (priorité `ctx.workspace` → local → defaults).
2. Recharge profil `user.*` + entités connues.
3. Bootstrap snapshot si TTL > 7j.
4. Délègue search-and-extract (axes tech + competitive) avec dédup hashes.
5. Délègue extract-entities → upsert `entity:*` mémoire.
6. Délègue synthesize-report → VeilleReport JSON validé Pydantic.
7. Filter critical findings (threshold + keywords).
8. Rendu Jinja2 (`templates/report.md.j2`).
9. Persiste épisode + nouveaux `seen:*` + procédurale.
10. Écrit fichier + notifie (desktop + webhook si critique).

## Structure du package

```
agents/veille-ia/
├── agent.toml                    # Manifest package (4 agents + trigger cron)
├── veille-ia-agent.py            # Director (state machine)
├── workers/
│   ├── web-search-worker.py
│   ├── entity-extraction-worker.py    # NOUVEAU v3.0.0
│   └── synthesis-worker.py
├── schemas.py                    # Pydantic (Pilier 1)
├── state.py                      # Enum VeilleStep (Pilier 2)
├── datasources/                  # YAML externalisés (Pilier 3)
│   ├── feeds.yaml
│   ├── competitors.yaml
│   ├── queries.yaml
│   └── scoring.yaml
├── templates/                    # Jinja2 (Pilier 1)
│   ├── report.md.j2
│   ├── alert.md.j2
│   └── summary.md.j2
├── eval/                         # Eval suite (eval-driven dev)
│   ├── cases.jsonl
│   ├── run-eval.py
│   └── golden/
├── APOLLIA.md                    # Sections client (à coller dans workspace APOLLIA.md)
├── setup.md                      # Guide install
├── CHANGELOG.md
└── README.md
```

## Mécanismes

### Mémoire à entités (saut qualitatif v3.0.0)

| Clé | Type | Contenu | Politique |
|---|---|---|---|
| `entity:company:{id}` | sémantique | `{name, threat_level, first_seen, last_event_date, signals: [...]}` | Merge incrémental |
| `entity:product:{id}` | sémantique | idem produits | idem |
| `entity:event:{id}` | sémantique | événement ponctuel daté | append-only |
| `entity:topic:{id}` | sémantique | sujet récurrent | running summary |
| `seen:{hash}` | sémantique | URL hash | confidence=1.0, dédup cross-run |
| `bootstrap.snapshot` | sémantique | landscape (concurrents + queries) | TTL 7j |
| `procedure:daily-veille` | procédurale | workflow appris | learn une fois, recall ensuite |
| `last_run_date`, runs épisodiques | mixte | métriques | append-only |

### Datasources externalisées

Lecture priorisée :
1. **`ctx.workspace.get("Veille IA — *")`** — sections APOLLIA.md du workspace (custom client).
2. **Local YAML** dans `datasources/*.yaml` (livré avec l'agent).
3. **Defaults Python** (fallback minimal).

Modifier la liste des concurrents ou les requêtes sans toucher au code : éditer YAML local OU sections APOLLIA.md du workspace.

### HITL conditionnel

Détection automatique de findings critiques :
- Score >= `critical_threshold` (défaut 5).
- OU keyword critique présent (`Series A/B/C/D`, `acquired`, `breach`, `0-day`, etc.).

→ Notification webhook prioritaire séparée (`alert.md.j2`).

## Prérequis & installation

Cf. [`setup.md`](./setup.md).

```bash
apollia agent install ./agents/veille-ia
apollia agent run veille-ia-agent --input "Génère la veille IA du jour"
```

## Eval

```bash
cd agents/veille-ia
python eval/run-eval.py
```

Métriques cibles L2 : success rate ≥ 80%, consistency ≥ 0.7.

## Liens utiles

- [État de l'art agents IA 2026](../../docs/internal/research/agents-2026/README.md)
- [Skill apollia-agent-forge](../../.claude/skills/apollia-agent-forge/SKILL.md)
- [MoSCoW scorecard Apollia](../../docs/internal/strategy/apollia-moscow-scorecard-2026-05.md)
- [Plan refonte M5a](../../docs/internal/release/M5-veille-ia-refonte-plan.md)
