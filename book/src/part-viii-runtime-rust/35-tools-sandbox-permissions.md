# Outils, sandbox, permissions

Quand un agent appelle `ctx.tools.call("bash_executor", ...)`, le code ne s'exécute pas directement dans le processus runtime. Il s'exécute dans une **sandbox**, et son invocation passe par le **moteur de permissions** à 3 couches. Ce chapitre couvre ces deux mécanismes plus le routing MCP.

---

## Pourquoi la sandbox existe

Sans isolation, un agent malveillant ou buggé pourrait :

- Lire les fichiers d'autres agents ou de l'utilisateur.
- Accéder au réseau sans autorisation.
- Voir les processus en cours sur la machine.
- Consommer des ressources système sans limite.

La sandbox d'Apollia OS répond à ces risques sans dépendance externe. Pas de Docker, pas de daemon système. Juste les **Linux user namespaces**, une fonctionnalité native du kernel disponible sans privilèges root.

---

## Ce qui est isolé

### Filesystem

Chaque agent dispose d'un répertoire sandbox dédié : `~/.apollia/sandboxes/<agent_id>/`. Les outils fichiers (`file_read`, `file_write`, etc.) sont limités à ce répertoire via `SandboxRoot`.

Tout chemin qui tenterait de sortir du sandbox (`../../etc/passwd`) est rejeté **avant** toute opération disque, avec le code `sandbox_violation`.

Les chemins absolus comme `/data/rapport.txt` sont autorisés tant que le fichier est accessible par le processus runtime. La protection vise les traversals, pas les chemins absolus légitimes. En production, configurez la sandbox pour limiter les chemins autorisés via `apollia.toml`.

### Réseau

Par défaut, les outils sandboxés n'ont pas accès au réseau. L'accès est explicitement déclaré dans le manifeste de l'agent et appliqué par le namespace réseau Linux. Les outils `web_read` et `web_search` ont un profil `NetworkRestricted` qui filtre les destinations.

### Processus

Le namespace PID isole les processus de l'outil. Un outil ne peut pas voir, lister, ni signaler les processus du système hôte ou des autres agents.

### Identité utilisateur

L'outil s'exécute comme `uid=0` (root) **dans son namespace** mais cet UID est mappé sur un UID non-privilégié sur le système hôte. En pratique, l'outil n'a aucun privilège réel.

---

## Sous le capot

Pour `bash_executor`, le runtime exécute en substance :

```bash
unshare \
  --user \      # user namespace : UID/GID mapping
  --mount \     # mount namespace : filesystem isolé
  --pid \       # PID namespace : processus isolés
  --net \       # network namespace : réseau isolé
  --fork \      # fork dans le nouveau namespace
  /bin/bash -c "command"
```

`unshare` est un binaire standard disponible sur toute installation Linux. Pas de daemon, pas d'installation préalable.

Pour `python_executor`, la même isolation namespace est appliquée, plus un venv Python dédié par agent :

```
~/.apollia/sandboxes/
└── pdf-worker/
    ├── venv/                 venv Python isolé
    │   ├── bin/python3
    │   └── lib/...
    └── workspace/            répertoire de travail
```

Les packages installés dans le venv d'un agent (`pypdf>=4`, etc.) ne contaminent ni les autres agents ni le Python système.

---

## Profils de sandbox

Chaque outil natif a un profil prédéfini qui détermine le niveau d'isolation :

| Profil | Filesystem | Réseau | Utilisé par |
|---|---|---|---|
| `ReadOnly` | Lecture seule | Aucun | `file_read`, `file_glob`, `file_grep`, `memory_search` |
| `FileSystem` | Lecture/écriture sandbox | Aucun | `file_write`, `file_edit`, `bash_executor`, `python_executor` |
| `NetworkRestricted` | ReadOnly | Whitelist + garde anti-SSRF | `web_read`, `web_search` |
| `Full` | Accès complet | Accès complet | Outils marqués `dangerous=true` |

Les profils sont prédéfinis par outil. Pour les outils MCP, le profil dépend du flag `requires_approval` côté config MCP.

---

## Pré-requis kernel Linux

La sandbox utilise les **unprivileged user namespaces**, disponibles sans root sur les kernels modernes. Vérification :

```bash
cat /proc/sys/kernel/unprivileged_userns_clone
# Doit afficher 1
```

Si la valeur est `0` :

```bash
sudo sysctl -w kernel.unprivileged_userns_clone=1
# ou de manière permanente
echo "kernel.unprivileged_userns_clone = 1" | sudo tee /etc/sysctl.d/apollia.conf
sudo sysctl -p /etc/sysctl.d/apollia.conf
```

Distributions qui activent les user namespaces par défaut : Ubuntu 22.04+, Debian 12+, Fedora 37+, Arch Linux.

---

## macOS : mode développement

Les Linux namespaces ne sont pas disponibles sur macOS. La sandbox est **désactivée par défaut** en mode développement sur macOS :

