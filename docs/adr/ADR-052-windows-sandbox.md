# ADR-052 — Sandbox Windows : modèle Chromium 3 couches

**Date :** 2026-04-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 34 — Beta Hardening

---

## Contexte

La sandbox bash actuelle (`bash_executor`) utilise `bubblewrap` (namespaces Linux) pour isoler
l'exécution des commandes shell. Cette approche est Linux-only et ne compile pas sur Windows.

Le Principe #2 (Zéro dépendance externe) exclut WSL ou Docker comme couche de sandbox. Le runtime
doit fonctionner sur Windows natif sans installation préalable — le binaire seul suffit.

STORY-451 introduit le support Windows. Il faut définir la stratégie de sandbox pour cette plateforme.

Le modèle de référence est Chromium, qui implémente une sandbox Windows robuste depuis 2008 avec
trois couches d'isolation, toutes disponibles via les APIs Win32 sans dépendance externe :

1. **Job Object** — restreint les ressources et les droits du groupe de processus
2. **Restricted Token** — réduit les privilèges du token de sécurité du processus
3. **AppContainer** — isole le processus dans un profil réseau/filesystem dédié (Windows 8+)

---

## Décision

### Architecture 3 couches

La sandbox Windows d'Apollia OS implémente les 3 couches de Chromium par ordre croissant de
rigueur, activées ensemble pour l'exécution de commandes via `bash_executor` sur Windows.

#### Couche 1 — Job Object

Chaque processus sandboxé est attaché à un `JOBOBJECT` avec les restrictions suivantes :

```
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE  → terminaison auto quand le handle est fermé
JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION → pas de boîte de dialogue d'erreur Windows
JOB_OBJECT_UILIMIT_HANDLES          → pas d'accès aux handles de l'UI parente
JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS → pas de modification des paramètres système
```

Le Job Object garantit que tous les processus enfants sont tués à la fermeture du runtime,
même en cas de crash. Implémenté via `win32job::Job` (crate `win32job = "2"`).

#### Couche 2 — Restricted Token

Le token de sécurité du processus sandboxé est créé via `CreateRestrictedToken` avec :

- Suppression des groupes sensibles (`Administrators`, `BUILTIN\Users` élevé)
- Ajout du SID de restriction `SECURITY_NULL_SID` pour réduire l'accès aux objets système
- Retrait des privilèges dangereux (`SeDebugPrivilege`, `SeLoadDriverPrivilege`, etc.)

Via la crate `windows` (feature `Security`).

#### Couche 3 — AppContainer (Windows 8+)

Création d'un profil AppContainer dédié via `CreateAppContainerProfile` avec :

- Profil nommé `apollia-sandbox-<agent_id>` — isolé par agent
- Capabilities minimales : `internetClientServer` désactivé par défaut
- Suppression du profil à la fin de l'exécution (`DeleteAppContainerProfile`)

Si la création du profil AppContainer échoue (Windows 7 ou erreur API), le runtime dégrade
gracieusement vers Couche 1 + Couche 2 uniquement, avec un warning dans les logs. Le Principe #4
(Fail fast) s'applique uniquement aux configurations explicitement non-supportées.

### Portabilité

L'implémentation est dans `crates/apollia-tools/src/sandbox_windows.rs` avec
`#[cfg(target_os = "windows")]`. Sur Linux/macOS, `bubblewrap`/`sandbox-exec` restent utilisés.
Aucune dépendance Windows n'est tirée sur les autres plateformes.

### Commandes autorisées

La liste d'autorisation des commandes (`bash_executor` whitelist) est identique sur Windows et Linux.
Seul l'environnement de sandbox change — pas le filtrage des commandes.

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| **WSL (Windows Subsystem for Linux)** | Dépendance externe — WSL n'est pas installé par défaut sur toutes les machines Windows (notamment Windows Server). Viole Principe #2. |
| **Docker Desktop** | Dépendance externe lourde, non portable sur Windows Server Core, coûteux en ressources. Viole Principe #2. |
| **Pas de sandbox Windows** | Inacceptable — l'exécution non-sandboxée de commandes arbitraires viole la promesse de sécurité du runtime. |
| **Hyper-V isolé** | Overhead de VM complet. Latence inacceptable pour des micro-tâches bash. Non disponible sur toutes les éditions Windows. |
| **Couche 1 uniquement (Job Object)** | Insuffisant — ne restreint pas les droits filesystem ni les privilèges réseau. Chromium utilise les 3 couches car chacune couvre des vecteurs différents. |

---

## Conséquences

**Positives :**
- Sandbox native Windows sans aucune dépendance externe — le binaire Apollia suffit.
- Les 3 couches couvrent des vecteurs d'attaque complémentaires (ressources, privilèges, réseau/filesystem).
- Dégradation gracieuse sur Windows 7 / configurations sans AppContainer.
- Pattern établi et audité (Chromium l'utilise en production depuis 15 ans).

**Négatives / Compromis :**
- Complexité d'implémentation élevée — les APIs Win32 pour AppContainer sont peu documentées en Rust.
- Référence `appjaillauncher-rs` de Trail of Bits comme guide mais pas comme dépendance directe.
- AppContainer crée un profil persistant sur disque — nettoyage requis en cas de crash du runtime.
- Tests uniquement sur `windows-latest` en CI — pas de runners Windows ARM disponibles.

**Neutres / À surveiller :**
- La crate `windows = "0.58"` est une dépendance lourde (bindings Win32 générés) — à activer
  uniquement avec `cfg(target_os = "windows")` pour ne pas impacter la compilation Linux/macOS.
- Tester sur Windows Server 2022 (cible entreprise probable) en plus de Windows 11.

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : Sandbox locale, pas de service cloud d'isolation. Conforme.
- **Principe #2 — Zéro dépendance externe** : APIs Win32 natives, pas de WSL/Docker. Renforcé.
- **Principe #4 — Fail fast** : Permissions incorrectes → erreur au démarrage. Dégradation
  AppContainer → warning explicite, pas d'échec silencieux. Conforme.

---

## Liens

- Story d'implémentation : STORY-451 (Sprint 34)
- Implémenté dans : `crates/apollia-tools/src/sandbox_windows.rs`
- Référence Chromium sandbox : https://chromium.googlesource.com/chromium/src/+/HEAD/docs/design/sandbox.md
- Référence Rust : `appjaillauncher-rs` (Trail of Bits)
- ADR sandbox Linux : [ADR-005](ADR-005-sandbox-sans-docker.md)
- ADR devmode macOS : [ADR-012](ADR-012-sandbox-devmode-macos.md)
