---
title: Consulter la chronologie d'activité
slug: /operator-help/observability/read-the-activity-timeline
sidebar_position: 1
---

# Consulter la chronologie d'activité

> Pour les operators qui veulent voir ce qui s'est passé dans l'application sur une fenêtre temporelle donnée : tâches lancées, outils appelés, approbations, appels LLM, mémoire, délégations entre agents, erreurs.

## Prérequis

- Au moins une tâche ou un agent s'est exécuté récemment.

## Où regarder selon le besoin

| Vous cherchez… | Allez plutôt sur… |
|---|---|
| Un **coup d'œil sur l'instant** : ce qui attend votre décision, ce qui vient d'être livré, ce qui tourne | L'**Accueil**, l'écran sur lequel Apollia s'ouvre (section ci-dessous). |
| L'historique d'**un agent précis** (statuts, durées, input/output des tâches) | **Mes assistants → Logs** - voir [Consulter les logs d'un agent](../agents/consulter-les-logs-d-un-agent.md). |
| Un **événement précis** (un appel LLM, un outil exécuté, une approbation) sur une fenêtre temporelle | **Observabilité → Chronologie** (cette page). |
| Une **invocation d'outil** avec ses entrées-sorties | **Observabilité → Piste d'audit** - voir [Consulter l'audit trail](consulter-l-audit-trail.md). |

## L'Accueil, pour l'instant présent

C'est l'écran sur lequel s'ouvre l'application. Là où la chronologie répond à
« que s'est-il passé », l'Accueil répond à « où en est-on maintenant ».

Trois cartes côte à côte, et une bande d'activité en dessous :

- **Décisions en attente** *(la plus large, à gauche)* : les actions qui attendent votre approbation. Compteur en en-tête, liste compacte des premiers items, et un lien *« Voir tout → »* vers la **Boîte de réception**.
- **Livrables prêts** : les tâches récemment complétées. Un clic sur une ligne ouvre la page **Mon travail**.
- **Au travail** : les agents actuellement actifs. Un clic ouvre le détail de l'agent.

![l'écran Accueil en mode opérateur, trois cartes en grille, Décisions en attente à gauche occupant deux colonnes](/img/operator-help/observabilite-lire-le-digest-quotidien-1.png)

Sous les cartes, **Activité récente** liste les dernières tâches tous statuts confondus sous forme de mini-cartes, et mène à la page **Mon travail**.

Les compteurs se mettent à jour tout seuls : lancez une tâche et *« Au travail »* s'incrémente sans rafraîchissement manuel. Si tout reste vide alors qu'un agent vient de tourner, la connexion temps réel a probablement sauté ; quittez et rouvrez l'application.

## La chronologie, pour ce qui s'est passé

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Chronologie**.

2. En haut, **quatre KPIs** résument la fenêtre courante : Événements · Outils · Appels LLM · Erreurs (compteur en rouge si > 0). Les KPIs réagissent aux filtres : si vous masquez les outils, leur compteur reste mais le total **Événements** descend.

3. Choisissez la **fenêtre temporelle** : **30 min / 1 h / 6 h / 24 h / 7 j**. Par défaut : 1 h. Les événements se rechargent automatiquement environ toutes les 15 secondes.
   ![Onglet Chronologie : la bande de KPIs, la barre de filtres, puis les événements groupés par jour](/img/operator-help/observabilite-consulter-l-historique-des-taches-1.png)

4. **Filtrez les événements** :
   - **Type** - 7 chips arrondies (Tâche / Outil / LLM / Approbation / Mémoire / Délégation / Erreur). Chaque chip active/désactive sa catégorie ; les chips grisés sont désactivés.
   - **Agent** - sélecteur déroulant pour ne voir que les événements d'un assistant précis. *Tous les agents* par défaut.

5. Les événements sont **groupés par jour** avec un en-tête (« Aujourd'hui », « Hier » ou date complète) et un compteur à droite. Chaque ligne affiche :
   - Une **pastille colorée** + **icône lucide** correspondant au type (ClipboardList pour Tâche, Wrench pour Outil, Bot pour LLM, Hand pour Approbation, Brain pour Mémoire, Link2 pour Délégation, AlertTriangle pour Erreur).
   - Le **titre lisible** *« Tâche → completed »*, *« Tool: bash (2.1 s) »*, *« LLM: claude-sonnet-4 · $0.42 »*…
   - Un **badge** de type, l'**agent**, l'**horodatage** précis à la seconde, dans la forme employée par la langue de l'application, **et** l'âge relatif (*« il y a 3 min »*).

6. Cliquez sur une ligne pour **déplier le payload brut** de l'événement (JSON formaté en monospace, incluant le champ `source` qui indique de quelle base SQLite provient l'événement). Re-cliquez pour replier.

## Suivre une tâche de bout en bout

En haut de l'onglet **Chronologie**, un sélecteur **Portée** propose deux positions :

- **Toute l'activité**, la fenêtre décrite ci-dessus, tous agents et tous types d'événements mélangés.
- **Une tâche**, qui restreint tout l'onglet à une seule exécution. Une liste **Tâche** apparaît à côté, chaque entrée se lisant *agent · statut · identifiant court*.

En position **Une tâche**, l'onglet affiche la **chronologie de la tâche** : tout ce qu'Apollia a enregistré pour cette exécution, du plus ancien au plus récent, agrégé depuis cinq sources (transitions de statut, étapes de plan, appels au modèle, invocations d'outils, approbations). Chaque ligne porte son icône, son titre et les faits machine qui vont avec : l'outil et sa durée, le code de sortie, le backend et son modèle avec les tokens entrants et sortants et le coût, l'attente avant qu'une approbation soit tranchée. Une étape ou un outil en échec teinte la ligne et porte la mention **Échec**, et un contenu que le runtime a dû couper est signalé comme tronqué.

Vous n'êtes pas obligé de choisir la tâche à la main : le bouton **Voir les logs** d'une exécution en échec, dans l'onglet Activité de la **Boîte de réception**, ouvre cet onglet déjà positionné dessus.

Attention à ne pas confondre. La chronologie de la tâche lit ce qui a été persisté ; l'onglet **Trace** d'une tâche, sur la page **Mon travail**, rejoue le flux d'événements en direct. Deux lectures de la même exécution, depuis deux sources.

## L'onglet Hooks

En mode **Builder**, la barre d'onglets en porte trois de plus, dont **Hooks** : les handlers de cycle de vie enregistrés au démarrage du runtime, lus dans le fichier de configuration. Une ligne par handler, avec son type (`command` ou `http`), les événements auxquels il s'abonne, son timeout et sa cible (la ligne de commande, ou l'URL).

C'est en lecture seule, et c'est un fait de démarrage, pas un flux vivant : le registre est construit une fois depuis la configuration, sans enregistrement dynamique ni rechargement à chaud. Une liste vide signifie qu'aucun hook n'est déclaré, ce qui est un état sain, pas une panne.

## Vérification

Vous retrouvez vos exécutions récentes dans la fenêtre choisie. Élargir la fenêtre depuis `1h` à `24h` fait apparaître plus d'événements anciens sans rafraîchissement manuel.

## Si ça ne marche pas

- **La chronologie est vide** : la fenêtre par défaut (1 h) ne contient peut-être pas d'activité. Élargissez à `24 h` ou `7 j`. La chronologie scanne désormais directement chaque source SQLite par horodatage : tâches, outils (audit), appels LLM, HITL, déclenchements de triggers, ouvertures de sessions chat, raisonnements et erreurs runtime. Si elle reste vide sur `7 j`, aucune activité n'a été enregistrée dans cette fenêtre.
- **Mes appels LLM faits depuis Chat n'apparaissent pas** : seul l'**ouverture/fermeture de la session chat** et ses **approbations d'outils** apparaissent. Les appels LLM internes au chat ne sont pas (encore) persistés dans `llm_calls.db` - limitation connue (issue tracker). Les sessions chat avec un agent qui déclenche une tâche font, eux, remonter tous les événements normalement.
- **Un événement attendu n'apparaît pas** : vérifiez les chips de type et le sélecteur d'agent - un filtre actif peut masquer la ligne. Les chips de type fonctionnent en logique additive : si tous sont grisés, rien ne s'affiche.
- **Je veux le détail d'une tâche complète, pas un événement granulaire** : passez par **Mes assistants → Logs** sur l'agent concerné. La chronologie est volontairement granulaire et factuelle.

> **Concept :** [Explication Apollia](/explanation)
