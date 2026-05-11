# ADR-038 — Mémoire utilisateur globale

**Date :** 2026-03-23
**Statut :** Accepté — **amendé par [ADR-087](ADR-087-user-profile-redesign.md) (2026-05-11)**
**Décideur :** Nidal (solo)
**Sprint :** 22

> **Amendement (ADR-087, 2026-05-11)** — Le namespace `__user__` reste la source de
> vérité globale du profil utilisateur, et le fallback automatique de
> `ctx.memory.recall("user.X")` vers `__user__` est conservé. En revanche, la
> structure interne (catégories, sources multi-valeurs, score de confiance, badge
> validated) est simplifiée au profit d'un **schéma canonique déclaratif** et d'une
> API SDK dédiée `ctx.profile.*`. Voir ADR-087 pour le détail.

---

## Contexte

Les sessions chat Apollia OS sont isolées — aucune mémoire cross-session. Le système ne connaît ni le nom de l'utilisateur, ni ses préférences, ni son expertise, ni ses habitudes. Chaque conversation repart de zéro. Les assistants de niveau Claude Code/Desktop maintiennent un contexte utilisateur entre les sessions. Nous avons besoin d'un système de mémoire utilisateur globale qui enrichit chaque interaction — mais il ne doit JAMAIS être déterministe. Le LLM reçoit le contexte comme information et DÉCIDE ce qu'il utilise.

## Décision

Nous créons un namespace mémoire spécial `__user__` dans le MemoryManager, stockant les préférences, habitudes et contexte de l'utilisateur. Le contenu est organisé en 3 catégories (preferences, habits, context) via SemanticMemory (key/value + confidence). L'injection dans le system prompt est NON-DÉTERMINISTE : le bloc est informatif ("for reference, use as you see fit"), jamais une règle runtime. Les sources sont : onboarding (0.9 confidence), chat_inference (0.5), user_explicit (0.95), agent_observation (0.5).

## Alternatives considérées

### Option A — Per-session user context only (rejetée)
**Pour :** Simple, pas de persistance nécessaire.
**Contre :** Ne résout pas l'amnésie cross-session.

### Option B — Deterministic rule engine for preferences (rejetée)
**Pour :** Comportement prévisible.
**Contre :** Viole le Principe #6, rend les agents heuristiques plutôt qu'intelligents, règles fragiles.

### Option retenue — LLM-informed injection via SemanticMemory
**Pour :** Non-déterministe (le LLM décide), utilise l'infrastructure existante, pondéré par confidence, corrigeable par l'utilisateur.
**Compromis acceptés :** Le LLM peut ignorer le contexte injecté. La précision d'extraction dépend de la qualité du LLM.

## Conséquences

**Positives :**
- Continuité cross-session
- Interactions personnalisées
- L'utilisateur peut voir, éditer et valider ses mémoires

**Négatives / Compromis :**
- Risque de mémoires incorrectes issues de l'extraction LLM (mitigé par confidence basse + boucle de feedback)
- Namespace SQLite additionnel

**Neutres / À surveiller :**
- Surveiller le taux d'adoption de la boucle de feedback utilisateur
- Considérer un auto-purge des entrées non-validées à faible confidence après 30 jours

## Principes architecturaux impactés
- Principe #6 — Mémoire à initiative de l'agent : étendu au niveau utilisateur. La mémoire est DISPONIBLE mais jamais IMPOSÉE.
- Principe #1 — Local-first : toutes les données utilisateur restent dans le SQLite local.

## Liens
- Story associée : STORY-251, STORY-252, STORY-253, STORY-254, STORY-255
