# Activer les context providers

> Pour les operators qui veulent que l'IA arrive briefée sur leur projet sans avoir à coller du contexte à chaque message.

## Prérequis

- Un projet déjà créé.
- (Pour Git) Le dossier racine est un repo git.

## Étapes

1. Dans la sidebar, cliquez sur **Projets**, puis sur la carte du projet à configurer. Le panneau de détail s'ouvre.

2. Faites défiler jusqu'à la section **Context providers**. Trois types de fournisseurs sont disponibles.
   ![section Context providers dans le panneau projet, liste des providers avec toggle ON/OFF](/img/operator-help/projets-activer-les-context-providers-1.png)

3. **Git Status** (`git`) - cliquez sur **Ajouter un fournisseur** et sélectionnez *Git Status*. Actif, ce fournisseur injecte l'état git courant (fichiers modifiés, branch) dans chaque message envoyé aux agents liés au projet.

4. **Arborescence** (`tree`) - ajoutez *Directory Tree* pour inclure la structure de fichiers du dossier racine dans le contexte.

5. **Project Rules** (`rules`) - ajoutez *Project Rules (APOLLIA.md)* pour inclure automatiquement les instructions du fichier `APOLLIA.md` à la racine du projet.

6. Basculez l'interrupteur de chaque fournisseur sur ON ou OFF selon vos besoins.
   ![provider Git Status activé (toggle vert), provider Directory Tree désactivé (toggle gris)](/img/operator-help/projets-activer-les-context-providers-2.png)

7. Pour voir exactement ce qui sera transmis à l'IA, cliquez sur **Aperçu du contexte** (Workspace Snapshot). Un panneau dépliable affiche le contenu de chaque fournisseur actif.
   ![aperçu détaillé d'un context provider avec contenu git diff / arborescence](/img/operator-help/projets-activer-les-context-providers-3.png)

   > **⚠️ Non disponible dans cette version :** un bandeau "Contexte injecté" avec le total estimé de tokens n'est pas encore disponible dans l'interface. Pour estimer la taille du contexte, consultez l'aperçu et tenez compte de la règle approximative : 1 token ≈ 4 caractères.

## Vérification

Ouvrez un chat lié au projet et posez une question précise (par exemple : *"Quels fichiers ont changé cette semaine ?"*). La réponse doit citer des fichiers et des commits réels.

## Si ça ne marche pas

- **L'aperçu Git est vide** : votre dossier n'est pas un repo git ou n'a aucun commit. Initialisez-le ou désactivez le fournisseur.
- **Le provider n'apparaît pas** : cliquez sur **Ajouter un fournisseur** pour le créer s'il n'existe pas encore.
- **Le contexte est trop lourd** : désactivez le provider *Directory Tree* si la structure de fichiers est volumineuse.

> **Concept :** [Explication Apollia](/explanation) - savoir quel fournisseur activer selon le type de projet.
