# Construire un Worker Agent

Tout au long des chapitres précédents, vous avez utilisé `file-assistant` — un agent générique qui reçoit des instructions à la volée et improvise sa stratégie à chaque exécution. C'est puissant pour les tâches ouvertes. Mais pour les domaines spécialisés — CSV, Excel, SQL, Git, PDF — cette flexibilité se retourne contre vous.

Donnez à un modèle léger une tâche CSV sans guide précis : il choisira peut-être les mauvais séparateurs, oubliera de détecter l'encodage, interprétera les types numériques comme des chaînes. Les résultats varient selon le modèle, selon le prompt, selon l'humeur du contexte. C'est inacceptable pour un outil qu'on déploie en production.

Les **Worker Agents** résolvent ce problème en compilant l'expertise dans le code — pas en la récitant à chaque tâche.

---

## Ce que vous allez construire

Un Worker Agent complet pour l'analyse de fichiers CSV : `csv-data-worker`. Vous pourrez lui déléguer des tâches comme celle-ci depuis n'importe quel autre agent :

```python
# Depuis un Director Agent
result = await ctx.delegate(
    "analyze-csv",
    {"input": {"text": "Analyse /data/ventes-2026.csv et donne-moi les totaux par région"}}
)
```

Et l'installer d'une commande :

```bash
apollia-os agent install agents/community/csv-data-worker.py
# → Validation du manifest...
# ✔ Manifest valide (name: csv-data-worker, version: 0.1.0)
# → Exécution des tests...
# ✔ 4/4 tests passés
# ✔ Agent "csv-data-worker" installé
```

---

## Les cinq étapes

1. Comprendre le **pattern Worker Agent** — quand l'utiliser, comment il se différencie d'un agent générique
2. Écrire le **manifest** — outils, packages, skills A2A
3. Construire le **SYSTEM_PROMPT** — la colonne vertébrale de l'expertise domaine
4. Ajouter des **garde-fous dans le code** — la deuxième ligne de défense
5. Écrire les **tests** et **publier** dans le registre communautaire

---

## Ce que vous allez apprendre

- **Section 1 — Le pattern** : Worker vs. agent générique, la règle des 2 conditions sur 3, l'anatomie d'un fichier agent
- **Section 2 — Le SYSTEM_PROMPT** : les quatre sections, comment formuler des guardrails efficaces avec `JAMAIS`/`TOUJOURS`/`RAISON`
- **Section 3 — Les garde-fous domaine** : comment durcir les règles dans le code Python au-delà du prompt
- **Section 4 — Les tests** : les trois tests minimaux, les fixtures de domaine, les tests live avec le runtime
- **Section 5 — La publication** : l'installation, la validation du registre, les critères d'acceptation communautaires

---

## Prérequis

Avant de commencer ce chapitre, assurez-vous :

- Que le runtime est démarré (`apollia-os start --foreground`)
- Que `python_executor` est disponible (`apollia-os tool list | grep python`)
- Que `pandas` peut être installé dans votre environnement (`pip install pandas` — l'agent le fait automatiquement, mais vérifiez l'accès réseau)
