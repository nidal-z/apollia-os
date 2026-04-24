# veille-ia — Agent de veille IA/LLM

Agent Apollia OS qui produit chaque matin un **rapport de veille quotidienne** sur
l'écosystème IA/LLM : nouveaux modèles, frameworks, et mouvements des concurrents
directs d'Apollia (n8n, Make, Zapier AI, Lindy AI, Dust.tt…).

Conçu comme **cas d'usage de référence** de la plateforme : il exploite la mémoire
cross-session, la délégation A2A, les outils web natifs, et le trigger cron.

---

## Architecture

```
veille-ia-agent  (Director — veille-ia-agent.py)
│
│  Délègue via A2A
├── web-search-worker   (workers/web-search-worker.py)
│     skill: search-and-extract
│     → web_search + web_read → articles filtrés + dédupliqués
│
└── synthesis-worker    (workers/synthesis-worker.py)
      skill: synthesize-report
      → LLM only → rapport Markdown + scoring de pertinence
```

**Trois agents collaborent :**

| Agent | Rôle | Outils |
|---|---|---|
| `veille-ia-agent` | Director — orchestre, mémorise, sauvegarde | `file_write` |
| `web-search-worker` | Recherche web, extraction contenu, déduplication | `web_search`, `web_read` |
| `synthesis-worker` | Analyse, scoring 1–5★, rapport Markdown | LLM uniquement |

---

## Mécanismes

### 1. Mémoire cross-session

Le Director maintient un espace mémoire SQLite isolé (`namespace: veille-ia`) :

| Clé | Type | Contenu |
|---|---|---|
| `bootstrap.snapshot` | Sémantique | Liste concurrents + requêtes de recherche |
| `bootstrap.status` / `meta` | Sémantique | Statut et date du bootstrap (TTL 7 jours) |
| `seen:{url_hash}` | Sémantique | Articles déjà vus → déduplication inter-session |
| `last_run_date` | Sémantique | Date ISO du dernier run |
| `total_runs` | Sémantique | Compteur de runs |
| *(par run)* | Épisodique | `"Run du {date} : N articles, succès"` |

### 2. Bootstrap du paysage concurrentiel

Au premier run (et tous les 7 jours), le Director initialise en mémoire la liste
des concurrents et les requêtes de recherche. Cela évite de reconfigurer à la main
et rend l'agent opérationnel dès le premier lancement.

### 3. Déduplication par hash d'URL

À chaque run, les URLs déjà traitées sont récupérées depuis la mémoire et passées
au `web-search-worker`. Les articles déjà vus sont filtrés avant extraction,
ce qui garantit que chaque rapport ne contient que des **nouveautés**.

### 4. Délégation A2A

Le Director ne fait pas de recherche web lui-même. Il délègue :
1. `search-and-extract` → récupère les articles pour l'axe tech
2. `search-and-extract` → récupère les articles pour l'axe concurrentiel
3. `synthesize-report` → produit le rapport Markdown final

Si un worker est indisponible, l'agent continue et génère un rapport partiel.

### 5. Rapport sauvegardé localement

Le rapport est écrit dans `~/.apollia/reports/veille-YYYY-MM-DD.md`.
Une notification (desktop et/ou Discord) est envoyée à la fin du run —
configurée via l'interface Apollia.

---

## Structure du package

```
agents/veille-ia/
├── agent.toml               ← déclaration du package (agents, outils, trigger)
├── veille-ia-agent.py       ← Director
├── workers/
│   ├── web-search-worker.py ← Worker recherche/extraction
│   └── synthesis-worker.py  ← Worker synthèse/rapport
└── README.md
```

### `agent.toml`

Fichier de configuration **auto-suffisant** du package. Il déclare les agents,
active les outils web, et configure le trigger cron. Le runtime l'injecte en base
de données au chargement — aucune modification de la configuration globale nécessaire.

```toml
[package]
name = "veille-ia"

[[agents]]
name  = "veille-ia-agent"
entry = "veille-ia-agent.py"
role  = "director"

[tools]
web = { enabled = true, ssrf_guard = true }

[[triggers]]
id       = "daily-veille-ia"
agent    = "veille-ia-agent"
on_busy  = "skip"

[triggers.source]
type     = "cron"
schedule = "0 7 * * 1-5"   # Lun–Ven à 7h00
```

---

## Prérequis

- Apollia OS installé et configuré
- Un backend LLM actif (configuré via l'interface Apollia)
- Les outils web activés (déclarés dans `agent.toml` — chargés automatiquement)

---

## Installation

```bash
# Installer le package depuis le dossier
apollia agent install ./agents/veille-ia

# Vérifier que les 3 agents sont enregistrés
apollia agent list
# → veille-ia-agent   (director)
# → web-search-worker (worker)
# → synthesis-worker  (worker)

# Vérifier que le trigger est planifié
apollia trigger list
# → daily-veille-ia   cron "0 7 * * 1-5"   veille-ia-agent
```

---

## Lancement

### Run manuel

```bash
# Démarrer les workers en premier
apollia agent start web-search-worker
apollia agent start synthesis-worker

# Démarrer le Director
apollia agent start veille-ia-agent

# Lancer un run de veille maintenant
apollia task run veille-ia-agent '{"text": "Génère la veille IA du jour"}'
```

### Run automatique (trigger cron)

Le trigger `daily-veille-ia` se déclenche automatiquement chaque lundi–vendredi
à 7h00 dès que le runtime tourne. Aucune action manuelle nécessaire.

```bash
# Vérifier l'historique des runs
apollia trigger history daily-veille-ia

# Activer / désactiver le trigger
apollia trigger enable  daily-veille-ia
apollia trigger disable daily-veille-ia
```

### Consulter les rapports

```bash
# Lister les rapports générés
ls ~/.apollia/reports/veille-*.md

# Lire le rapport du jour
cat ~/.apollia/reports/veille-$(date +%Y-%m-%d).md
```

### Inspecter la mémoire

```bash
# Voir toutes les clés en mémoire
apollia memory list veille-ia

# Lire le snapshot concurrentiel
apollia memory show veille-ia bootstrap.snapshot

# Voir l'historique des runs (mémoire épisodique)
apollia memory events veille-ia
```

---

## Notifications

Les canaux de notification (desktop, Discord, webhook) sont configurés via
l'interface Apollia — pas dans le code ni dans `agent.toml`.

```bash
# Ajouter une notification desktop
apollia notification channel add desktop

# Ajouter un webhook Discord
apollia notification channel add discord \
  --webhook-url "https://discord.com/api/webhooks/..."

# Vérifier les canaux actifs
apollia notification channel list
```

Le runtime envoie automatiquement une notification sur `task.completed` avec
le résumé exécutif du rapport.

---

## Personnalisation

Pour ajuster les concurrents surveillés ou les requêtes de recherche, modifier
le dictionnaire `_INITIAL_SNAPSHOT` dans `veille-ia-agent.py`, puis réinitialiser
le bootstrap :

```bash
apollia memory forget veille-ia bootstrap.status
apollia task run veille-ia-agent '{"text": "Réinitialise le bootstrap"}'
```

Pour modifier la fréquence du trigger, éditer `agent.toml` et recharger :

```bash
# Modifier schedule dans agent.toml, puis :
apollia agent reload ./agents/veille-ia
```
