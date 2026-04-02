# Sandbox et sécurité

Quand un agent appelle `bash_executor` ou `python_executor`, le code ne s'exécute pas directement dans le processus runtime. Il s'exécute dans un **sandbox** — un environnement isolé qui empêche l'outil d'accéder à des ressources non autorisées.

---

## Pourquoi le sandbox existe

Sans isolation, un agent malveillant ou buggé pourrait :
- Lire les fichiers d'autres agents ou de l'utilisateur
- Accéder au réseau sans autorisation
- Voir les processus en cours sur la machine
- Consommer des ressources système sans limite

Le sandbox d'Apollia OS répond à ces risques sans dépendance externe : pas de Docker, pas de daemon système. Juste les **Linux user namespaces** — une fonctionnalité native du kernel disponible sans privilèges root.

---

## Ce qui est isolé

### Filesystem

Chaque agent dispose d'un répertoire sandbox dédié : `~/.apollia/sandboxes/<agent_id>/`. Les outils fichiers (`file_read`, `file_write`, etc.) sont limités à ce répertoire via `SandboxRoot`.

Tout chemin qui tenterait de sortir du sandbox (`../../etc/passwd`) est rejeté **avant** toute opération disque, avec le code `sandbox_violation`.

> **Les chemins absolus comme `/data/rapport.txt` sont autorisés** tant que le fichier est accessible par le processus runtime. La protection vise les traversals, pas les chemins absolus légitimes. En production, configurez le sandbox pour limiter les chemins autorisés via `apollia.toml`.

### Réseau

Par défaut, les outils sandboxés n'ont pas accès au réseau. L'accès réseau est explicitement déclaré dans le manifest via `network_allowlist` et appliqué par le namespace réseau Linux.

```python
# Manifest — accès à deux domaines spécifiques
"network_allowlist": ["api.openai.com", "*.anthropic.com"]

# Manifest — accès complet (avec avertissement au démarrage)
"network_allowlist": ["*"]

# Manifest — aucun accès réseau (défaut)
"network_allowlist": None
```

### Processus

Le namespace PID isole les processus de l'outil. Un outil ne peut pas voir, lister, ni signaler les processus du système hôte ou des autres agents.

### Identité utilisateur

L'outil s'exécute comme `uid=0` (root) **dans son namespace** — mais cet UID est mappé sur un UID non-privilégié sur le système hôte. En pratique, l'outil n'a aucun privilège réel sur la machine.

---

## Comment ça fonctionne en coulisse

Pour `bash_executor`, le runtime exécute en substance :

```bash
unshare \
  --user \      # user namespace — UID/GID mapping
  --mount \     # mount namespace — filesystem isolé
  --pid \       # PID namespace — processus isolés
  --net \       # réseau namespace — réseau isolé
  --fork \      # fork dans le nouveau namespace
  /bin/bash -c "commande_agent"
```

`unshare` est un binaire standard disponible sur toute installation Linux. Pas de daemon, pas d'installation préalable. Apollia OS appelle ce binaire directement.

Pour `python_executor`, la même isolation namespace est appliquée, plus un venv Python dédié par agent :

```
~/.apollia/sandboxes/
└── file-assistant/
    ├── venv/                ← venv Python isolé
    │   ├── bin/python3
    │   └── lib/...
    └── workspace/           ← répertoire de travail de l'agent
```

Les packages installés dans le venv d'un agent ne contaminent ni les autres agents ni le Python système.

---

## Les profils sandbox

Chaque outil a un profil sandbox prédéfini qui détermine le niveau d'isolation :

| Profil | Filesystem | Réseau | Utilisé par |
|---|---|---|---|
| `ReadOnly` | Lecture seule | Aucun | `file_read`, `file_glob`, `file_grep`, `file_list`, `memory_search` |
| `FileSystem` | Lecture/écriture sandbox | Aucun | `file_write`, `file_edit`, `bash_executor`, `python_executor` |
| `NetworkRestricted` | ReadOnly | Whitelist uniquement | `http_fetch` |
| `Full` | Accès complet | Accès complet | Outils `dangerous: true` |

Les profils sont **prédéfinis par outil** — vous ne pouvez pas choisir un profil différent pour un outil natif. Pour les outils MCP, le profil dépend du champ `requires_approval` dans `mcp.toml`.

---

## Prérequis noyau Linux

Le sandbox utilise les **unprivileged user namespaces**, disponibles sans root sur les kernels modernes. Vérifiez :

```bash
cat /proc/sys/kernel/unprivileged_userns_clone
# Doit afficher 1
```

Si la valeur est `0` :
```bash
# Activer pour la session courante
sudo sysctl -w kernel.unprivileged_userns_clone=1

# Activer de manière permanente
echo "kernel.unprivileged_userns_clone = 1" | sudo tee /etc/sysctl.d/apollia.conf
sudo sysctl -p /etc/sysctl.d/apollia.conf
```

Distributions qui activent les user namespaces par défaut : Ubuntu 22.04+, Debian 12+, Fedora 37+, Arch Linux.

---

## macOS — mode développement

Les Linux namespaces ne sont pas disponibles sur macOS. Sur macOS, le sandbox est **désactivé** par défaut en mode développement :

```toml
# ~/.apollia/apollia.toml — macOS uniquement
[tools]
sandbox = false
```

Le runtime affiche un avertissement au démarrage :
```
⚠ sandbox désactivé (mode dev macOS)
```

**Ne déployez jamais `sandbox = false` sur un système Linux de production.** Ce mode existe uniquement pour le développement local.

---

## Ce que le sandbox ne protège pas

Le sandbox d'Apollia OS OS v0.1–v0.2 a des limitations connues :

**Pas de quota RAM** — un outil peut consommer autant de mémoire qu'il veut dans son namespace. Surveillez `/var/lib/apollia` et configurez les cgroups si nécessaire.

**Pas de quota disque** — un outil peut remplir le sandbox de l'agent. Surveillez l'espace disque en production.

**Protection path traversal uniquement pour les outils fichiers** — `bash_executor` avec `--net` peut potentiellement accéder à des chemins hors sandbox si le working_dir n'est pas correctement configuré.

La roadmap sandbox prévoit `nsjail` (Google) en v0.2 pour une isolation plus stricte, et gVisor optionnel en v1.0 pour les déploiements production sensibles.

---

## L'audit trail — tracer chaque appel

Indépendamment du sandbox, chaque invocation d'outil est enregistrée dans `~/.apollia/audit.db` :

```bash
$ apollia-os audit --last 5
  HEURE          AGENT            TÂCHE    OUTIL         DURÉE   RÉSULTAT
  10:00:05       file-assistant   t-xyz    file_write    12ms    ✔
  10:00:04       file-assistant   t-xyz    file_read     8ms     ✔
  09:58:11       file-assistant   t-abc    file_read     6ms     ✗ not_found
```

L'audit trail est persisté dans SQLite local — jamais envoyé à l'extérieur. C'est votre journal de traçabilité pour le débogage et la conformité.
