# ADR-058 - Context Window Management

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 - Workspace Intelligence & Execution Performance

---

## Contexte

**Limite Anthropic :** 200 000 tokens de contexte par requête.

**Calcul de croissance :**
- Session 100 steps × 500 tokens/step (texte seul) = 50 000 tokens → 25% de la limite, gérable.
- Avec tools : average 10 000 tokens/step (définitions outils + outputs) × 100 steps = 1 000 000 tokens → **dépassement ×5**.

En pratique, les sessions d'analyse de repo (sprint 35 sprint goal) impliquent de nombreux appels `file_read`, `file_grep` avec des outputs verbeux. Sans gestion active du contexte, ces sessions échouent avec `context_length_exceeded` après 50–100 steps.

---

## Décision

**`ContextManager::maybe_compact()`** est appelé avant chaque appel Reasoner dans ORIA.

**Seuil :** 80% de la fenêtre de contexte du modèle actif (configurable dans `apollia.toml`). Valeur conservatrice intentionnelle - 20% de marge pour la réponse du modèle.

**Stratégie de compactage :**
1. Estimer le nombre de tokens de l'historique courant via `estimate_tokens()` (approximation : `chars / 4 × 1.2`)
2. Si `estimated_tokens > context_limit × 0.80` → déclencher le compactage
3. Appeler `route_light()` pour résumer l'historique en un seul message
4. Remplacer `messages` par `[system_msg_original, summary_msg]`
5. Émettre `RuntimeEvent::ContextCompacted { original_tokens, compacted_tokens, session_id }`

**Structure des messages après compactage :**
```
messages = [
    { role: System, content: <system prompt original> },
    { role: User,   content: "[Résumé de la session jusqu'ici] ..." }
]
```

Le system prompt original est **toujours préservé en messages[0]** - il contient les instructions fondamentales de l'agent et le contexte workspace injecté.

### Rejet de "garder les N derniers messages"

L'alternative évidente est de tronquer l'historique en gardant les N derniers messages. Cette approche est rejetée car :

- Elle perd le contexte de la tâche initiale - le message `User` d'origine contient souvent des contraintes critiques pour la finalisation de la tâche
- Elle crée un historique incohérent (références à des messages supprimés)
- Elle est difficile à debugger (l'agent ne sait pas ce qu'il a "oublié")

Un résumé LLM préserve la continuité sémantique de la session.

### Comportement en cas d'échec du résumé

Si `route_light()` échoue (timeout, erreur LLM) :
- Un message de substitution `"[Résumé indisponible - contexte tronqué]"` est utilisé
- La session **continue** - pas d'échec fatal
- Un `tracing::warn!` est émis avec la raison de l'échec

---

## Conséquences

**Positives :**
- Sessions longues (100+ steps, analyse de repo) ne crashent plus sur `context_length_exceeded`
- `RuntimeEvent::ContextCompacted` permet l'observabilité - le dashboard peut afficher les compactages
- Le seuil 80% est configurable : les déploiements avec des modèles à petite fenêtre peuvent l'abaisser

**Négatives / Compromis :**
- `estimate_tokens()` est une approximation (chars/4 × 1.2) - peut déclencher des compactages prématurés sur du contenu très condensé (code, JSON)
- Le résumé LLM est un appel supplémentaire - latence additionnelle lors du compactage
- Le résumé peut perdre des détails subtils - acceptable en production (le contexte essentiel est préservé)

**Neutres / À surveiller :**
- TTL du résumé : si le `git HEAD` change entre deux steps (l'agent fait un commit), le workspace context injecté dans le résumé peut être stale. Ce cas est documenté comme limitation v1 - une invalidation par changement de HEAD est différée.

---

## Principes architecturaux impactés

- **Principe #4 - Fail fast** : Le compactage est déclenché avant le dépassement, pas après l'erreur API. Conforme.
- **Principe #5 - Un acteur, une responsabilité** : `ContextManager` est responsable uniquement du compactage - pas du routing LLM, pas de l'historique des messages. Conforme.
- **Principe #8 - CLI humaine, API machine** : `RuntimeEvent::ContextCompacted` est visible dans le dashboard et les logs CLI. Conforme.

---

## Liens

- Story d'implémentation : STORY-461
- Implémenté dans : `crates/apollia-runtime/src/context_manager.rs`
- Wiki : [Briques ORIA Engine - Gestion de la fenêtre de contexte](../wiki/Briques-ORIA-Engine.md#gestion-de-la-fenetre-de-contexte)
- ADR connexe : [ADR-057](ADR-057-prompt-caching-strategy.md) - Prompt caching (réduction de la croissance)
