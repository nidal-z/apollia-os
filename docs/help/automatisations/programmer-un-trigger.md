# Programmer un trigger

> Pour les operators qui veulent qu'une tâche IA s'exécute toute seule, à heure fixe ou sur événement, sans intervention manuelle.

## Prérequis

- Au moins un agent installé et démarrable depuis la page Agents.
- Un fournisseur d'IA connecté (la connexion est verte dans le bandeau supérieur).
- Vous savez à quelle fréquence vous voulez que la tâche se répète.

## Étapes

1. Dans la sidebar, cliquez sur **Automatisations**.

2. Cliquez sur le bouton **Créer une automatisation** en haut à droite.
   `[SCREENSHOT: page Automatisations, bouton "Créer une automatisation" surligné en haut à droite]`

3. Donnez un nom clair à votre automatisation (par exemple : *Rapport hebdo lundi*). Ce nom apparaîtra partout dans l'interface et dans les notifications.

4. Choisissez le **type de déclenchement** :
   - **Sur un calendrier** — pour une fréquence régulière complexe (tous les lundis à 8h, le 1er du mois à 6h…).
   - **À intervalle régulier** — pour une répétition simple (toutes les 30 minutes, toutes les heures, tous les jours).
   - **Une seule fois** — pour une seule exécution programmée.
   - **Quand un fichier change** — déclencher quand un fichier est créé, modifié ou supprimé.
   - **Via une URL externe** — déclencher sur un appel HTTP entrant.

5. Saisissez le paramètre du déclenchement choisi. Pour le type **Sur un calendrier**, une aide statique rappelle la syntaxe : `min heure jour mois jour-semaine`.
   `[SCREENSHOT: modal Créer une automatisation, type "Sur un calendrier" sélectionné, champ expression cron, aide syntaxe affichée en gris]`

   > **Note :** Apollia n'affiche pas de traduction en langage naturel de l'expression cron. Vérifiez votre expression avec un outil externe si besoin (par exemple `crontab.guru`).

6. Sélectionnez l'**agent cible** dans la liste déroulante. Seuls les agents installés apparaissent.

7. (Optionnel) Renseignez un **payload** — un texte qui sera transmis à l'agent au déclenchement. Vide par défaut, l'agent suit son comportement standard.

8. Cliquez sur **Créer**. L'automatisation apparaît dans la liste, prête à se déclencher.

9. Pour vérifier que tout fonctionne, cliquez sur **Déclencher maintenant** sur la ligne de l'automatisation. Une exécution se lance immédiatement.
   `[SCREENSHOT: liste des automatisations, ligne "Rapport hebdo lundi" avec bouton Déclencher maintenant à droite]`

10. Suivez l'exécution en cliquant sur **Historique**. Vous voyez la durée, le statut et le lien vers les détails en cas de problème.

## Vérification

L'automatisation figure dans la liste avec un voyant vert et un compteur **Prochaine exécution** affiche bien la date prévue.

## Si ça ne marche pas

- **L'agent cible n'apparaît pas dans la liste** : il n'est pas installé ou pas démarrable, retournez sur la page Mes assistants.
- **L'expression cron est refusée** : vérifiez la syntaxe (`min heure jour mois jour-semaine`) et utilisez `crontab.guru` pour valider.
- **Le déclenchement immédiat ne fait rien** : vérifiez que le fournisseur d'IA est connecté (bandeau vert en haut).

> **Référence technique :** [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers) — table complète des types et expressions supportées.
