# ADR-039 - Vérification et critic sur le chemin orchestré ORIA

**Date :** 2026-07-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

`apollia-oria` définit depuis plusieurs sprints une boucle de vérification post-run : `VerificationLoop` (checks shell déterministes) et `CriticPass` (critic LLM optionnel, dégradable), dans `crates/apollia-oria/src/verification.rs`. Vérifié dans le code : ces types n'étaient câblés **que côté chat** (`apollia-runtime/src/chat/manager.rs` et `builtin_agent.rs`). Sur le chemin orchestré (`ORIAEngine::execute_orchestrated_plan`), un run se terminait donc **sans** vérification ni critique. La capacité 2.8 était à l'état "échafaudage" côté orchestré.

Le chemin chat sert de référence : la vérification y est gated par le tier d'autonomie (`AutonomyLevelConfig.run_verification`, faux pour `assisted`, vrai au-dessus), le critic LLM est **off-budget** (il route directement, ne touche jamais le `StepBudget`), et sur verdict fail le chat injecte une correction et **relance** sa boucle ReAct, borné et gardé par le budget.

Le chemin orchestré n'exécute pas une boucle ReAct à tampon de messages : il exécute un plan via l'`ActorLoop`. La question "que fait le critic d'un verdict fail en orchestré" (annoter, gater, ou replanifier) n'avait pas de réponse évidente calquée sur le chat. C'est une vraie décision d'architecture, remontée pour arbitrage.

Contrainte de valeur : la redevabilité d'Apollia repose sur audit + verify + rollback (cap 4.3). Un verdict de vérification doit être **traçable dans le journal signé**, pas seulement loggé (le chat, lui, abandonne son verdict sans le persister : trou à ne pas reproduire).

## Décision

Nous adoptons, sur le chemin orchestré, une vérification post-run **gated par le tier d'autonomie** qui, sur verdict fail, **replanifie et ré-exécute** de façon bornée et sous budget partagé :

- **Activation** : à la fin d'un run orchestré complété, si le tier résout `run_verification = true` (parité chat, via `AutonomyLevelConfig::default_for(tier)`), le moteur exécute `VerificationLoop` (alimenté par `manifest.check_commands`) plus `CriticPass` sur le résultat final.
- **Sémantique du verdict = replan-on-fail** : sur verdict fail, le moteur produit un feedback structuré, appelle `Reasoner::plan_with_feedback`, ré-exécute l'`ActorLoop`, et recommence jusqu'à `oria_config.verification_max_replans` (défaut 2, `0` désactive le replan). Le `StepBudget` est créé **une fois** et partagé sur toutes les itérations : il reste le plafond non-bypassable du run entier (principe #7). Le critic LLM est **off-budget** (parité chat) ; la ré-exécution de plan est on-budget (l'`ActorLoop` incrémente), et la boucle s'arrête sur budget épuisé.
- **Traçabilité** : chaque verdict est émis comme `RuntimeEvent::VerificationCompleted` sur l'EventBus, mappé par le subscriber `audit_journal` sous le `task_id` du run (comme les événements de plan-gate). Le verdict atterrit donc dans le journal signé.
- **Checks shell** : `VerificationLoop` est construit depuis `manifest.check_commands` mais avec un invoker no-op (parité chat, qui ne lance pas de commande). L'exécution shell réelle sous garde reste un chantier ultérieur.

## Alternatives considérées

### Annoter et tracer seulement (rejetée pour ce chantier)
- **Pour :** le plus simple et sûr ; aucun risque de boucle ; strict parité "observabilité" sans changer le flux d'exécution.
- **Contre :** le moteur constate un défaut sans agir dessus. La valeur "l'agent se corrige seul" (différenciateur des agents ReAct autonomes vs pipelines déterministes) n'est pas rendue en orchestré. C'était l'option de repli du brief, écartée au profit du replan.

### Plan-gate sur verdict fail (rejetée)
- **Pour :** met un humain dans la boucle avant de re-livrer un résultat douteux.
- **Contre :** la porte plan-gate existe déjà avant exécution ; en rajouter une après vérification alourdit le flux headless (A2A, triggers) et n'apporte de valeur qu'en mode supervisé interactif. Hors périmètre.

### Retenue - replan-on-fail borné sous budget partagé
- **Pour :** rend l'auto-correction en orchestré ; réutilise `plan_with_feedback` déjà éprouvé au reject du plan-gate ; le budget partagé garantit qu'aucun replan ne contourne le plafond.
- **Compromis acceptés :** une boucle de plus dans le moteur (complexité, tests) ; un run peut coûter plusieurs plans avant de converger (borné par `verification_max_replans` et le budget).

## Conséquences

**Positives :**
- La cap 2.8 passe d'échafaudage à câblée et prouvée en orchestré (verdict produit et émis sur un run réel).
- Le verdict est traçable dans le journal signé, renforçant la primitive de redevabilité (cap 4.3).
- L'agent orchestré se corrige seul de façon bornée, sans jamais dépasser le `StepBudget`.

**Négatives / Compromis :**
- Nouvelle variante publique `RuntimeEvent::VerificationCompleted` dans `apollia-core` (additive, consommateurs à catch-all non cassés).
- Nouveau champ `ORIAConfig.verification_max_replans` (additive, défaut 2).
- Le critic est off-budget : cohérent avec le chat, mais un critic coûteux n'est pas compté au budget du run (la ré-exécution, elle, l'est).

**Neutres / À surveiller :**
- Le taux de replan déclenché par la vérification : s'il est élevé, c'est que la planification initiale est faible.
- L'exécution shell réelle des `check_commands` (invoker no-op aujourd'hui) : à câbler sous garde dans un chantier suivant.

## Principes architecturaux impactés

- **Principe #7 - Safeguards non-bypassables** : `StepBudget` créé une fois et partagé sur tous les replans ; la boucle s'arrête sur budget épuisé ; le critic ne le contourne pas (il n'exécute rien de gouverné, il route un appel LLM comme le chat).
- **Principe #4 - Fail fast, dégradable** : sans backend critic, le pass est skippé (verdict `skipped`), le run n'échoue pas.
- **Moat audit / redevabilité** : le verdict entre dans le journal signé via l'EventBus.

## Liens

- ADR-038 (contrat d'arguments des steps orchestrés) : chantier #3, précédent immédiat et modèle de procédure STOP -> ADR.
- ADR-031 (modèle de plan unifié) : le replan réutilise `Reasoner::plan_with_feedback`.
- Cartographie : `docs/internal/cartography/capability-registry.md` (cap 2.8, cap 4.3).
- Origine : chantier #4 (vérification / critic sur le chemin orchestré).
