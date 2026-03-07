# ADR-012 — Mode DevSandbox sur macOS : pas de sandbox réel en développement

**Date :** 2026-03-06
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 2 (STORY-013)

---

## Contexte

ADR-005 acte que le sandbox MVP utilise Linux namespaces via `unshare(1)`. Cette décision est
correcte pour la production, mais soulève une question pratique : **que se passe-t-il sur macOS**,
qui est l'environnement de développement actuel (Darwin 25.0.0) ?

`unshare` n'existe pas sur macOS. Deux questions se posent :

1. **Existe-t-il un équivalent macOS** qui permettrait une implémentation réelle sans surcoût ?
2. **Si non, quelle est la bonne approche** pour le développement local sans créer de dette technique ?

### Analyse des alternatives macOS

#### `sandbox-exec` / Seatbelt (SBPL)

macOS expose `sandbox-exec`, un outil en ligne de commande basé sur le Sandbox Profile Language
(SBPL) d'Apple, mécanisme sous-jacent à l'App Sandbox.

```sh
sandbox-exec -p "(version 1)(deny default)(allow file-read*)" /bin/sh -c "echo hello"
```

**Problème critique :** Apple a explicitement marqué `sandbox-exec` comme **deprecated depuis
macOS 10.15 (Catalina, 2019)**. Les API sous-jacentes ne sont pas documentées publiquement.
L'outil fonctionne encore sur macOS Sequoia (15.x) mais peut être retiré à tout moment sans
préavis. Construire dessus serait construire sur du sable.

De plus, le modèle SBPL est fondamentalement différent des Linux namespaces : il s'applique
au processus courant (pas en spawn isolé), la syntaxe est propriétaire, et les profils ne sont
pas transférables vers Linux.

#### Docker Desktop

Docker isole effectivement via des VM légères sur macOS (HyperKit / Virtualization.framework).

**Problème :** Viole Principe #2 (dépendance externe). Docker Desktop est commercial pour les
organisations > 250 personnes depuis 2022. Le daemon doit être lancé séparément. Incompatible
avec l'objectif de binaire unique zero-dep.

#### `qemu` / Lima / OrbStack

Des alternatives à Docker Desktop (Lima, OrbStack) permettent de lancer des VMs Linux légères
sur macOS.

**Problème :** Encore plus lourdes que Docker. Nécessitent un setup manuel. Non acceptables.

#### Compilation conditionnelle `#[cfg(target_os = "macos")]` avec fallback

Utiliser `cfg` pour implémenter deux chemins de code : namespace Linux sur Linux, et une
implémentation macOS différente.

**Problème :** Il n'existe pas d'alternative macOS viable (cf. ci-dessus). Ce chemin conduit
inévitablement à "pas de sandbox sur macOS", mais avec plus de complexité de code.

---

## Décision

Nous adoptons une architecture en deux couches pour `BashExecutor` :

```
SandboxMode::LinuxNamespaces   — production (Linux uniquement)
SandboxMode::Dev               — développement local (macOS et autres non-Linux)
```

**Sur Linux :** `unshare --pid --mount --fork /bin/sh -c "<cmd>"` (conforme ADR-005).

**Sur macOS (dev) :** Exécution directe via `tokio::process::Command` sans isolation, avec :
- `tracing::warn!` émis à **chaque invocation** en mode Dev (pas seulement au démarrage)
- Le mode actif est détecté automatiquement via `#[cfg(target_os = "linux")]` à la compilation
- `BashExecutor::sandbox_mode()` expose le mode actif pour les tests et la CLI

```rust
// Détection à la compilation — pas à l'exécution
#[cfg(target_os = "linux")]
fn build_command(input: &BashInput) -> tokio::process::Command {
    // unshare --pid --mount --fork /bin/sh -c "<cmd>"
}

#[cfg(not(target_os = "linux"))]
fn build_command(input: &BashInput) -> tokio::process::Command {
    tracing::warn!(
        command = %input.command,
        "bash_executor: running in Dev mode — no sandbox active. \
         Linux namespaces are not available on this platform. \
         Production deployments require Linux."
    );
    // tokio::process::Command directement
}
```

**La CI tourne sur Linux** (GitHub Actions `ubuntu-latest`) et valide le chemin sandbox réel.

---

## Alternatives considérées

### Option A — `sandbox-exec` sur macOS (rejetée)

**Pour :** Fournirait une isolation partielle réelle sur macOS.

**Contre :**
- API deprecated depuis macOS 10.15, retrait possible sans préavis
- Syntaxe SBPL propriétaire → deux modèles mentaux distincts à maintenir
- N'est pas testable de manière déterministe (comportement variable selon la version macOS)
- Crée une fausse impression de sécurité sur un chemin non supporté officiellement

**Verdict : Rejetée. Construire sur une API deprecated est de la dette technique garantie.**

### Option B — Docker obligatoire en dev (rejetée)

**Pour :** Isolation identique prod/dev, parity parfaite.

**Contre :**
- Viole Principe #2
- Ajoute friction au setup du dev (docker daemon, image)
- Docker Desktop commercial pour les orgs > 250 personnes

**Verdict : Rejetée. Incompatible avec les principes architecturaux.**

### Option C — Avertissement uniquement au démarrage (rejetée)

**Pour :** Moins verbeux à l'usage.

**Contre :** Un développeur qui ne lit pas les logs du démarrage peut oublier qu'il est en mode
sans sandbox. Un warning à chaque invocation est intentionnellement visible.

**Verdict : Rejetée. La visibilité du warning est une feature de sécurité, pas un bug.**

### Option retenue — SandboxMode::Dev avec warning par invocation

**Pour :** Honnête, explicite, zero dette technique, zero dépendance supplémentaire.

**Compromis accepté :** Le développeur sur macOS n'a pas d'isolation réelle. Acceptable car :
- Les agents en développement sont du code de confiance (le propre code du dev)
- Les tests d'intégration qui valident l'isolation tournent en CI Linux
- Le warning rend l'absence de sandbox impossible à ignorer

---

## Conséquences

**Positives :**
- Zero code macOS-spécifique à maintenir sur le long terme
- Zero dépendance supplémentaire
- `cfg(target_os)` est résolu à la compilation — pas de runtime overhead
- La CI Linux valide le chemin de production sans biais macOS

**Négatives / Compromis :**
- Pas d'isolation réelle en développement local sur macOS
- Différence de comportement prod/dev (atténuée par les warnings explicites)

**Neutres / À surveiller :**
- Si un contributeur Linux développe en VM Linux, il aura le sandbox réel — c'est un avantage
- Si Apple retire `sandbox-exec` dans une future version, cette décision reste correcte

## Principes architecturaux impactés

- Principe #2 — Zéro dépendance externe : aucune dépendance ajoutée, ni `sandbox-exec`, ni Docker
- Principe #4 — Fail fast : le mode Dev est explicite et visible à chaque invocation
- Principe #7 — Garde-fous non-négociables : en production (Linux), le sandbox est toujours actif

## Liens

- ADR précédent : ADR-005 (Sandbox sans Docker — décision fondatrice)
- Story impactée : STORY-013 (bash_executor — implémente `SandboxMode`)
- Story impactée : STORY-014 (python_executor — même pattern DevMode)
- Plan sprint 2 : docs/internal/STORIES/sprint-2/plan.md (Risque #1)
