---
title: Gérer les autorisations d'outils
sidebar_position: 2
---

# Gérer les autorisations d'outils

> Pour les operators qui veulent visualiser, filtrer ou révoquer les autorisations accordées aux outils d'un agent - sans attendre qu'une action déclenche une nouvelle carte d'approbation.

## Prérequis

- L'application est ouverte et au moins un agent a été exécuté.
- Des autorisations ont été accordées lors d'une session précédente (lors d'une approbation, vous avez choisi « Toujours autoriser » ou une portée persistée).

## Comment les autorisations sont-elles créées ?

Les règles apparaissent automatiquement lorsqu'un agent demande l'accès à un outil et que vous choisissez une portée persistée dans la carte d'approbation (par exemple : *Ce projet* ou *Partout*). Les règles de chat (portée *Chat*) sont créées via le bouton **"Toujours autoriser"** dans le chat libre. Vous pouvez aussi en créer une à la main, avec le bouton **Ajouter une règle** en haut de la section Règles, ce qui est le moyen d'autoriser quelque chose avant même qu'un agent le demande.

## Visualiser les autorisations actives

1. Dans la sidebar, cliquez sur **Paramètres**.

2. Dans le menu de gauche, sélectionnez **Autorisations**.
   ![page Settings > Autorisations, liste de cartes d'autorisation (PermissionRuleCard) avec badges de portée](/img/operator-help/controle-configurer-les-permissions-de-fichiers-1.png)

3. Le panneau central affiche toutes les règles actives sous forme de **liste de cartes**. Chaque carte indique :
   - le **nom de l'outil** autorisé (ex. : `bash_executor`, `file_write`, `http_fetch`)
   - un **badge de portée** : *Ce projet* ou *Partout*
   - le **préfixe d'argument** (si la règle est limitée à certaines invocations)
   - la **date d'expiration** ou la mention *Permanente*
   - l'**auteur** de la décision (agent ou utilisateur)

<!-- claim:prefix-rules-evaluated-per-invocation -->
> **Ce qu'une règle fait réellement :** une règle sans préfixe qui autorise un outil ordinaire auto-approuve chaque invocation de cet outil. Une règle portant un **préfixe d'argument** est évaluée à chaque invocation, contre l'argument de l'appel : elle auto-approuve tout argument commençant par le préfixe, et le préfixe correspondant le plus long l'emporte quand plusieurs règles s'appliquent. Une règle **deny** a toujours priorité : elle refuse un appel correspondant même quand l'outil est par ailleurs couvert par un "Toujours autoriser".
<!-- claim:executor-guard-blocks-command-chaining -->
> **Les exécuteurs de code** (`bash_executor`, `python_executor`) sont plus stricts : une règle de préfixe ne s'applique qu'à une **commande simple unique** partageant ce préfixe, sans enchaînement (`;`, `&&`, `||`), pipe, redirection (`>`, `<`) ni substitution (`` ` ``, `$(...)`). Une règle sans préfixe n'auto-approuve jamais un exécuteur de code : chaque invocation redemande une confirmation.

4. Utilisez les filtres dans le panneau de gauche pour affiner la liste :
   - **Portée** : *Toutes*, *Ce projet*, *Chat / agent*, *Partout*
   - **Outil** : sélectionnez un outil précis dans la liste des outils présents

## Révoquer une autorisation individuelle

1. Repérez la carte correspondant à la règle à supprimer.
2. Cliquez sur le bouton **Révoquer** (icône corbeille, à droite de la carte).
3. Un message de confirmation apparaît brièvement. La carte disparaît immédiatement.
   ![carte d'autorisation avec bouton Révoquer visible, toast de confirmation "Règle bash révoquée"](/img/operator-help/controle-configurer-les-permissions-de-fichiers-2.png)

Une fois révoquée, l'outil concerné redemandera une approbation manuelle à la prochaine invocation.

## Sessions actives

La section **Sessions actives** liste les outils auto-approuvés via « Pour cette session » dans les conversations de chat en cours. Ces autorisations sont **in-memory uniquement** - elles disparaissent à la fermeture de la session et ne sont pas persistées.

Chaque entrée indique le nom de l'outil, la session concernée (titre ou identifiant court), le mode (*Apollia Chat*, *Agent*, *Companion*) et un badge *Session* orange. Cliquez sur **Révoquer** pour retirer l'autorisation immédiatement. L'outil demandera de nouveau confirmation lors du prochain appel dans cette session.

![section Sessions actives, liste d'entrées avec badge orange Session et bouton Révoquer](/img/operator-help/controle-configurer-les-permissions-de-fichiers-3.png)

## Révoquer toutes les autorisations d'un coup

1. Cliquez sur le bouton rouge **Tout révoquer** en haut à droite.
2. Choisissez la portée à purger :
   - *Ce projet* - supprime les règles liées au projet courant
   - *Chat / agent* - supprime les règles liées à l'agent Apollia Chat et aux agents Python
   - *Partout* - supprime les règles globales
   - *Toutes portées* - supprime toutes les règles persistées
3. Vérifiez le nombre de règles concernées affiché dans la boîte de dialogue, puis cliquez sur **Révoquer**.
   ![Dialogue Tout révoquer : le sélecteur de portée et le bouton de révocation](/img/operator-help/controle-configurer-les-permissions-de-fichiers-1bis.png)

## Règles du chat (Apollia Chat)

La section **Chat - Apollia** liste les outils auto-approuvés pour toutes les sessions du chat libre. Ces règles sont créées via **"Toujours autoriser"** dans le chat et persistent d'une session à l'autre. Révoquez-les individuellement ici pour que l'outil redemande confirmation lors de la prochaine invocation depuis le chat. Les exécuteurs de code (`bash_executor`, `python_executor`) n'y figurent jamais : ils ne peuvent pas être auto-approuvés en bloc et repassent toujours par une confirmation par invocation.

## Consulter l'audit récent

En bas de la page, la section **Audit récent** (lecture seule) liste les 20 dernières décisions de permission : outil, décision (allow / deny), portée, numéro de règle appliquée et agent impliqué. Cela permet de vérifier qu'une règle est bien appliquée sans avoir à lancer un agent.

## Si ça ne marche pas

- **Aucune autorisation affichée** : aucune règle persistée n'existe pour les filtres sélectionnés. Réinitialisez les filtres ou exécutez un agent et accordez une autorisation persistée via la carte d'approbation.
- **La règle revient après révocation** : un autre agent (ou une configuration globale) crée la même règle automatiquement. Vérifiez vos agents ou contactez le support.
- **Le bouton "Tout révoquer" est grisé** : la liste est vide - il n'y a rien à révoquer pour les filtres actuels.

> **Référence technique :** [Référence Apollia](/reference)
