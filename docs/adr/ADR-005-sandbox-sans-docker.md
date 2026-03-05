# ADR-005 — Sandbox sans Docker (Linux namespaces natifs)

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

L'exécution d'outils natifs (bash, python) par des agents IA requiert une isolation pour éviter qu'un agent compromis ne puisse accéder au système hôte. Docker est la solution standard, mais c'est une dépendance lourde (daemon, socket `/var/run/docker.sock`, droits root ou groupe docker) incompatible avec Principe #2.

Le runtime doit fonctionner sur "tout Linux" sans que l'utilisateur n'installe quoi que ce soit en plus du binaire `apollia-os`.

## Décision

Nous utilisons les Linux namespaces natifs (PID namespace + mount namespace) via `unshare(1)` pour le MVP. L'isolation sandbox est configurable via `SandboxProfile` : `ReadOnly`, `FileSystem`, `NetworkRestricted`, `Full`. La roadmap prévoit nsjail (v0.2) puis gVisor optionnel (v1.0).

**MVP (Sprint 2) :** `subprocess` + `unshare --pid --mount` — isolation basique, zéro dépendance.

## Alternatives considérées

### Option A — Docker obligatoire (rejetée)
**Pour :** Isolation éprouvée, large adoption, images riches.
**Contre :** Viole directement Principe #2. Nécessite Docker daemon actif. Non viable sur serveurs sans Docker ou environnements restrictifs.

### Option B — Firecracker microVM (rejetée)
**Pour :** Isolation maximale, quasi-kernel séparé.
**Contre :** Complexité opérationnelle excessive pour un MVP. Nécessite KVM. Startup latency incompatible avec les appels d'outils fréquents.

### Option C — WebAssembly (rejetée)
**Pour :** Isolation portable, zero syscall non autorisé.
**Contre :** Écosystème Python WASM immature. Wasi-python est expérimental. Empêcherait l'exécution d'agents Python réels.

### Option D — Aucune isolation (rejetée)
**Pour :** Simplicité maximale.
**Contre :** Inacceptable pour un runtime de production. Un agent compromis accède au système complet de l'opérateur.

### Option retenue — Linux namespaces + unshare
**Pour :** Zéro dépendance externe. Disponible sur tout Linux moderne. Suffisant pour le niveau de sécurité MVP PME.
**Compromis acceptés :** Requiert que `user namespaces` soient activés sur l'OS hôte (standard sur Linux 3.8+, parfois désactivé sur certains kernels durcis).

## Conséquences

**Positives :**
- Zéro dépendance : `unshare` est dans util-linux, présent sur tout Linux.
- Isolation PID + mount empêche l'accès au filesystem hôte hors tmpfs.
- Chemin clair vers nsjail et gVisor pour les versions futures.

**Négatives / Compromis :**
- `user namespaces` peuvent être désactivés (`/proc/sys/kernel/unprivileged_userns_clone`).
- Isolation réseau incomplète en MVP (NetworkRestricted ajouté en v0.2).
- Moins robuste que Docker ou gVisor pour des agents vraiment malveillants.

**Neutres / À surveiller :**
- Compatibilité avec les distributions Linux cibles (Ubuntu, Debian, RHEL).
- Audit de sécurité du SandboxProfile `Full` avant production.

## Principes architecturaux impactés

- Principe #2 — Zéro dépendance externe : unshare est disponible partout.
- Principe #7 — Garde-fous non-négociables : Le sandbox est appliqué par le runtime, non configurable par l'agent.

## Liens

- Story associée : STORY-013 (bash_executor avec Linux namespaces)
- ADR précédent sur le même sujet : aucun
