# ADR-083 — Trust model des agents Python : code utilisateur, pas de sandbox process-per-agent en v0.1.0

**Date :** 2026-04-29
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation release publique

---

## Contexte

Apollia OS exécute du code Python arbitraire fourni par l'opérateur via le bridge PyO3 — chaque agent est un module Python qui implémente `manifest()` + `run()` async (Principe #3 — Contrat minimal). Ce code tourne dans le processus du runtime Rust et a accès à tout ce que peut faire le processus : système de fichiers, réseau, exécution de sous-processus via les tools natifs, lecture des credentials utilisateur stockés dans le keyring.

La cible explicite de la release publique v0.1.0 (annonce 19 mai 2026) est constituée de **builders avancés** : ingénieurs et chercheurs qui écrivent eux-mêmes leurs agents ou installent des agents qu'ils ont audités. Le positionnement marketing assume cette cible — ce n'est pas un produit grand public. SEC-02 (sandbox process-per-agent avec isolation OS) figure sur la roadmap v1.0 mais ne sera pas livré pour la release publique.

La question doit être tranchée maintenant car elle conditionne :
- le contenu du bandeau d'onboarding (M3) ;
- la formulation marketing du site vitrine (M8) et de l'annonce (M9) ;
- la documentation Help (M11) ;
- ce que les démos peuvent ou non promettre aux opérateurs.

## Décision

**Nous adoptons un trust model pur pour les agents Python en v0.1.0** : le code Python d'un agent est traité comme du code utilisateur de confiance, exécuté avec les droits du processus runtime Apollia OS — c'est-à-dire les droits de l'utilisateur courant. Aucune isolation process-per-agent, aucun sandbox OS, aucun confinement Wasm.

Le bandeau d'onboarding affiche en clair : *« Apollia OS exécute du code Python avec vos droits utilisateur. N'installez que des agents que vous avez audités ou qui proviennent d'une source de confiance. »*

## Alternatives considérées

### Option A — Sandbox process-per-agent (rejetée pour v0.1.0)
**Pour :** isolation OS forte (namespaces Linux, sandbox-exec macOS), credentials non lisibles depuis l'agent, kill propre sur misbehavior, defense-in-depth si un agent malveillant passe la revue de l'opérateur.
**Contre :** coût d'implémentation très élevé (IPC entre runtime et processus agent, sérialisation des appels tool, gestion du cycle de vie des sous-processus, tests de portabilité Linux/macOS), latence accrue à chaque appel cross-process, complexité opérationnelle pour le débogage et l'observabilité, divergence majeure avec l'architecture acteurs Tokio actuelle. Reporté à v1.0 sous SEC-02.

### Option B — Wasm runtime pour Python (rejetée)
**Pour :** isolation forte par construction, capability-based security, portabilité.
**Contre :** l'écosystème Python sur Wasm (Pyodide, RustPython) est immature pour les besoins d'Apollia : pas de support PyO3 côté hôte, écosystème ML/LLM (`numpy`, `torch`, `httpx`) non viable sous Wasm, performance dégradée d'un ordre de grandeur. Incompatible avec le Principe #3 (duck typing Python sans contrainte sur les bibliothèques importables).

### Option retenue — Trust model pur
**Pour :** aligné avec la cible builders v0.1.0 qui auditent leur propre code, complexité minimale, performance maximale (zéro overhead IPC), cohérence avec l'architecture acteurs Tokio. Permet de livrer la release dans la fenêtre du 19 mai sans sacrifier la qualité du runtime.
**Compromis acceptés :** un agent malveillant installé volontairement peut exfiltrer des credentials, lire le système de fichiers utilisateur, ou exécuter du code arbitraire avec les droits utilisateur. La défense repose entièrement sur la chaîne d'installation (l'opérateur audite avant d'installer).

## Conséquences

**Positives :**
- Architecture runtime simple, performante, alignée avec les 8 principes — pas de couche IPC à construire.
- Release v0.1.0 livrable dans la fenêtre du 19 mai sans dette technique cachée.
- Les agents peuvent utiliser tout l'écosystème Python (LangGraph, CrewAI, ML libs custom) sans restriction d'imports.

**Négatives / Compromis :**
- Aucune barrière technique entre un agent et le système utilisateur — la sécurité est procédurale (audit avant install), pas technique.
- Le bandeau onboarding doit être explicite et non-skippable au premier lancement (livré dans M3).
- La documentation publique (Help, site vitrine, README) ne doit jamais sous-entendre une isolation forte — formulations à proscrire : « agents sandboxés », « exécution isolée », « sécurisé par défaut ».
- SEC-02 (sandbox process-per-agent) reste un engagement v1.0 vis-à-vis de la communauté.

**Neutres / À surveiller :**
- Si la cible utilisateur s'élargit après la release publique (au-delà des builders avancés), réévaluer la priorité de SEC-02.
- Surveiller les retours communautaires post-launch sur la perception du modèle de confiance — un incident public d'agent malveillant rendrait SEC-02 critique.

## Principes architecturaux impactés

- **Principe #1 — Local-first** : préservé. Le trust model ne change rien à la localité des données ; tout reste sur la machine de l'utilisateur.
- **Principe #3 — Contrat minimal** : renforcé. Aucune contrainte additionnelle sur le code Python des agents (imports, syscalls, FFI restent libres).
- **Principe #7 — Garde-fous non-négociables** : non impacté. Le `StepBudget`, le `PermissionEngine` et l'audit trail restent appliqués par le runtime indépendamment du trust model — ce sont des garde-fous fonctionnels, pas de sécurité OS.

## Liens

- Roadmap sécurité : SEC-02 — sandbox process-per-agent (v1.0)
- WEEK-PLAN : M3 (onboarding banner), M4 (Apollia Guide), M8 (site vitrine), M11 (screenshots Help)
- Story future : sandbox process-per-agent (à créer pour cycle v1.0)
