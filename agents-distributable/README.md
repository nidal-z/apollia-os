# agents-distributable — Apollia agent bundles

Dossier des agents distribués sous le format **agent bundle** défini par
[ADR-074](../docs/adr/ADR-074-agent-bundle-format.md).

## Structure d'un bundle

```
<agent-name>/
├── manifest.toml   ← métadonnées statiques (obligatoire)
├── agent.py        ← point d'entrée (obligatoire)
├── lib/            ← modules locaux (optionnel)
├── assets/         ← ressources read-only (optionnel)
└── requirements.txt ← deps pip (informatif v0.1.0)
```

## Bundles présents

| Bundle | Version | Description |
|---|---|---|
| `spec-assistant/` | 2.0.0 | Consultant qui transforme une idée floue en TaskSpec structurée. |
| `dev-assistant/` | 2.0.0 | Exploration de codebase + implémentation (orchestrateur A2A vers code-worker). |
| `review-assistant/` | 2.0.0 | Review post-impl : lit la spec, run les tests, produit un rapport 🟢🟡🔴. |
| `document-assistant/` | 2.0.0 | Orchestrateur A2A pour Excel/CSV/PDF/SQLite, langage métier. |

## Installation

**Via UI desktop :** Inactifs → bouton « Installer un agent » → sélectionner le
dossier du bundle ou un `.tar.gz` (v0.2+).

**Via CLI :**

```bash
apollia-os agent install ./agents-distributable/spec-assistant
```

## Build d'un tarball (v0.2+)

```bash
tar czf spec-assistant-2.0.0.tar.gz -C agents-distributable spec-assistant
```

## Contribuer un agent

Voir [ADR-074](../docs/adr/ADR-074-agent-bundle-format.md) pour la spec complète
du format.
