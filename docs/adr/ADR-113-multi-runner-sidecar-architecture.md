# ADR-113 : Architecture multi-runner sidecar pour l'inférence LLM/STT

**Date :** 2026-05-25
**Statut :** Proposé
**Sprint :** Pré-implémentation (refactor post-v0.1.0 launch decision)

---

## Contexte

### Situation actuelle

Le runtime `apollia-os` link statiquement `llama-cpp-2` (et `whisper-rs` pour le STT) dans le binaire principal. Le backend GPU est choisi à la compilation via les features Cargo :

- `local-cuda` (NVIDIA)
- `local-rocm` (AMD)
- `local-vulkan` (cross-vendor)
- `local-metal` (Apple Silicon)
- `local-cpu` (fallback)

Ces features sont **mutuellement exclusives** car `llama.cpp` ne supporte pas deux backends GPU dans la même build (conflits de symboles GGML, double registration des kernels, collisions libstdc++).

Conséquence : on produit aujourd'hui 13 binaires distincts dans `release.yml`, un par combinaison OS x arch x accélérateur. La page de download présente 3 à 5 variants par OS au user.

### Problème

Trois douleurs concrètes apparues pendant la phase release :

1. **UX download confuse pour les non-techs** : un user qui télécharge Apollia ne sait pas quel variant prendre. Auto-détection JS aide mais ne couvre pas 100 % des cas (Safari masque WebGL renderer, Firefox aussi en mode strict, GPUs intégrés multi-vendor).

2. **Maintenance multipliée** : tout incident sur un backend (CVE, bug llama.cpp) demande de rebuild et re-signer 5 binaires par OS. Idem pour les release notes.

3. **Impossibilité d'évoluer** : pas moyen d'ajouter un nouveau backend (Intel oneAPI, Apple Neural Engine, runner cloud distant) sans pousser un nouveau binaire et obliger les users à re-télécharger.

### Pourquoi maintenant

La release v0.1.0 a été décalée pour permettre un refactor architectural propre avant l'exposition publique. C'est la dernière fenêtre où on peut casser l'architecture sans risque de rétrocompatibilité (les users early n'ont pas encore d'install à migrer).

L'industrie a convergé sur le pattern multi-runner sidecar : Ollama, LM Studio, llama.cpp server avec workers. Le pattern est validé en production.

## Décision

**Nous adoptons une architecture multi-runner sidecar.** Le binaire principal `apollia-os` ne charge plus llama.cpp directement. Il détecte le GPU au boot et spawn un process enfant `apollia-runner-{backend}` qui contient le binding `llama-cpp-2` avec le bon backend compilé. La communication daemon ↔ runner se fait par HTTP/JSON sur loopback TCP.

### Architecture cible

```
+-----------------------------------------------------------+
| apollia-os (daemon main process)                          |
| ─────────────────────────────────                         |
| - REST API axum (TCP 7771 + Unix socket)                  |
| - Tray menu / Tauri GUI                                   |
| - Agent registry, A2A, memory, tools, MCP                 |
| - GPU detection at boot                                   |
| - Runner supervisor (spawn, health, restart)              |
| - RunnerProxy : forwards LLM/STT calls via IPC            |
+-----------------------------------------------------------+
                      |
                      | HTTP/JSON sur 127.0.0.1:<runner-port>
                      | (port choisi dynamiquement au spawn)
                      v
+-----------------------------------------------------------+
| apollia-runner-{cuda|rocm|vulkan|metal|cpu}               |
| ──────────────────────────────────────                    |
| - Petit serveur HTTP (axum embarqué)                      |
| - Charge llama-cpp-2 avec UN backend compilé              |
| - Charge whisper-rs avec backend identique                |
| - Endpoints : /llm/complete, /llm/stream, /stt/transcribe |
| - Stateless : modèles chargés à la demande, cache LRU     |
+-----------------------------------------------------------+
```

### Choix techniques actés

| Aspect | Choix | Justification |
|---|---|---|
| Transport IPC | HTTP/JSON sur 127.0.0.1 | Debuggable avec curl, standard industrie, latence négligeable en loopback (50-100 µs/appel) |
| Sérialisation | JSON (serde_json) | Types Rust déjà annotés serde, lisible dans les logs, pas de schéma .proto à maintenir |
| Crash recovery | Restart auto + fail task | Le runner est redémarré transparent par le supervisor, mais la task en cours échoue avec `RUNNER_CRASH`. User retry manuel. Standard industrie. |
| Lifecycle | Spawn unique au boot | Le daemon spawn 1 runner au boot après détection GPU. Le runner vit toute la session. Pas de cold start par requête. |
| Port runner | Auto-bind 127.0.0.1:0 | Le runner bind sur un port libre random, le daemon récupère le port via stdout du child. Pas de conflit port utilisateur. |

