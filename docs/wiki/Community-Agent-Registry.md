# Community Agent Registry

Le registre communautaire permet aux développeurs tiers de distribuer des Worker Agents qui étendent Apollia OS au-delà des quatre agents bundled.

**V1** : installation depuis un chemin local (`agents/community/`).  
**V2** (ADR-026) : installation directe depuis une URL Git - **disponible**.

> **Référence technique :** [ADR-026 - Community Registry : distribution Git-based peer-to-peer](../adr/ADR-026-agent-install-distribution.md)

---

## Table des matières

1. [Format d'un agent communautaire](#format)
2. [Installation et validation](#installation)
3. [Registre distribué Git (V2)](#git-registry)
4. [Guide de contribution](#contributing)
5. [Agents de référence](#reference-agents)

---

## Format d'un agent communautaire {#format}

### Structure d'un repo Git communautaire (V2)

```
my-worker/
├── agent.py          ← Source de l'agent - obligatoire
├── manifest.json     ← Métadonnées AIP - obligatoire
├── requirements.txt  ← Packages pip - optionnel
├── README.md         ← Description et usage
└── tests/
    └── test_smoke.py ← Test de smoke - optionnel mais recommandé
```

### Contrat AIP

Le fichier Python doit exposer une variable module-level `agent` dont la classe implémente :

| Méthode | Signature | Requis |
|---|---|---|
| `manifest()` | ` → dict` (synchrone) | Oui |
| `run()` | `async (task, ctx) → dict` | Oui |
| `on_start()` | `async → None` | Non |
| `on_stop()` | `async → None` | Non |

### Champs manifest

```python
{
    "name":           "my-worker",    # unique, kebab-case
    "version":        "0.1.0",        # semver
    "description":    "...",
    "tools_required": ["bash_executor"],
    # Optionnel - déclarer explicitement si nécessaire :
    "dangerous_tools_allowed": False,
}
```

Si `dangerous_tools_allowed` est `True`, l'installeur affiche un avertissement de sécurité et demande confirmation avant de procéder.

---

## Installation et validation {#installation}

### Depuis un chemin local (V1)

```bash
apollia-os agent install agents/community/sql-worker.py
apollia-os agent install ./path/to/my-agent.py
```

### Depuis une URL Git (V2)

```bash
# Installation directe par URL Git
apollia-os agent install https://github.com/org/my-worker.git

# Recherche dans l'index communautaire (si configuré)
apollia-os agent search "browser"

# Lister les agents communautaires installés
apollia-os agent list --source community

# Mettre à jour un agent communautaire
apollia-os agent update my-worker
```

`agent update` re-clone le repo, re-valide, et remplace l'installation existante. L'ancienne version est conservée dans `~/.apollia/agent-backups/` pendant 7 jours.

### Étapes de validation (inchangées V1 → V2)

L'installeur effectue les vérifications suivantes dans l'ordre :

1. **Manifest conforme** - `manifest()` appelable, schéma `AgentManifest` vérifié.
2. **Scan sécurité** - si `dangerous_tools_allowed: True`, confirmation opérateur requise.
3. **Packages pip** - résolution PyPI vérifiée (pas d'installation immédiate).
4. **Test smoke** - `tests/test_smoke.py` si présent (`pytest`). Un code de sortie non nul bloque l'installation.

### Ignorer les tests

```bash
apollia-os agent install ./my-agent.py --skip-tests
```

Un avertissement est affiché. Non recommandé.

### Désinstaller

```bash
apollia-os agent uninstall my-worker
```

---

## Registre distribué Git (V2) {#git-registry}

### Architecture peer-to-peer

Chaque agent communautaire est un repo Git autonome - le repo **est** le registre. Pas de serveur central requis.

La découverte optionnelle passe par un index `registry.json` dans un repo Git public (ex. `apollia-os/community-registry`) :

```json
{
  "version": "1",
  "agents": [
    {
      "name": "browser-worker",
      "description": "Navigation web et capture d'écran",
      "git_url": "https://github.com/apollia-os/browser-worker.git",
      "version": "0.1.0",
      "skills": ["browse-url", "screenshot-url"],
      "maintainer": "apollia-community"
    }
  ]
}
```

Cet index est optionnel - `apollia-os agent install <git-url>` fonctionne sans lui.

### Pas de signature cryptographique en V2

La confiance repose sur l'URL Git présentée à l'utilisateur. La signature GPG des commits est encouragée mais non requise. Un mécanisme de signature est différé à V3.

### Fallback `gitoxide` si Git absent

Sur les machines sans `git` (Windows notamment), le runtime utilise la lib Rust `gitoxide` pour cloner le repo.

> **Référence technique :** [ADR-026](../adr/ADR-026-agent-install-distribution.md) - décisions détaillées sur le format d'index, la validation, et la sécurité.

---

## Guide de contribution {#contributing}

### Critères d'acceptation

Un agent communautaire doit satisfaire **les trois** critères suivants :

1. **Séquence non-triviale** - l'agent effectue un workflow multi-étapes spécifique au domaine. Un wrapper autour d'un seul appel d'outil n'est pas un Worker Agent.

2. **Garde-fous domaine codés** - au moins une règle de sécurité doit être encodée dans le code source (pas seulement dans `SYSTEM_PROMPT`). Exemples :
   - Prévention SQL injection via requêtes paramétrées
   - Blocage des commandes Git destructives

3. **Suite de tests** - un fichier `tests/test_smoke.py` couvrant au moins un cas d'erreur.

### Checklist avant soumission

- [ ] `manifest()` retourne un dict conforme (`name`, `version`, `tools_required`, `description`).
- [ ] `dangerous_tools_allowed` déclaré **explicitement** si nécessaire.
- [ ] Au moins un garde-fou domaine dans le code source.
- [ ] `pytest tests/test_smoke.py` sort avec code 0.
- [ ] `apollia-os agent install <git-url>` réussit sur une installation propre.

---

## Agents de référence {#reference-agents}

### sql-worker

| Champ | Valeur |
|---|---|
| Repo | `agents/community/sql-worker.py` |
| Skills | `query-sql`, `schema-inspect`, `data-export` |
| Outils requis | `python_executor`, `file_read` |
| Packages externes | aucun (stdlib Python `sqlite3`) |
| `dangerous_tools_allowed` | `False` (mutations requièrent opt-in explicite) |

Garde-fous codés dans l'agent :
- SELECT-only par défaut - INSERT/UPDATE/DELETE/DROP bloqués sauf `dangerous_tools_allowed: True`.
- Requêtes paramétrées uniquement - interpolation f-string dans le SQL interdite.
- Timeout 30 secondes.
- Vérification existence fichier + `PRAGMA integrity_check` à la première connexion.
- Connexion fermée via context manager `with`.

```bash
apollia-os agent install agents/community/sql-worker.py
```

### git-worker

| Champ | Valeur |
|---|---|
| Repo | `agents/community/git-worker.py` |
| Skills | `git-status`, `git-diff`, `git-commit` |
| Outils requis | `bash_executor`, `file_read` |
| Packages externes | aucun (délègue au `git` système) |
| `dangerous_tools_allowed` | `False` |

Garde-fous codés dans l'agent :
- Commandes destructives refusées : `git push --force`, `git reset --hard`, `git clean -fd`, `git branch -D`, `git checkout --.`
- Messages de commit au format conventionnel Apollia : `type(scope): description`.
- `git status` toujours exécuté avant tout `git add` ou `git commit`.
- Opérations distantes (`push`, `pull`, `fetch`) requièrent une approbation explicite.

```bash
apollia-os agent install agents/community/git-worker.py
```

### browser-worker

| Champ | Valeur |
|---|---|
| Repo | `agents/browser-worker.py` |
| Skills | `browse-url`, `screenshot-url` |
| Outils requis | `bash_executor`, `file_write` |
| Packages externes | `playwright`, `pillow` |
| `dangerous_tools_allowed` | `False` |

Garde-fous codés dans l'agent :
- Validation de l'URL avant navigation (schéma `http`/`https` uniquement).
- Timeout par page configurable (défaut : 30 secondes).
- Screenshots sauvegardés dans un répertoire temporaire, pas dans le workspace agent.

```bash
apollia-os agent install https://github.com/apollia-os/browser-worker.git
```

### email-worker

| Champ | Valeur |
|---|---|
| Repo | `agents/email-worker.py` |
| Skills | `send-email`, `read-inbox` |
| Outils requis | `python_executor` |
| Packages externes | `smtplib` (stdlib) |
| `dangerous_tools_allowed` | `False` |

Garde-fous codés dans l'agent :
- `send-email` est une action HITL - l'opérateur doit approuver avant envoi.
- Validation des adresses email avant soumission.
- Pas de pièces jointes sans `dangerous_tools_allowed: True`.

```bash
apollia-os agent install https://github.com/apollia-os/email-worker.git
```

### slack-worker

| Champ | Valeur |
|---|---|
| Repo | `agents/slack-worker.py` |
| Skills | `send-message`, `read-channel` |
| Outils requis | `python_executor` |
| Packages externes | `slack-sdk` |
| `dangerous_tools_allowed` | `False` |

Garde-fous codés dans l'agent :
- `send-message` est une action HITL - l'opérateur doit approuver avant envoi.
- `read-channel` en lecture seule - aucune modification de canal possible.
- Token Slack lu depuis variable d'environnement `SLACK_BOT_TOKEN`, jamais hardcodé.

```bash
apollia-os agent install https://github.com/apollia-os/slack-worker.git
```

---

*Voir aussi : [Worker-Agent-Pattern](./Worker-Agent-Pattern) · [ADR-025](../adr/ADR-025-worker-agents-a2a-routing.md) · [ADR-026](../adr/ADR-026-agent-install-distribution.md) · [ADR-026](../adr/ADR-026-agent-install-distribution.md)*
