# Surveiller les coûts d'IA

> Pour les operators qui veulent suivre la dépense des appels à leur fournisseur d'IA sur la semaine écoulée.

## Prérequis

- Au moins une conversation ou un agent a déjà appelé votre fournisseur d'IA.
- Vous utilisez un fournisseur facturé (Anthropic, OpenAI, Bedrock, Vertex…). Les modèles locaux n'apparaissent pas dans les coûts.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Coûts LLM**.

2. En haut à droite de la carte, un **sélecteur de période** vous permet de basculer entre **7 j / 14 j / 30 j / 90 j / 1 an**. Tous les indicateurs, le graphique et la légende se recalculent instantanément sur la nouvelle fenêtre. La densité de l'axe horizontal s'adapte automatiquement (étiquettes thinnées au-delà de 14 jours).

3. En haut, **quatre indicateurs clés** (KPI) résument la fenêtre sélectionnée :
   - **Total** — somme de toutes les dépenses sur la fenêtre.
   - **Moyenne / jour** — total divisé par le nombre de jours de la fenêtre.
   - **Jour le plus cher** — montant + date du jour qui a le plus consommé.
   - **Backend principal** — nom du backend qui pèse le plus, avec son total cumulé.
   ![onglet Coûts LLM — sélecteur de période en haut à droite, 4 KPI, histogramme empilé centré, légende en past...](../_screenshots/observabilite-surveiller-les-couts-llm-1.png)

4. Au centre, un **histogramme empilé** présente la période sélectionnée. Une barre par jour, chaque barre découpée en segments colorés par **backend** (Anthropic, OpenAI, etc.). L'axe vertical est en dollars avec des ticks arrondis ; l'axe horizontal indique la date (jour de la semaine + date courte pour les courtes fenêtres, date seule pour 30 j et plus).

5. Survolez une colonne : les autres jours s'estompent légèrement et le **total du jour** s'affiche au-dessus de la barre. Un tooltip apparaît aussi sur chaque segment avec le **nom du backend** et son **coût exact** (ex. `anthropic: $0.42 — May 11`).

6. Sous le graphique, la **légende** liste tous les backends actifs sous forme de **pastilles** affichant le **total cumulé par backend** sur la fenêtre. Permet de comparer la part de chaque fournisseur d'un coup d'œil.

## Vérification

Les chiffres en bas correspondent à votre intuition de la consommation. Les données se rafraîchissent automatiquement environ une fois par minute.

## Si ça ne marche pas

- **Le graphique est vide** : aucun appel facturé n'a été enregistré sur 7 jours. Vérifiez que votre fournisseur n'est pas un modèle 100 % local.
- **Les coûts paraissent trop élevés** : ouvrez les **Logs** de l'assistant le plus actif (page **Mes assistants**) et regardez les tâches les plus longues — un contexte injecté volumineux gonfle rapidement les jetons d'entrée.

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug)
