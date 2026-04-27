# Gérer les autorisations d'outils

> Pour les operators qui veulent visualiser, filtrer ou révoquer les autorisations accordées aux outils d'un agent — sans attendre qu'une action déclenche une nouvelle carte d'approbation.

## Prérequis

- L'application est ouverte et au moins un agent a été exécuté.
- Des autorisations ont été accordées lors d'une session précédente (lors d'une approbation, vous avez choisi « Toujours autoriser » ou une portée persistée).

## Comment les autorisations sont-elles créées ?

Les règles apparaissent automatiquement lorsqu'un agent demande l'accès à un outil et que vous choisissez une portée persistée dans la carte d'approbation (par exemple : *Ce projet* ou *Partout*). Vous ne créez pas de règle manuellement depuis cet écran : cet onglet sert uniquement à les consulter et à les révoquer.

## Visualiser les autorisations actives

1. Dans la sidebar, cliquez sur l'icône **Settings** (engrenage en bas).

2. Dans le menu de gauche, sélectionnez **Autorisations**.
   `[SCREENSHOT: page Settings > Autorisations, liste de cartes d'autorisation (PermissionRuleCard) avec badges de portée]`

3. Le panneau central affiche toutes les règles actives sous forme de **liste de cartes**. Chaque carte indique :
   - le **nom de l'outil** autorisé (ex. : `bash`, `file_write`, `mcp_call`)
   - un **badge de portée** : *Session en cours*, *Ce projet* ou *Partout*
   - le **préfixe d'argument** (si la règle est limitée à certaines invocations)
   - la **date d'expiration** ou la mention *Permanente*
   - l'**auteur** de la décision (agent ou utilisateur)

4. Utilisez les filtres dans le panneau de gauche pour affiner la liste :
   - **Portée** : *Toutes*, *Session*, *Ce projet*, *Partout*
   - **Outil** : sélectionnez un outil précis dans la liste des outils présents

## Révoquer une autorisation individuelle

1. Repérez la carte correspondant à la règle à supprimer.
2. Cliquez sur le bouton **Révoquer** (icône corbeille, à droite de la carte).
3. Un message de confirmation apparaît brièvement. La carte disparaît immédiatement.
   `[SCREENSHOT: carte d'autorisation avec bouton Révoquer visible, toast de confirmation "Règle bash révoquée"]`

Une fois révoquée, l'outil concerné redemandera une approbation manuelle à la prochaine invocation.

## Révoquer toutes les autorisations d'un coup

1. Cliquez sur le bouton rouge **Tout révoquer** en haut à droite.
2. Choisissez la portée à purger :
   - *Session uniquement* — supprime uniquement les règles non persistées (disparaissent de toute façon à la fermeture)
   - *Ce projet* — supprime les règles liées au projet courant
   - *Partout* — supprime les règles globales
   - *Toutes portées* — supprime absolument tout
3. Vérifiez le nombre de règles concernées affiché dans la boîte de dialogue, puis cliquez sur **Révoquer**.
   `[SCREENSHOT: dialog "Tout révoquer", sélecteur de portée, compteur de règles concernées, bouton Révoquer]`

## Consulter l'audit récent

En bas de la page, la section **Audit récent** (lecture seule) liste les 20 dernières décisions de permission : outil, décision (allow / deny), portée, numéro de règle appliquée et agent impliqué. Cela permet de vérifier qu'une règle est bien appliquée sans avoir à lancer un agent.

## Si ça ne marche pas

- **Aucune autorisation affichée** : aucune règle persistée n'existe pour les filtres sélectionnés. Réinitialisez les filtres ou exécutez un agent et accordez une autorisation persistée via la carte d'approbation.
- **La règle revient après révocation** : un autre agent (ou une configuration globale) crée la même règle automatiquement. Vérifiez vos agents ou contactez le support.
- **Le bouton "Tout révoquer" est grisé** : la liste est vide — il n'y a rien à révoquer pour les filtres actuels.

> **Référence technique :** [Securite-Guardrails](https://github.com/nidal-z/apollia-os/wiki/Securite-Guardrails)
