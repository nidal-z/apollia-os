# ADR-092 — Spec exposition `resources` MCP côté agent ReAct

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

La capability MCP `resources` permet à un serveur d'exposer du contenu (fichiers, données, contexte) consommable par le client. Méthodes : `resources/list`, `resources/read`, `resources/subscribe`. Claude Desktop expose les resources via un picker @-mention où l'utilisateur les épingle au contexte.

Apollia respecte le **principe #6 (mémoire à initiative de l'agent)** : jamais d'injection automatique de contexte mémoriel. Cela contraint comment exposer `resources` à l'agent ReAct sans violer ce principe.

## Décision

Nous exposons les `resources` MCP selon **deux voies complémentaires**, jamais auto-injectées :

1. **Voie agent** (initiative agent) : les resources sont accessibles via deux tools implicites `mcp_resources.list()` et `mcp_resources.read(uri)`. L'agent ReAct les appelle de sa propre initiative quand il a besoin du contenu — exactement comme `file_read` aujourd'hui.
2. **Voie utilisateur** (initiative user) : l'UI desktop expose un **sélecteur @-mention** dans la barre de prompt. Quand l'utilisateur épingle une resource, elle est ajoutée comme **message system prefix** au tour suivant — sous contrôle utilisateur explicite, jamais Apollia qui décide.

Les notifications `resources/updated` invalident le cache mais ne déclenchent **aucune ré-injection automatique**.

## Alternatives considérées

### Option A — Auto-injection des resources actives au contexte du LLM (rejetée)
**Pour :** UX type "tout est là pour l'agent".
**Contre :** **viole le principe #6**. Pollue le contexte avec du contenu non-demandé. Mauvais pour le coût token et la performance ReAct.

### Option B — Resources comme tools natifs uniquement, pas de @-mention (rejetée)
**Pour :** purisme principe #6.
**Contre :** mauvais alignement avec l'UX Claude Desktop. Un utilisateur qui veut explicitement injecter une resource (un PDF, une page Confluence) n'a pas de voie naturelle.

### Option C — @-mention uniquement, pas de tool (rejetée)
**Pour :** simple.
**Contre :** l'agent ne peut pas chercher de lui-même des resources pertinentes. Réduit l'autonomie ReAct.

### Option retenue — Tool implicite (agent) + @-mention (user)
**Pour :** respecte principe #6 (agent prend l'initiative quand il en a besoin). Donne une voie user explicite (épingle). Aligné UX Claude Desktop pour le @-mention.
**Compromis acceptés :** double surface (tool + UI) à documenter clairement.

## Conséquences

**Positives :**
- Principe #6 respecté (zéro auto-injection).
- L'agent peut découvrir et lire des resources de sa propre initiative.
- L'utilisateur peut épingler explicitement quand il veut (UX Claude-like).

**Négatives / Compromis :**
- Documentation help à soigner pour expliquer la double voie.

**À surveiller :**
- Coût token : si un agent appelle `mcp_resources.list` à chaque tour, ajouter un cache local TTL.
- Notifications `resources/updated` : ne pas saturer l'inbox (debounce).

## Principes architecturaux impactés

- Principe #6 — Mémoire à initiative de l'agent : ✅ strictement respecté.
- Principe #3 — Contrat minimal : 2 tools implicites, pas de surface API supplémentaire.

## Liens

- ADR-007 — Mémoire à initiative de l'agent (principe #6 d'origine)
- Plan : §3.2
