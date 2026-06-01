# ADR-093 - `sampling` MCP avec HITL pré-approval

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

La capability MCP `sampling` permet à un serveur d'invoquer **le LLM du client** via `sampling/createMessage`. Cas d'usage : un MCP server qui veut faire un appel LLM secondaire (résumé, classification, sous-décision) sans avoir sa propre API key.

C'est puissant pour les sous-agents, mais c'est aussi une surface d'attaque : un serveur malveillant ou compromis pourrait spammer le client de requêtes LLM coûteuses (DoS budget + leak via les prompts). La spec MCP recommande explicitement un **consent explicite** sur le prompt envoyé ET sur le résultat renvoyé au serveur.

Apollia dispose déjà d'une infrastructure HITL mature (`apollia-tools::governance`, `HITLCard` UI component, `chat.user_input_required` event pipeline).

## Décision

Nous implémentons `sampling/createMessage` en routant via `apollia-llm::LlmRouter` (chemin existant pour les appels LLM principaux), avec **HITL pré-approval obligatoire** :

1. Le handler `apollia-mcp` reçoit `sampling/createMessage`.
2. Un event `mcp_sampling_approval` est émis vers l'inbox desktop avec preview du prompt complet et identification du serveur source.
3. L'utilisateur voit dans la boîte de réception (onglet "À traiter") un `HITLCard` réutilisé sans modification : `[Approuver] [Refuser]`.
4. Sur Approuver → `LlmRouter::sample(...)` exécute → résultat renvoyé au serveur via la response JSON-RPC correspondante.
5. Sur Refuser ou timeout → erreur `cancelled` retournée au serveur.

**Rate limiting + budget par serveur** : chaque serveur MCP source porte un budget (par défaut 100 sampling calls / heure, configurable). Au-delà → erreur `rate_limited` au serveur sans demander à l'utilisateur.

## Alternatives considérées

### Option A - Sampling sans HITL (rejetée)
**Pour :** zéro friction UX.
**Contre :** **viole les recommandations spec MCP**. Permet à un serveur malveillant de pomper le LLM. Aucune visibilité utilisateur.

### Option B - HITL post-call (approuver le résultat avant retour serveur) (rejetée)
**Pour :** utilisateur voit le contenu réel échangé.
**Contre :** trop tard pour empêcher l'appel LLM (coût déjà engagé). UX confuse (qu'approuve-t-on exactement ?).

### Option retenue - HITL pré-approval + rate limiting
**Pour :** alignement spec MCP (consent explicite). Économise le LLM call en cas de refus. Rate limit empêche DoS budget.
**Compromis acceptés :** friction UX (un sampling = un prompt à approuver). Acceptable v0.1.0 pour la cible power user.

## Conséquences

**Positives :**
- Aligné spec MCP (consent explicite sur prompt + results).
- Économies LLM en cas de refus.
- Réutilise `HITLCard` existant - zéro nouveau composant UI.
- Rate limiting empêche burst malveillant.

**Négatives / Compromis :**
- Friction UX. Si un serveur fait beaucoup de sampling, l'utilisateur peut être saturé.

**À surveiller :**
- Si saturation observée : implémenter une "session approval" (auto-approve N sampling pour ce serveur pendant T minutes, opt-in user).
- SEP-1577 (tools dans sampling) : pour les sous-agents avancés, surveiller adoption.

## Principes architecturaux impactés

- Principe #1 - Local-first : sampling reste local (via `LlmRouter`).
- Principe #7 - Garde-fous non-négociables : HITL pré-approval + rate limit.

## Liens

- ADR-023 - HITL is resumed input/response (réutilisé)
- ADR-082 - Tool Governance
- Spec MCP 2025-11-25 - Section sampling consent
- Plan : §3.5, §8.4
