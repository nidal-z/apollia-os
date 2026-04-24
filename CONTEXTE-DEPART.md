# Contexte de départ — Apollia OS

> Document de référence pour le lancement de l'offre de service "Création d'agents sur mesure".
> Audience : Nidal (auteur), futurs collaborateurs, prestataires vidéo, prospects.
> Date : 2026-04-24. Sprint référence : 40.
> Convention : six livrables autoportants, lisibles indépendamment.

---

# 1. Audit & charte documentaire

## 1.1 Constat de départ

Apollia OS dispose aujourd'hui de deux systèmes documentaires :

- **`book/src/`** — book mdBook pédagogique type "The Rust Book", chapitres ch01 → ch19 plus annexes A-F. Cible : builder Python.
- **`docs/wiki/`** — 69 pages de référence technique (briques, specs API, ADRs, architecture). Cible : développeur ou contributeur du runtime Rust.

Il n'existe **aucun système orienté operator** (utilisateur final qui déploie et supervise des agents sans coder). C'est précisément ce que l'offre commerciale impose de combler. On ouvre donc un troisième système : **`help/`**.

Règle absolue de séparation, déjà mentionnée dans `CLAUDE.md` mais à étendre :

```
book  = apprendre le produit (builder, progressif, exemples)
wiki  = référence technique (toute audience, exhaustif, specs)
help  = faire une tâche (operator, actionnable, pas-à-pas)
```

## 1.2 Tableau de propriété — qui couvre quoi

Chaque thématique appartient à **un seul** système. Si elle figure dans deux colonnes, c'est une faute à corriger.

| Thématique produit | Book (apprendre) | Wiki (référence) | Help (faire) |
|---|---|---|---|
| Installation runtime Rust | — | `INSTALL-Production.md` | — |
| Installation app desktop | — | — | "Installer Apollia" |
| Premiers pas (premier agent Python) | ✅ ch01–ch02 | — | — |
| Contrat AIP (manifest, run, AIPResult) | ✅ ch03 (narratif) | `Briques-AIP-Specification.md` (table) | — |
| SDK Python `apollia-sdk` | ✅ ch04, ch08 (utilisation) | `Agents-SDK-Guide.md` (signatures) | — |
| RuntimeContext (services injectés) | ✅ ch03 (apprendre) | `Agents-RuntimeContext-Guide.md` (table API) | — |
| Outils natifs (10 outils) | ✅ ch04 (3 exemples) | `Outils-Reference.md` (table complète) | — |
| Sandbox & audit trail | — | `Briques-Tool-Registry.md` | — |
| Mémoire — concept & API Python | ✅ ch05 | `Briques-Memory-Engine.md` | — |
| Mémoire — consulter dans l'UI | — | — | "Consulter et nettoyer la mémoire" |
| LLM backends — config Python | ✅ ch06 | `Briques-LLM-Backend.md` | — |
| LLM backends — connecter dans l'UI | — | — | "Connecter un fournisseur d'IA" |
| Garde-fous (StepBudget Python) | ✅ ch07 | — | — |
| ORIA / mode orchestré | ✅ ch09 | `Briques-ORIA-Engine.md` (spec interne) | — |
| HITL — concept & code Python | ✅ ch10 | — | — |
| HITL — approuver dans l'UI | — | — | "Approuver ou refuser une action" |
| Chat operator | — | `Briques-Chat.md` | "Discuter avec votre IA" |
| Projets + context providers | — | `Briques-Workspace.md` | "Créer un projet et activer les providers" |
| Pipelines — concept & DSL Python | ✅ ch13 | `Briques-Pipelines.md` | — |
| Pipelines — lancer dans l'UI | — | — | "Lancer un pipeline" |
| Triggers — types & expressions | — | `Briques-Triggers.md` (table) | "Programmer un trigger" |
| MCP — concept & client Python | ✅ ch04 (mention) | `Briques-MCP.md` | — |
| MCP — installer dans l'UI | — | — | "Connecter un serveur MCP" |
| STT (Whisper local) | — | `Briques-STT.md` (à créer) | "Activer la dictée vocale" |
| Notifications | — | `Briques-Notifications.md` | "Configurer les notifications" |
| Observabilité (digest, coûts, audit) | — | `Ops-Exploitation-et-Debug.md` | "Lire le digest quotidien" |
| Sécurité (permission rules, local-first) | — | `Securite-*.md` (3 pages) | "Configurer les permissions de fichiers" |
| Worker pattern (spécialisation) | ✅ ch08, ch11 | `Worker-Agent-Pattern.md` | — |
| Adapters (LangGraph/CrewAI) | ✅ ch18 | `Agents-Adapter-Existants.md` (recettes) | — |
| App desktop — architecture | ✅ ch17 | `Briques-Desktop.md` | — |
| Runtime Rust — architecture interne | ✅ ch16 | `Briques-Runtime-Core.md` | — |
| CLI complète | ✅ ch19 | `Briques-CLI.md` (table) | "Utiliser la CLI" (page avancée) |
| Architecture — vision système | — | `Architecture-Vue-Ensemble.md` | — |
| Décisions architecturales | — | `Decisions-Log.md` (ADRs) | — |
| Inbox / Approvals / Tasks (UI transversal) | — | — | "Utiliser l'inbox" |
| Companion (assistant flottant) | — | — | "Activer la compagnonne IA" |
| Command palette | — | — | "Naviguer au clavier" |

**Lecture du tableau :** chaque ligne a au plus un ✅ par paire (book/wiki/help). Une thématique peut apparaître dans book **et** wiki si — et seulement si — la première l'aborde narrativement et la seconde fournit la table de référence (ex : ligne "Outils natifs" : book = 3 exemples concrets, wiki = table des 10 outils avec paramètres). Jamais le même contenu dans les deux.

## 1.3 Redondances détectées (à corriger)

L'audit du wiki a identifié **6 pages narratives qui dupliquent le book** :

| Page wiki | Redondance avec | Décision |
|---|---|---|
| `Agents-Quickstart.md` (3829 lignes) | `book/ch01-ch02` | **Supprimer** — réduire à un stub d'une page qui renvoie vers `book/ch01.html`. Quickstart = pédagogique, sa place est dans le book. |
| `Agents-Tutoriel-Hello-Agent.md` | `book/ch01-ch02` | **Supprimer** — même raison. Doublon explicite du tutoriel d'introduction. |
| `Agents-SDK-Guide.md` (747 lignes) | `book/ch03-ch04` | **Réécrire en table de référence pure** — supprimer les passages narratifs, ne garder que les signatures de méthodes, paramètres, retours, exceptions. |
| `Agents-RuntimeContext-Guide.md` (780 lignes) | `book/ch03` | **Réécrire en référence** — table des services injectés (`ctx.llm`, `ctx.memory`, `ctx.tools`, `ctx.delegate`), une ligne par méthode. |
| `Agents-ContextBootstrap-Guide.md` | `book/ch08` (worker bootstrap) | **Garder en spec** — le book présente le pattern, le wiki documente la classe abstraite `ContextBootstrap` et les implémentations livrées. Vérifier qu'aucun tutoriel narratif ne reste. |
| `Agents-Mode-Orchestre.md` | `book/ch09` | **Réécrire en référence** — ne garder que la table des opcodes ORIA, les conditions de replanification, les codes d'erreur. Le narratif va au book. |

**Action recommandée** : ouvrir une story `book-wiki-cleanup-v2` au prochain sprint, qui exécute ces 6 transformations + ajoute un test CI qui interdit les sections H2 commençant par "Comment", "Tutoriel", "Premiers pas" dans `docs/wiki/`.

**Pages wiki à conserver telles quelles** (référence pure, sans risque) : `Briques-*.md` (16 pages), `API-HTTP-Reference.md`, `Outils-Reference.md`, `Decisions-Log.md`, `Config-apollia-toml.md`, `Architecture-*`, `Securite-*`, `Sprint-Summary.md`.

## 1.4 Charte de séparation — 10 règles

1. **Une thématique = un système propriétaire.** Si elle apparaît dans deux colonnes, c'est une faute, pas un choix.
2. **Le book apprend en faisant.** Chaque chapitre construit ou modifie du code. Si le lecteur n'écrit rien, le contenu n'est pas dans le book.
3. **Le wiki est consulté, jamais lu de bout en bout.** Tables, signatures, codes d'erreur, paramètres. Pas de "Bienvenue", pas de "Avant de commencer".
4. **Le help guide une action UI.** Si l'utilisateur ne clique nulle part, ce n'est pas du help.
5. **Sens autorisé des liens** : help → wiki (spec), help → book (concept), book → wiki (table). Jamais wiki → book.
6. **Pas de code Python dans le help.** L'operator ne lit pas de code. Si une étape exige du code, c'est un lien vers le book.
7. **Pas de capture d'écran dans le wiki.** Une capture pédagogique ou actionnable ne se réfère pas, elle se montre.
8. **Pas de table de spec exhaustive dans le book.** Si une table dépasse 10 lignes ou évoque des codes d'erreur, elle vit dans le wiki.
9. **Une page = un objectif.** Si on rédige "et aussi…", on scinde.
10. **Help et wiki visent < 800 mots/page.** Le book peut s'étendre par chapitre, mais pas par sous-section.

## 1.5 Arborescence cible `help/`

