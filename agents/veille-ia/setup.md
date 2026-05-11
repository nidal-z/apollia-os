# Setup — veille-ia v3.0.0

Veille quotidienne IA/LLM avec state machine déterministe, mémoire à entités, et templates Jinja2 customisables.

## Prérequis

- Apollia OS v0.1.0+ installé (`apollia --version`).
- Backend LLM actif (configuré via `apollia llm config`).
- Outils web activés (déclarés dans `agent.toml` — chargés automatiquement).

## Installation

```bash
apollia agent install ./agents/veille-ia
```

Vérifier l'enregistrement des 4 agents :

```bash
apollia agent list | grep -E "(veille-ia-agent|web-search-worker|synthesis-worker|entity-extraction-worker)"
```

Vérifier le trigger :

```bash
apollia trigger list | grep daily-veille-ia
```

## Configuration

### 1. Sections APOLLIA.md (optionnel mais recommandé)

Copier le contenu de `APOLLIA.md` (fourni dans ce package) dans le `APOLLIA.md` à la racine de votre workspace pour customiser feeds / competitors / queries / scoring / règles métier.

L'agent lit ces sections au début de chaque run en priorité sur les YAML locaux livrés.

### 2. Datasources locaux (pour customisations permanentes)

Adapter les fichiers `datasources/*.yaml` à votre cas :

- `feeds.yaml` — sources RSS/Atom à monitorer.
- `competitors.yaml` — entités à suivre (concurrents, partenaires).
- `queries.yaml` — requêtes par axe.
- `scoring.yaml` — critères de scoring pondérés.

Le rechargement se fait au prochain run (pas besoin de redémarrer l'agent).

### 3. Profil utilisateur

Pour adapter le ton du rapport, configurer via l'onboarding (CLI ou desktop) :

```bash
apollia memory remember-user user.role "CTO"
apollia memory remember-user user.tech.stack "[\"Rust\",\"Python\",\"Tauri\"]"
apollia memory remember-user user.domain.sector "AI infrastructure"
```

### 4. Triggers

```bash
# Activer le trigger cron quotidien (lun-ven 7h)
apollia trigger enable daily-veille-ia

# Désactiver
apollia trigger disable daily-veille-ia
```

## Premier run

```bash
apollia agent run veille-ia-agent --input "Génère la veille IA du jour"
```

Le rapport est sauvegardé dans `~/.apollia/reports/veille-YYYY-MM-DD.md` et une notification desktop est envoyée.

## Customisation avancée

- **Modifier la fréquence** : éditer `agent.toml` (`schedule = "..."`) puis `apollia agent reload ./agents/veille-ia`.
- **Modifier le template du rapport** : éditer `templates/report.md.j2` (Jinja2).
- **Ajouter un nouveau worker** : voir [`docs/wiki/Worker-Agent-Pattern.md`](https://github.com/nidal-z/apollia-os/wiki/Worker-Agent-Pattern).
- **Modifier les seuils de scoring** : éditer `datasources/scoring.yaml` (`critical_threshold`, `include_threshold`).
- **HITL agent-driven** : modifier `CRITICAL_KEYWORDS` dans `veille-ia-agent.py`.

## Inspection

```bash
# Lister les rapports générés
ls ~/.apollia/reports/veille-*.md

# Lire le rapport du jour
cat ~/.apollia/reports/veille-$(date +%Y-%m-%d).md

# Voir toutes les clés en mémoire
apollia memory list veille-ia

# Voir les entités trackées
apollia memory list veille-ia | grep entity:

# Lire le snapshot concurrentiel bootstrap
apollia memory show veille-ia bootstrap.snapshot

# Voir l'historique des runs (mémoire épisodique)
apollia memory events veille-ia
```

## Eval (validation comportementale)

```bash
# Run la suite eval (5 cas × 5 runs)
cd agents/veille-ia
python eval/run-eval.py
```

Métriques cibles (L2) : success rate ≥ 80%, consistency ≥ 0.7, tool calls ≤ 15, wall clock ≤ 5min.

## Troubleshooting

| Erreur | Cause probable | Solution |
|---|---|---|
| `NO_LLM` | Backend LLM non configuré | `apollia llm config` |
| `STATE_LOOP` | Boucle dans la state machine (rare) | Inspecter `progress` + `errors` dans le `data` retourné, augmenter step_budget |
| `permission_denied` | Permission web absente | Run manuel + accepter HITL ; ou `apollia permissions allow web_search` |
| Trigger ne se déclenche pas | Cron mal formé ou disabled | `apollia trigger list` puis `apollia trigger enable daily-veille-ia` |
| Pydantic ValidationError répétée | LLM ne respecte pas le schema | Auto-repair tente 3 fois ; si échec persistant, downgrade temperature ou changer modèle |
| YAML invalide | Erreur syntaxe dans datasources/*.yaml | Valider avec `python -c "import yaml; yaml.safe_load(open('datasources/feeds.yaml'))"` |

## FAQ

**Q : Comment forcer un run hors horaire ?**
R : `apollia agent run veille-ia-agent --input "..."` directement.

**Q : Où sont les logs ?**
R : `~/.apollia/logs/veille-ia-agent.log` ou `apollia logs veille-ia-agent --tail 100`.

**Q : Le rapport contient des doublons inter-jours, normal ?**
R : Non, c'est un bug. Vérifier que `seen:*` est bien persisté (`apollia memory list veille-ia | grep seen:`). Si vide après plusieurs runs, possible problème de mémoire.

**Q : Comment ajouter une catégorie d'entité (ex: `entity:partner:*`) ?**
R : Modifier `schemas.py` (Literal `type`) + `entity-extraction-worker.py` (prompt). Bumper version manifest en mineur.

## Liens utiles

- [État de l'art agents IA 2026](../../docs/internal/research/agents-2026/README.md)
- [Skill apollia-agent-forge](../../.claude/skills/apollia-agent-forge/SKILL.md)
- [MoSCoW scorecard Apollia](../../docs/internal/strategy/apollia-moscow-scorecard-2026-05.md)
- [Plan refonte M5a](../../docs/internal/release/M5-veille-ia-refonte-plan.md)
