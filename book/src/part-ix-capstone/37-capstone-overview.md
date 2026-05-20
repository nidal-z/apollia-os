# Capstone : vue d'ensemble

Vous arrivez au bout du book. Vous avez vu les 4 patterns d'agent, les 14 services du Ctx, le modèle d'erreurs typées, les tests isomorphiques, et le runtime Rust. Cette dernière partie consolide tout dans un projet réel : un assistant de **préparation de RDV commercial pour PME**.

Cas d'usage : votre commercial a un rendez-vous avec un prospect demain matin. Il tape une phrase dans le chat Apollia : *« Prépare-moi le RDV avec Acme Corp demain à 10h »*. Quinze secondes plus tard, il a un brief markdown structuré : qui est l'entreprise, ses signaux récents, l'historique CRM, les points à aborder, les questions à poser.

C'est un workflow multi-agent typique d'une prestation Apollia : un **director** qui orchestre trois **workers** spécialisés via A2A et `apollia.react`.

---

## Le résultat attendu

```markdown
# RDV Acme Corp, demain 10:00

## L'entreprise
Acme Corp est une PME industrielle (~80 salariés, 12 M€ CA).
Spécialisée dans la fabrication de pièces métalliques de précision.
Siège à Lyon, deux sites de production (Rhône, Drôme).

## Signaux récents
- 2026-05-10 : levée de fonds 4 M€ (série A) auprès d'un fonds régional.
- 2026-04-22 : recrutement annoncé d'un Directeur SI (LinkedIn).
- 2026-04-15 : nouvelle norme ISO 9001:2026 obtenue.

## Historique CRM
- 3 contacts précédents avec Pierre Martin (DSI).
- Dernier échange : 2026-03-08, demande de devis pour un audit infra (sans suite).
- Notes : préoccupation principale = traçabilité production.

## Points à aborder
1. Suivi du devis du 8 mars.
2. Cas d'usage traçabilité avec les nouveaux ateliers.
3. Lien possible avec leur récente levée de fonds.

## Questions à poser
- Quels objectifs après la levée de fonds ?
- Le Directeur SI fraîchement recruté a-t-il défini sa roadmap ?
- Quels sont les points de friction dans le système actuel ?
```

Le commercial ouvre le brief, le lit en 30 secondes, arrive au RDV mieux préparé que la concurrence.

---

## Architecture en bref

```
                     ┌─────────────────────────────────┐
                     │  meeting-director               │
                     │  (apollia.react + @on_message)  │
                     └────────────┬────────────────────┘
                                  │
                  ┌───────────────┼───────────────┐
                  │               │               │
        ┌─────────▼────┐  ┌───────▼─────┐  ┌─────▼────────┐
        │ web-research │  │ crm-lookup  │  │ meeting-prep │
        │  (3 skills)  │  │ (2 skills)  │  │  (2 skills)  │
        └──────────────┘  └─────────────┘  └──────────────┘
```

Quatre agents en tout. Un director qui orchestre, trois workers qui font le travail concret. Chaque worker a une responsabilité unique et stricte, conformément au principe #5 (un acteur, une responsabilité).

Détails au [chapitre 38](38-capstone-architecture.md).

---

## Pourquoi ce cas d'usage

Trois raisons :

1. **Aligné business.** Le modèle de monétisation Apollia est la prestation : vous facturez la création d'agents sur mesure pour des PME. Préparer des RDV commerciaux est un cas concret, courant, à fort ROI perçu.
2. **Multi-domaine.** L'agent doit fouiller le web, consulter un CRM (via API), formater un brief. Trois domaines = trois workers = un director utile.
3. **Sécurité réaliste.** Le CRM est une donnée sensible. Le brief est destiné au commercial, pas au prospect. Les credentials API du CRM sont des secrets. Tous les patterns de sécurité Apollia (gating, secrets, audit trail) trouvent leur place.

---

## Ce que vous allez voir

Le capstone se déroule en 4 chapitres :

- [Chapitre 38](38-capstone-architecture.md) : architecture détaillée. Découpage des responsabilités, schéma de séquence A2A, gating des ressources (datasources, templates, secrets), choix de patterns.
- [Chapitre 39](39-capstone-workers.md) : implémentation des 3 workers en `@agent + @skill`. Chacun avec ses TypedDict, ses `Annotated`, ses `DomainError`. ~80 lignes par worker.
- [Chapitre 40](40-capstone-director-result.md) : implémentation du director en `@on_message + apollia.react`. Observabilité (`ctx.events`), tests isomorphiques (`apollia.testing.mock`), résultat final avec template Jinja2.

À la fin, vous aurez un projet complet, structuré, testable, sandboxé, et déployable.

---

## Pré-requis pour suivre

- Avoir lu (ou survolé) les [Parties II](../part-ii-the-decorators/06-agent-decorator.md), [III](../part-iii-the-ctx-protocol/10-ctx-overview.md), [V](../part-v-error-handling/22-domain-errors.md) et [VI](../part-vi-testing/24-testing-isomorphic-mock.md).
- Avoir suivi le [quickstart director](../part-i-getting-started/04-quickstart-director.md), au moins en lecture.
- Une machine de dev avec Apollia OS installé, un backend LLM configuré (local llama.cpp ou cloud).

Si vous reproduisez tout en parallèle de la lecture, comptez 1h à 1h30 pour avoir l'ensemble qui tourne.

---

## Ce que ce capstone n'est pas

- **Un produit fini.** C'est un squelette pédagogique. La logique métier est volontairement simple (un seul scrape web, un seul appel CRM mocké). En prestation réelle, chaque worker grossirait.
- **Une refonte de votre CRM.** Le worker CRM se connecte via API REST à un CRM existant (HubSpot, Pipedrive, Salesforce). Apollia ne remplace pas le CRM.
- **Un cas optimisé.** Les choix faits (3 workers + 1 director plutôt que 1 worker qui tout fait) sont **pédagogiques**, pour illustrer A2A et `apollia.react`. Dans un cas réel à 1 ou 2 skills, un worker unique suffirait.

---

## Prochaines étapes

Passez au [chapitre 38](38-capstone-architecture.md) pour découvrir l'architecture.
