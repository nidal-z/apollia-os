# Apollia Operator Glossary

Ce glossaire est la source de vérité pour le vocabulaire visible à l'utilisateur
selon le mode UI (`operator` vs `builder`). Toute nouvelle clé i18n
utilisateur-facing doit être validée ici avant d'atterrir dans `fr.json` /
`en.json`. Les termes builder restent techniques ; les termes operator restent
métaphoriques et humains.

Convention i18n : les clés suivent `operator.<domaine>.<label>` pour le mode
operator et `builder.<domaine>.<label>` pour le mode builder. Les clés
partagées (tooltips généraux, libellés de statuts) restent sous leur namespace
d'origine.

## Règles

- Le mode operator ne montre **jamais** : `version`, `install_path`,
  `tool_call_id`, `event_id`, `task_id` brut, `agent_id` brut, `namespace`
  technique, chemins Python, identifiants de trigger cron bruts.
- Le mode builder garde **tous** les termes techniques (aucune simplification).
- Chaque nouveau terme utilisateur-facing passe d'abord ici.

## Termes

| Concept            | Operator (FR)           | Operator (EN)         | Builder (FR / EN)        |
| ------------------ | ----------------------- | --------------------- | ------------------------ |
| Agent              | Assistant               | Assistant             | Agent                    |
| Agent worker A2A   | Assistant spécialisé    | Specialized assistant | Worker (A2A)             |
| Trigger            | Automatisation          | Automation            | Trigger                  |
| Trigger cron       | Automatisation planifiée| Scheduled automation  | Cron trigger             |
| Trigger webhook    | Automatisation externe  | External automation   | Webhook trigger          |
| Trigger filewatch  | Surveillance de fichiers| File watch            | Filewatch trigger        |
| Trigger interval   | Automatisation récurrente| Recurring automation | Interval trigger         |
| Pipeline           | Automatisation avancée  | Advanced automation   | Pipeline                 |
| MCP server         | Connexion               | Connection            | MCP server               |
| MCP tool           | Outil                   | Tool                  | MCP tool                 |
| Tool call          | Action                  | Action                | Tool call                |
| Tool call id       | -                       | -                     | tool_call_id             |
| Event id           | -                       | -                     | event_id                 |
| Task id            | -                       | -                     | task_id                  |
| Agent id           | -                       | -                     | agent_id                 |
| HITL approval      | Demande de validation   | Approval request      | HITL approval            |
| Approval queue     | Boîte de réception      | Inbox                 | Approvals inbox          |
| Approval resolved  | Décision prise          | Decision made         | Resolved approval        |
| Activity           | Mon travail             | My work               | Activity                 |
| Task               | Tâche                   | Task                  | Task                     |
| Task running       | En cours                | In progress           | Running                  |
| Task submitted     | Démarrée                | Started               | Submitted                |
| Task queued        | En attente              | Queued                | Queued                   |
| Task failed        | Échec                   | Failed                | Failed                   |
| Task canceled      | Annulée                 | Canceled              | Canceled                 |
| Memory namespace   | Mémoires                | Memories              | Memory namespace         |
| Memory episodic    | Souvenirs               | Memories              | Episodic memory          |
| Memory semantic    | Connaissances           | Knowledge             | Semantic memory          |
| Memory procedural  | Routines                | Routines              | Procedural memory        |
| Templates          | Modèles                 | Templates             | Templates                |
| Pipeline template  | Modèle de scénario      | Scenario template     | Pipeline template        |
| Agent template     | Modèle d'assistant      | Assistant template    | Agent template           |
| Fire (trigger)     | Lancer                  | Run                   | Fire                     |
| Run (task)         | Démarrer                | Start                 | Submit                   |
| Cancel             | Annuler                 | Cancel                | Cancel                   |
| Retry              | Réessayer               | Retry                 | Retry                    |
| Install path       | -                       | -                     | install_path             |
| Agent version      | -                       | -                     | version                  |
| Agent manifest     | Fiche d'assistant       | Assistant card        | Manifest                 |
| Runtime status     | État                    | Status                | Runtime status           |
| LLM backend        | Modèle IA               | AI model              | LLM backend              |
| LLM router         | Routeur de modèles      | Model router          | LLM router               |
| Token count        | Longueur                | Length                | Tokens                   |
| Context drawer     | Panneau contextuel      | Context panel         | Context drawer           |
| Companion          | Compagnon               | Companion             | Companion                |
| Onboarding         | Découverte              | Discovery             | Onboarding               |
| Transcription      | Transcription           | Transcription         | STT transcript           |
| STT backend        | Moteur vocal            | Voice engine          | STT backend              |
| Notification       | Notification            | Notification          | Notification             |
| Trigger schedule   | Programmation           | Schedule              | Cron expression          |
| Trigger condition  | Condition               | Condition             | Predicate                |
| Pipeline step      | Étape                   | Step                  | Stage                    |
| Pipeline fan-out   | Distribution            | Fan-out               | Fan-out                  |
| Pipeline HITL      | Point de validation     | Approval point        | HITL stage               |
| Observability      | Suivi                   | Monitoring            | Observability            |
| Event bus          | -                       | -                     | Event bus                |
| Supervisor         | -                       | -                     | Supervisor               |
| Resilience layer   | -                       | -                     | Resilience layer         |
| Step budget        | Quota d'étapes          | Step quota            | Step budget              |
| Audit trail        | Journal d'activité      | Activity log          | Audit trail              |
| Sandbox            | Bac à sable             | Sandbox               | Sandbox                  |
| Permission rule    | Règle d'accès           | Access rule           | Permission rule          |
| Projects           | Projets                 | Projects              | Projects                 |
| Chat session       | Conversation            | Conversation          | Session                  |
| Apollia Guide      | Apollia                 | Apollia               | Apollia Guide            |
| Next Step          | Prochaine étape         | Next step             | Next step                |
| Digest (daily)     | Résumé du jour          | Daily digest          | Daily digest             |
| Dashboard          | Accueil                 | Home                  | Dashboard                |

## Termes interdits en mode operator

Les éléments suivants ne doivent jamais apparaître dans la colonne operator ;
ils doivent être wrappés dans `<BuilderOnly>` dans les composants partagés :

- `version`, `v1.2.3`, `install_path`
- `tool_call_id`, `event_id`, `task_id`, `agent_id`
- `namespace` brut (toujours exposé comme "Mémoires de X")
- Codes cron (`0 9 * * 1-5`) - affichés en langage naturel en operator
- Stack traces complets - remplacés par un message utilisateur + CTA builder

## Processus d'ajout d'un terme

1. Ouvrir ce fichier et ajouter la ligne dans le tableau approprié.
2. Ajouter la clé i18n correspondante dans `fr.json` + `en.json` sous
   `operator.*` ou `builder.*`.
3. Si le composant est partagé, wrapper les sections techniques dans
   `<BuilderOnly>` / `<OperatorOnly>`.
4. Exécuter `pnpm test` pour vérifier que les tests i18n (`i18n-locale-switch`,
   `i18n-tools`) restent verts.
