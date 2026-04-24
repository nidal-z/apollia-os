# Consulter les logs d'un agent

> Pour tout operator qui veut comprendre ce qu'un agent a fait, ou pourquoi il a échoué : ouvrir, filtrer et lire son journal d'activité.

## Prérequis

- L'agent est installé et a été démarré au moins une fois.
- Idéalement, l'agent a déjà exécuté une mission (au moins quelques entrées de logs disponibles).

## Étapes

1. Dans la sidebar, cliquez sur **Agents**, puis ouvrez l'onglet **Mes agents**.

2. Localisez la carte de l'agent dont vous voulez consulter les logs.

3. Cliquez sur **Logs** sur sa carte. Un panneau s'ouvre à droite, listant les entrées chronologiques.
   `[SCREENSHOT: panneau Logs ouvert avec liste d'entrées colorées par sévérité, colonnes Date, Niveau, Message]`

4. Filtrez par **niveau** en haut du panneau :
   - **Info** — déroulement normal.
   - **Warning** — comportement inattendu mais non bloquant.
   - **Error** — échec d'une étape.

5. Tapez un mot-clé dans la barre de recherche pour filtrer le contenu des messages (par exemple : *timeout*, *Notion*, le nom d'un fichier).
   `[SCREENSHOT: panneau Logs avec filtre "Error" actif et recherche "timeout" tapée, 3 résultats affichés]`

6. Cliquez sur une ligne pour afficher le **contenu complet** du message, y compris la pile d'erreur si présente.

7. (Optionnel) Cliquez sur l'icône **Copier** à côté d'une entrée pour la coller dans un ticket de support ou un fichier de débogage.

8. Repérez la première entrée **Error** chronologiquement : c'est presque toujours là que se trouve la cause d'un problème. Les erreurs suivantes en sont souvent la conséquence.

9. Fermez le panneau pour revenir à la liste des agents.

## Vérification

Vous voyez la liste des actions de l'agent avec leur horodatage. Une erreur récente est immédiatement identifiable et son message est compréhensible.

## Si ça ne marche pas

- **Aucun log affiché :** l'agent n'a jamais été démarré. Lancez-le et envoyez-lui une mission.
- **Logs vides après une exécution :** vérifiez que l'agent a bien démarré (statut vert sur sa carte).
- **Erreur incompréhensible :** copiez le message et consultez [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md).

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug) — interprétation des codes d'erreur, dépannage agent bloqué ou en timeout.
