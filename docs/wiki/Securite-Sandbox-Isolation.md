# Sécurité - Sandbox et Isolation - Apollia OS

> Comment Apollia OS isole l'exécution des outils système et protège l'API REST locale : Linux namespaces, authentification par token, sandbox Windows 3 couches (ADR-003).
> Public cible : administrateur système, contributeur Rust

---

## Prérequis

- **Linux :** kernel ≥ 4.18 avec `unprivileged_userns_clone = 1`. Vérifier : `sysctl kernel.unprivileged_userns_clone`. Si la valeur est 0, activer avec `sudo sysctl -w kernel.unprivileged_userns_clone=1` (persistant : ajouter dans `/etc/sysctl.d/apollia.conf`).
- **macOS :** le sandbox Linux n'est **pas disponible**. En développement, `sandbox = false` est utilisé par défaut. **Ne JAMAIS déployer avec `sandbox = false` en production.**

## Vue d'ensemble

Apollia OS utilise les **Linux user namespaces** (via `unshare`) pour isoler l'exécution de `bash_executor` et `python_executor`. Pas de Docker, pas de daemon requis - les [user namespaces](https://man7.org/linux/man-pages/man7/user_namespaces.7.html) sont une fonctionnalité native du kernel Linux disponible sans privilèges root.

La commande `unshare` crée un environnement isolé avec ses propres identifiants utilisateur (`--user`), son propre filesystem (`--mount`) et sa propre pile réseau (`--net`). L'outil s'exécute comme root dans son namespace (uid 0) mais n'a aucun privilège réel sur le système hôte.

Cette approche implémente le Principe #2 (Zéro dépendance externe) et ADR-023.

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

## SandboxProfile - niveaux d'isolation

Le Tool Registry attribue un `SandboxProfile` à chaque outil :

| Profile | Filesystem | Réseau | Usage |
|---|---|---|---|
| `None` | Accès complet | Accès complet | Pas de sandbox - outils internes uniquement |
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

## BashExecutor - isolation Linux namespaces

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

## PythonExecutor - venv isolation

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

## Authentification API TCP (ADR-002)

L'API REST TCP `:7771` est protégée par un token statique.

```
~/.apollia/api-token   # 64 hex chars + newline, chmod 0600
```

- Généré automatiquement au premier démarrage via `rand::rngs::OsRng`.
- Le runtime refuse de démarrer si les permissions sont trop ouvertes (`0640`, `0644`, etc.).
- Toutes les requêtes TCP doivent porter `Authorization: Bearer <token>`.
- Comparaison à **temps constant** via `subtle::ConstantTimeEq` - pas de timing attack.
- Le **socket Unix** reste non authentifié (permissions filesystem suffisent).

Configurable dans `apollia.toml` :

```toml
[api]
require_token = true      # défaut : true - NE PAS désactiver en production
bind = "127.0.0.1"        # loopback uniquement par défaut
```

Commandes de gestion :

```bash
apollia-os config show-token    # afficher le token courant
apollia-os config rotate-token  # régénérer un nouveau token (redémarre le runtime)
```

> **Référence technique :** [ADR-016](../adr/ADR-016-secrets-keyring-api-auth.md)

---

## Sandbox Windows - modèle Chromium 3 couches *(ADR-003 - déploiement différé)*

L'ADR-003 définit la stratégie de sandbox pour Windows natif (implémentation différée). Le design est formalisé et implémentable dès qu'un environnement Windows est disponible.

**Architecture 3 couches :**

| Couche | Mécanisme | Vecteurs couverts |
|---|---|---|
| 1 - Job Object | `win32job::Job` | Terminaison auto, pas de boîte de dialogue d'erreur Windows |
| 2 - Restricted Token | `CreateRestrictedToken` (Win32) | Suppression groupes sensibles, retrait privilèges dangereux |
| 3 - AppContainer | `CreateAppContainerProfile` (Win32) | Isolation réseau/filesystem par profil dédié |

**Dégradation gracieuse :** si AppContainer échoue (Windows 7, erreur API), le runtime se replie sur couches 1+2 avec un warning explicite.

