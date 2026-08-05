# Surveiller les coûts d'IA

> Pour les operators qui veulent suivre la dépense des appels à leur fournisseur d'IA sur la semaine écoulée.

## Prérequis

- Au moins une conversation ou un agent a déjà appelé votre fournisseur d'IA.
- Vous utilisez un fournisseur facturé (Anthropic, OpenAI, Bedrock, Vertex…). Les modèles locaux n'apparaissent pas dans les coûts.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Coûts LLM**.

2. En haut à droite de la carte, un **sélecteur de période** vous permet de basculer entre **7 j / 14 j / 30 j / 90 j / 1 an**. Tous les indicateurs, le graphique et la légende se recalculent instantanément sur la nouvelle fenêtre. La densité de l'axe horizontal s'adapte automatiquement (étiquettes thinnées au-delà de 14 jours).

3. En haut, **quatre indicateurs clés** (KPI) résument la fenêtre sélectionnée :
   - **Total 7 jours** - le libellé est figé et annonce sept jours quelle que soit la période choisie. Lisez-le comme la somme sur la fenêtre, pas sur une semaine.
   - **Moyenne / jour** - total divisé par le nombre de jours de la fenêtre.
   - **Jour le plus cher** - montant + date du jour qui a le plus consommé.
   - **Backend principal** - nom du backend qui pèse le plus, avec son total cumulé.
   ![Onglet Couts LLM, selecteur de periode, quatre KPI, graphique en barres empilees et legende des backends](/img/operator-help/observabilite-surveiller-les-couts-llm-1.png)

4. Au centre, un **histogramme empilé** présente la période sélectionnée. Une barre par jour, chaque barre découpée en segments colorés par **backend** (Anthropic, OpenAI, etc.). L'axe vertical est en dollars avec des ticks arrondis ; l'axe horizontal indique la date (jour de la semaine + date courte pour les courtes fenêtres, date seule pour 30 j et plus).

5. Survolez une colonne : les autres jours s'estompent légèrement et le **total du jour** s'affiche au-dessus de la barre. Un tooltip apparaît aussi sur chaque segment avec le **nom du backend** et son **coût exact** (ex. `anthropic: $0.42 - May 11`).

6. Sous le graphique, la **légende** liste tous les backends actifs sous forme de **pastilles** affichant le **total cumulé par backend** sur la fenêtre. Permet de comparer la part de chaque fournisseur d'un coup d'œil.

## Le seuil d'alerte, et où le lire

Le graphique dit ce que vous avez dépensé. Il ne dit pas ce que vous avez décidé d'autoriser. Ce plafond, c'est `cost_alert_threshold_usd`, dans la section `[llm]` de `apollia.toml`, et il s'affiche en mode **Builder** uniquement, sur la page **Modèles LLM** de la sidebar, en tête du bloc **Statistiques de session (7 jours)**.

Ce que vous y voyez :

- **Aucun seuil configuré** : une ligne qui le dit, et qui nomme le réglage à ajouter. C'est l'état par défaut, aucun plafond n'est posé à l'installation.
- **Un seuil configuré** : une bande **Seuil d'alerte de coût** avec un chiffre *dépense sur plafond*, une jauge, et une légende qui nomme la journée jugée. La bande passe en ambre au-delà de 80 % du plafond (*Proche du seuil*) et en rouge au-dessus (*Seuil dépassé*).

Lisez la comparaison pour ce qu'elle est. Le runtime applique ce seuil au **coût cumulé d'une session**, alors que cette surface ne connaît que des totaux journaliers. La bande compare donc la **journée la plus chargée** des 7 derniers jours au plafond, ce qui est une borne supérieure : une journée sous le plafond ne peut pas cacher une session au-dessus. La légende nomme explicitement cette journée.

C'est une alerte, pas un plafond bloquant. Le franchir n'interrompt rien : aucun appel n'est refusé, aucune exécution n'est arrêtée. Le runtime marque le franchissement sur le budget de session qu'il publie, et cette bande le montre du côté des coûts.

## Vérification

Les chiffres en bas correspondent à votre intuition de la consommation. Les données se rafraîchissent automatiquement environ une fois par minute.

> **Note - routage hybride :** si vous utilisez le routage hybride (`[llm.routing.hybrid]`), les étapes escaladées vers le modèle frontier apparaissent sous le backend frontier dans le graphique et dans la légende. Surveillez ce backend pour contrôler votre consommation réelle par rapport au plafond `cost_ceiling_usd` configuré.

## Si ça ne marche pas

- **Le graphique est vide** : aucun appel facturé n'a été enregistré sur 7 jours. Vérifiez que votre fournisseur n'est pas un modèle 100 % local.
- **Les coûts paraissent trop élevés** : ouvrez les **Logs** de l'assistant le plus actif (page **Mes assistants**) et regardez les tâches les plus longues - un contexte injecté volumineux gonfle rapidement les jetons d'entrée.
- **Coûts en hausse après activation du routage hybride** : le frontier est appelé plus souvent que prévu. Abaissez `cost_ceiling_usd` dans `[llm.routing.hybrid]` pour limiter l'escalade, ou désactivez temporairement le routage hybride. Voir [Connecter un modèle distant](../installation/connecter-un-modele-distant.md).

> **Référence technique :** [Référence Apollia](/reference)
