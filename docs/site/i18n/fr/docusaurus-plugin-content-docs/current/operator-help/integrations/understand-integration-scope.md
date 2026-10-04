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

Quand un agent tente d'utiliser un outil, Apollia applique trois filtres dans l'ordre :

1. **Le manifest de l'agent** déclare la liste d'outils requis et optionnels. Un outil absent du manifest n'est pas accessible à cet agent.
2. **Les règles de permission** (voir [Comprendre les permissions MCP](understand-mcp-permissions.md)) décident si l'outil peut s'exécuter automatiquement ou demande une approbation.
<!-- claim:organisation-policy-gates-connecting-not-calling -->
3. **La politique de l'organisation**, qui n'est pas un troisième filtre sur l'appel d'outil. Quand votre organisation a déployé une politique signée qui interdit les connexions distantes, Apollia refuse d'ajouter une connexion à un service en ligne : le flux OAuth d'un compte Google ou Microsoft ne démarre pas, et le refus est vérifié par le runtime, pas par la fenêtre. Sans politique, rien n'est refusé à cette étape. Elle n'inspecte pas les appels passant par une connexion déjà établie, donc traitez-la comme un verrou sur la connexion, pas sur l'usage.

Si l'un des deux premiers refuse, l'outil ne s'exécute pas.

## Côté agent

Ouvrez la fiche d'un agent, onglet **Outils**. La liste affiche en lecture seule :

- Les outils requis par le manifest, avec leur identifiant (par exemple `outlook.send`).
- Les outils optionnels.
- Un badge indique si l'outil exige une approbation HITL par défaut.

![Fiche d'un agent, onglet Outils : la liste des outils requis et optionnels avec leurs badges d'approbation](/img/operator-help/integration-comprendre-la-portee-d-une-integration-1.png)

Cette liste **n'est pas modifiable depuis l'interface en v0.1.0**. Pour ajouter ou retirer un outil à un agent, il faut éditer son manifest et le réinstaller. Voir la page Aide [Installer un agent](../agents/install-an-agent.md).

## Côté sources

Les documents et les dossiers qu'un agent lit sont ses **sources**, cochées sur sa fiche. **Ce ne sont pas des outils MCP** : une source dit ce qu'un agent peut chercher, pas quel outil il peut appeler.

Tous les MCPs installés et tous les connecteurs natifs sont visibles par tous les agents qui les déclarent dans leur manifest.

## Vérifier ce qu'un agent peut faire

- Onglet **Outils** de l'agent, liste complète.
- Dans le chat, demander à l'agent *"Liste les outils que tu peux utiliser"*. Il répond avec sa toolbelt si son prompt système l'autorise.
- Tester un appel concret. Si l'outil est refusé, l'agent retourne un message clair, `outil non autorisé` par exemple. Aucun refus de ce type par la politique ne peut apparaître ici : la politique est vérifiée à l'ajout d'une connexion, jamais à l'appel d'un outil.

## Si ça ne marche pas

- **L'agent dit "outil non autorisé"** : l'outil n'est pas dans son manifest. Mettez à jour l'agent (réinstallation avec un manifest étendu) ou utilisez un autre agent qui le déclare.
- **L'agent voit l'outil mais l'appel échoue** : c'est probablement une question de permission, un token expiré ou un scope manquant. Voir [Comprendre les permissions MCP](understand-mcp-permissions.md) et [Gérer les tokens OAuth](manage-oauth-tokens.md).
- **Deux agents devraient voir le même outil et un seul le voit** : vérifiez les deux manifests, l'outil doit être déclaré dans chacun.

> **Référence technique :** [Référence Apollia](/reference) , résolution des outils par agent.