```toml
# ~/.config/apollia/apollia.toml (macOS uniquement)
[tools]
sandbox = false
```

Le runtime affiche un avertissement au démarrage :

```
⚠ sandbox désactivé (mode dev macOS)
```

Ne déployez jamais `sandbox = false` sur un système Linux de production. Ce mode existe uniquement pour le développement local sur macOS.

---

## Limitations connues v0.1

**Pas de quota RAM.** Un outil peut consommer autant de mémoire qu'il veut dans son namespace. Surveillez `/var/lib/apollia` et configurez les cgroups au niveau système si nécessaire.

**Pas de quota disque.** Un outil peut remplir le sandbox de l'agent. Surveillez l'espace disque en production.

**Protection path traversal uniquement pour les outils fichiers.** `bash_executor` avec accès réseau peut potentiellement accéder à des chemins hors sandbox si le `working_dir` n'est pas correctement configuré.

La roadmap sandbox prévoit `nsjail` (Google) en v0.2 pour une isolation plus stricte, et gVisor optionnel pour les déploiements production sensibles.

---

## Le moteur de permissions

Indépendamment de la sandbox, chaque invocation d'outil passe par le **PermissionEngine** qui applique 3 couches de règles. Les couches sont évaluées dans l'ordre, la première qui matche décide.

| Couche | Portée | Persistance |
|---|---|---|
| **Session** | Une session de chat ou une tâche | En mémoire, perdu au reboot |
| **Project** | Un projet (workspace) | `governance.db`, projet-scoped |
| **Global** | Toute la machine | `governance.db`, global |

À chaque appel d'outil :

```
ctx.tools.call("bash_executor", input={"cmd": "find /tmp"})
  │
  ▼
PermissionEngine.resolve(agent, tool, args)
  │
  ├── Session rules : match ?
  │     Allow  → OK, exécute
  │     Deny   → Refuse avec "not allowed for this session"
  │
  ├── Project rules : match ?
  │     Allow / Deny → idem
  │
  ├── Global rules : match ?
  │     Allow / Deny → idem
  │
  └── Aucune règle : déclenche HITL (prompt humain)
        └── Réponse "Toujours pour ce projet" → enregistrée en project rule
```

Le mécanisme est conçu pour démarrer permissif (HITL à chaque appel inconnu) puis se durcir progressivement à mesure que l'opérateur valide ou refuse.

---

## Lister et révoquer les règles

```bash
# Voir les règles persistées
$ apollia-os permissions list
  ID    OUTIL          PORTÉE    ARGUMENT             EXPIRATION   CRÉÉ LE
  1     file_write     project   /tmp/ @ /mon/proj    permanente   2026-04-25
  2     web_search     global    (tous)               permanente   2026-04-22

# Révoquer une règle
$ apollia-os permissions revoke 1
  ✔ Règle #1 révoquée

# Auditer les décisions automatiques
$ apollia-os permissions audit --tool web_search --limit 10
```

La sous-commande opère directement sur `governance.db`, pas besoin de runtime démarré.

---

## Routing MCP

Les outils préfixés `mcp:<server>/<name>` sont routés vers un serveur MCP connecté. Le runtime gère :

- L'enregistrement des serveurs MCP via `apollia-os mcp enable <server>`.
- Le lifecycle de la connexion (stdio, HTTP, SSE).
- Le marshalling des appels (JSON-RPC 2.0 selon spec MCP).
- L'audit trail commun (les appels MCP apparaissent dans `audit.db` avec un préfixe distinctif).

Côté agent, **aucune différence** : `ctx.tools.call("mcp:github/list_issues", input={...})` se comporte exactement comme un outil natif. Le routage est interne.

> **Référence technique :** la spec complète du client MCP (lifecycle, transport, ressources, sampling) sera dans la page wiki `Briques-MCP-Client` *(wiki disponible prochainement)*.

---

## Audit trail

Chaque invocation d'outil est enregistrée dans `~/.apollia/audit.db` :

```bash
$ apollia-os audit --last 5
  HEURE          AGENT            TÂCHE    OUTIL         DURÉE   RÉSULTAT
  10:00:05       pdf-worker       t-xyz    file_write    12ms    ✔
  10:00:04       pdf-worker       t-xyz    file_read     8ms     ✔
  09:58:11       pdf-worker       t-abc    file_read     6ms     ✗ not_found
```

L'audit trail est persisté en local SQLite, jamais envoyé à l'extérieur. C'est votre journal de traçabilité pour le debug et la conformité.

---

## ADRs

- `ADR-005` : Sandbox sans Docker
- `ADR-012` : Sandbox devmode macOS
- `ADR-044` : Client MCP natif
- `ADR-052` : Windows sandbox
- `ADR-061` : Permission engine 3 layers
- `ADR-082` : Tool governance unifiée
- `ADR-096` : Tool execution paths convergence

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
