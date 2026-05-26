# Capstone : architecture multi-agent

Avant d'écrire une ligne de Python, posons les choix architecturaux. C'est l'étape la plus importante d'un projet multi-agent : un mauvais découpage rend les workers difficiles à tester, à réutiliser, et à composer dans d'autres workflows.

---

## Les quatre agents

| Agent | Rôle | Type | Skills |
|---|---|---|---|
| `meeting-director` | Orchestrateur, point d'entrée chat | conversational + react | (aucune skill A2A exposée) |
| `web-research` | Recherche d'infos publiques sur une entreprise | worker | `web.research.company`, `web.research.signals`, `web.research.linkedin` |
| `crm-lookup` | Lookup CRM (contacts, historique) | worker | `crm.lookup.account`, `crm.lookup.history` |
| `meeting-prep` | Formate le brief markdown final | worker | `prep.build_brief`, `prep.format_questions` |

Trois workers de domaine + un director qui agrège. Chaque worker peut être réutilisé dans d'autres projets (un agent de veille concurrentielle utilise `web-research`, un agent de support utilise `crm-lookup`, etc.).

---

## Diagramme de séquence

```
Commercial                Director              web-research   crm-lookup   meeting-prep
    │                        │                       │             │             │
    │ "RDV Acme demain 10h"  │                       │             │             │
    ├───────────────────────►│                       │             │             │
    │                        │ apollia.react        │             │             │
    │                        │  step 1 : LLM         │             │             │
    │                        │   décide : recherche  │             │             │
    │                        │                       │             │             │
    │                        │ ctx.a2a.invoke        │             │             │
    │                        ├──────────────────────►│             │             │
    │                        │   web.research.company│             │             │
    │                        │                       │             │             │
    │                        │◄──── {company:...}    │             │             │
    │                        │                       │             │             │
    │                        │ ctx.a2a.invoke        │             │             │
    │                        ├──────────────────────►│             │             │
    │                        │   web.research.signals│             │             │
    │                        │◄──── {signals:...}    │             │             │
    │                        │                       │             │             │
    │                        │ ctx.a2a.invoke        │             │             │
    │                        ├─────────────────────────────────────►│             │
    │                        │   crm.lookup.account                │             │
    │                        │◄──── {contacts:...}                 │             │
    │                        │                       │             │             │
    │                        │ ctx.a2a.invoke        │             │             │
    │                        ├─────────────────────────────────────────────────────►│
    │                        │   prep.build_brief                                  │
    │                        │◄──── {markdown:...}                                 │
    │                        │                       │             │             │
    │   "# RDV Acme..."      │                       │             │             │
    │◄───────────────────────┤                       │             │             │
```

Le director appelle 4 skills A2A en séquence. Le LLM décide de l'ordre et des paramètres exacts. Si une étape échoue (CRM indisponible, par exemple), `apollia.react` adapte (skip, fallback, retry selon le `system_prompt`).

---

## Découpage des responsabilités

**Pourquoi 3 workers + 1 director, et pas 1 agent monolithique ?**

| Argument | 1 agent | 3 workers + director |
|---|---|---|
| Réutilisation | Aucune, tout est couplé | `web-research` réutilisable par 3 autres projets |
| Test | Difficile (mock LLM + CRM + scraping) | Chaque worker testable isolément |
| Debug | Stacktrace géante | Chaque worker = un audit trail propre |
| Scaling | Tout dans un venv | Chaque worker dans son venv (dépendances isolées) |
| Permissions | Tout-ou-rien | Granulaire (le director ne touche pas aux secrets CRM) |

Le découpage en workers est l'application directe du **principe #5** (un acteur, une responsabilité). C'est plus de fichiers à maintenir, mais une architecture qui scale.

**Pourquoi un director et pas trois invocations CLI séquentielles ?**

Parce que le LLM doit **décider** : aller chercher quels signaux web, faut-il consulter le CRM en parallèle, et comment formater le brief selon ce qui a été trouvé. Un script déterministe ne saurait pas adapter. Un director ReAct, si.

---

## Gating des ressources

Chaque agent déclare strictement ce qu'il consomme.

### `meeting-director`

```python
@agent(
    name="meeting-director",
    version="0.1.0",
    description="Prepare a commercial meeting briefing.",
)
```

Aucune ressource directe : le director ne lit pas de datasource, ne rend pas de template, ne consomme pas de secret. Il orchestre via A2A. Les workers font le travail.

### `web-research`

```python
@agent(
    name="web-research",
    version="0.1.0",
    description="Public web research about a company.",
    agent_type="worker",
    tools_required=("web_search", "web_read"),
)
```