```
help/
├── index.md                                      # quoi faire en premier
├── installation/
│   ├── installer-apollia.md
│   └── connecter-un-fournisseur-d-ia.md
├── chat/
│   ├── discuter-avec-votre-ia.md
│   └── activer-la-dictee-vocale.md
├── agents/
│   ├── installer-un-agent.md
│   ├── demarrer-un-agent.md
│   └── consulter-les-logs-d-un-agent.md
├── projets/
│   ├── creer-un-projet.md
│   ├── activer-les-context-providers.md
│   └── lier-un-projet-a-un-chat.md
├── memoire/
│   ├── consulter-la-memoire.md
│   └── supprimer-une-entree.md
├── automatisations/
│   ├── programmer-un-trigger.md
│   └── suivre-l-historique-d-un-trigger.md
├── pipelines/
│   ├── lancer-un-pipeline.md
│   └── approuver-une-etape-hitl.md
├── controle/
│   ├── approuver-ou-refuser-une-action.md
│   └── configurer-les-permissions-de-fichiers.md
├── integrations/
│   ├── connecter-un-serveur-mcp.md
│   └── tester-une-connexion-mcp.md
├── observabilite/
│   ├── lire-le-digest-quotidien.md
│   ├── surveiller-les-couts-llm.md
│   └── consulter-l-audit-trail.md
├── notifications/
│   ├── configurer-un-canal.md
│   └── choisir-les-evenements-notifies.md
├── transversal/
│   ├── utiliser-l-inbox.md
│   ├── activer-la-compagnonne-ia.md
│   └── naviguer-au-clavier-command-palette.md
└── troubleshooting/
    ├── le-fournisseur-d-ia-ne-repond-pas.md
    ├── un-agent-est-bloque.md
    ├── une-action-est-refusee.md
    └── la-dictee-vocale-ne-transcrit-rien.md
```

