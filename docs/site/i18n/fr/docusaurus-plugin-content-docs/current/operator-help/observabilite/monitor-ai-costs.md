---
title: Surveiller les coûts d'IA
slug: /operator-help/observability/monitor-ai-costs
sidebar_position: 3
---

# Surveiller les coûts d'IA

> Pour les operators qui veulent suivre la dépense des appels à leur fournisseur d'IA sur la semaine écoulée.

## Prérequis

- Au moins une conversation ou un agent a déjà appelé votre fournisseur d'IA.
- Vous utilisez un serveur facturé. Les modèles locaux n'apparaissent pas dans les coûts.
- Le mode développeur est allumé (**Réglages**, section **Développeur**) : **Observabilité** est l'un des écrans techniques, affiché dans la barre latérale seulement avec ce mode.

## Étapes

1. Dans la barre latérale, cliquez sur **Observabilité**, puis sur l'onglet **Coûts LLM**.

2. En haut à droite de la carte, un **sélecteur de période** vous permet de basculer entre **7 j / 14 j / 30 j / 90 j / 1 an**. Tous les indicateurs, le graphique et la légende se recalculent instantanément sur la nouvelle fenêtre. La densité de l'axe horizontal s'adapte automatiquement (étiquettes espacées au-delà de 14 jours).

3. En haut, **quatre indicateurs clés** (KPI) résument la fenêtre sélectionnée :
   - **Total 7 jours** - le libellé est figé et annonce sept jours quelle que soit la période choisie. Lisez-le comme la somme sur la fenêtre, pas sur une semaine.
   - **Moyenne / jour** - total divisé par le nombre de jours de la fenêtre.
   - **Jour le plus cher** - montant + date du jour qui a le plus consommé.
   - **Backend principal** - nom du backend qui pèse le plus, avec son total cumulé.
   ![Onglet Couts LLM, selecteur de periode, quatre KPI, graphique en barres empilees et legende des backends](/img/operator-help/observabilite-surveiller-les-couts-llm-1.png)

4. Au centre, un **histogramme empilé** présente la période sélectionnée. Une barre par jour, chaque barre découpée en segments colorés par **backend**. L'axe vertical est en dollars avec des ticks arrondis ; l'axe horizontal indique la date (jour de la semaine + date courte pour les courtes fenêtres, date seule pour 30 j et plus). Un backend n'apparaît que pour les appels qu'Apollia a su tarifer, ce qui n'est pas tous ceux qu'il a passés.

5. Survolez une colonne : les autres jours s'estompent légèrement et le **total du jour** s'affiche au-dessus de la barre. Un tooltip apparaît aussi sur chaque segment avec le **nom du backend** et son **coût exact**.

6. Sous le graphique, la **légende** liste tous les backends actifs sous forme de **pastilles** affichant le **total cumulé par backend** sur la fenêtre. Permet de comparer la part de chaque fournisseur d'un coup d'œil.

## Vérification

Les chiffres en bas correspondent à votre intuition de la consommation. Les données se rafraîchissent automatiquement environ une fois par minute.

> **Note - routage hybride :** avec le routage hybride (`[llm.routing.hybrid]`), le frontier est l'un des serveurs que vous avez déclarés : un serveur compatible OpenAI, puisqu'Apollia n'a pas de fournisseur cloud à lui, et un modèle hébergé s'atteint par une passerelle que vous opérez. Une étape lui est confiée après trois appels d'outil en échec d'affilée. Ses appels n'apparaissent sous son nom dans le graphique et la légende que lorsqu'Apollia sait les tarifer, et `cost_ceiling_usd` additionne ces mêmes appels tarifés : avec un modèle que la table de prix ne connaît pas, le frontier n'affiche aucun coût et le plafond n'arrête jamais l'escalade. Voir [Configurer le routage hybride](../installation/configure-hybrid-routing.md).

## Si ça ne marche pas

- **Observabilité ne figure pas dans la barre latérale** : le mode développeur est éteint. Allumez-le dans **Réglages**, section **Développeur**.
- **Le graphique est vide** : aucun appel tarifé n'a été enregistré sur la fenêtre, et il y a deux causes. Votre backend est peut-être un modèle 100 % local, qui ne coûte rien. Ou le modèle que vous appelez n'a pas de prix dans Apollia : le client des serveurs compatibles OpenAI, qui sert tout serveur que vous déclarez, dont une passerelle que vous opérez, ne tarife que `gpt-4o-mini`, `gpt-4o` et `gpt-3.5-turbo`. Tout autre identifiant est enregistré sans aucun coût.
- **Les coûts paraissent trop élevés** : ouvrez **Agents**, puis **Logs** dans le menu de l'agent le plus actif, et regardez les tâches les plus longues - un contexte injecté volumineux gonfle rapidement les jetons d'entrée.
- **Coûts en hausse après activation du routage hybride** : le frontier est appelé chaque fois que trois appels d'outil échouent d'affilée, ce qu'une exécution qui échoue sans cesse fait souvent. Abaissez `cost_ceiling_usd` dans `[llm.routing.hybrid]` pour limiter l'escalade, ou retirez la section pour couper le routage hybride. Le plafond ne compte que les appels tarifés : il ne limite pas un frontier que le graphique affiche à zéro. Voir [Configurer le routage hybride](../installation/configure-hybrid-routing.md).

> **Référence technique :** [Référence Apollia](/reference)
