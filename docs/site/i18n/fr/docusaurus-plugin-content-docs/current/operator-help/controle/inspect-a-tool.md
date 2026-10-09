---
title: Inspecter un outil
slug: /operator-help/control/inspect-a-tool
sidebar_position: 3
---

# Inspecter un outil

> Pour les operators qui veulent savoir ce qu'est vraiment un outil avant de l'autoriser : ce qu'il prend en entrée, ce qu'il rend, ce qu'il exige, et quels identifiants sont enregistrés pour lui.

## Prérequis

- L'application est ouverte.
- Vous savez reconnaître l'outil dans la liste (Recherche Web, Écriture de fichiers, Bash, etc.).
- Pour lire le contrat d'un outil, le mode développeur est allumé (**Réglages**, section **Développeur**).

## La différence avec l'écran Autorisations

Deux écrans, deux questions :

- **Autorisations** répond à « *quels agents peuvent se servir de cet outil, et que demandent-ils d'abord* » : les accès de chaque agent, les accords donnés, leur retrait. Voir [Gérer les autorisations d'outils](manage-tool-permissions.md).
- **Connexions**, section **Outils intégrés**, répond à « *qu'est-ce que cet outil* » : s'il est activé, sa configuration, et son contrat. C'est l'objet de cette page.

## Lire le contrat d'un outil

1. Dans la barre latérale, cliquez sur **Connexions**, puis sur **Apollia, outils intégrés** sous **Outils intégrés**. La page liste les outils fournis avec Apollia, groupés par catégorie, chacun avec son interrupteur et son état (**Actif**, **Désactivé** ou **Clé requise**).

2. Mode développeur allumé, cliquez sur le bouton **Contrat** de la carte de l'outil, ou faites un clic droit sur la carte et choisissez **Voir le contrat**. Un panneau s'ouvre à droite. Mode développeur éteint, la page ne montre ni le bouton ni les identifiants techniques.

3. Le panneau montre, dans l'ordre :

   - Le **nom affiché** et, en dessous, l'identifiant technique utilisé partout ailleurs (la piste d'audit, les règles d'autorisation, un manifeste).
   - Deux badges : le **type** d'outil (**Natif**, **Connexion** ou **Outil Python**) et la **version**, chacun affiché seulement si le runtime le renseigne.
   - La **description** que l'outil déclare de lui-même.
   - Les **permissions requises**, sous forme de badges. Cette section n'apparaît que si l'outil déclare des permissions : une section absente signifie « non renseigné », jamais « n'exige rien ».
   - L'**entrée** et la **sortie**, sous forme de liste de champs indentée : nom, type, obligatoire ou facultatif, valeur par défaut et valeurs autorisées quand le schéma les précise. Un schéma qui n'est pas une liste de champs, et tous les cas que la lecture ne sait pas aplatir, retombent sur le document brut, à un clic derrière **JSON brut** dans les deux sections.
   - Les **identifiants** enregistrés pour cet outil.

4. Fermez le panneau avec la ✕ de l'en-tête. Rien ne s'écrit depuis ce panneau, il lit.

Un outil qui n'est pas enregistré dans le runtime n'expose aucun contrat, et le panneau le dit précisément au lieu d'afficher un cadre vide.

## Voir les identifiants configurés

En bas de la même page, une section **Identifiants** liste tous les identifiants enregistrés sur cette machine, une ligne par couple outil et clé : l'outil concerné et sa date d'ajout. Le nom de la clé s'affiche mode développeur allumé. **Recharger** relit la liste.

**Aucune valeur, ni aucun fragment de valeur, n'est jamais affiché.** Les clés restent chiffrées sur la machine ; seuls leurs noms et leurs dates arrivent jusqu'à l'interface. Une clé s'enregistre dans le panneau de configuration propre à l'outil, derrière le bouton **Configurer** de sa carte.

Une section vide est l'état normal sur une installation neuve : elle signifie qu'aucun outil ne s'est encore vu confier de clé.

## Vérification

- Le panneau de contrat de l'outil **Écriture de fichiers** affiche une entrée qui liste les champs attendus par l'outil.
- L'identifiant technique affiché sous le titre du panneau est celui que vous retrouvez dans les entrées du [Journal](../observabilite/read-the-audit-trail.md).

## Si ça ne marche pas

- **Il n'y a pas de bouton Contrat** : le mode développeur est éteint. Allumez-le dans **Réglages**, section **Développeur**.
- **Le panneau annonce un contrat indisponible** : l'outil figure dans la liste mais n'est pas enregistré dans le runtime. Rien n'est cassé de votre côté, il n'y a simplement rien à décrire.
- **La section Entrée affiche du JSON brut au lieu d'une liste de champs** : le schéma n'est pas une simple liste de champs. Le document brut est le contrat entier, rien n'a été masqué.
- **Un outil a une clé configurée mais la section Identifiants ne montre rien** : cliquez sur **Recharger**. La liste est lue à l'ouverture de la page, pas à chaque changement fait ailleurs.

> **Référence technique :** [Référence Apollia](/reference)