**Ton éditorial du help/** :

- Verbe d'action en titre, à l'infinitif ou à l'impératif. Pas de "Comment", pas de "Guide pour".
- Vouvoiement systématique. L'operator est un professionnel.
- Présent indicatif, jamais de conditionnel ("Cliquez sur…", pas "Vous devriez cliquer sur…").
- Une capture d'écran pour trois étapes maximum (densité visuelle). Pas une par étape.
- Aucun jargon : pas de "runtime", "acteur", "FTS5", "IPC", "PyO3". Si nécessaire, lien vers `Glossaire.md`.
- < 800 mots par page. Au-delà, on scinde ou on délègue au book/wiki.

---

# 2. Angles benefit-centric — 13 parcours

**Test de validation :** retirer le mot "Apollia" du titre. S'il reste compréhensible et désirable pour un prospect qui ne connaît pas le produit, il est benefit-centric. S'il devient flou, il est encore feature-centric.

| # | Titre (10 mots max, sans "Apollia") | Sous-titre — gain mesurable / temps récupéré | Problème opérateur (1 phrase) | Promesse (1 phrase) |
|---|---|---|---|---|
| 1 | **Soyez productif avec votre IA en moins de 5 minutes** | Aucun cloud, aucun compte, première réponse en 4 minutes chrono | Configurer un assistant IA avancé prend habituellement une heure entre comptes, clés API et documentation. | Un assistant local opérationnel le temps d'un café, sans aucune inscription externe. |
| 2 | **Une IA qui connaît votre travail et agit sur vos fichiers** | Économisez 20 minutes de copier-coller par session | Vous ré-expliquez votre contexte à ChatGPT à chaque conversation, et il ne peut rien faire concrètement sur votre machine. | Un chat qui voit vos fichiers, lit votre historique git, et exécute les actions que vous validez. |
| 3 | **Un employé numérique qui exécute, pas un chatbot qui répond** | Récupérez 5 à 15 heures par semaine sur les tâches répétitives | Un chatbot vous donne des conseils ; vous restez la personne qui fait le travail. | Des agents qui prennent une tâche, l'exécutent de bout en bout, et reviennent avec le résultat. |
| 4 | **Votre IA arrive briefée à chaque session** | Zéro ré-explication, le contexte se charge tout seul | Décrire son projet à une IA prend dix minutes et reste imprécis. | Vos fichiers, l'historique git et vos guidelines sont injectés automatiquement à chaque conversation. |
| 5 | **Une IA qui se souvient de vos préférences et de vos décisions** | Évitez de répéter les mêmes consignes 50 fois par mois | Chaque conversation repart de zéro ; vos préférences de format, de ton, de style se perdent. | Une mémoire persistante que vous voyez, contrôlez et nettoyez à tout moment. |
| 6 | **Une IA qui travaille pendant que vous dormez** | Récupérez 8 heures par semaine d'actions récurrentes | Le rapport hebdo, la veille, le tri des fichiers entrants — c'est toujours vous qui démarrez. | Programmez l'agent une fois, il s'exécute seul tous les lundis à 8h, à chaque mail entrant, ou sur appel HTTP. |
| 7 | **Des workflows multi-étapes sans écrire une ligne de code** | Industrialisez en deux jours ce qui prenait deux semaines | Les workflows réels enchaînent collecte, analyse, validation, livraison — les outils no-code s'arrêtent à 3 étapes. | Composez plusieurs agents en séquence ou parallèle, avec validation humaine intercalée à n'importe quelle étape. |
| 8 | **Déléguer sans jamais perdre la main** | Zéro action destructive sans votre feu vert explicite | Confier des actions à une IA fait peur — un mauvais script peut effacer un dossier entier. | Chaque action sensible est bloquée en attente de votre approbation, avec aperçu clair de ce qui va se passer. |
| 9 | **Branchez votre IA sur tous vos outils métier** | Notion, GitHub, Slack, Gmail — disponibles en deux clics | Une IA isolée de vos outils ne sait ni lire vos emails, ni créer un ticket, ni consulter votre base. | Un catalogue de connecteurs prêts à l'emploi, installables et révocables sans code. |
| 10 | **Dictez à votre IA, elle exécute** | Trois fois plus rapide que taper, mains occupées comprises | Taper des instructions complexes est lent, surtout en réunion ou en déplacement. | Un raccourci, une phrase parlée, une transcription locale instantanée — sans envoyer votre voix dans le cloud. |
| 11 | **Voyez d'un coup d'œil ce que votre IA a fait cette nuit** | Un seul écran pour le digest, les coûts et les actions en attente | Plusieurs agents tournent en parallèle ; impossible de savoir où ils en sont, combien ils coûtent, qui attend une décision. | Un tableau de bord centralisé avec digest quotidien, coûts par jour, actions en attente, audit trail complet. |
| 12 | **Vos données restent chez vous, point final** | Zéro octet ne quitte la machine sans une action explicite | ChatGPT, Copilot, Gemini envoient votre code et vos documents vers des serveurs tiers — sans contrôle, sans audit. | Tout fonctionne en local, modèles inclus si vous le voulez, avec des règles de permission par dossier. |
| 13 | **Soyez alerté seulement quand ça compte vraiment** | Plus d'agents qui terminent dans le silence ; moins de spam non lu | Un agent qui finit sans prévenir, c'est une attente inutile ; une avalanche de notifications, c'est ignoré. | Choisissez par événement et par canal (desktop, webhook) qui vous prévient, et quand. |

**Rappel d'usage :** ces 13 angles sont les titres canoniques. Ils sont réutilisés tels quels dans :
- les titres des vidéos (livrable 3),
- les titres H2 du help (livrable 4),
- l'accroche de la démo commerciale (livrable 5),
- les sections de la checklist QA (livrable 6).

---

# 3. Scripts vidéos courtes — 10 vidéos

Ordre de production (du plus impactant au moins) :

1. Triggers
2. Agents
3. Projets + Chat
4. HITL
5. MCP
6. Pipelines
7. Mémoire
8. STT
9. Observabilité
10. Onboarding

Format strict pour chacun : **[ACCROCHE 30s]** → **[DONNÉES DE DÉMO]** → **[DÉMO PAS-À-PAS]** (action → narration → durée) → **[RÉSULTAT 30s]** → **[CTA 15s]**.

Cible : 5 à 8 minutes par vidéo. Chaque vidéo est autonome — pas besoin d'avoir vu les précédentes.

---

## Vidéo 1 — Une IA qui travaille pendant que vous dormez (Triggers)

**[ACCROCHE — 30s]**
> "Tous les lundis matin, vous y passez trente minutes. Compiler le rapport hebdo : copier ici, coller là, mettre en forme, envoyer. Multipliez par cinquante-deux semaines, ça fait vingt-six heures par an que vous offrez à une tâche qu'un humain ne devrait plus faire en 2026. Dans six minutes, vous saurez configurer votre IA pour qu'elle le fasse à votre place — pendant que vous dormez."

**[DONNÉES DE DÉMO]**
- Agent installé : `weekly-digest-agent` (worker, lit `~/Notes/semaine`, produit `~/Rapports/digest-YYYY-WW.md`).
- Dossier `~/Notes/semaine/` rempli de 6 à 8 fichiers `.md` fictifs : notes de réunion, idées, todos.
- Backend LLM connecté et pingué (Anthropic ou Ollama, peu importe).
- Canal de notification "Bureau" préconfiguré dans Notifications.
- App ouverte sur la page **Dashboard** au début.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Cliquer "Automatisations" dans la sidebar (route `/Triggers.svelte`) | "Toutes les automatisations vivent ici. Triggers programmés, leur historique, leurs prochains déclenchements." | 20s |
| 2 | Bouton "+ Nouveau trigger" → modal `CreateTriggerDialog` | "On crée un trigger. Apollia propose cinq types : cron, intervalle régulier, date unique, surveillance de fichiers, webhook." | 30s |
| 3 | Onglet "Cron" → expression `0 8 * * MON` → label "Rapport hebdo lundi" | "Tous les lundis à 8h. Pas besoin de connaître la syntaxe par cœur, Apollia traduit en clair sous le champ." | 40s |
| 4 | Champ "Agent cible" → sélectionner `weekly-digest-agent` | "On choisit l'agent qui va exécuter le travail. Ici, mon agent de digest hebdo, déjà installé." | 20s |
| 5 | Champ "Payload" → laisser vide → "Créer" | "Le payload, c'est ce qu'on transmet à l'agent au déclenchement. Optionnel : l'agent sait déjà ce qu'il a à faire." | 20s |
| 6 | Trigger apparaît dans la liste → bouton "Déclencher maintenant" | "Avant d'attendre lundi, on teste. Un clic sur 'déclencher', l'agent démarre tout de suite." | 30s |
| 7 | Onglet "Historique" → run en cours → status `running` puis `completed` | "L'historique montre chaque exécution, sa durée, son résultat. En cas d'échec, on a le détail dans les logs." | 40s |
| 8 | Notification desktop "Digest hebdo prêt" → clic → ouvre l'inbox | "Apollia notifie quand c'est terminé. Un clic, on voit le rapport produit." | 30s |
| 9 | Ouvrir `~/Rapports/digest-2026-17.md` dans Finder/Explorer | "Le fichier existe vraiment, sur votre machine, prêt à être envoyé." | 20s |

**[RÉSULTAT — 30s]**
> "En quatre minutes, vous venez de récupérer trente minutes par semaine. Sur l'année, vingt-six heures rendues à votre vraie valeur ajoutée. Le trigger continue de tourner sans que vous ayez à y penser. Aucune donnée ne quitte votre machine."

**[CTA — 15s]**
> "La prochaine vidéo va plus loin : on construit l'agent qui exécute ces tâches autonomes. Pour démarrer maintenant, ouvrez le help guide *Programmer un trigger*."

---

## Vidéo 2 — Un employé numérique qui exécute (Agents)

**[ACCROCHE — 30s]**
> "Un chatbot vous répond. Un agent fait. La différence ? Cinq à quinze heures par semaine. Vous allez voir un agent qui prend une mission complexe, l'analyse, exécute plusieurs étapes, et revient avec le résultat — sans que vous ayez à le pousser à chaque coup."

**[DONNÉES DE DÉMO]**
- Agent installé : `competitive-watch-agent` (assistant, multi-tour, accès web + filesystem).
- Trois sources de veille pré-configurées dans son manifest (URLs concurrents).
- Backend LLM Anthropic Claude (raisonnement de qualité).
- Dossier de sortie `~/Veille/` créé et vide.
- Permission rule pré-créée : agent autorisé à écrire dans `~/Veille/` uniquement.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Cliquer "Agents" dans la sidebar (route `/Agents.svelte`) | "La page Agents montre vos agents installés. Ici, mon agent de veille concurrentielle." | 20s |
| 2 | Onglet "Assistant" → carte `competitive-watch-agent` → "Démarrer" | "Un agent assistant accepte des missions multi-tours. On le démarre." | 25s |
| 3 | Bouton "Ouvrir le chat" → conversation liée s'ouvre | "Apollia ouvre une conversation dédiée. Tout ce qu'on dit ici va à cet agent." | 20s |
| 4 | Taper : "Compare nos trois concurrents sur leur positionnement souveraineté et écris-moi un rapport." → Envoyer | "Une mission concrète, en langage naturel. Pas de prompt magique." | 30s |
| 5 | Volet `ReasoningSequence` apparaît à droite — étapes : "lit URL 1", "lit URL 2", "lit URL 3", "synthèse", "écriture fichier" | "Le panneau de droite montre le raisonnement en temps réel. Vous voyez exactement ce que l'agent fait, pas une boîte noire." | 60s |
| 6 | Étape "écriture fichier" → carte HITL `ApprovalCard` apparaît | "Une action sensible : écrire un fichier. Apollia s'arrête et demande votre feu vert. Vous voyez le chemin et un aperçu du contenu." | 40s |
| 7 | Bouton "Approuver" sur la carte | "On approuve. L'agent reprend." | 15s |
| 8 | Message final : "Rapport écrit dans ~/Veille/concurrents-2026-W17.md" | "Le rapport est sur votre machine, prêt à être ouvert ou partagé." | 25s |
| 9 | Ouvrir le fichier dans le Finder | "Tâche complexe, déléguée, exécutée — sans perdre le contrôle." | 20s |

**[RÉSULTAT — 30s]**
> "Cette mission vous aurait pris une heure. L'agent l'a faite en six minutes pendant que vous regardiez. Multipliez par les dizaines de tâches récurrentes que vous accumulez chaque semaine, et vous comprenez pourquoi un agent n'est pas un assistant — c'est un employé."

**[CTA — 15s]**
> "Vidéo suivante : on apprend à votre IA tout votre contexte de travail en deux clics. Pour installer un agent maintenant, ouvrez le help *Installer un agent*."

---

## Vidéo 3 — Une IA qui connaît votre travail (Projets + Chat)

**[ACCROCHE — 30s]**
> "Vous ouvrez ChatGPT. Vous ré-expliquez votre stack, votre rôle, votre contexte. Dix minutes plus tard, vous commencez enfin à travailler. Si vous le faites cinq fois par jour, c'est cinquante minutes perdues. Aujourd'hui : votre IA arrive briefée, sans copier-coller, dès l'ouverture du chat."

**[DONNÉES DE DÉMO]**
- Repo de démo : un petit projet Rust ou Python avec 5-8 commits récents et un `README.md` significatif.
- Projet Apollia "Demo-app" déjà créé, lié au dossier ci-dessus.
- Aucun chat existant lié à ce projet (on en crée un live).

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Sidebar → "Projets" → carte "Demo-app" → "Ouvrir" | "Un projet Apollia, c'est un dossier sur votre machine + un contexte automatique pour vos chats." | 25s |
| 2 | Onglet "Context providers" dans `ProjectDetail` | "Ici, on choisit quelles informations injecter. Quatre fournisseurs disponibles : git, arborescence, sortie de commande, documents." | 30s |
| 3 | Activer "Git" (toggle) → preview montre les 5 derniers commits | "On active git. Apollia lira l'historique récent et l'injectera dans chaque conversation liée à ce projet." | 30s |
| 4 | Activer "Arborescence" → preview montre la structure du dossier | "On active l'arborescence. L'IA verra où sont vos fichiers, sans que vous les listiez." | 25s |
| 5 | Bouton "+ Nouveau chat lié" → un chat s'ouvre, attaché au projet | "On crée un chat lié au projet. Tout le contexte qu'on vient d'activer y est déjà disponible." | 20s |
| 6 | Taper : "Qu'est-ce qui a changé cette semaine sur ce projet ?" → Envoyer | "Question simple, mais qui exige du contexte précis. On verrait sinon une réponse générique." | 25s |
| 7 | Réponse de l'IA : résumé concret des 5 commits, fichiers touchés, intentions | "L'IA a réellement lu votre historique. La réponse est spécifique, pas du blabla générique." | 40s |
| 8 | Question de suivi : "Crée-moi un brief.md pour mon équipe à partir de ça" | "Une demande d'action. Apollia va vouloir écrire un fichier." | 20s |
| 9 | Carte HITL `ApprovalCard` → preview du contenu → "Approuver" | "Une action fichier, on approuve. Le brief atterrit sur votre disque." | 30s |

**[RÉSULTAT — 30s]**
> "Zéro copier-coller. La différence entre un assistant générique et un assistant qui *connaît* votre travail. Multipliez par toutes vos sessions de la semaine — vous récupérez plusieurs heures."

**[CTA — 15s]**
> "Prochaine vidéo : comment garder le contrôle quand vous déléguez. Pour créer un projet maintenant, ouvrez le help *Créer un projet*."

---

## Vidéo 4 — Déléguer sans jamais perdre la main (HITL)

**[ACCROCHE — 30s]**
> "Confier des actions à une IA fait peur. Et c'est légitime : un mauvais script peut effacer un dossier entier en trois secondes. La solution n'est pas d'éviter de déléguer — c'est de garder le dernier mot, sur chaque action sensible, sans ralentir le travail."

**[DONNÉES DE DÉMO]**
- Agent assistant `cleanup-helper-agent` installé (sait organiser, déplacer, renommer des fichiers).
- Dossier `~/Téléchargements-démo/` rempli de 15 fichiers en désordre (PDFs, images, .zip, .docx).
- Permission rule **non créée** initialement (toutes les actions doivent demander l'approbation).

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Ouvrir un chat avec `cleanup-helper-agent` | "Mon agent de rangement. Il sait classer des fichiers, mais il n'a aucune permission par défaut." | 20s |
| 2 | Taper : "Range les fichiers de ~/Téléchargements-démo dans des sous-dossiers par type." | "Une mission qui implique plusieurs actions sur le système de fichiers." | 25s |
| 3 | Première carte HITL : "Créer dossier `~/Téléchargements-démo/PDF/`" → preview chemin → "Approuver" | "Première action : créer un dossier. Apollia montre le chemin, je vois exactement ce qui va se passer." | 35s |
| 4 | Apparition rapide de plusieurs cartes HITL successives (déplacement de fichiers) | "Pour chaque action, une carte d'approbation. Granulaire, mais lisible." | 30s |
| 5 | Cliquer sur "Toujours autoriser ce type d'opération sur ce dossier" dans une carte | "Quand je fais confiance, je peux automatiser. Cette case crée une règle de permission." | 30s |
| 6 | Sidebar → Settings → "Règles de permission" (route `SettingsPermissionRules.svelte`) | "Toutes les règles vivent ici. Je les vois, je les modifie, je les supprime à tout moment." | 30s |
| 7 | Retour au chat — l'agent continue automatiquement, sans plus demander | "L'agent file. Plus aucune interruption, parce que j'ai validé la règle, pas juste l'action." | 25s |
| 8 | Tâche terminée → résumé "15 fichiers triés" → vérifier dans Finder | "Quinze actions exécutées, dont une seule a vraiment exigé mon attention. C'est le bon ratio." | 30s |

**[RÉSULTAT — 30s]**
> "Vous avez délégué une tâche fastidieuse, sans jamais perdre le contrôle. Le pattern est universel : une IA qui demande poliment ne fait jamais d'erreur que vous n'auriez pas vue venir."

**[CTA — 15s]**
> "Prochaine vidéo : comment connecter votre IA à tous vos outils métier. Pour configurer les permissions maintenant, ouvrez le help *Configurer les permissions*."

---

## Vidéo 5 — Branchez votre IA sur tous vos outils (MCP)

**[ACCROCHE — 30s]**
> "Une IA isolée ne sert à rien. Si elle ne peut pas lire votre Notion, créer un ticket GitHub, ou consulter vos emails, elle reste un jouet. La bonne nouvelle : on peut tout brancher en deux clics. Sans code, sans serveur à maintenir."

**[DONNÉES DE DÉMO]**
- Aucun MCP installé au début (état initial propre).
- Token Notion préparé dans le presse-papier (variable d'env documentée).
- Page Notion de test "Demo Apollia" prête, contenant 3 tâches.
- Agent `notion-helper-agent` (assistant, sait utiliser les outils MCP disponibles) déjà installé.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Sidebar → "Intégrations" (route `/Integrations.svelte`) → onglet "Catalogue" | "Le catalogue MCP. Des connecteurs prêts à l'emploi, classés par catégorie et niveau de confiance." | 25s |
| 2 | Filtrer "Productivité" → carte "Notion" → bouton "Installer" | "Notion. Officiel, niveau de confiance vérifié. Un clic." | 25s |
| 3 | Wizard `ConnectorWizard` étape 1 : choisir le transport (stdio par défaut) | "Apollia gère trois types de connexions : processus local, serveur HTTP, événements en streaming. Stdio par défaut, c'est le plus sécurisé." | 30s |
| 4 | Étape 2 : champ "Token Notion" → coller → "Tester la connexion" | "On colle le token. Apollia teste la connexion immédiatement." | 30s |
| 5 | Indicateur vert → liste des outils Notion disponibles s'affiche | "Connexion réussie. Apollia liste les outils que ce serveur expose : créer page, lister bases, etc." | 30s |
| 6 | Bouton "Activer" → connecteur passe en "Actif" | "On active. Tous les agents qui en ont l'autorisation peuvent désormais utiliser ces outils." | 20s |
| 7 | Ouvrir un chat avec `notion-helper-agent` → "Liste mes tâches dans la page Demo Apollia" | "Test direct depuis le chat." | 25s |
| 8 | Volet de raisonnement : "appel outil notion.list_pages" → "appel outil notion.read_page" → réponse | "L'agent appelle Notion pour de vrai, lit la page, vous renvoie le résultat structuré." | 40s |
| 9 | "Ajoute une tâche : tester la démo" → carte HITL appel MCP → "Approuver" | "Action sensible : écrire dans Notion. Apollia demande l'approbation. On approuve." | 30s |

**[RÉSULTAT — 30s]**
> "Notion connecté, sans une ligne de code, en moins de cinq minutes. Le même pattern marche pour GitHub, Slack, Gmail, votre base SQL, votre CRM. Votre IA n'est plus enfermée."

**[CTA — 15s]**
> "Prochaine vidéo : enchaîner plusieurs agents pour des workflows complets. Pour connecter un MCP maintenant, ouvrez le help *Connecter un serveur MCP*."

---

## Vidéo 6 — Des workflows multi-étapes sans code (Pipelines)

**[ACCROCHE — 30s]**
> "Un agent fait une tâche. Mais un vrai processus métier en enchaîne dix : collecter, vérifier, valider, produire, archiver. Les outils no-code s'arrêtent au troisième nœud. Aujourd'hui : un workflow réel, multi-agents, avec validation humaine intercalée — et zéro ligne de code."

**[DONNÉES DE DÉMO]**
- Pipeline défini : "onboarding-client" (4 étapes : `collect-infos` → `draft-contract` → HITL `human-review` → `send-email`).
- 4 agents workers spécialisés installés (un par étape automatique).
- Backend LLM connecté.
- Dossier `~/Clients/` prêt à recevoir l'output.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Sidebar → "Pipelines" (route `/Pipelines.svelte`) → onglet "Définitions" | "Mes pipelines. Un pipeline, c'est une chaîne d'agents avec des règles de passage." | 25s |
| 2 | Carte "onboarding-client" → "Voir le DAG" | "Apollia visualise la topologie : qui appelle qui, qui attend qui." | 30s |
| 3 | Bouton "Lancer" → modal demande les paramètres (nom client, email) → soumettre | "On lance avec un cas concret : un nouveau client à onboarder." | 30s |
| 4 | Vue temps réel : étape 1 `collect-infos` passe `pending` → `running` → `completed` | "Première étape : l'agent collecte les infos depuis nos sources internes. Vert, c'est terminé." | 40s |
| 5 | Étape 2 `draft-contract` démarre → `running` → `completed` | "Deuxième étape : un autre agent rédige le contrat à partir des infos collectées. Notez : agent différent, prompt différent, spécialisé." | 40s |
| 6 | Étape 3 `human-review` passe en jaune `awaiting-approval` → carte HITL apparaît | "Étape humaine intercalée. Le pipeline s'arrête, attend ma validation. Je vois le contrat brouillon, je peux modifier ou approuver." | 50s |
| 7 | Approuver → étape 4 `send-email` démarre → `completed` | "Validé. Le pipeline reprend, l'email part, archivage automatique." | 30s |
| 8 | Onglet "Historique" → run terminé avec timeline et durée totale | "Toute l'exécution est tracée. On peut rejouer, débugger, exporter." | 25s |

**[RÉSULTAT — 30s]**
> "Un processus client complet, du contact à l'envoi, automatisé en sept minutes — avec une étape humaine pile au bon endroit. C'est l'industrialisation du travail intellectuel."

**[CTA — 15s]**
> "Prochaine vidéo : comment votre IA mémorise vos préférences. Pour lancer un pipeline maintenant, ouvrez le help *Lancer un pipeline*."

---

## Vidéo 7 — Une IA qui se souvient de vous (Mémoire)

**[ACCROCHE — 30s]**
> "Combien de fois par mois répétez-vous à votre IA : 'Réponds-moi en bullet points', 'Évite le ton corporate', 'Mon stack c'est X' ? Multipliez par cinquante sessions. C'est du temps gaspillé, et c'est frustrant. Une IA qui apprend ne devrait jamais demander deux fois."

**[DONNÉES DE DÉMO]**
- Aucun contenu mémoire au départ (état nettoyé).
- Backend LLM connecté.
- Agent assistant standard ouvert.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Ouvrir un chat avec un assistant standard | "Conversation neuve. Aucune mémoire associée." | 15s |
| 2 | Taper : "Note ma préférence : je veux toujours des réponses en bullet points concis, sans introductions polies." → Envoyer | "Je donne une consigne durable, pas juste pour cette session." | 25s |
| 3 | Réponse de l'IA confirmant + carte HITL "Enregistrer en mémoire utilisateur" → Approuver | "L'IA propose de mémoriser. Je vois exactement ce qu'elle veut sauver, j'approuve." | 30s |
| 4 | Sidebar → "Mémoire" (route `/Memory.svelte`) → onglet "User" | "La mémoire est visible. Ici, mes préférences globales, partagées entre tous les agents." | 25s |
| 5 | Recherche FTS : "format" → l'entrée "bullet points concis" remonte | "Recherche plein-texte. Je peux retrouver et auditer ce que mes agents savent de moi." | 25s |
| 6 | Onglet "Episodic" / "Semantic" / "Procedural" — montrer 3 entrées | "Quatre types de mémoire : épisodique (événements), sémantique (faits), procédurale (méthodes), utilisateur (préférences). Chacun adapté à un usage." | 40s |
| 7 | Retour chat → fermer session → ouvrir un NOUVEAU chat avec le même type d'agent | "Test décisif : nouvelle session, nouveau chat." | 20s |
| 8 | Question banale : "Résume-moi le dernier livre que tu connais" → réponse en bullet points sans intro | "L'IA applique ma préférence sans que je l'aie redemandée. C'est la mémoire qui parle." | 25s |
| 9 | Retour Memory → bouton "Supprimer" sur l'entrée → confirmer | "Je peux retirer une préférence à tout moment. Le contrôle reste de mon côté." | 25s |

**[RÉSULTAT — 30s]**
> "Une consigne donnée une fois, appliquée pour toujours, vue et contrôlable. Vos préférences ne se perdent plus entre les sessions. Sur un mois, ça représente des dizaines de répétitions évitées."

**[CTA — 15s]**
> "Prochaine vidéo : parler à votre IA au lieu de taper. Pour gérer la mémoire maintenant, ouvrez le help *Consulter la mémoire*."

---

## Vidéo 8 — Dictez à votre IA (STT)

**[ACCROCHE — 30s]**
> "Taper une instruction complexe à une IA, c'est lent. En réunion, en déplacement, les mains occupées, c'est carrément impossible. La voix est trois fois plus rapide que le clavier — à condition que la transcription soit instantanée et qu'aucun octet de votre voix ne parte dans le cloud."

**[DONNÉES DE DÉMO]**
- Modèle Whisper local installé (small ou medium, GGML).
- Hotkey configuré (par exemple `Cmd+Shift+Espace`).
- Mode push-to-talk activé.
- Microphone fonctionnel.

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Settings → onglet "Dictée vocale" | "Configuration. Le modèle Whisper tourne en local — votre voix ne quitte jamais la machine." | 25s |
| 2 | Vérifier modèle chargé, langue FR, mode push-to-talk, hotkey défini | "Tout est prêt. Je maintiens le raccourci, je parle, je relâche." | 25s |
| 3 | Ouvrir un chat → cliquer dans le champ de saisie | "Curseur dans le champ. Je peux dicter directement dans n'importe quelle zone de texte." | 15s |
| 4 | Maintenir hotkey → dicter : "Note cette idée, refaire la page d'accueil avec un angle plus émotionnel et une démo en haut." → relâcher | "Je dicte naturellement. Une phrase complète, ponctuation incluse." | 30s |
| 5 | Texte apparaît instantanément dans le champ, transcrit | "Transcription en moins d'une seconde, en local." | 20s |
| 6 | Envoyer → l'IA répond et propose un brief | "L'IA traite la dictée comme du texte normal. Workflow fluide." | 25s |
| 7 | Settings STT → activer "Mode presse-papier global" | "Bonus : la dictée marche partout. N'importe quelle app, le texte va dans le presse-papier, prêt à coller." | 30s |
| 8 | Tester dans une note hors Apollia (TextEdit, Notes…) | "Validation hors Apollia. La dictée locale devient un outil système." | 25s |

**[RÉSULTAT — 30s]**
> "Trois fois plus rapide que taper, sans aucune fuite cloud. Vous pouvez piloter votre IA en marchant, en cuisinant, en réunion. Et ça marche pour tout votre OS."

**[CTA — 15s]**
> "Prochaine vidéo : avoir une vue d'ensemble sur ce que tournent vos agents. Pour activer la dictée maintenant, ouvrez le help *Activer la dictée vocale*."

---

## Vidéo 9 — Voir d'un coup d'œil ce qui se passe (Observabilité)

**[ACCROCHE — 30s]**
> "Cinq agents tournent en parallèle. L'un coûte de plus en plus cher, l'autre est en attente d'une décision depuis hier soir, le troisième a planté à 3h du matin. Sans un tableau de bord unique, vous découvrez les problèmes le lundi matin, trop tard."

**[DONNÉES DE DÉMO]**
- 3 à 5 agents installés et actifs depuis quelques jours.
- Triggers programmés ayant tourné plusieurs fois.
- Approvals en attente : au moins 2 dans l'inbox.
- Historique LLM avec coûts non triviaux (>1$ accumulé).

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | App ouverte au démarrage → page Dashboard | "L'écran d'accueil. Conçu pour 30 secondes le matin." | 20s |
| 2 | Bloc `DigestHero` : "5 actions terminées cette nuit, 2 en attente, 1 erreur" | "Le digest condense la nuit. Trois chiffres, je sais quoi faire." | 30s |
| 3 | Bloc `CompletedTodayBlock` → liste scrollable | "Détail de ce qui a été fait. Chaque ligne cliquable pour voir l'output." | 25s |
| 4 | Bloc "Pending" → cliquer un approval → carte HITL → Approuver | "Les attentes humaines centralisées. Une approbation, une décision, on avance." | 30s |
| 5 | Sidebar → "Observabilité" (route `/Observability.svelte`) → onglet "LLM costs" | "Vue détaillée des coûts. Par jour, par agent, par modèle." | 30s |
| 6 | Composant `LlmCostChart` : courbe sur 30 jours → spike isolé | "Un pic suspect mardi. On clique pour drill-down." | 25s |
| 7 | Drill-down → conversation responsable du spike | "On identifie l'agent et la conversation. Cause racine en 30 secondes." | 30s |
| 8 | Onglet "Audit trail" → filtrer par agent → liste exhaustive des appels outils | "Audit complet. Chaque appel d'outil, chaque approbation, chaque refus, tracé. Conformité, debug, post-mortem." | 35s |
| 9 | Onglet "Plan cache" → hit ratio, taille | "Le cache de plans ORIA, pour optimiser les coûts. On voit l'efficacité." | 20s |

**[RÉSULTAT — 30s]**
> "Trente secondes le matin, vous savez tout. Coûts maîtrisés, problèmes détectés, décisions prises. Sans dashboard, c'est exactement ce que vous découvrez le vendredi soir."

**[CTA — 15s]**
> "Dernière vidéo : démarrer Apollia en 5 minutes chrono. Pour explorer l'observabilité maintenant, ouvrez le help *Lire le digest quotidien*."

---

## Vidéo 10 — Soyez productif en moins de 5 minutes (Onboarding)

**[ACCROCHE — 30s]**
> "Configurer un assistant IA avancé, ça prend habituellement une heure. Comptes à créer, clés API à générer, documentation à lire, cluster à provisionner. Aujourd'hui : de l'installation à la première vraie réponse, en moins de cinq minutes. Sans cloud, sans inscription."

**[DONNÉES DE DÉMO]**
- Apollia jamais lancé sur cette machine (premier démarrage propre).
- Clé API Anthropic disponible dans le presse-papier.
- Réseau actif (pour le ping initial).

**[DÉMO PAS-À-PAS]**

| # | Action UI | Narration | Durée |
|---|---|---|---|
| 1 | Lancer Apollia (premier démarrage) → wizard `Onboarding.svelte` | "Premier lancement. Wizard en 4 étapes." | 15s |
| 2 | Étape 1 — Profil : choisir "Operator" → Suivant | "Operator, c'est l'utilisateur final. Builder, c'est le développeur. On choisit selon notre usage." | 30s |
| 3 | Étape 2 — Backend LLM : sélectionner "Anthropic" → coller clé API → "Tester la connexion" | "Quatre fournisseurs proposés. Anthropic pour la qualité, Ollama pour le 100% local. On teste immédiatement." | 45s |
| 4 | Indicateur vert "Connexion réussie, latence 320ms" → Suivant | "Connexion validée. Apollia est prêt à parler à un modèle." | 20s |
| 5 | Étape 3 — Tour spotlight : pointe "Sidebar", "Chat", "Agents", "Settings" | "Tour rapide des zones clés. Vingt secondes par zone, on retient l'essentiel." | 60s |
| 6 | Étape 4 — Premier chat : champ pré-rempli "Bonjour Apollia, qu'est-ce que tu peux faire pour moi ?" → Envoyer | "Une vraie réponse, en local, dans les premières secondes." | 30s |
| 7 | Streaming de la réponse — markdown rendu, suggestions de prochaines actions | "Streaming temps réel, formatage propre, et trois pistes pour aller plus loin." | 35s |
| 8 | Cliquer "Installer mon premier agent" → page `/Agents.svelte` → carte assistant communautaire | "On enchaîne sur la suite logique : installer un premier agent prêt à l'emploi." | 30s |

**[RÉSULTAT — 30s]**
> "Quatre minutes vingt depuis le premier lancement. Une vraie IA, locale, branchée, avec un agent installé. Maintenant, le vrai travail commence."

**[CTA — 15s]**
> "Vous avez vu les dix possibilités d'Apollia. Si vous voulez un assistant taillé pour votre métier, parlons-en : RDV de qualification, 30 minutes, gratuit, lien en description."

---

# 4. Help guide — 5 pages prioritaires

Format strict : titre verbe+objet → Prérequis (3 lignes max) → Étapes numérotées → placeholders `[SCREENSHOT: …]` → 1 lien sortant book OU wiki, jamais les deux.
Cible : < 800 mots/page, ton operator, zéro jargon.

---

## Page 4.1 — Programmer un trigger

**Prérequis**
- Au moins un agent installé et démarrable depuis la page Agents.
- Un fournisseur d'IA connecté (la connexion est verte dans le bandeau supérieur).
- Vous savez à quelle fréquence vous voulez que la tâche se répète.

**Étapes**

1. Dans la sidebar, cliquez sur **Automatisations**.

2. Cliquez sur le bouton **+ Nouveau trigger** en haut à droite.
   `[SCREENSHOT: page Automatisations, bouton + Nouveau trigger surligné en haut à droite]`

3. Donnez un nom clair à votre trigger (par exemple : *Rapport hebdo lundi*). Ce nom apparaîtra partout dans l'interface et dans les notifications.

4. Choisissez le **type de déclenchement** :
   - **Cron** — pour une fréquence régulière complexe (tous les lundis à 8h, le 1er du mois à 6h…).
   - **Intervalle** — pour une répétition simple (toutes les 30 minutes, toutes les heures, tous les jours).
   - **Date unique** — pour une seule exécution programmée.
   - **Surveillance de fichier** — déclencher quand un fichier est créé, modifié ou supprimé.
   - **Webhook** — déclencher sur un appel HTTP entrant.

5. Saisissez le paramètre du déclenchement choisi. Apollia traduit l'expression en langage clair sous le champ pour vous éviter les erreurs.
   `[SCREENSHOT: modal Nouveau trigger, type Cron sélectionné, expression 0 8 * * MON, traduction "Tous les lundis à 8h00" affichée en gris]`

6. Sélectionnez l'**agent cible** dans la liste déroulante. Seuls les agents installés apparaissent.

7. (Optionnel) Renseignez un **payload** — un texte qui sera transmis à l'agent au déclenchement. Vide par défaut, l'agent suit son comportement standard.

8. Cliquez sur **Créer**. Le trigger apparaît dans la liste, prêt à se déclencher.

9. Pour vérifier que tout fonctionne, cliquez sur **Déclencher maintenant** sur la ligne du trigger. Une exécution se lance immédiatement.
   `[SCREENSHOT: liste des triggers, ligne "Rapport hebdo lundi" avec bouton Déclencher maintenant à droite]`

10. Suivez l'exécution en cliquant sur **Historique**. Vous voyez la durée, le statut (succès, échec), et le lien vers les logs détaillés en cas de problème.

> **Référence technique :** [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers) (table complète des types et expressions supportées)

---

## Page 4.2 — Installer et démarrer un agent

**Prérequis**
- Un fournisseur d'IA connecté.
- Connexion internet active (pour parcourir le catalogue communautaire).
- Vous savez quelle tâche vous voulez confier à l'agent.

**Étapes**

1. Dans la sidebar, cliquez sur **Agents**.

2. Cliquez sur l'onglet **Catalogue** en haut de la page.
   `[SCREENSHOT: page Agents, onglets "Mes agents" et "Catalogue" en haut, onglet Catalogue actif]`

3. Filtrez par catégorie (productivité, veille, développement, communication…) ou tapez un mot-clé dans la barre de recherche.

4. Cliquez sur la carte de l'agent qui vous intéresse. Une fiche détaillée s'ouvre avec sa description, ses outils requis, son auteur et son niveau de confiance.

5. Vérifiez les **outils requis**. Si l'agent demande des MCP non installés (par exemple Notion, GitHub), Apollia vous le signale en orange. Installez d'abord ces MCP via la page Intégrations.

6. Cliquez sur **Installer**. L'agent est téléchargé localement (quelques secondes).
   `[SCREENSHOT: fiche agent, bouton Installer en haut à droite, bandeau "Installé localement" après clic]`

7. Retournez à l'onglet **Mes agents**. Votre nouvel agent apparaît dans la liste, séparé en deux sections : **Worker** (agents spécialisés appelés par d'autres) ou **Assistant** (agents conversationnels avec lesquels vous discutez directement).

8. Cliquez sur la carte de l'agent → bouton **Démarrer**. Le statut passe au vert.

9. Pour un assistant, cliquez sur **Ouvrir le chat**. Une conversation dédiée s'ouvre, prête à recevoir vos missions.
   `[SCREENSHOT: page Agents > Assistant > carte agent avec statut vert et boutons "Ouvrir le chat" et "Logs"]`

10. Pour un worker, l'agent est désormais disponible dans les triggers et les pipelines comme cible.

> **Référence technique :** [Community-Agent-Registry](https://github.com/nidal-z/apollia-os/wiki/Community-Agent-Registry) (catalogue complet et critères de confiance)

---

## Page 4.3 — Lier un projet à un chat

**Prérequis**
- Un dossier sur votre machine qui correspond au projet (un repo, un dossier de travail, un workspace).
- Un fournisseur d'IA connecté.
- (Optionnel) Le dossier est un repo git, pour activer le provider git.

**Étapes**

1. Dans la sidebar, cliquez sur **Projets**.

2. Cliquez sur **+ Nouveau projet** en haut à droite.
   `[SCREENSHOT: page Projets, bouton + Nouveau projet surligné]`

3. Donnez un **nom** au projet (par exemple : *Site marketing 2026*) et sélectionnez son **dossier racine** sur votre machine.

4. Cliquez sur **Créer**. Le projet apparaît dans la liste.

5. Cliquez sur la carte du projet pour ouvrir sa **page de détail**.

6. Allez dans l'onglet **Context providers**. Quatre fournisseurs sont disponibles :
   - **Git** — historique des commits récents et diffs.
   - **Arborescence** — structure des fichiers et dossiers.
   - **Sortie de commande** — résultat d'une commande shell que vous définissez.
   - **Documents** — fichiers que vous uploadez explicitement.
   `[SCREENSHOT: ProjectDetail, onglet Context providers, 4 toggles avec aperçus]`

7. Activez les providers utiles à votre projet en basculant les interrupteurs. Un aperçu du contenu injecté apparaît à côté de chaque toggle activé.

8. Cliquez sur **+ Nouveau chat lié** depuis la page projet. Un chat s'ouvre, attaché au projet. Tout le contexte choisi y sera disponible automatiquement.

9. Vous pouvez aussi lier un chat existant : ouvrez le chat, cliquez sur le menu en haut, puis **Lier à un projet** et sélectionnez votre projet.
   `[SCREENSHOT: en-tête de chat, menu déroulant avec option "Lier à un projet"]`

10. Pour vérifier que le contexte est bien injecté, posez une question spécifique : *"Quels fichiers ont changé cette semaine ?"* La réponse doit être précise et citer des fichiers réels.

> **Concept détaillé :** [book ch12 — Chat interactif](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch12-00-chat-interactif.md) (apprendre comment le contexte est utilisé par l'IA)

---

## Page 4.4 — Approuver ou refuser une action d'agent

**Prérequis**
- Un agent en cours d'exécution sur une tâche qui touche fichiers, commandes ou outils externes.
- Vous comprenez ce que l'agent est censé faire (la mission est claire pour vous).

**Étapes**

1. Dès qu'un agent veut effectuer une action sensible (écrire un fichier, lancer une commande, appeler un outil externe), une **carte d'approbation** apparaît :
   - en haut du chat si vous discutez avec l'agent,
   - dans l'**Inbox** (sidebar → Inbox) si l'agent tourne en arrière-plan.
   `[SCREENSHOT: carte ApprovalCard dans le chat, montrant "L'agent veut écrire dans ~/Rapports/digest.md" avec aperçu du contenu]`

2. Lisez attentivement le **type d'action**, le **chemin** ou la **commande** concernée, et l'**aperçu**. Apollia affiche systématiquement ce qui sera fait avant de le faire.

3. Trois choix s'offrent à vous :
   - **Approuver** — l'action s'exécute immédiatement, l'agent reprend.
   - **Refuser** — l'action est bloquée, l'agent reçoit l'information et adapte (ou s'arrête).
   - **Toujours autoriser** — case à cocher facultative qui crée une **règle de permission** durable, pour éviter de réapprouver le même type d'action à l'avenir.

4. Si vous cochez **Toujours autoriser**, précisez le **périmètre** :
   - Pour cet agent uniquement, ou pour tous les agents.
   - Pour ce dossier précis, ce dossier et ses sous-dossiers, ou tout chemin équivalent.
   `[SCREENSHOT: case "Toujours autoriser" ouverte, deux radio-boutons pour le périmètre]`

5. Cliquez sur **Approuver** (ou **Refuser**). L'action se déclenche (ou non) sans délai supplémentaire.

6. Pour voir et gérer les règles de permission existantes, allez dans **Settings → Règles de permission**. Vous pouvez modifier ou supprimer une règle à tout moment.

7. Pour consulter l'historique de toutes les approbations passées, ouvrez **Inbox → Approvals**. Tri par date, par agent, par type d'action.
   `[SCREENSHoT: page Approvals avec colonnes Date, Agent, Action, Statut]`

> **Référence technique :** [Securite-Guardrails](https://github.com/nidal-z/apollia-os/wiki/Securite-Guardrails) (modèle de permissions et garde-fous)

---

## Page 4.5 — Connecter un serveur MCP

**Prérequis**
- Vous savez quel outil métier vous voulez brancher (Notion, GitHub, Slack, base de données…).
- Vous avez les **identifiants** ou **token** d'accès nécessaires à cet outil.
- Connexion internet active (pour parcourir le catalogue).

**Étapes**

1. Dans la sidebar, cliquez sur **Intégrations**.

2. Onglet **Catalogue**. Filtrez par catégorie ou tapez le nom de l'outil cherché.
   `[SCREENSHOT: catalogue Intégrations, filtres Catégorie et niveau de confiance, recherche "Notion" tapée]`

3. Cliquez sur la carte du serveur souhaité. Vous voyez sa description, son auteur, son niveau de confiance (officiel, vérifié, communautaire), et la liste des outils qu'il expose.

4. Cliquez sur **Installer**. Apollia télécharge et prépare le serveur (quelques secondes).

5. Le **wizard de configuration** s'ouvre automatiquement.

6. Étape 1 du wizard — **Type de transport** :
   - **stdio** (recommandé) — le serveur tourne comme processus local, isolé, le plus sécurisé.
   - **HTTP** — le serveur tourne ailleurs, accessible via une URL.
   - **SSE** — pour les serveurs qui poussent des événements en streaming.
   `[SCREENSHOT: ConnectorWizard étape 1, trois cartes de transport]`

7. Étape 2 — **Identifiants**. Renseignez les paramètres demandés (token, URL, clé). Apollia les chiffre localement.

8. Étape 3 — **Tester la connexion**. Cliquez sur **Tester**. Un voyant vert apparaît avec la liste des outils détectés. Si le voyant est rouge, le message d'erreur indique précisément ce qui manque.
   `[SCREENSHOT: étape Test, voyant vert "Connecté", liste des outils détectés en dessous]`

9. Cliquez sur **Activer**. Le connecteur passe en statut **Actif** dans la liste de vos intégrations.

10. Pour utiliser le MCP depuis un chat, ouvrez une conversation avec un agent qui a la permission d'utiliser ses outils, et formulez votre demande en langage naturel (par exemple : *"Liste mes pages Notion récentes"*). L'agent appelle automatiquement les bons outils.

> **Concept détaillé :** [book ch04 — Les outils](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch04-00-les-outils.md) (apprendre comment l'IA décide d'appeler un outil MCP)

---

# 5. Script démo commerciale 30 min

**Contexte d'usage** : RDV prospect (visio ou présentiel), 30 minutes, objectif vendre une prestation de création d'agent sur mesure.
**Format** : Nidal seul, sans slides, écran partagé sur l'UI Apollia.
**Données préparées avant chaque RDV** : 3 environnements de démo prêts (un par scenario A/B/C), basculables en moins de 30 secondes.

---

## Phase 1 — Découverte (5 min)

Objectif : identifier le **pain point principal** du prospect pour choisir le scenario de démo le plus percutant. Trois questions, dans l'ordre.

| # | Question à poser | Ce qu'on écoute attentivement | Scenario démo recommandé |
|---|---|---|---|
| 1 | "Décrivez-moi une journée type. Qu'est-ce qui vous prend du temps que vous aimeriez ne plus faire ?" | Tâches **récurrentes** (rapports, mails, classements, veilles), volume estimable en heures/semaine. | **Scenario A** (triggers + agents) — automatisation pure. |
| 2 | "Quels sont les outils que votre équipe utilise au quotidien ? Et qu'est-ce qui circule mal entre eux ?" | Mention de **plusieurs outils** (Notion + Gmail + Slack…), frustrations de copier-coller, ruptures de chaîne d'information. | **Scenario B** (agents + MCP + mémoire) — intégration métier. |
| 3 | "Y a-t-il un processus que vous redoutez parce qu'il enchaîne trop d'étapes manuelles ? Lequel ?" | Description d'un **workflow complet** avec étapes décisionnelles, validation hiérarchique, livrable formel. | **Scenario C** (pipelines + HITL) — industrialisation processus. |

**Règle d'or** : ne pas insister, ne pas chercher à recouvrir les trois. La première réponse riche oriente le scenario. Les deux autres restent en réserve si le prospect rebondit. Si le prospect hésite ou évoque plusieurs choses, **par défaut Scenario A** — c'est celui qui se vend le mieux et le plus vite.

---

## Phase 2 — Démo adaptative (15 min)

Un seul scenario joué, choisi en fin de Phase 1. Les trois sont équivalents en ROI, le choix dépend du profil prospect.

### Scenario A — Tâches répétitives manuelles (cible : *Assistant Starter* 1 490 €HT)

**Données préparées** : agent `weekly-digest-agent` installé, dossier `~/Notes/` rempli, backend LLM connecté.

**Accroche (1 min)** :
> "Vous me disiez que [tâche X] vous prend [N heures] par semaine. Regardez ce que ça donne quand on confie ça à un agent qui tourne sur votre machine."

**Démo (12 min)** : exécuter intégralement la **Vidéo 1 — Triggers** (cf. Livrable 3), en remplaçant le rapport hebdo générique par un exemple inspiré du prospect.

**Atterrissage (2 min)** :
> "Là, vous avez vu la mécanique. Sur votre cas réel, on remplace cet agent générique par un agent qui *connaît* vos formats, vos sources, vos destinataires. C'est exactement ce que je conçois sur mesure."

---

### Scenario B — Veille / analyse / déléguation cognitive (cible : *Assistant Métier* 4 900 €HT)

**Données préparées** : agent `competitive-watch-agent` installé, MCP Notion + GitHub branchés, dossier `~/Veille/` prêt.

**Accroche (1 min)** :
> "Vous me disiez qu'entre [outil 1], [outil 2] et [outil 3], il y a beaucoup d'aller-retour manuels. Regardez ce que ça donne quand l'IA les enchaîne pour vous."

**Démo (12 min)** : combiner les **Vidéos 2 (Agents) + 5 (MCP) + 7 (Mémoire)**, condensées sur un cas unique :
- L'agent lit 3 sources (Notion + GitHub + web).
- Il synthétise dans un rapport structuré.
- Il mémorise les préférences de format données en cours de route.
- HITL sur l'écriture finale.

**Atterrissage (2 min)** :
> "Cet agent générique sait faire de la veille. Le vôtre saura veiller *votre* secteur, dans *vos* formats, sur *vos* sources, en cohérence avec *votre* base de connaissance interne. C'est l'objet de l'offre Métier."

---

### Scenario C — Workflows multi-étapes avec validation (cible : *Assistant Métier* 4 900 €HT ou *Sur mesure*)

**Données préparées** : pipeline `onboarding-client` défini, 4 agents workers spécialisés, dossier `~/Clients/` prêt.

**Accroche (1 min)** :
> "Vous me disiez que [processus X] enchaîne plusieurs étapes manuelles avec des validations entre. Regardez à quoi ça ressemble quand chaque étape devient un agent et que vous gardez la main sur les validations clés."

**Démo (12 min)** : exécuter intégralement la **Vidéo 6 — Pipelines** (cf. Livrable 3), en personnalisant le nom du pipeline et le client fictif avec un exemple du prospect.

**Atterrissage (2 min)** :
> "Le pipeline générique fait quatre étapes. Le vôtre en fera autant qu'il faut, branché à *vos* outils, avec les validations humaines pile aux bons endroits. C'est typiquement un Métier ou un projet Sur mesure selon la profondeur d'intégration."

---

## Phase 3 — ROI (5 min)

Calcul fait **avec le prospect**, pas devant lui. L'objectif est qu'il dise lui-même les chiffres.

**Script** :

1. **Quantifier le temps actuel** :
   > "On a vu [tâche démontrée]. Aujourd'hui, sans agent, combien d'heures par semaine cette tâche-là — ou ses équivalents — vous prend, à vous ou à votre équipe ?"

   *Laisser répondre. Reformuler haut et fort le chiffre.*

2. **Estimer le facteur de gain** :
   > "Sur la démo qu'on vient de faire, on était à [X minutes humaines] vs [Y minutes agent]. Disons que sur votre cas réel, l'agent récupère 70 à 80% du temps. C'est conservateur."

3. **Calcul devant le prospect** :
   > "Donc si vous y passez [N heures] par semaine aujourd'hui, l'agent vous en rend [N × 0,75]. Sur l'année, ça fait [N × 0,75 × 45 semaines de travail] heures. À [taux horaire chargé du prospect, demandé poliment] €, ça représente [montant] € par an."

4. **Recadrer sur le coût** :
   > "L'investissement initial sur l'offre [Starter à 1 490 € / Métier à 4 900 €] est rentabilisé en [montant ÷ tarif] semaines. Tout ce qui suit est gain net."

5. **Le coup de grâce** :
   > "Et c'est sans compter ce qu'on libère de cognitif : moins d'erreurs, moins de tâches qui s'accumulent, plus de temps pour le travail à forte valeur."

**Règle** : ne pas inventer les chiffres. Si le prospect refuse de communiquer son taux horaire, donner une fourchette indicative (50 à 150 € chargé selon le métier).

---

## Phase 4 — Offre (5 min)

Présentation directe, sans détour. Format : process, formats, lancement.

### Process en 3 étapes

> "Voilà comment on travaille ensemble :
>
> **Étape 1 — Appel de qualification (30 min, ce qu'on est en train de faire).** Vous me décrivez votre activité, je pose des questions précises, on identifie ce qu'un agent peut vraiment vous faire gagner. À la fin, vous savez si Apollia est la bonne réponse — et moi aussi.
>
> **Étape 2 — Devis forfaitaire sous 48 heures.** Si on continue, je vous envoie un devis détaillé sous 48h. Prix fixe, périmètre clair, délai engageant. Pas de facturation au temps passé. Vous validez ou vous n'y revenez pas, sans engagement.
>
> **Étape 3 — Livraison, support et garantie.** Je développe votre assistant, je le teste sur vos vrais fichiers, je vous le livre installé et documenté. Deux engagements après livraison : 30 jours de support premium (réponses sous 24h ouvrées), et 90 jours de garantie conformité — si l'assistant ne fait pas ce qui a été contractualisé, je corrige sans surcoût."

### Trois formats

| Format | Tarif | Délai | Cible |
|---|---|---|---|
| **Starter** | dès **1 490 €HT** forfait | 1 à 2 semaines | Une tâche bien définie, sans complexité particulière. *Exemples : tri et priorisation de mails, veille quotidienne sur 5-10 sources, classement automatique de documents.* 30 jours de support inclus. |
| **Métier** ★ Le plus demandé | dès **4 900 €HT** forfait | 3 à 5 semaines | Un process complet de bout en bout, mémoire longue, validation humaine, intégration avancée. *Exemples : préparation automatique de devis, qualification et enrichissement de leads, comptes rendus structurés.* 60 jours de support inclus. |
| **Sur mesure** | sur devis après cadrage | variable | Plusieurs assistants qui collaborent, intégration SI (ERP, CRM), formation et accompagnement étendu. *Cas typiques : déploiement d'un parc d'assistants en PME, intégration ERP propriétaire, équipe IA interne accompagnée.* |

### Offre de lancement (5 premiers clients)

> "Apollia OS vient d'ouvrir ses portes. Pour les **5 premiers clients**, package de lancement :
> - **−30 % sur la première mission**, quel que soit le format choisi.
> - **Onboarding personnalisé** : installation accompagnée, tour complet d'Apollia, présentation du mode Chat libre, mise en main de votre assistant sur mesure.
> - **Masterclass *IA au quotidien* (2h)** pour vous et jusqu'à 4 collaborateurs.
>
> En échange, un **retour public en fin de mission** (témoignage court ou étude de cas, à votre accord). Vous participez à construire les premiers cas documentés d'Apollia. Vous y gagnez financièrement et en maîtrise IA. Nous y gagnons en crédibilité."

### Closing

> "Concrètement : on continue sur cette idée, je vous prépare le devis sous 48 heures avec un périmètre précis ?"

*Silence. Laisser le prospect répondre.*

---

## Aide-mémoire timing

| Phase | Durée cible | Marge |
|---|---|---|
| Découverte | 5 min | déborder OK jusqu'à 7 min si signal riche |
| Démo | 15 min | strict — couper si on traîne |
| ROI | 5 min | accélérer si prospect convaincu visiblement |
| Offre | 5 min | strict — pas de digression sur les détails techniques |
| **Total** | **30 min** | viser 28 min pour laisser 2 min de Q&A |

---

# 6. Checklist QA parcours opérateur

**Usage** : à exécuter par Nidal **avant chaque démo publique ou release**. Cible : exécution complète en moins de 30 minutes.
**Format** : ☐ [SECTION] Action | Critère de succès | Bloquant (oui/non).
**Convention bloquant** :
- **Oui** = on ne lance pas la démo / la release tant que ce n'est pas vert.
- **Non** = on peut lancer, mais on connaît le défaut et on l'évite ou on le mentionne.

---

### 1. Onboarding

- ☐ [ONBOARDING] Lancer Apollia sur une machine vierge | Le wizard `Onboarding.svelte` s'ouvre automatiquement | **Oui**
- ☐ [ONBOARDING] Choisir profil "Operator" → étape suivante | Transition fluide, pas de message d'erreur | Oui
- ☐ [ONBOARDING] Tour spotlight pointe Sidebar / Chat / Agents / Settings | Les 4 zones sont surlignées dans l'ordre, sans overlap | Non
- ☐ [ONBOARDING] Premier message envoyé via wizard reçoit une vraie réponse streamée | La réponse s'affiche en moins de 5 secondes | Oui
- ☐ [ONBOARDING] Bouton "Installer mon premier agent" en fin de wizard mène à `/Agents` catalogue | Page catalogue affichée, pas la page vide | Non

### 2. LLM (connexion fournisseur)

- ☐ [LLM] Settings → LLM Backends → ajouter Anthropic avec clé valide | Voyant vert "Connecté", latence affichée | **Oui**
- ☐ [LLM] Ajouter Anthropic avec clé invalide | Message d'erreur clair "Clé invalide ou révoquée" | Oui
- ☐ [LLM] Ajouter Ollama local | Détection automatique du serveur sur localhost:11434, voyant vert | Non
- ☐ [LLM] Bandeau supérieur affiche le backend actif et son statut | Indicateur cohérent avec la page Settings | Oui
- ☐ [LLM] Switch de backend en cours de conversation | Le chat continue sans crash, message info "Backend changé" | Non

### 3. Chat libre

- ☐ [CHAT] Ouvrir un nouveau chat (assistant standard) | Page `/Chat` s'ouvre, champ de saisie focus | **Oui**
- ☐ [CHAT] Envoyer un message court | Streaming visible caractère par caractère, markdown rendu | **Oui**
- ☐ [CHAT] Envoyer un message qui implique un appel d'outil (ex : "lis README.md") | Carte HITL apparaît, action exécutée après approbation, résultat dans le chat | Oui
- ☐ [CHAT] Bouton "Annuler" pendant streaming | Réponse arrêtée proprement, message marqué "Annulé" | Non
- ☐ [CHAT] Recharger la page | Conversation persistée, scroll au bon endroit | Oui

### 4. Projets et context providers

- ☐ [PROJETS] Créer un nouveau projet pointant vers un dossier git existant | Projet apparaît dans la liste, page de détail accessible | **Oui**
- ☐ [PROJETS] Activer provider Git → preview montre les commits récents | Commits réels du repo affichés, pas vide | Oui
- ☐ [PROJETS] Activer provider Arborescence → preview montre les fichiers/dossiers | Structure cohérente avec le disque | Oui
- ☐ [PROJETS] Lier un nouveau chat au projet | Chat ouvert avec snapshot contexte visible dans `ContextDrawer` | Oui
- ☐ [PROJETS] Question contextuelle ("qu'est-ce qui a changé ?") → réponse cite des éléments réels | Pas de réponse générique vague | Oui

### 5. Agents

- ☐ [AGENTS] Page `/Agents` affiche les agents installés en deux sections (Worker / Assistant) | Catégorisation correcte | **Oui**
- ☐ [AGENTS] Catalogue → installer un agent | Téléchargement, apparition dans "Mes agents", statut "Installé" | Oui
- ☐ [AGENTS] Démarrer un assistant → bouton "Ouvrir le chat" actif | Chat dédié s'ouvre | **Oui**
- ☐ [AGENTS] Volet `AgentLogs` accessible et peuplé pendant exécution | Logs streamés en temps réel | Oui
- ☐ [AGENTS] Volet `ReasoningSequence` montre les étapes du raisonnement | Étapes lisibles, ordre cohérent | Oui

### 6. Mémoire

- ☐ [MÉMOIRE] Page `/Memory` affiche les 4 onglets (User, Episodic, Semantic, Procedural, Tools) | Onglets accessibles | Oui
- ☐ [MÉMOIRE] Ajouter une préférence via chat ("Note ma préférence : X") | Carte HITL d'enregistrement, entrée visible dans onglet User après approbation | **Oui**
- ☐ [MÉMOIRE] Recherche FTS sur un mot-clé existant | Résultat retourné, surlignage du match | Oui
- ☐ [MÉMOIRE] Supprimer une entrée | Confirmation demandée, entrée disparaît, mémoire utilisateur à jour | Oui
- ☐ [MÉMOIRE] Préférence appliquée dans un nouveau chat sans rappel | Comportement IA conforme à la préférence enregistrée | **Oui**

### 7. Triggers

- ☐ [TRIGGERS] Page `/Triggers` accessible, liste affichée | Pas de crash, pas d'erreur réseau | Oui
- ☐ [TRIGGERS] Créer un trigger Cron via `CreateTriggerDialog` | Trigger apparaît dans la liste, prochain fire calculé correctement | **Oui**
- ☐ [TRIGGERS] Bouton "Déclencher maintenant" sur un trigger | Exécution démarre, status passe à `running` puis `completed` | **Oui**
- ☐ [TRIGGERS] Onglet Historique d'un trigger | Au moins 1 entrée si déjà déclenché, durée et statut visibles | Oui
- ☐ [TRIGGERS] Créer un trigger File Watch pointant vers un dossier | Modifier un fichier dans ce dossier déclenche l'agent | Non

### 8. Pipelines

- ☐ [PIPELINES] Page `/Pipelines` affiche définitions / actifs / historique | Trois onglets accessibles | Oui
- ☐ [PIPELINES] Lancer un pipeline depuis l'onglet Définitions | Vue temps réel, étapes passent en `running` | **Oui**
- ☐ [PIPELINES] Étape HITL bloque l'exécution | Status `awaiting-approval`, carte d'approbation visible | **Oui**
- ☐ [PIPELINES] Approuver l'étape HITL | Pipeline reprend, étapes suivantes s'exécutent | Oui
- ☐ [PIPELINES] Pipeline terminé avec timeline complète dans Historique | Toutes les étapes loggées, durée totale affichée | Oui

### 9. HITL (contrôle humain)

- ☐ [HITL] Action filesystem (write) déclenche carte HITL | Carte affiche chemin, aperçu contenu, boutons Approuver/Refuser | **Oui**
- ☐ [HITL] Action bash déclenche carte HITL | Commande complète visible avant exécution | **Oui**
- ☐ [HITL] Action MCP (appel outil externe) déclenche carte HITL | Nom outil + paramètres visibles | **Oui**
- ☐ [HITL] Cocher "Toujours autoriser ce type d'opération" | Règle créée, visible dans `/SettingsPermissionRules` | Oui
- ☐ [HITL] Action couverte par règle → pas de carte HITL, exécution directe | Comportement automatique conforme à la règle | Oui
- ☐ [HITL] Refuser une action | Agent reçoit le refus, gère proprement (continue ou s'arrête) | Oui

### 10. MCP (intégrations)

- ☐ [MCP] Catalogue accessible, filtres catégorie / niveau de confiance fonctionnels | Filtres réduisent la liste | Oui
- ☐ [MCP] Installer un MCP simple (ex : filesystem ou test) | Installation aboutit, wizard de config s'ouvre | **Oui**
- ☐ [MCP] Tester la connexion d'un MCP avec credentials valides | Voyant vert, liste des outils affichée | **Oui**
- ☐ [MCP] Tester avec credentials invalides | Voyant rouge, message d'erreur précis | Oui
- ☐ [MCP] Activer un MCP → outils utilisables depuis chat | Agent appelle l'outil avec succès | **Oui**

### 11. STT (dictée vocale)

- ☐ [STT] Settings STT → modèle Whisper local installé et chargé | Statut "Modèle prêt" affiché | Oui
- ☐ [STT] Hotkey configuré et fonctionnel | Maintien hotkey ouvre `RecordingOverlay` | **Oui**
- ☐ [STT] Dicter une phrase courte en français | Transcription correcte injectée dans le champ actif en moins de 2s | **Oui**
- ☐ [STT] Mode push-to-talk vs toggle | Les deux modes fonctionnent comme attendu | Non
- ☐ [STT] Mode presse-papier global activé → dictée hors Apollia | Texte transcrit collable dans une autre app | Non

### 12. Notifications

- ☐ [NOTIFS] Configurer un canal Desktop | Canal apparaît dans la liste, statut "Actif" | **Oui**
- ☐ [NOTIFS] Configurer un canal Webhook (URL valide) | Test webhook reçoit un payload réel | Oui
- ☐ [NOTIFS] Sélectionner les événements notifiés (task completed, approval required…) | Sauvegarde persistée | Oui
- ☐ [NOTIFS] Déclencher un événement → notification reçue sur le bon canal | Notif desktop affichée OU webhook appelé | **Oui**
- ☐ [NOTIFS] Filtrer les notifs par agent / sévérité | Filtres respectés | Non

### 13. Observabilité

- ☐ [OBS] Page Dashboard affiche `DigestHero` avec chiffres cohérents | 3 chiffres clés visibles | **Oui**
- ☐ [OBS] `CompletedTodayBlock` liste des actions du jour | Au moins 1 entrée si activité, sinon état vide propre | Oui
- ☐ [OBS] Page `/Observability` → onglet "LLM costs" → `LlmCostChart` peuplé | Courbe sur 30 derniers jours, valeurs > 0 si activité | **Oui**
- ☐ [OBS] Onglet "Audit trail" → liste filtrable par agent | Filtres opérationnels, lignes cliquables vers détail | Oui
- ☐ [OBS] Drill-down sur une ligne d'audit → conversation source accessible | Navigation vers le chat correspondant | Non

### 14. Démo commerciale (préparation)

- ☐ [DÉMO] Données scenario A prêtes (`weekly-digest-agent` + `~/Notes/`) | Test à blanc trigger → succès | **Oui**
- ☐ [DÉMO] Données scenario B prêtes (`competitive-watch-agent` + MCP Notion + GitHub) | Connexions vertes, agent répond à un test simple | **Oui**
- ☐ [DÉMO] Données scenario C prêtes (pipeline `onboarding-client` + 4 workers) | Lancement test → toutes les étapes vert | **Oui**
- ☐ [DÉMO] Bandeau supérieur sans erreur de connexion LLM | Voyant vert visible | **Oui**
- ☐ [DÉMO] Notifications desktop activées sur la machine | Test rapide via System Settings macOS / Windows | Oui
- ☐ [DÉMO] Mode Ne Pas Déranger désactivé | Pas de notif système parasite à venir | Oui
- ☐ [DÉMO] Résolution écran et zoom adaptés à la visio | Lisibilité validée par capture test | Oui
- ☐ [DÉMO] Scenario sélectionnable en moins de 30 secondes | Bascule entre A/B/C testée | Non
- ☐ [DÉMO] Scenario complet rejoué en moins de 15 minutes | Chrono à blanc validé | **Oui**

---

**Score cible avant Go** :
- 100 % des items **Bloquant Oui** au vert.
- ≥ 80 % des items **Bloquant Non** au vert.
- Tout item rouge en bloquant Non est consigné dans une note "défauts connus" pour ne pas être surpris pendant la démo.