**Portabilité :** tout le code Windows est dans `crates/apollia-tools/src/sandbox_windows.rs` sous `#[cfg(target_os = "windows")]`. Zéro impact sur la compilation Linux/macOS.

> **Référence technique :** [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) · [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) (sandbox Linux)

---

## Autonomie filesystem

Les agents Apollia OS sont véritablement autonomes sur le filesystem, régulés par une friction graduée HITL et protégés par un journal réversible. Ce modèle remplace le "tout bloquer" par "approuver intelligemment".

### Architecture 4 couches

| Couche | Mécanisme | Rôle |
|---|---|---|
| 0 - Workspace déclaratif | `Project.workspace_path` + file picker natif | Définit le périmètre de travail de l'agent |
| 1 - RiskClassifier | Classification pré-exécution des opérations fichier | Évalue le risque de chaque mutation |
| 2 - HITL graduée | Modal diff/preview pour opérations sensibles | L'utilisateur approuve ou refuse avec contexte |
| 3 - Journal réversible | Log JSONL de chaque mutation avant exécution | Restauration post-hoc via `apollia rollback` |

### RiskClassifier - niveaux de risque filesystem

Le `RiskClassifier` (étendu depuis `apollia-permissions`) évalue chaque opération fichier avant exécution :

| Niveau | Opérations | Comportement |
|---|---|---|
| **None** | Lecture dans le workspace | Exécution immédiate, zéro friction |
| **Low** | Écriture/création dans le workspace | Exécution avec log dans le journal |
| **Medium** | Écriture hors workspace, suppression fichier | Modal HITL : diff/preview avant approbation |
| **High** | Écriture dans `/etc`, `/usr`, suppression récursive | Modal HITL bloquant : explication requise |

### Workspace par session

Chaque session de chat a un `workspace_path` dédié, résolu depuis le `Project` associé. Le `NativeChatToolInvoker` (refactoré) injecte ce chemin dans chaque outil natif - l'agent travaille dans le bon répertoire sans configuration manuelle.

### Journal réversible et `apollia rollback`

Chaque mutation filesystem est loggée avant exécution dans un fichier JSONL par session. Le journal contient l'opération inverse (contenu original pour les écritures, chemin pour les créations).

```bash
# Voir les mutations d'une session (dry-run)
$ apollia rollback <session-id> --dry-run
  [1] WRITE  ./src/main.rs (42 bytes → 58 bytes)
  [2] CREATE ./src/new_file.rs
  [3] DELETE ./src/old_file.rs

# Restaurer l'état avant la session
$ apollia rollback <session-id>
  ✔ 3 mutations annulées
```

### HITL filesystem - modal diff/preview

Pour les opérations classées Medium ou High, un modal desktop affiche :
- Le **diff** complet (avant/après) pour les écritures
- Le **chemin et contenu** pour les suppressions
- 3 actions : **Approuver**, **Refuser**, **Approuver toujours** (pour ce type d'opération dans cette session)

> **Référence technique :** [ADR-015](../adr/ADR-015-permission-tool-governance.md)

---

## Limitations connues

**Pas d'isolation mémoire RAM :** Un outil peut consommer autant de RAM qu'il veut. Contrôler via le `wall_clock_timeout` et le monitoring système.

**Pas d'isolation disque (quota) :** Un outil peut remplir le disque dans son working dir. Surveiller `/var/lib/apollia` en production.

**macOS : sandbox désactivé :** Sur macOS, les outils s'exécutent sans isolation. Réservé au développement local.

**Noyaux anciens (< 4.18) :** Les user namespaces avec réseau isolé peuvent ne pas fonctionner. Mettre à jour le kernel ou désactiver le sandbox réseau.

**Windows : sandbox différée :** (implémentation Windows) est déférée. Sur Windows, `bash_executor` s'exécute sans isolation jusqu'à livraison de.

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) - Principe #2 Zéro dépendance externe
- [Sécurité Local-First](./Securite-Local-First) - souveraineté des données, token API
- [Sécurité Guardrails](./Securite-Guardrails) - StepBudget
- [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) - pourquoi namespaces plutôt que Docker
- [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) - mode dev macOS
- [ADR-016](../adr/ADR-016-secrets-keyring-api-auth.md) - authentification API TCP
- [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) - sandbox Windows 3 couches
