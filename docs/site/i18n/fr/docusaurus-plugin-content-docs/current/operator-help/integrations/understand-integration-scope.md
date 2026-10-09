---
title: Comprendre la portée d'une intégration
slug: /operator-help/integrations/understand-integration-scope
sidebar_position: 2
---

# Comprendre la portée d'une intégration

> Pour tout operator qui se demande pourquoi un agent peut appeler un outil alors qu'un autre ne peut pas, ou comment scoper une intégration à un agent précis.

## Prérequis

- Au moins une intégration connectée (connecteur natif ou serveur MCP).
- Au moins un agent installé.

## Les trois filtres qui contrôlent un appel d'outil

Quand un agent tente d'utiliser un outil, Apollia applique trois filtres :

1. **Les accès de l'agent** listent les outils qui lui sont accordés, par famille d'action. Un outil qui n'en fait pas partie n'est pas accessible à cet agent.
2. **La règle de sa famille d'action** (voir [Comprendre les permissions MCP](understand-mcp-permissions.md)) décide si l'outil s'exécute sans demander ou demande d'abord votre accord.
<!-- claim:organisation-policy-gates-connecting-not-calling -->
3. **La politique de l'organisation**, quand votre organisation en a déployé une, signée. Elle est lue au démarrage d'Apollia et passe avant tout le reste : un type d'action, un outil ou une connexion qu'elle refuse reste refusé, quels que soient les accès et quoi que vous répondiez. Une connexion qu'elle interdit est listée sous **Interdites par la politique** dans **Connexions**. Quand elle interdit les connexions distantes, l'ajout d'une connexion à un service en ligne est refusé dans la fenêtre, avant que le flux OAuth d'un compte Google ou Microsoft ne démarre. Sans politique, rien n'est refusé à cette étape.

Si l'un des trois refuse, l'outil ne s'exécute pas.

## Côté agent

Ouvrez **Agents**, puis la fiche d'un agent. Ses sections sont **Mission et exemples**, **Modèle**, **Accès**, **Sources consultées** et **Activité**. **Accès** montre ce que l'agent a le droit de faire :

- Les outils accordés à l'agent, regroupés par famille d'action (**Lire**, **Écrire ou supprimer**, **Envoyer vers l'extérieur**, etc.).
- La règle de chaque famille : **Sans demander**, **Votre accord est demandé** ou **Accord à chaque fois**.
- Un badge **Distant** sur un accès qui vient d'une connexion distante.

Cette liste se modifie sur la fiche elle-même. Cliquez sur **Ajuster** : chaque accès devient un interrupteur, **Accorder un outil, un agent ou un workflow** en ajoute un, et **Accorder une connexion** accorde les outils d'une connexion, famille par famille. Cliquez sur **Enregistrer** ; chaque changement est noté au **Journal**.

## Côté sources

Les documents et les dossiers qu'un agent lit sont ses **sources**, cochées dans la section **Sources consultées** de sa fiche. **Ce ne sont pas des outils MCP** : une source dit ce qu'un agent peut chercher, pas quel outil il peut appeler.

Un serveur MCP ou un connecteur natif est utilisable par un agent une fois ses outils accordés à cet agent, sur sa fiche.

## Vérifier ce qu'un agent peut faire

- Section **Accès** de la fiche de l'agent, liste complète.
- Dans le chat, demander à l'agent *« Liste les outils que tu peux utiliser »*.
- Tester un appel concret. Un outil qui n'a pas été accordé à l'agent ne lui est pas proposé : il répond donc qu'il ne peut pas le faire. Un refus qui vient de la politique dit que la politique de l'organisation a refusé l'action.

## Si ça ne marche pas

- **L'agent dit qu'il ne peut pas utiliser l'outil** : l'outil ne fait pas partie de ses accès. Ouvrez sa fiche, cliquez sur **Ajuster** et accordez-le, ou utilisez un autre agent qui l'a déjà.
- **L'agent voit l'outil mais l'appel échoue** : c'est probablement une question de permission, un token expiré ou un scope manquant. Voir [Comprendre les permissions MCP](understand-mcp-permissions.md) et [Gérer les tokens OAuth](manage-oauth-tokens.md).
- **Deux agents devraient voir le même outil et un seul le voit** : vérifiez les **Accès** des deux, l'outil doit être accordé à chacun.

> **Référence technique :** [Référence Apollia](/reference) , résolution des outils par agent.
