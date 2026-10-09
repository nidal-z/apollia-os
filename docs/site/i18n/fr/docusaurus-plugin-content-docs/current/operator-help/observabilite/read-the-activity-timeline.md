---
title: Consulter la chronologie d'activité
slug: /operator-help/observability/read-the-activity-timeline
sidebar_position: 1
---

# Consulter la chronologie d'activité

> Pour les operators qui veulent voir ce qui s'est passé dans l'application sur une fenêtre temporelle donnée : tâches lancées, outils appelés, approbations, appels LLM, mémoire, délégations entre agents, erreurs.

## Prérequis

- Au moins une tâche ou un agent s'est exécuté récemment.
- Le mode développeur est allumé (**Réglages**, section **Développeur**). **Observabilité** est l'un des écrans techniques : la barre latérale ne l'affiche qu'avec ce mode.

## Où regarder selon le besoin

| Vous cherchez… | Allez plutôt sur… |
|---|---|
| Ce qui **attend votre réponse** en ce moment | **À valider**, voir [Utiliser la liste À valider](../transversal/use-the-inbox.md). |
| L'historique d'**un agent précis** (statuts, durées, entrées et sorties des tâches) | **Agents**, puis **Logs** dans le menu de l'agent, voir [Consulter les logs d'un agent](../agents/view-an-agent-logs.md). |
| Un **événement précis** (un appel LLM, un outil exécuté, une approbation) sur une fenêtre temporelle | **Observabilité**, onglet **Chronologie** (cette page). |
| **Qui a fait quoi, et avec quel résultat**, pour un contrôle ou une enquête | **Journal**, voir [Consulter la piste d'audit](read-the-audit-trail.md). |

## La chronologie, pour ce qui s'est passé

1. Dans la barre latérale, cliquez sur **Observabilité**, puis sur l'onglet **Chronologie**.

2. En haut, **quatre KPIs** résument la fenêtre courante : Événements · Outils · Appels LLM · Erreurs (compteur en rouge si > 0). Les KPIs réagissent aux filtres : si vous masquez les outils, le total **Événements** descend.

3. Choisissez la **fenêtre temporelle** : **30 min / 1 h / 6 h / 24 h / 7 j**. Par défaut : 1 h. Les événements se rechargent automatiquement environ toutes les 15 secondes.
   ![Onglet Chronologie : la bande de KPIs, la barre de filtres, puis les événements groupés par jour](/img/operator-help/observabilite-consulter-l-historique-des-taches-1.png)

4. **Filtrez les événements** :
   - **Type** : 7 pastilles arrondies (Tâche / Outil / LLM / Approbation / Mémoire / Délégation / Erreur). Chaque pastille active ou désactive sa catégorie ; les pastilles grisées sont désactivées.
   - **Agent** : sélecteur déroulant pour ne voir que les événements d'un agent précis.

5. Les événements sont **groupés par jour** avec un en-tête (« Aujourd'hui », « Hier » ou date complète) et un compteur à droite. Chaque ligne affiche :
   - Une **pastille colorée** et une **icône** correspondant au type.
   - Un **titre lisible**.
   - Un **badge** de type, l'**agent**, l'**horodatage** précis à la seconde, dans la forme employée par la langue de l'application, **et** l'âge relatif (*« il y a 3 min »*).

6. Cliquez sur une ligne pour **déplier le contenu brut** de l'événement, en JSON formaté. Cliquez de nouveau pour replier.

## Suivre une tâche de bout en bout

En haut de l'onglet **Chronologie**, un sélecteur **Portée** propose deux positions :

- **Toute l'activité**, la fenêtre décrite ci-dessus, tous agents et tous types d'événements mélangés.
- **Une tâche**, qui restreint tout l'onglet à une seule exécution. Une liste **Tâche** apparaît à côté, chaque entrée se lisant *agent · statut · identifiant court*.

En position **Une tâche**, l'onglet affiche la **Chronologie de la tâche** : tout ce qu'Apollia a enregistré pour cette exécution, du plus ancien au plus récent, agrégé depuis cinq sources (transitions de statut, étapes de plan, appels au modèle, invocations d'outils, approbations). Chaque ligne porte son icône, son titre et les faits machine qui vont avec : l'outil et sa durée, le code de sortie, le backend et son modèle avec les tokens entrants et sortants et le coût, l'attente avant qu'une approbation soit tranchée. Une étape ou un outil en échec teinte la ligne et porte la mention **Échec**, et un contenu que le moteur a dû couper est signalé comme tronqué.

## L'onglet Hooks

En plus de **Chronologie**, **Coûts LLM** et **Messagerie**, la barre d'onglets porte quatre onglets techniques de plus : **Délégation**, **Cache de plans**, **Hooks** et **Politique de capture**. **Hooks** liste les hooks de cycle de vie enregistrés au démarrage du moteur, lus dans le fichier de configuration. Une ligne par hook, avec son type (`command` ou `http`), les événements auxquels il s'abonne, son timeout et sa cible (la ligne de commande, ou l'URL).

C'est en lecture seule, et c'est un fait de démarrage, pas un flux vivant : le registre est construit une fois depuis la configuration, sans enregistrement dynamique ni rechargement à chaud. Une liste vide signifie qu'aucun hook n'est déclaré, ce qui est un état sain, pas une panne.

## Vérification

Vous retrouvez vos exécutions récentes dans la fenêtre choisie. Élargir la fenêtre de `1 h` à `24 h` fait apparaître plus d'événements anciens sans rafraîchissement manuel.

## Si ça ne marche pas

- **Observabilité ne figure pas dans la barre latérale** : le mode développeur est éteint. Allumez-le dans **Réglages**, section **Développeur**.
- **La chronologie est vide** : la fenêtre par défaut (1 h) ne contient peut-être pas d'activité. Élargissez à `24 h` ou `7 j`. Si elle reste vide sur `7 j`, aucune activité n'a été enregistrée dans cette fenêtre.
- **Un événement attendu n'apparaît pas** : vérifiez les pastilles de type et le sélecteur d'agent, un filtre actif peut masquer la ligne. Si toutes les pastilles de type sont grisées, rien ne s'affiche.
- **Je veux le détail d'une tâche complète, pas un événement granulaire** : utilisez la position **Une tâche** du sélecteur **Portée**, ou passez par **Agents**, puis **Logs** dans le menu de l'agent concerné.

> **Concept :** [Explication Apollia](/explanation)
