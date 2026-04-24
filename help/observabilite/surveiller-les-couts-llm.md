# Surveiller les coûts d'IA

> Pour les operators qui veulent suivre la dépense des appels à leur fournisseur d'IA et identifier les jours, agents ou conversations qui pèsent le plus dans la facture.

## Prérequis
- Au moins une conversation ou un agent a déjà appelé votre fournisseur d'IA.
- Vous utilisez un fournisseur facturé (Anthropic, OpenAI, Bedrock…). Les modèles locaux n'apparaissent pas dans les coûts.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Coûts**.

2. La courbe principale affiche votre dépense quotidienne sur les 30 derniers jours. L'axe vertical est en dollars, l'axe horizontal est la date.
   `[SCREENSHOT: onglet Coûts, courbe sur 30 jours, légende par modèle à droite]`

3. Survolez un point haut pour voir le total de la journée. La légende sur le côté répartit la dépense par modèle utilisé.

4. Cliquez sur un jour précis pour ouvrir le détail. Vous voyez la liste des conversations de ce jour, avec pour chacune les jetons consommés, l'agent impliqué et le coût.

5. Cliquez sur une conversation pour voir le détail message par message, avec le prix de chaque appel.
   `[SCREENSHOT: vue détail d'une journée, liste de conversations avec colonnes Agent, Jetons, Coût]`

6. En bas de la page, trois indicateurs résument le mois : **Total**, **Moyenne par jour**, **Jour le plus cher**.

7. Pour être prévenu si la dépense dépasse un seuil, allez dans **Settings → Fournisseur d'IA** et renseignez un **seuil d'alerte mensuel**. Une notification s'affichera dès que la limite sera approchée.

## Vérification
La courbe couvre bien les 30 derniers jours, et les chiffres en bas correspondent à votre intuition de la consommation.

## Si ça ne marche pas
- **La courbe est vide** : aucun appel facturé n'a été enregistré. Vérifiez que votre fournisseur n'est pas un modèle local.
- **Les coûts paraissent trop élevés** : ouvrez la conversation la plus chère et regardez la taille du contexte injecté. Un projet avec beaucoup de fichiers peut gonfler les jetons d'entrée.
- **Aucune alerte ne se déclenche** : vérifiez que le seuil dans Settings est bien actif (interrupteur vert) et que votre canal de notification par défaut fonctionne.

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug)
