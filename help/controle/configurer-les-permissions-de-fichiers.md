# Configurer les permissions de fichiers

> Pour les operators qui veulent pré-définir ou ajuster les règles de permission d'accès aux fichiers, sans attendre qu'un agent demande chaque approbation.

## Prérequis

- L'application est ouverte et au moins un agent est installé.
- Vous savez quels dossiers vos agents doivent (ou ne doivent pas) toucher.
- Vous avez identifié les opérations à autoriser : lecture, écriture, exécution.

## Étapes

1. Dans la sidebar, cliquez sur l'icône **Settings** (engrenage en bas).

2. Dans le menu de gauche, sélectionnez l'onglet **Permissions**. Vous voyez le tableau des règles déjà créées (issues de cartes d'approbation passées ou ajoutées manuellement).
   `[SCREENSHOT: page Settings > Permissions, tableau avec colonnes Agent, Chemin, Opération, Périmètre, Actions]`

3. Pour créer une nouvelle règle, cliquez sur **+ Nouvelle règle** en haut à droite.

4. Sélectionnez l'**agent concerné** dans la liste déroulante, ou choisissez **Tous les agents** pour appliquer la règle de manière globale.

5. Saisissez le **chemin** du dossier (par exemple : `~/Rapports/`). Cliquez sur l'icône dossier pour parcourir et choisir visuellement.
   `[SCREENSHOT: formulaire de création de règle, champ Agent, champ Chemin avec icône dossier, listes déroulantes Opération et Périmètre]`

6. Choisissez l'**opération** autorisée :
   - **Lecture** — l'agent peut consulter les fichiers.
   - **Écriture** — l'agent peut créer ou modifier les fichiers.
   - **Exécution** — l'agent peut lancer des scripts ou commandes depuis ce chemin.

7. Choisissez le **périmètre** :
   - **Ce dossier exact** — uniquement les fichiers à la racine.
   - **Ce dossier et ses sous-dossiers** — récursif.
   - **N'importe quel dossier** — règle globale (à utiliser avec prudence).

8. Cliquez sur **Créer**. La règle apparaît immédiatement dans le tableau et s'applique aux exécutions à venir.

9. Pour modifier une règle, cliquez sur l'icône crayon sur sa ligne. Pour la supprimer, cliquez sur la croix : les actions équivalentes redemanderont alors une approbation manuelle.
   `[SCREENSHOT: tableau de règles, ligne sélectionnée avec icônes crayon et croix visibles à droite]`

## Vérification

La règle apparaît dans le tableau avec ses paramètres. Lancez un agent qui touche au chemin concerné : aucune carte d'approbation ne s'affiche, l'action est exécutée directement et tracée dans l'audit trail.

## Si ça ne marche pas

- **Une approbation est encore demandée alors que la règle existe** : vérifiez que le périmètre couvre bien le chemin réel (un dossier exact ne couvre pas les sous-dossiers).
- **L'agent ne peut pas écrire malgré la règle** : l'opération sélectionnée est peut-être *Lecture*. Modifiez la règle et passez sur *Écriture*.
- **Trop de règles, on s'y perd** : utilisez le filtre par agent en haut du tableau, ou supprimez les règles obsolètes pour repartir sur une base claire.

> **Référence technique :** [Securite-Guardrails](https://github.com/nidal-z/apollia-os/wiki/Securite-Guardrails)
