# ADR-040 — Onboarding comme agent conversationnel

**Date :** 2026-03-23
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 23

---

## Contexte

Apollia OS a besoin d'un flux d'onboarding pour collecter le contexte initial de l'utilisateur (nom, préférences, outils, domaine). La plupart des applications utilisent des wizards déterministes avec des étapes numérotées et des formulaires fixes. Cependant, cela entre en conflit avec la philosophie agentique — les agents doivent être conversationnels, adaptatifs et non-déterministes. L'onboarding devrait être une vitrine des capacités du système, pas une rupture avec celles-ci.

## Décision

Nous implémentons l'onboarding comme un agent ConversationalAgent standard (SDK Sprint 21), déployé via une session chat (Sprint 18/22). Le system prompt guide 5 domaines (identité, préférences, outils, domaine, agents) mais l'agent DÉCIDE l'ordre et la profondeur des questions. L'utilisateur peut quitter à tout moment — chaque insight est persisté immédiatement dans UserMemory via ctx.memory.remember(). Pas de schéma rigide, pas d'étapes numérotées, pas de validation de complétude.

## Alternatives considérées

### Option A — Deterministic wizard with numbered steps (rejetée)
**Pour :** Prévisible, couverture complète garantie.
**Contre :** Ressenti mécanique, ne met pas en valeur les capacités agentiques, viole la philosophie agentique, schéma rigide.

### Option B — Passive learning only, no explicit onboarding (rejetée)
**Pour :** Zéro friction.
**Contre :** Nécessite de nombreuses sessions pour construire un profil utile, mauvaise première expérience.

### Option retenue — Conversational agent with guided system prompt
**Pour :** UX naturelle, vitrine des capacités agentiques, adaptatif à l'expertise de l'utilisateur, chaque réponse immédiatement persistée.
**Compromis acceptés :** Couverture potentiellement incomplète si l'utilisateur quitte tôt (mitigé par la persistance immédiate + re-déclenchement via `apollia-os onboard --topic`). Nécessite un bon travail d'ingénierie du system prompt.

## Conséquences

**Positives :**
- Démonstration first-class des capacités agentiques
- Interaction naturelle et adaptable
- Valeur immédiate (les mémoires sont utilisées dès la première vraie conversation après l'onboarding)

**Négatives / Compromis :**
- Couverture incomplète si l'utilisateur quitte tôt (mitigé par persistance immédiate + re-déclenchement via `apollia-os onboard --topic`)
- Nécessite une bonne ingénierie du system prompt

**Neutres / À surveiller :**
- Surveiller les taux de complétion et la couverture des domaines
- Considérer un A/B testing des variations de system prompt

## Principes architecturaux impactés
- Principe #3 — Contrat minimal : l'agent d'onboarding utilise le même contrat SDK que tout autre agent.
- Principe #6 — Mémoire à initiative de l'agent : l'agent d'onboarding décide ce qu'il mémorise, pas un schéma fixe.

## Liens
- Story associée : STORY-264, STORY-265, STORY-266, STORY-267
