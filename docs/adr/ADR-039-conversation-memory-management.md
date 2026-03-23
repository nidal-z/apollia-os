# ADR-039 — Conversation memory management

**Date :** 2026-03-23
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 22

---

## Contexte

Les conversations chat grandissent indéfiniment. L'historique complet des messages finira par dépasser la fenêtre de contexte du LLM. Nous avons besoin d'une stratégie pour gérer la mémoire conversationnelle sans perdre le contexte important. La solution doit gérer gracieusement les longues conversations tout en préservant les décisions clés et le contexte.

## Décision

Nous adoptons un sliding window de N derniers messages (défaut 20) avec résumé LLM des messages hors fenêtre. Le résumé est stocké dans `chat_sessions.summary` (nouveau champ SQLite). Il est recalculé quand la fenêtre glisse (pas à chaque message). Le résumé est injecté comme premier message système, avant la fenêtre active. Le contexte complet pour chaque LLM call est : system prompt + user memory block + summary + last N messages + current message.

## Alternatives considérées

### Option A — Keep all messages, truncate on overflow (rejetée)
**Pour :** Le plus simple.
**Contre :** Perd le contexte le plus ancien de manière abrupte, pas de dégradation gracieuse, le modèle voit une coupure en pleine conversation.

### Option B — Hierarchical summarization (rejetée)
**Pour :** Compression multi-niveaux pour les très longues conversations.
**Contre :** Sur-ingénierie pour les besoins actuels, multiples appels LLM, difficile à débugger.

### Option retenue — Sliding window + single summary
**Pour :** Taille de contexte prévisible, un seul appel LLM de résumé quand la fenêtre glisse, le résumé préserve les décisions clés.
**Compromis acceptés :** Le résumé peut perdre des nuances des messages anciens. Un appel LLM supplémentaire quand la fenêtre se déplace.

## Conséquences

**Positives :**
- Taille de contexte bornée
- Contexte clé préservé dans le résumé
- Consommation de tokens LLM prévisible

**Négatives / Compromis :**
- Appel LLM supplémentaire au déplacement de la fenêtre (amorti — se produit environ tous les 20 messages)
- La qualité du résumé dépend du LLM

**Neutres / À surveiller :**
- Taille de la fenêtre configurable par session
- Surveiller la qualité des résumés en pratique

## Principes architecturaux impactés
- Principe #8 — CLI humaine, API machine : le résumé est généré par la machine pour consommation machine (injection dans le system prompt).

## Liens
- Story associée : STORY-257, STORY-258, STORY-259
