---
title: Inspecter un outil
slug: /operator-help/control/inspect-a-tool
sidebar_position: 3
---

# Inspecter un outil

> Pour les operators qui veulent savoir ce qu'est vraiment un outil avant de l'autoriser : ce qu'il prend en entrée, ce qu'il rend, ce qu'il exige, et quels identifiants sont enregistrés pour lui.

## Prérequis

- L'application est ouverte.
- Vous connaissez l'outil par son nom, ou vous savez le reconnaître dans la liste (`bash_executor`, `file_write`, `http_fetch`, etc.).

## La différence avec la page Autorisations

Deux pages, deux questions :

- **Paramètres → Autorisations** répond à « *qu'est-ce que cet outil a le droit de faire en ce moment* » : les règles persistées, leur portée, leur révocation. Voir [Gérer les autorisations d'outils](manage-tool-permissions.md).
- **Paramètres → Outils** répond à « *qu'est-ce que cet outil* » : son activation, sa configuration, et son contrat. C'est l'objet de cette page.

## Lire le contrat d'un outil

1. Dans la sidebar, cliquez sur **Paramètres**, puis sur **Outils** dans le menu de gauche.

2. Sur la carte de l'outil, cliquez sur le bouton **Contrat** (icône accolades), ou faites un clic droit sur la carte et choisissez **Voir le contrat**. Un panneau s'ouvre à droite.

3. Le panneau montre, dans l'ordre :

   - Le **nom affiché** et, en dessous, l'identifiant technique utilisé partout ailleurs (la piste d'audit, les règles d'autorisation, un manifeste).
   - Deux badges : le **type** d'outil (natif, servi par un serveur MCP, custom) et la **version**, chacun affiché seulement si le runtime le renseigne.
   - La **description** que l'outil déclare de lui-même.
   - Les **permissions requises**, sous forme de badges. Cette section n'apparaît que si l'outil déclare des permissions : une section absente signifie « non renseigné », jamais « n'exige rien ».
   - L'**entrée** et la **sortie**, sous forme de liste de champs indentée : nom, type, obligatoire ou facultatif, valeur par défaut et valeurs autorisées quand le schéma les précise. Un schéma qui n'est pas une liste de champs, et tous les cas que la lecture ne sait pas aplatir, retombent sur le document brut, à un clic derrière **JSON brut** dans les deux sections.
   - Les **identifiants** enregistrés pour cet outil.

4. Fermez le panneau avec la ✕ de l'en-tête. Rien ne s'écrit depuis ce panneau, il lit.

Un outil qui n'est pas enregistré dans le runtime n'expose aucun contrat, et le panneau le dit précisément au lieu d'afficher un cadre vide.

## Voir les identifiants configurés

En bas de la page **Outils**, une section **Identifiants** liste tous les identifiants enregistrés sur cette machine, une ligne par couple outil et clé : l'outil concerné, le nom de la clé, et sa date d'ajout. Un compteur à côté du titre donne le total, et **Recharger** relit la liste.

**Aucune valeur, ni aucun fragment de valeur, n'est jamais affiché.** Les clés restent chiffrées sur la machine ; seuls leurs noms et leurs dates arrivent jusqu'à l'interface. Enregistrer, tester et supprimer une clé se font dans le panneau de configuration propre à l'outil, depuis sa carte.

Une section vide est l'état normal sur une installation neuve : elle signifie qu'aucun outil ne s'est encore vu confier de clé.

## Vérification

- Le panneau de contrat de `file_write` affiche un schéma d'entrée qui liste les champs attendus par l'outil.
- L'identifiant technique affiché sous le titre du panneau est celui que vous retrouvez dans la colonne **Outil** de la [piste d'audit](../observabilite/read-the-audit-trail.md).

## Si ça ne marche pas

- **Le panneau annonce un contrat indisponible** : l'outil figure dans la liste mais n'est pas enregistré dans le runtime. Rien n'est cassé de votre côté, il n'y a simplement rien à décrire.
- **La section Entrée affiche du JSON brut au lieu d'une liste de champs** : le schéma n'est pas une simple liste de champs. Le document brut est le contrat entier, rien n'a été masqué.
- **Un outil a une clé configurée mais la section Identifiants ne montre rien** : cliquez sur **Recharger**. La liste est lue à l'ouverture de la page, pas à chaque changement fait ailleurs.

> **Référence technique :** [Référence Apollia](/reference)
