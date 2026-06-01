# ADR-057 - Prompt Caching Strategy

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 - Workspace Intelligence & Execution Performance

---

## Contexte

Sur une session de 50 steps avec un system prompt de 8 000 tokens : chaque appel LLM renvoie l'intégralité du system prompt et de l'historique. Le coût total ≈ 50 × coût d'un appel avec contexte plein.

L'API Anthropic propose depuis juillet 2024 un mécanisme de prompt caching via l'en-tête beta `anthropic-beta: prompt-caching-2024-07-31`. En marquant des blocs de messages avec `cache_control: { type: "ephemeral" }`, Anthropic met en cache les prefixes de contexte entre les appels successifs.

**Coûts comparatifs (claude-sonnet-4-6) :**
- `input_tokens` standard : $3.00 / MTok
- `cache_write_input_tokens` : $3.75 / MTok (légèrement plus cher à la première écriture)
- `cache_read_input_tokens` : $0.30 / MTok (10× moins cher que le standard)

**Impact estimé :** Sur une session répétant le même contexte (sessions longues, tâches récurrentes), −80% de coût sur les tokens en entrée.

---

## Décision

Trois breakpoints de cache dans chaque requête `AnthropicClient`, appliqués dans cet ordre :

1. **System prompt** - le message `Role::System` reçoit `cache_control: { type: "ephemeral" }`. Stable pour toute la durée d'une session.
2. **Liste des outils (`tools`)** - les définitions JSON des outils reçoivent `cache_control`. Change rarement (uniquement si l'agent modifie ses outils à chaud).
3. **3ème message depuis la fin** - breakpoint glissant sur l'historique des messages. Maximise le hit-rate car les messages récents changent à chaque step, mais l'historique plus ancien est stable.

**En-tête beta :** `anthropic-beta: prompt-caching-2024-07-31` est toujours envoyé par `AnthropicClient`, même si les breakpoints ne sont pas utilisés - l'API l'ignore sans cet en-tête.

**Backends non supportés :**
- `OpenAICompatibleClient` : pas de mécanisme équivalent - `cache_control` ignoré, `cache_*` tokens = 0
- `OllamaClient` : idem

**Nouveau champ dans `TokenUsage` :**

```rust
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cache_read_input_tokens: u32,   // Tokens lus depuis le cache (< prompt_tokens)
    pub cache_write_input_tokens: u32,  // Tokens écrits dans le cache lors de cet appel
    pub cost_usd: Option<f64>,
}
```

---

## Conséquences

**Positives :**
- Impact estimé : −80% de coût sur les tokens en entrée pour les sessions longues répétant le même contexte
- Rétrocompatibilité : `cache_read_input_tokens` et `cache_write_input_tokens` = 0 par défaut pour les autres backends
- Monitoring : `session_costs.jsonl` trace `cache_read` / `cache_write` par session pour vérifier le hit-rate

**Négatives / Compromis :**
- La première requête de chaque session paie `cache_write` (légèrement plus cher) - rentable dès la 2ème requête avec le même contexte
- Le breakpoint glissant (3ème depuis la fin) est une heuristique : sur les sessions très courtes (<3 messages), il n'y a pas de 3ème message → pas de breakpoint glissant

**Neutres / À surveiller :**
- Le TTL du cache Anthropic est de 5 minutes. Sur les sessions inactives longtemps, le cache expire et le coût reprend son niveau normal.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Le caching est côté serveur Anthropic - il s'applique uniquement aux backends cloud. Conforme (opt-in, pas de changement de comportement local).
- **Principe #4 - Fail fast** : Si l'API retourne une erreur liée au prompt caching, `AnthropicClient` traite la réponse normalement sans les champs cache - pas de régression.

---

## Liens

- Story d'implémentation : STORY-453
- Implémenté dans : `crates/apollia-llm/src/backends/anthropic.rs`
- Wiki : [Briques LLM Backend - Prompt Caching](../wiki/Briques-LLM-Backend.md#prompt-caching)
- Documentation Anthropic : https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