Outils natifs `web_search` et `web_read` (sandboxés). La liste des sources fiables (Les Échos, La Tribune, etc.) qui sert à scorer les hits est codée comme constante Python dans le worker pour rester en mode mono-fichier. Un projet réel exposerait ces sources comme datasource YAML via le mode package.

### `crm-lookup`

```python
@agent(
    name="crm-lookup",
    version="0.1.0",
    description="Read-only CRM lookup (HubSpot).",
    agent_type="worker",
    secrets=("hubspot_api_token",),
    tools_required=("web_read",),  # appel API HubSpot via web_read
)
```

Le secret `hubspot_api_token` est obligatoire. Le worker l'utilise pour appeler l'API HubSpot. Pas d'écriture côté CRM (lecture seule).

### `meeting-prep`

```python
@agent(
    name="meeting-prep",
    version="0.1.0",
    description="Format the final meeting briefing.",
    agent_type="worker",
)
```

Aucun secret, aucun outil. C'est un agent purement de formatting : il assemble une chaîne markdown à partir du payload reçu. Les workers Apollia qui ont besoin de templates Jinja2 embarqués passent par le mode package `agent.toml` ; ici, on garde le formatting en code Python pur pour rester en mode mono-fichier (cf. section « Organisation des fichiers » ci-dessus).

---

## Choix de patterns

### Pourquoi `apollia.react` plutôt que `@orchestrated`

Le director enchaîne au moins 3 appels A2A et doit potentiellement adapter (skip CRM si indisponible, demander plus de signaux web si peu d'infos). Une boucle ReAct gérée explicitement dans le code donne le contrôle. ORIA serait moins lisible ici parce que l'enchaînement est essentiellement déterministe avec quelques variantes.

### Pourquoi un `@on_message` plutôt qu'une `@skill`

Le director est consommé par un humain dans le chat Apollia. Il prend du texte libre (« RDV Acme demain 10h »), pas un payload structuré. `@on_message` est le bon pattern (cf. [chapitre 8](../part-ii-the-decorators/08-on-message-decorator.md)).

Si on voulait l'invoquer aussi en A2A par un autre agent (un agent calendrier qui voit un nouveau meeting), on ajouterait une `@skill("meeting.prepare", description="...")` en plus.

### TypedDict ou dict simple pour les payloads A2A

Pour ce capstone, on utilise des **TypedDict** pour les payloads structurés (par exemple `CompanyInfo`, `SignalEntry`, `ContactRecord`). Le LLM director génère des appels A2A plus propres parce qu'il voit la structure exacte (cf. [chapitre 21](../part-iv-llm-friendly-design/21-typeddict-schemas.md)).

Les sous-modules `schemas.py` de chaque worker contiennent les TypedDict. Convention : pas de `from __future__ import annotations` dans ces fichiers (PEP 563 casse `__required_keys__`).

---

## Organisation des fichiers

```
agents/
├── meeting-director/
│   └── director.py
├── web-research/
│   └── web_research.py        (TypedDicts + agent dans un seul .py)
├── crm-lookup/
│   └── crm_lookup.py
└── meeting-prep/
    └── meeting_prep.py
```

Un dossier par agent, **un fichier `.py` par agent**. `apollia-os agent install <file>.py` copie uniquement le fichier passé en argument, pas un `schemas.py` ou un dossier `datasources/` voisins : tout ce que l'agent utilise (TypedDicts, helpers internes) doit donc vivre dans son `.py` principal. Pour les agents qui nécessitent vraiment des templates Jinja2 ou des datasources YAML embarqués, la voie est le mode package via `agent.toml` (à traiter dans le chapitre `apollia-os agent package` du wiki, *disponible prochainement*). Le capstone reste en mode mono-fichier pour ne pas multiplier les concepts.

---

## Coûts estimés

Sur un cas typique « RDV Acme Corp demain à 10h » :

| Action | Coût |
|---|---|
| 1 appel `web.research.company` (web_search + 1 web_read) | ~$0.001 |
| 1 appel `web.research.signals` (3-5 web_read) | ~$0.005 |
| 1 appel `crm.lookup.account` (HubSpot API) | $0 (API HubSpot gratuite jusqu'à 250k/mois) |
| 1 appel `prep.build_brief` (template Jinja2) | $0 |
| 5-7 round-trips LLM dans `apollia.react` | ~$0.02 avec Haiku-4.5 |
| Total | ~$0.03 par brief |

Si vous générez 50 briefs par mois pour 5 commerciaux, ça fait $7.50/mois. Acceptable pour un agent à fort ROI perçu.

---

## Prochaine étape

Passez au [chapitre 39](39-capstone-workers.md) pour implémenter les 3 workers.
