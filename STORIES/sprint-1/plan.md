# Sprint 1 — Plan

**Sprint Goal :** EventBus + AgentRegistry fonctionnels, validés par un test d'intégration qui prouve les transitions ProcessState via broadcast Tokio.
**Durée estimée :** 11h / budget 16-20h
**Dates :** semaines 3-4

---

## Stories du sprint (ordre d'implémentation)

| Priorité | ID | Story | Taille | Dépend de |
|---|---|---|---|---|
| 1 | STORY-006 | EventBus broadcast Tokio + RuntimeEvent catalogue | M | STORY-005 ✅ |
| 2 | STORY-007 | AgentRegistry acteur Tokio (Register/Unregister/UpdateState) | M | STORY-006 |
| 3 | STORY-008 | AgentRegistryHandle API publique async | S | STORY-007 |
| 4 | STORY-009 | Test d'intégration EventBus ↔ AgentRegistry | M | STORY-008 |

---

## Dépendances vérifiées

- STORY-006 dépend de STORY-005 (CI) ✅ — livré Sprint 0
- STORY-007 dépend de STORY-006 (EventBusSender disponible)
- STORY-008 est la couche publique de STORY-007 — même implémentation, split logique
- STORY-009 dépend des 3 stories précédentes pour le test cross-composant

Aucune dépendance bloquante externe au sprint.

---

## Risques identifiés

1. **EventBus broadcast lagged** : `tokio::sync::broadcast` signale un `RecvError::Lagged` si le consumer est trop lent. Le comportement attendu (log warning, pas de panic) doit être spécifié dans STORY-006.
2. **Transitions ProcessState invalides** : L'AgentRegistry doit rejeter les transitions non autorisées (ex. `Active → Initializing`). Le type `ProcessState::can_transition_to()` est déjà dans `apollia-core` — le vérifier avant STORY-007.
3. **Canal mpsc saturé** : Buffer de 256 messages par défaut. Pas un risque au Sprint 1 (tests unitaires), mais à documenter.

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint : test d'intégration EventBus ↔ AgentRegistry passe
- [ ] `cargo test --workspace` passe sans ignorer de test
- [ ] `cargo clippy --workspace -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : formatté
- [ ] `sprint-index.md` mis à jour (toutes stories Sprint 1 → ✅)
- [ ] `STORIES/sprint-1/bilan.md` rédigé