## Alternatives considérées

### Option B : Multi-binary launcher (rejetée)

**Pour :**
- 1 download per OS
- Refactor minimal (juste un launcher ~200 lignes)
- Pas de refactor de `apollia-llm`

**Contre :**
- Bundle ~400-500 MB (les 4 backends dans le même installer)
- Le user paye le download de backends qu'il n'utilise jamais
- Pas de crash isolation (si llama.cpp segfault, c'est tout le daemon qui tombe)
- Pas de capacité future d'hot-swap de runner ou de runners distants
- Code dead pour les 75 % de users qui n'ont qu'un GPU

### Option C : Multi-installer status quo (rejetée)

**Pour :**
- Déjà implémenté
- Bundle minimal par variant (~150-280 MB)
- Aucun refactor nécessaire

**Contre :**
- 3-5 SKUs par OS sur la page download = mauvaise UX grand public
- Pas d'auto-détection GPU runtime (le user doit choisir au download)
- Maintenance des release notes multipliée par 5
- Confusion sur le mauvais binaire = bug report "ça marche pas" = vrai problème support

### Option retenue : Multi-runner sidecar (Approche A)

**Pour :**
- 1 download per OS (UX VS Code / Cursor / Docker Desktop)
- Auto-détection GPU au runtime, transparent pour l'user
- Crash isolation : segfault llama.cpp ne tue pas le daemon ni les autres services (memory, A2A, tools)
- Architecture extensible : ajouter un backend = ajouter une feature de `apollia-runner`, pas modifier le daemon
- Future-proof : permet runner distant (cloud), runner shared between users sur LAN, runner versionné indépendamment du daemon
- Convention industrie (Ollama, LM Studio)

**Compromis acceptés :**
- Bundle plus gros : ~400 MB installer (vs 150-280 MB par variant en C)
- Overhead IPC : 50-100 µs par appel LLM (négligeable vs 100-1000 ms d'inférence)
- Complexité ops : 2 process à monitorer au lieu d'1
- Refactor : 6-8 semaines d'engineering

## Conséquences

### Positives

- **UX simplifiée** : 1 bouton de download par OS, le runtime fait le bon choix
- **Robustesse** : crash isolation des kernels GPU (notoirement buggy sur ROCm Linux et Vulkan AMD)
- **Évolutivité** : ajouter un backend Intel oneAPI ou Apple ANE devient trivial
- **Observabilité** : le runner expose ses propres metrics (GPU mem usage, kernel exec time) sans polluer le daemon
- **Multi-tenancy futur** : un runner pourrait servir plusieurs daemons (use case lab/équipe)

### Négatives / Compromis

- **Latence par appel LLM** : +50-100 µs IPC. Mesuré négligeable face aux 100ms+ d'inférence GGUF mais significatif si on fait beaucoup d'appels courts (embeddings batch). À surveiller.
- **Bundle taille** : +200 MB par installer (4 backends bundlés au lieu d'1). Mitigation : compression installeur (NSIS, AppImage), ou télécharger les runners à la demande au premier launch (rejeté pour v0.2.0, peut-être plus tard).
- **Complexité opérationnelle** : doctor command doit checker le runner, logs split entre daemon et runner, debugging plus complexe. Mitigation : aggregation des logs côté daemon.
- **Refactor coût** : 6-8 semaines, gel de certaines features pendant ce temps.

### À surveiller

- **Port conflicts** : si un user a déjà quelque chose qui bind sur 127.0.0.1:N, le runner doit choisir un autre port. Plan : auto-bind sur :0 et passer le port au daemon via stdout. Tester sur Windows où le firewall peut bloquer même loopback.
- **Latence cold-start runner** : combien de temps pour spawn + load model ? À mesurer. Si > 5s, c'est une régression visible.
- **Memory footprint** : le runner garde des modèles en RAM. Le daemon doit pouvoir lui demander de unload. API à concevoir.
- **Compatibilité Windows IPC** : 127.0.0.1 fonctionne mais subject à Windows Defender Firewall. Tester en CI réelle.

## Principes architecturaux impactés

- **Principe #1 (Local-first)** : préservé. Tout reste local, juste 2 process locaux au lieu d'1.
- **Principe #2 (Zéro dépendance externe)** : préservé. Le runner est embarqué dans l'installer.
- **Principe #4 (Fail fast)** : amélioré. Le daemon peut détecter un runner unhealthy en quelques secondes et notifier l'user.
- **Principe #5 (Un acteur, une responsabilité)** : renforcé. Le runner a une seule responsabilité : inférence. Le daemon orchestre.
- **Principe #7 (Garde-fous non-négociables)** : préservé. StepBudget reste appliqué côté daemon, le runner ne voit que les appels individuels.

Aucun principe rompu.

## Plan d'implémentation

Le refactor se découpe en 6 phases, chacune un sprint indépendant. Chaque phase laisse le binaire compilable et l'application fonctionnelle (pas de big-bang).

| Phase | Objectif | Sortie | Risque |
|---|---|---|---|
| 1. Foundation | Crate `apollia-runner` skeleton + trait IPC + transport HTTP loopback | Binaire `apollia-runner-cpu` standalone qui répond à `/health` | Faible |
| 2. Backend extraction | Migrer code llama.cpp + whisper.cpp de `apollia-llm` vers `apollia-runner` | 4 binaires `apollia-runner-{cuda,rocm,vulkan,cpu}` (+ Metal) qui exposent `/llm/*` et `/stt/*` | Moyen (refactor traversant) |
| 3. Daemon integration | GPU detection cross-platform, `RunnerSupervisor` qui spawn/health/restart, `RunnerProxy` qui remplace les appels llama directs | Daemon qui tourne avec un runner enfant, agents fonctionnent identiquement | Élevé (intégration end-to-end) |
| 4. CLI client refactor | Couche transport agnostique dans `apollia-cli/src/client.rs` (Unix socket / TCP / future named pipes) | CLI fonctionne sur Windows sans hack | Moyen |
| 5. Packaging | CI produit 4 runners + 1 daemon dans 1 seul installer par OS + signing | 1 .dmg / 1 .msi / 1 .deb / 1 .AppImage par OS | Moyen (Tauri + signing certs) |
| 6. Tests + Docs | Tests intégration 3 OS x backends GPU, user docs, monitoring | Coverage validée, release notes prêtes | Faible |

Détail des stories : voir `docs/internal/STORIES/sprint-multirunner/` (à créer par le skill `apollia-sprint`).

## Migration & rétrocompatibilité

### Côté agents Python (SDK)

**Aucun changement.** L'API `ctx.llm.complete()` / `ctx.llm.stream()` / `ctx.stt.transcribe()` reste identique. La couche `apollia-aip` qui expose ces interfaces Python via PyO3 continue de pointer sur `RunnerProxy` côté Rust au lieu du `LlmRouter` direct. Transparent pour les agents installés.

### Côté CLI users

**Aucun changement.** Les commandes `apollia-os run`, `apollia-os a2a invoke`, etc. restent identiques. La GUI Tauri non plus.

### Côté config user (.apollia)

**Changement minimal.** Le fichier `system.db` qui stocke les configs de backends LLM passe d'un schéma "1 backend = 1 row" à "1 backend = 1 row + champ runner_type". Migration auto au premier boot post-upgrade.

### Rollback strategy

Si le refactor multi-runner révèle un blocker, on peut rollback en gardant les 13 binaires monolithiques actuels (approche C). Le SDK Python et l'API HTTP du daemon n'auront pas bougé entre temps, donc aucun agent ne devra être réinstallé. Rollback cost : revert le merge multi-runner, re-tag, re-publish.

## Liens

- Sprint associé : à créer (`docs/internal/STORIES/sprint-multirunner/plan.md`)
- ADRs liés :
  - ADR-042 : llama-cpp-2 (le crate qu'on continue d'utiliser, juste déplacé)
  - ADR-101 : Ctx Protocol (préservé, pas d'impact visible côté agent)
- Spec compagnon : `docs/internal/architecture/IPC-PROTOCOL.md` (à rédiger en suivant cet ADR)
- Référence externe : [Ollama runners architecture](https://github.com/ollama/ollama/blob/main/docs/development.md)
