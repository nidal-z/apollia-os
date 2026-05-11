# APOLLIA.md — sections veille-ia (à coller dans le `APOLLIA.md` du workspace)

> Copier ces sections dans le `APOLLIA.md` à la racine de votre workspace pour customiser le comportement de l'agent veille-ia sans toucher au code.
>
> L'agent lit ces sections via `ctx.workspace.get(...)` au début de chaque run, en priorité sur les YAML locaux livrés dans `datasources/`.

---

## Veille IA — Feeds

```yaml
# Vos sources personnalisées (RSS/Atom). Format identique à datasources/feeds.yaml.
# Si vide ou absent, l'agent utilise les feeds par défaut shipped avec l'agent.

feeds:
  - id: my-internal-blog
    url: https://blog.mycompany.fr/feed
    category: tech-news
    weight: 0.8
    enabled: true
```

## Veille IA — Competitors

```yaml
# Vos concurrents personnalisés. Format identique à datasources/competitors.yaml.
# Cette liste s'AJOUTE aux concurrents par défaut (n'override pas, augmente).

competitors:
  - id: my-competitor
    name: My Competitor
    category: direct
    urls: [https://competitor.com]
    signals: ["AI", "agent"]
    threat_level: medium
```

## Veille IA — Queries

```yaml
# Requêtes additionnelles par axe. S'ajoutent aux queries par défaut.

axes:
  tech:
    queries:
      - "ma technologie 2026"
  competitive:
    queries:
      - "my-competitor news"
```

## Veille IA — Scoring

```yaml
# Override des critères de scoring. Si présent, REMPLACE intégralement les critères par défaut.

criteria:
  - id: my-custom-criterion
    weight: 4
    description: "Critère métier custom."
    matchers:
      keywords: ["mon mot-clé", "autre mot-clé"]
```

## Veille IA — Custom Rules

<!--
Règles métier libres en texte (lues par le synthesis-worker via ctx.workspace).
Utilisé pour ajuster le ton, ajouter des exclusions, renforcer la personnalisation.

Exemples :
- Ne JAMAIS inclure les articles d'un domaine X.
- Si la news touche le secteur "santé", flagger automatiquement comme critique.
- Ton du rapport : direct, sans hyperboles.
-->

## Datasource Paths

- veille-ia : datasources gérés en interne (`datasources/*.yaml`). Pour customiser, utiliser les sections ci-dessus.
