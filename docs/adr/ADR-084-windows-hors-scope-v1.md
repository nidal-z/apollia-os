# ADR-084 — Windows hors scope v0.1.0 et v1.0

**Date :** 2026-04-29
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation release publique

---

## Contexte

Apollia OS s'appuie sur plusieurs primitives système non portables sur Windows :

- L'API locale d'`apollia-runtime` est exposée sur un **Unix socket** via axum (en plus du TCP 7771). Windows ne supporte pas les Unix sockets de la même façon — les named pipes existent mais l'API et le code de gestion seraient à réécrire.
- Le crate `apollia-notifications` utilise **`notify-rust`**, qui cible D-Bus (Linux) et NSUserNotificationCenter (macOS). Le backend Windows existe mais n'est ni testé ni packagé dans le pipeline actuel.
- Le backend LLM local s'appuie sur **llama.cpp avec Metal** sur macOS et CUDA sur Linux. Aucun backend GPU Windows n'est intégré.
- Les sandbox de tools (`bash_executor`, `python_executor`) supposent un environnement POSIX (`/tmp`, permissions Unix, signaux POSIX pour le timeout).

Le développement quotidien est macOS-first ; Linux est supporté en best-effort via la CI. La cible release publique v0.1.0 (annonce 19 mai 2026) est macOS prioritairement, Linux best-effort. Aucun budget temps n'est alloué à Windows dans le WEEK-PLAN, ni n'est prévu pour le cycle v1.0.

La question doit être tranchée maintenant pour aligner le marketing (site vitrine M8, annonce M9), la documentation (Help M11, README), et les attentes communautaires post-launch.

## Décision

**Windows n'est pas supporté en v0.1.0 ni en v1.0.** Le binaire ne build pas sur Windows (gates `cfg(target_os)` sur les composants concernés). La documentation publique, le site vitrine et l'annonce ne mentionnent ni « Windows », ni « cross-platform », ni « bientôt sur Windows ». La compatibilité Windows pourra être réévaluée au cycle v1.x si une demande communautaire significative émerge — la décision sera tracée dans un ADR ultérieur.

## Alternatives considérées

### Option A — Support Windows natif via named pipes + backend notifications Windows (rejetée)
**Pour :** élargit la cible utilisateur potentielle, signal positif pour la perception cross-platform.
**Contre :** coût d'ingénierie significatif (IPC, notifications, sandbox tools, builds CI Windows, signature Authenticode, tests de portabilité), peu de demande dans la cible builders v0.1.0 qui sont massivement sur macOS/Linux. Détournerait du temps des chantiers M1–M11 critiques pour la release.

### Option B — WSL2 uniquement (rejetée)
**Pour :** réutilise le binaire Linux sans porter le code.
**Contre :** dégrade fortement l'UX desktop (Tauri ne tourne pas correctement sous WSL2 avec WSLg pour l'usage prolongé, intégration shell/notifications dégradée, complexité d'installation pour un public non technique). Donne une fausse promesse de support Windows tout en livrant une expérience inférieure.

### Option retenue — Pas de support Windows
**Pour :** focus total sur la qualité macOS/Linux pour la release, communication honnête avec la communauté, dette technique zéro à porter par la suite.
**Compromis acceptés :** une partie du marché potentiel (builders sur Windows) n'est pas adressée à la release publique.

## Conséquences

**Positives :**
- Le temps libéré est ré-investi dans les chantiers M1–M11 (distribution macOS .dmg, updater, onboarding, Apollia Guide, démos, site vitrine).
- Communication communautaire claire — pas de promesse implicite qu'on ne pourra pas tenir.
- Aucune dette technique cross-platform à porter (pas de `#[cfg(target_os = "windows")]` orphelins, pas de tests CI Windows à maintenir).

**Négatives / Compromis :**
- Une fraction des builders potentiels (Windows) n'aura pas accès au produit à la release.
- Si la traction communautaire post-launch est forte côté Windows, le coût d'ajouter le support a posteriori reste équivalent — la décision n'est pas définitive, elle est calibrée pour v0.1.0 et v1.0.
- Les tickets GitHub demandant Windows seront fermés avec une réponse type pointant vers cet ADR.

**Neutres / À surveiller :**
- Surveiller le volume et la qualité des demandes Windows post-launch (issues GitHub, Discord) pour décider si v1.x doit ouvrir le chantier.
- Vérifier que la formulation marketing (site, README, annonce) ne sous-entend jamais un support Windows imminent — relecture du corpus public au M7.

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : préservé sur les plateformes cibles (macOS/Linux). Windows aurait introduit une dépendance à des stacks Microsoft (named pipes, NSI/MSI installer, signature Authenticode) sans valeur pour la cible v0.1.0.
- **Principe #4 — Fail fast** : renforcé. Le build échoue immédiatement sur Windows — pas de surprise au runtime.

## Liens

- WEEK-PLAN : M7 (relecture corpus), M8 (site vitrine), M9 (Cloudflare Pages + annonce), M11 (Help screenshots)
- ADR-073 — macOS Code Signing (signature et distribution macOS)
- Réévaluation possible : ADR ultérieur en cycle v1.x si la demande communautaire le justifie
