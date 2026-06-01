# ADR-063 - Binary Feedback RLHF

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 36 (planifié)

---

## Contexte

La qualité des réponses d'un agent dépend de la température et du style du prompt. Sans mécanisme de feedback, Apollia OS ne peut pas apprendre des préférences de l'utilisateur pour améliorer ses réponses futures.

**Approche RLHF (Reinforcement Learning from Human Feedback) simplifiée :** présenter deux alternatives à l'utilisateur et enregistrer son choix pour calibrer les paramètres futurs.

**Référence :** Kaplan et al. 2020 (*Scaling Laws for Neural Language Models*) montre que la diversité d'échantillonnage (température) est le levier le plus efficace pour générer des alternatives qualitativement distinctes à moindre coût.

---

## Décision

**Génération de deux plans en parallèle** via `tokio::join!` :

```rust
let (plan_a, plan_b) = tokio::join!(
    reasoner.plan_with_temperature(context, temp_a),
    reasoner.plan_with_temperature(context, temp_b),
);
```

Les deux températures sont configurables dans `apollia.toml` :

```toml
[rlhf]
enabled = false            # Désactivé par défaut
temperature_a = 0.3        # Plan conservateur
temperature_b = 0.8        # Plan créatif
```

**Interface utilisateur :** l'agent présente les deux plans avec un numéro (A/B) et attend un choix. Le choix est enregistré dans SQLite avec le contexte de la tâche pour analyse future.

```sql
CREATE TABLE rlhf_feedback (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    task_id      TEXT NOT NULL,
    chosen       TEXT NOT NULL CHECK (chosen IN ('a', 'b')),
    temperature_a REAL NOT NULL,
    temperature_b REAL NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

### Rejet de plus de 2 alternatives

L'option de présenter 3 ou 4 alternatives est rejetée car :
1. **Cognitive load** : au-delà de 2 options, le temps de décision de l'utilisateur augmente exponentiellement (Loi de Hick)
2. **Coût LLM** : N alternatives = N appels parallèles - 3+ alternatives triplent le coût du step de planification
3. **Signal RLHF dégradé** : le choix parmi N > 2 options produit un signal moins exploitable qu'un choix binaire

Un feedback binaire A/B produit le signal le plus net pour calibrer la température.

---

## Conséquences

**Positives :**
- `tokio::join!` génère les deux plans sans latence additionnelle (parallèles)
- Le log SQLite permet d'analyser les préférences agrégées sans service externe
- Désactivé par défaut - zéro impact sur les utilisateurs non intéressés

**Négatives / Compromis :**
- Double appel LLM → coût doublé sur les sessions avec RLHF activé
- Le feedback n'est pas exploité automatiquement en V1 - c'est un log pour analyse manuelle. L'application automatique (ajustement dynamique de température) est différée.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Le log SQLite est local. Pas d'envoi à Anthropic ni à un service tiers. Conforme.
- **Principe #4 - Fail fast** : Si l'un des deux appels LLM échoue, `tokio::join!` retourne l'erreur immédiatement - pas de fallback silencieux sur un seul plan. Conforme.

---

## Liens

- Story d'implémentation : STORY-471 (Sprint 36)
- Implémenté dans : `crates/apollia-oria/src/feedback.rs`
- Référence : Kaplan et al. 2020 - https://arxiv.org/abs/2001.08361
