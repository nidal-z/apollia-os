# ADR-005 - Sandbox multi-plateforme : Linux namespaces, macOS DevMode, Windows 3-couches Chromium

**Date :** 2026-03 (Linux) / 2026-03-06 (macOS) / 2026-04-03 (Windows)
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation (Linux + macOS) → 34 Beta Hardening (Windows)

---

## Contexte

L'exécution d'outils natifs (bash, python) par des agents IA requiert une isolation pour éviter qu'un agent compromis ne puisse accéder au système hôte. Docker est la solution standard, mais c'est une dépendance lourde (daemon, socket `/var/run/docker.sock`) incompatible avec le Principe #2.

Le runtime doit fonctionner sur tout Linux/macOS/Windows sans que l'utilisateur n'installe quoi que ce soit en plus du binaire `apollia-os`. L'implémentation doit être différente par plateforme car les APIs d'isolation natives diffèrent - mais le comportement observable (exécution isolée avec warning si pas de sandbox réel) doit être cohérent.

---

## Décisions

### Linux - namespaces natifs via `unshare`

Isolation via PID namespace + mount namespace : `unshare --pid --mount --fork /bin/sh -c "<cmd>"`. L'isolation est configurable via `SandboxProfile` : `ReadOnly`, `FileSystem`, `NetworkRestricted`, `Full`. Roadmap : nsjail (v0.2) puis gVisor optionnel (v1.0).

`unshare` est dans util-linux, présent sur tout Linux moderne. Requiert que les user namespaces soient activés (standard depuis Linux 3.8+).

### macOS - SandboxMode::Dev avec warning par invocation

`unshare` n'existe pas sur macOS. `sandbox-exec` (SBPL Apple) est deprecated depuis macOS 10.15 Catalina - API propriétaire non documentée, retrait possible sans préavis, construit sur du sable. Docker Desktop viole le Principe #2.

**Décision :** architecture en deux couches détectée à la **compilation** :

```rust
#[cfg(target_os = "linux")]
fn build_command(input: &BashInput) -> tokio::process::Command {
    // unshare --pid --mount --fork /bin/sh -c "<cmd>"
}

#[cfg(not(target_os = "linux"))]
fn build_command(input: &BashInput) -> tokio::process::Command {
    tracing::warn!(
        command = %input.command,
        "bash_executor: running in Dev mode - no sandbox active. \
         Linux namespaces are not available on this platform. \
         Production deployments require Linux."
    );
    // tokio::process::Command directement
}
```

- `tracing::warn!` est émis à **chaque invocation** (pas seulement au démarrage) - visibilité intentionnelle
- La CI tourne sur Linux (`ubuntu-latest`) et valide le chemin sandbox réel
- `#[cfg(target_os)]` résolu à la compilation - zéro runtime overhead

### Windows - modèle Chromium 3 couches

Implémenté dans `crates/apollia-tools/src/sandbox_windows.rs` avec `#[cfg(target_os = "windows")]`. WSL et Docker Desktop sont rejetés pour violation du Principe #2.

**Couche 1 - Job Object :**

```
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE        → terminaison auto quand le handle est fermé
JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION → pas de boîte de dialogue d'erreur Windows
JOB_OBJECT_UILIMIT_HANDLES                → pas d'accès aux handles de l'UI parente
JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS       → pas de modification des paramètres système
```

Via `win32job::Job` (crate `win32job = "2"`). Garantit que tous les processus enfants sont tués à la fermeture du runtime.

**Couche 2 - Restricted Token :**

Via `CreateRestrictedToken` (crate `windows`, feature `Security`) :
- Suppression des groupes sensibles (`Administrators`, `BUILTIN\Users` élevé)
- Ajout du SID de restriction `SECURITY_NULL_SID`
- Retrait des privilèges dangereux (`SeDebugPrivilege`, `SeLoadDriverPrivilege`)

**Couche 3 - AppContainer (Windows 8+) :**

Via `CreateAppContainerProfile` :
- Profil nommé `apollia-sandbox-<agent_id>` - isolé par agent
- Capabilities minimales : `internetClientServer` désactivé par défaut
- Suppression du profil à la fin de l'exécution

