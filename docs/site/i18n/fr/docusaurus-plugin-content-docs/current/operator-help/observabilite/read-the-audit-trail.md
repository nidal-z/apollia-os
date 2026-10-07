---
title: Consulter la piste d'audit
slug: /operator-help/observability/read-the-audit-trail
sidebar_position: 2
---

# Consulter la piste d'audit

> Pour les opérateurs qui veulent retrouver précisément qui a fait quoi, et quand - typiquement pour un contrôle interne ou une investigation post-incident.

La piste d'audit, c'est le **Journal** : un seul registre, signé et chaîné, qui contient chaque action des agents et chaque changement d'accès. L'écran Observabilité ne porte plus d'onglet Piste d'audit à part.

## Prérequis

- Au moins une action (un appel d'outil, un consentement donné, un accès accordé) a eu lieu.
- Vous connaissez à peu près la période ou l'agent à investiguer.

## Étapes

1. Dans la barre latérale, cliquez sur **Journal**. Les entrées sont regroupées par jour, les plus récentes d'abord, chacune écrite comme une phrase : quand, quel agent, ce qui s'est passé, sur quelle cible, avec quel résultat.

2. Resserrez la liste avec les filtres placés au-dessus :
   - **Agent** - le travail d'un seul agent.
   - **Famille** - une famille d'action (lire, lire des données personnelles, écrire ou supprimer, envoyer à l'extérieur, exécuter du code).
   - **Nature** - **Actions**, **Changements d'accès** ou **Politique**.
   - **Résultat** - ce qui a réussi, ce qui a échoué, ce qui a été refusé.
   - **Période** - **Aujourd'hui**, **7 derniers jours**, **30 derniers jours** ou **Tout le journal**.

   **Effacer les filtres** ramène le journal entier. La ligne au-dessus de la liste dit combien d'entrées sont affichées et combien de filtres sont actifs.

3. Cliquez sur une entrée pour ouvrir son détail dans le panneau de détail. Pour un appel d'outil, le détail contient les arguments et la sortie quand l'exécution les a capturés.

4. En bas de la liste, **Afficher les entrées plus anciennes** la prolonge.

5. **Vérifier l'intégrité** recalcule la chaîne et les signatures, et dit si le journal est intact ou à quel endroit il a été altéré.

6. **Exporter** écrit les entrées qui correspondent aux filtres en vigueur, en **CSV** ou en **PDF**. Inclure le détail ajoute les paramètres de chaque action, qui peuvent contenir des données personnelles.

## Exporter et vérifier en ligne de commande

Le même journal se lit en ligne de commande :

```sh
apollia-os audit journal --limit 200     # consulter
apollia-os audit export --format csv --output journal.csv
apollia-os audit verify                  # vérifier toute la chaîne
apollia-os audit verify <RUN_ID>         # vérifier une exécution
apollia-os audit anchor                  # afficher l'ancre de tête
```

**`verify`** recalcule la chaîne de hachage et contrôle les signatures. Sans
argument, la commande parcourt le journal entier ; avec un identifiant
d'exécution, elle se limite à cette exécution. Une entrée modifiée après coup
casse la chaîne et se voit.

**`anchor`** affiche l'ancre de tête de la chaîne globale. La conserver hors de
la machine est la seule défense contre une troncature de la fin du journal par
quelqu'un qui aurait obtenu la clé de signature. Cette clé est un fichier local,
lisible par le compte qui fait tourner Apollia : l'ancre exportée est donc la
vraie protection, pas une précaution avancée.

**`export`** écrit les entrées qui passent les filtres, les plus récentes
d'abord, en CSV ou en JSON.

Le détail de ces commandes est dans
[Auditer et vérifier une exécution](/how-to/audit-and-verify).

## Vérification

Vous retrouvez dans la liste les actions que vous savez avoir eu lieu récemment, avec le bon résultat. Si vous filtrez par agent, le compte au-dessus de la liste baisse en conséquence.

## Si ça ne marche pas

- **Une action attendue manque** : vérifiez les filtres - un filtre actif peut masquer l'entrée. Élargissez la **Période**, ou cliquez sur **Afficher les entrées plus anciennes**.
- **Le détail d'un appel d'outil ne montre aucun argument** : aucune réponse du modèle de l'exécution ne porte cet appel, comme pour un outil appelé par le code d'un workflow.
- **La liste est vide** : rien n'a encore été journalisé. Demandez à un agent une tâche qui utilise un outil.

> **Référence technique :** [Référence Apollia](/reference)
