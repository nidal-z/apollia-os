# ADR-055 — Community Registry : distribution Git-based peer-to-peer

**Date :** 2026-04-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 34 — Beta Hardening

---

## Contexte

L'ADR-050 (Sprint 32) a défini la structure locale du registry communautaire V1 : un répertoire
`agents/community/` dans le projet Apollia, avec installation manuelle via path local.

La V2 promise dans ADR-050 — "le runtime peut résoudre une URL Git → cloner → valider → installer"
— est implémentée dans STORY-450. Cette ADR formalise les décisions d'architecture pour ce registre
distribué, notamment le format d'index, le protocole de découverte, et les garanties de sécurité.

Les contraintes fondamentales sont :
- Principe #1 (Local-first) : aucun endpoint central n'est requis pour le fonctionnement de base
- Principe #2 (Zéro dépendance externe) : pas de serveur géré par Apollia, pas de CDN, pas d'API externe
- Principe #4 (Fail fast) : validation complète avant installation, pas de confiance implicite

---

## Décision

### 1. Format du registre — repo Git public comme source de vérité

Chaque agent communautaire est un repo Git autonome. Le repo est le registre — pas un serveur HTTP
central. La commande d'installation :

```bash
apollia-os agent install https://github.com/org/my-worker.git
```

Clone le repo dans un répertoire temporaire, valide le manifest, et installe si valide.

### 2. Index optionnel — `registry.json` dans un repo dédié

Un repo d'index optionnel (`apollia-os/community-registry`) contient un fichier `registry.json`
listant les agents communautaires connus. Ce repo est configurable dans `apollia.toml` :

```toml
[agents.registry]
index_url = "https://github.com/apollia-os/community-registry.git"
```

Si non configuré ou si le repo est inaccessible : la découverte est désactivée, l'installation
directe par URL reste possible. Pas de point de défaillance central.

Format de `registry.json` :

```json
{
  "version": 1,
  "agents": [
    {
      "name": "browser-worker",
      "description": "Navigation web et capture d'écran",
      "git_url": "https://github.com/apollia-community/browser-worker.git",
      "skills": ["browse-url", "screenshot-url"],
      "maintainer": "apollia-community"
    }
  ]
}
```

### 3. Protocole de validation à l'installation

La validation suit les 4 étapes définies dans ADR-050, inchangées :

1. Validation du manifest (`manifest()` appelé, schéma `AgentManifest` vérifié)
2. Scan `dangerous_tools_allowed` (confirmation supplémentaire si présent)
3. Validation des packages pip (résolution PyPI, pas d'installation immédiate)
4. Test de smoke (`tests/test_smoke.py` si présent)

### 4. Pas de signature cryptographique en V2

Les agents communautaires ne sont pas signés cryptographiquement en V2. La confiance repose sur :

- L'URL Git présentée à l'utilisateur — ce qu'il voit est ce qui est cloné
- La validation du manifest à l'installation
- Le mécanisme `dangerous_tools_allowed` pour les agents nécessitant des permissions étendues

La signature GPG des commits Git est encouragée par la documentation mais pas requise par le runtime.
Un mécanisme de signature est différé à V3.

### 5. Commandes CLI

```bash
# Installation directe par URL Git
apollia-os agent install https://github.com/org/my-worker.git

# Recherche dans l'index (si configuré)
apollia-os agent search "browser"

# Listage des agents communautaires installés
apollia-os agent list --source community

# Mise à jour d'un agent communautaire
apollia-os agent update my-worker
```

`agent update` re-clone le repo à partir de la même URL, re-valide, et remplace l'installation
existante. L'ancienne version est conservée dans `~/.apollia/agent-backups/` pendant 7 jours.

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| **Registry HTTP centralisé géré par Apollia** | Point de défaillance unique, infrastructure à maintenir, coût opérationnel. Si le serveur est down, aucun agent ne peut être installé. Viole Principe #1 (local-first). |
| **npm-style registry (tarballs signés)** | Infrastructure complexe (serveur de packages, CDN, signatures). Sur-dimensionné pour la beta. Ollama et Homebrew montrent qu'un index Git suffit pour commencer. |
| **PyPI pour les agents Python** | Mélange les dépendances pip de l'agent (packages Python normaux) avec l'agent lui-même (code métier + manifest AIP). Confusion pour les utilisateurs. |
| **Aucun registre distant** | Acceptable pour V1 (path local), mais bloque l'écosystème communautaire — les utilisateurs ne peuvent pas partager leurs agents sans un mécanisme standardisé. |
| **Registry intégré dans le binaire Apollia** | Les agents communautaires évolueraient plus vite que le runtime. Un registre embarqué nécessiterait une mise à jour du binaire pour chaque nouvel agent. |

---

## Conséquences

**Positives :**
- Distribution P2P — aucun serveur central, chaque repo Git est sa propre source de vérité.
- Découverte optionnelle — l'index `registry.json` est un service de commodité, pas un prérequis.
- Compatible V1 : installation par path local toujours possible, aucune migration requise.
- Format `manifest.json` stable depuis ADR-050 — les agents V1 sont directement installables en V2.

**Négatives / Compromis :**
- Découvrabilité limitée sans index — les utilisateurs doivent connaître l'URL Git de l'agent.
  C'est acceptable pour la beta (communauté restreinte).
- Pas de vérification d'intégrité post-clonage (hash commit fixe vs HEAD) — un repo peut être
  modifié entre deux installations. Documenté comme limitation V2.
- `git clone` à l'installation implique que Git doit être disponible sur la machine cible.
  Sur Windows, Git n'est pas installé par défaut — fallback sur la lib `gitoxide` (Rust natif).

**Neutres / À surveiller :**
- Le repo d'index `apollia-os/community-registry` doit être modéré pour éviter les agents malveillants.
  Définir un processus de review avant la beta publique.
- La commande `agent update` doit gérer les changements incompatibles de manifest entre versions.

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : Chaque agent cloné localement. Le registre d'index est optionnel
  et ne bloque jamais l'installation directe. Conforme.
- **Principe #2 — Zéro dépendance externe** : Pas de serveur Apollia requis. Git est le protocole
  de transport — ubiquitaire et sans infrastructure dédiée. Fallback `gitoxide` si Git absent. Conforme.
- **Principe #4 — Fail fast** : Validation complète (4 étapes) à l'installation. Un agent invalide
  ne peut pas être installé. Conforme.

---

## Liens

- Story d'implémentation : STORY-450 (Sprint 34)
- Implémenté dans : `crates/apollia-cli/src/commands/agent.rs`, `crates/apollia-runtime/src/agent_installer.rs`
- ADR fondateur distribution : [ADR-050](ADR-050-distribution-worker-agents.md) — V1 (path local)
- ADR Worker Agents : [ADR-048](ADR-048-worker-agents-expertise-domaine.md)