Si la création du profil AppContainer échoue (Windows 7 ou erreur API), le runtime dégrade gracieusement vers Couche 1 + Couche 2 uniquement avec un warning. La crate `windows = "0.58"` est activée uniquement avec `cfg(target_os = "windows")` pour ne pas impacter la compilation Linux/macOS.

---

## Alternatives considérées

### Docker obligatoire (rejetée, toutes plateformes)

Isolation éprouvée, mais viole le Principe #2. Docker daemon requis. Non viable sur serveurs sans Docker ou environnements restrictifs. Docker Desktop commercial pour les orgs > 250 personnes.

### `sandbox-exec` / Seatbelt SBPL sur macOS (rejetée)

Deprecated depuis macOS 10.15. API propriétaire non documentée. Retrait possible sans préavis. Syntaxe SBPL non transférable vers Linux. Construit sur du sable - dette technique garantie.

### Warning uniquement au démarrage sur macOS (rejetée)

Un développeur qui ne lit pas les logs du démarrage peut oublier qu'il est en mode sans sandbox. Un warning à chaque invocation est intentionnellement visible - c'est une feature de sécurité, pas un bug.

### WSL ou Docker Desktop sur Windows (rejetée)

WSL n'est pas installé par défaut sur toutes les machines Windows (notamment Windows Server). Docker Desktop non portable sur Windows Server Core. Violent tous deux le Principe #2.

### Couche 1 uniquement (Job Object) sur Windows (rejetée)

Insuffisant - ne restreint pas les droits filesystem ni les privilèges réseau. Chromium utilise les 3 couches car chacune couvre des vecteurs d'attaque différents.

### Firecracker microVM / WebAssembly (rejetées)

Complexité opérationnelle excessive. Latence de démarrage incompatible avec les appels d'outils fréquents. Écosystème Python WASM immature.

---

## Conséquences

**Positives :**
- Zéro dépendance : `unshare` dans util-linux sur Linux, APIs Win32 natives sur Windows, aucun outillage tiers requis
- Sandbox Windows native sans dépendance externe - les 3 couches couvrent des vecteurs complémentaires (ressources, privilèges, réseau/filesystem)
- Dégradation gracieuse sur Windows 7 / configurations sans AppContainer
- Pattern Chromium audité et éprouvé en production depuis 15 ans

**Négatives / Compromis :**
- Pas d'isolation réelle en développement local sur macOS (atténuée par les warnings explicites)
- Différence de comportement prod (Linux) / dev (macOS) - acceptable car les tests d'intégration CI valident le chemin production
- AppContainer crée un profil persistant sur disque - nettoyage requis en cas de crash du runtime

**Neutres / À surveiller :**
- Compatibilité `user namespaces` Linux : `/proc/sys/kernel/unprivileged_userns_clone` parfois désactivé sur kernels durcis
- Tests Windows uniquement sur `windows-latest` CI - pas de runners Windows ARM disponibles
- Si un contributeur Linux développe en VM Linux, il aura le sandbox réel - avantage

---

## Principes architecturaux impactés

- **Principe #2 - Zéro dépendance externe** : `unshare` natif Linux, APIs Win32 natives, `#[cfg]` pour ne pas tirer les dépendances Windows sur Linux/macOS
- **Principe #4 - Fail fast** : mode Dev macOS explicite et visible à chaque invocation ; AppContainer failure → warning, pas d'échec silencieux
- **Principe #7 - Garde-fous non-négociables** : en production (Linux), le sandbox est toujours actif - appliqué par le runtime, non configurable par l'agent

---

## Liens

- Stories : STORY-013 (bash_executor Linux), STORY-014 (python_executor), STORY-451 (Sprint 34 Windows)
- Implémenté dans : `crates/apollia-tools/src/sandbox_linux.rs`, `crates/apollia-tools/src/sandbox_windows.rs`
- Référence Chromium sandbox : https://chromium.googlesource.com/chromium/src/+/HEAD/docs/design/sandbox.md
