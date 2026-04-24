# Approuver ou refuser une action d'agent

> Pour les operators qui veulent garder la main sur chaque action sensible déclenchée par un agent (écriture, commande, appel d'outil externe).

## Prérequis

- Un agent en cours d'exécution sur une tâche qui touche fichiers, commandes ou outils externes.
- Vous comprenez ce que l'agent est censé faire (la mission est claire pour vous).

## Étapes

1. Dès qu'un agent veut effectuer une action sensible, une **carte d'approbation** apparaît :
   - en haut du chat si vous discutez avec l'agent,
   - dans l'**Inbox** (sidebar → Inbox) si l'agent tourne en arrière-plan.
   `[SCREENSHOT: ApprovalCard dans le chat, titre "L'agent veut écrire dans ~/Rapports/digest.md", aperçu du contenu sur 10 lignes, trois boutons en bas]`

2. Lisez attentivement le **type d'action**, le **chemin** ou la **commande** concernée, et l'**aperçu**. Apollia affiche systématiquement ce qui sera fait avant de le faire.

3. Trois choix s'offrent à vous :
   - **Approuver** — l'action s'exécute immédiatement, l'agent reprend.
   - **Refuser** — l'action est bloquée, l'agent reçoit l'information et adapte (ou s'arrête).
   - **Toujours autoriser** — case facultative qui crée une règle de permission durable.

4. Si vous refusez, un dialog **Raison du refus** s'ouvre. Saisissez une explication courte (par exemple : *Pas le bon dossier* ou *Contenu incorrect*). C'est facultatif mais cela aide l'agent à corriger son comportement.
   `[SCREENSHOT: dialog RejectReasonDialog avec textarea pré-rempli "Pas le bon dossier", bouton Refuser en bas à droite]`

5. Si vous cochez **Toujours autoriser**, précisez le **périmètre** :
   - Pour cet agent uniquement, ou pour tous les agents.
   - Pour ce dossier précis, ce dossier et ses sous-dossiers, ou tout chemin équivalent.
   - Pour ce type d'opération uniquement (écrire, lire, exécuter).

6. Cliquez sur **Approuver** ou **Refuser**. L'action se déclenche (ou non) sans délai supplémentaire.

7. Pour voir et gérer les règles de permission existantes, allez dans **Settings → Permissions**. Vous pouvez modifier ou supprimer une règle à tout moment (voyez la page *Configurer les permissions de fichiers*).

8. Pour consulter l'historique de toutes les approbations passées, ouvrez **Inbox → Approvals**. Tri par date, par agent, par type d'action.
   `[SCREENSHOT: page Inbox > onglet Approvals, tableau avec colonnes Date, Agent, Action, Décision, Raison]`

## Vérification

Une fois la décision prise, la carte disparaît du chat (ou de l'Inbox) et l'action est tracée dans l'historique des approbations. Si vous avez créé une règle, les actions équivalentes futures seront approuvées automatiquement.

## Si ça ne marche pas

- **Aucune carte n'apparaît alors que l'agent semble bloqué** : vérifiez l'**Inbox**. Les agents qui tournent en arrière-plan y déposent leurs demandes.
- **L'agent réessaie sans cesse la même action refusée** : ouvrez ses logs depuis la page **Agents**, votre raison de refus y est visible et peut éclairer le comportement.
- **Une règle "Toujours autoriser" déclenche trop d'actions** : ouvrez **Settings → Permissions** et supprimez ou affinez le périmètre de la règle.

> **Concept :** [book ch10 — HITL (Human-In-The-Loop)](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch10-00-hitl.md)
