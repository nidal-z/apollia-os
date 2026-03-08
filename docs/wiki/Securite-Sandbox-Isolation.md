# Sécurité — Sandbox Isolation — Apollia OS

> Comment Apollia OS isole l'exécution des outils système via Linux namespaces, sans Docker.
> Public cible : administrateur système, contributeur Rust

---

## Vue d'ensemble

Apollia OS utilise les **Linux user namespaces** (via `unshare`) pour isoler l'exécution de `bash_executor` et `python_executor`. Pas de Docker, pas de daemon requis — les namespaces sont une fonctionnalité native du kernel Linux disponible sans privilèges root.

Cette approche implémente le Principe #2 (Zéro dépendance externe) et ADR-005.

---

## Ce que le sandbox protège

### Isolation filesystem

Chaque exécution d'outil voit un filesystem différent :
- Lecture seule sur les chemins système (`/usr`, `/lib`, `/etc`)
- Lecture/écriture uniquement dans le répertoire de travail de l'agent
- Pas d'accès aux fichiers d'autres agents ou d'autres utilisateurs

### Isolation réseau

Par défaut, un outil sandboxé n'a pas accès au réseau :
- `network_allowlist: null` dans le manifest = pas de réseau
- `network_allowlist: ["api.example.com"]` = seul ce domaine est accessible

### Isolation PID

Les processus lancés par l'outil ne peuvent pas voir ni signaler les autres processus du système.

### Isolation utilisateur

L'outil s'exécute avec un UID mappé qui apparaît root dans le namespace mais n'a aucun privilège sur le système hôte.

---

## SandboxProfile — niveaux d'isolation

Le Tool Registry attribue un `SandboxProfile` à chaque outil :

| Profile | Filesystem | Réseau | Usage |
|---|---|---|---|
| `None` | Accès complet | Accès complet | Pas de sandbox — outils internes uniquement |
| `ReadOnly` | Lecture seule | Aucun | Lecture de fichiers, inspection |
| `FileSystem` | Lecture/écriture sur le working dir | Aucun | `bash_executor`, `python_executor`, `file_io` |
| `NetworkRestricted` | ReadOnly | Whitelist uniquement | `http_client` (futur) |

```rust
// apollia-core/src/lib.rs
pub enum SandboxProfile {
    None,
    ReadOnly,
    FileSystem,
    NetworkRestricted,
}
```

---

## BashExecutor — isolation Linux namespaces

```bash
# Ce qu'Apollia OS exécute en coulisse pour chaque commande
unshare \
  --user \         # user namespace (UID/GID mapping)
  --mount \        # mount namespace (filesystem isolé)
  --pid \          # PID namespace (pas de vue des processus hôte)
  --net \          # net namespace (réseau isolé, par défaut)
  --fork \         # fork dans le nouveau namespace
  /bin/bash -c "commande_agent"
```

**Vérification que le sandbox fonctionne :**
```bash
# Depuis un agent, après apollo-os agent start
result = await ctx.tools.call("bash_executor", {
    "command": "id && ls /proc/1/exe"
})
# id affiche uid=0 (root DANS le namespace uniquement)
# /proc/1/exe est limité au namespace courant
```

---

## PythonExecutor — venv isolation

En plus du namespace, `python_executor` crée un venv Python dédié par agent :

```
/var/lib/apollia/venvs/
├── hello-agent/          ← venv de l'agent hello-agent
│   ├── bin/python3
│   └── lib/python3.13/...
└── devis-agent/          ← venv de l'agent devis-agent
    ├── bin/python3
    └── lib/python3.13/...
```

Les packages installés par un agent ne contaminent pas les autres agents et ne polluent pas le Python système.

---

## Mode développement macOS (ADR-012)

Les Linux namespaces ne sont pas disponibles sur macOS. En mode dev, `bash_executor` et `python_executor` s'exécutent sans namespace :

```toml
# apollia.toml sur macOS
[tools]
sandbox = false
```

**Important :** ne jamais déployer `sandbox = false` en production Linux. Ce mode existe uniquement pour le développement local sur macOS.

La détection est automatique : le runtime affiche un warning `⚠ sandbox désactivé (mode dev macOS)` au démarrage.

---

## Prérequis noyau Linux

```bash
# Vérifier les user namespaces
cat /proc/sys/kernel/unprivileged_userns_clone
# Doit afficher 1

# Si 0, activer :
sysctl -w kernel.unprivileged_userns_clone=1

# Permanent
echo "kernel.unprivileged_userns_clone = 1" >> /etc/sysctl.d/apollia.conf
sysctl -p /etc/sysctl.d/apollia.conf
```

Distributions qui activent les user namespaces par défaut : Ubuntu 22.04+, Debian 12+, Fedora 37+, Arch Linux.

---

## Limitations connues

**Pas d'isolation mémoire RAM :** Un outil peut consommer autant de RAM qu'il veut. Contrôler via le `wall_clock_timeout` et le monitoring système.

**Pas d'isolation disque (quota) :** Un outil peut remplir le disque dans son working dir. Surveiller `/var/lib/apollia` en production.

**macOS : sandbox désactivé :** Sur macOS, les outils s'exécutent sans isolation. Réservé au développement local.

**Noyaux anciens (< 4.18) :** Les user namespaces avec réseau isolé peuvent ne pas fonctionner. Mettre à jour le kernel ou désactiver le sandbox réseau.

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) — Principe #2 Zéro dépendance externe
- [Sécurité Local-First](./Securite-Local-First) — souveraineté des données
- [Sécurité Guardrails](./Securite-Guardrails) — StepBudget
- [ADR-005](../adr/ADR-005-sandbox-sans-docker) — pourquoi namespaces plutôt que Docker
- [ADR-012](../adr/ADR-012-sandbox-devmode-macos) — mode dev macOS
