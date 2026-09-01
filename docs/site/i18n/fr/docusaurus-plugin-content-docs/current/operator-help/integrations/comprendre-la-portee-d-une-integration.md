---
title: Comprendre la portée d'une intégration
slug: /operator-help/integrations/understand-integration-scope
sidebar_position: 2
---

# Comprendre la portée d'une intégration

> Pour tout operator qui se demande pourquoi un agent peut appeler un outil alors qu'un autre ne peut pas, ou comment scoper une intégration à un projet précis.

## Prérequis

- Au moins une intégration connectée (connecteur natif ou serveur MCP).
- Au moins un agent installé ou un projet actif.

## Les trois filtres qui contrôlent un appel d'outil

Quand un agent tente d'utiliser un outil, Apollia applique trois filtres dans l'ordre :

1. **Le manifest de l'agent** déclare la liste d'outils requis et optionnels. Un outil absent du manifest n'est pas accessible à cet agent.
2. **Les règles de permission** (voir [Comprendre les permissions MCP](comprendre-les-permissions-mcp.md)) décident si l'outil peut s'exécuter automatiquement ou demande une approbation.
<!-- claim:sovereignty-profile-gates-connecting-not-calling -->
3. **Le profil de souveraineté**, qui n'est pas un troisième filtre sur l'appel d'outil. Sur `local_only`, il refuse d'ouvrir une nouvelle connexion cloud : le flux OAuth ne démarre pas. Il n'inspecte pas les appels passant par une connexion déjà établie, donc traitez-le comme un verrou sur la connexion, pas sur l'usage.

Si l'un des deux premiers refuse, l'outil ne s'exécute pas.

## Côté agent

Ouvrez la fiche d'un agent, onglet **Outils**. La liste affiche en lecture seule :

- Les outils requis par le manifest, avec leur identifiant (par exemple `outlook.send`).
- Les outils optionnels.
- Un badge indique si l'outil exige une approbation HITL par défaut.

![Fiche d'un agent, onglet Outils : la liste des outils requis et optionnels avec leurs badges d'approbation](/img/operator-help/integration-comprendre-la-portee-d-une-integration-1.png)

Cette liste **n'est pas modifiable depuis l'interface en v0.1.0**. Pour ajouter ou retirer un outil à un agent, il faut éditer son manifest et le réinstaller. Voir la page Aide [Installer un agent](../agents/installer-un-agent.md).

## Côté projet

Ouvrez un projet, onglet **Contexte**. Vous y trouvez des **Context Providers** (dossiers locaux, dépôts Git, etc.), qui alimentent le contexte des chats du projet. **Ce ne sont pas des outils MCP**, et il n'est pas possible en v0.1.0 de scoper un MCP ou un connecteur à un projet précis.

Tous les MCPs installés et tous les connecteurs natifs sont visibles par tous les agents qui les déclarent dans leur manifest, indépendamment du projet actif.

## Vérifier ce qu'un agent peut faire

- Onglet **Outils** de l'agent, liste complète.
- Dans le chat, demander à l'agent *"Liste les outils que tu peux utiliser"*. Il répond avec sa toolbelt si son prompt système l'autorise.
- Tester un appel concret. Si l'outil est refusé, l'agent retourne un message clair (`outil non autorisé`, `SovereigntyBlocked`, etc.).

## Si ça ne marche pas

- **L'agent dit "outil non autorisé"** : l'outil n'est pas dans son manifest. Mettez à jour l'agent (réinstallation avec un manifest étendu) ou utilisez un autre agent qui le déclare.
- **L'agent voit l'outil mais l'appel échoue** : c'est probablement une question de permission (token expiré, scope manquant) ou de souveraineté. Voir [Comprendre les permissions MCP](comprendre-les-permissions-mcp.md) et [Gérer les tokens OAuth](gerer-les-tokens-oauth.md).
- **Deux agents devraient voir le même outil et un seul le voit** : vérifiez les deux manifests, l'outil doit être déclaré dans chacun.

> **Référence technique :** [Référence Apollia](/reference) , résolution des outils par agent, scoping projet, ContextProvider.
