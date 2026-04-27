# Consulter et nettoyer la mémoire

> Pour les operators qui veulent voir ce que leurs IA ont retenu, et supprimer ce qui ne devrait plus être là.

## Prérequis

- Au moins un agent qui a déjà conversé ou exécuté une tâche.
- Ou des préférences utilisateur déjà saisies dans **Mémoire**.

## Étapes

1. Dans la sidebar, cliquez sur **Mémoire**. La page affiche tout ce que vos IA ont retenu, organisé en trois onglets.
   `[SCREENSHOT: page Mémoire avec 3 onglets : "Mes Préférences" (ou "Mémoire Utilisateur"), "Mémoire", "Outils"]`

2. Ouvrez le premier onglet (**Mes Préférences** en mode Operator, **Mémoire Utilisateur** en mode Builder) pour voir vos préférences globales, partagées entre tous les agents (langue, format, ton, extractions récentes).

3. Ouvrez l'onglet **Mémoire** pour voir les entrées de mémoire des agents, organisées par namespace. Utilisez le sélecteur de namespace en haut pour naviguer entre les différents espaces de mémoire.

4. Ouvrez l'onglet **Outils** pour voir la liste des outils disponibles (natifs, MCP, Python) avec leur version et description.

5. Dans l'onglet **Mémoire**, utilisez la **barre de recherche** en haut pour retrouver une entrée précise. Tapez quelques mots-clés — les entrées correspondantes s'affichent en résultat.
   `[SCREENSHOT: barre de recherche avec terme tapé et résultats filtrés en dessous]`

6. Pour **supprimer une entrée** précise, cliquez sur la croix en bout de ligne, puis confirmez. L'entrée disparaît immédiatement et ne reviendra pas.

7. Pour **vider toute la mémoire d'un namespace**, cliquez sur le bouton **Vider** disponible et confirmez. Cette action est irréversible.

## Vérification

L'entrée supprimée n'apparaît plus dans la liste, même après recharge de la page. Une recherche par mot-clé sur cette entrée ne retourne plus de résultat.

## Si ça ne marche pas

- **La page est vide** : aucun agent n'a encore généré de mémoire. Lancez une conversation et revenez.
- **La suppression échoue** : l'agent est en train d'écrire dans la mémoire, attendez quelques secondes et réessayez.
- **Vous voulez tout effacer d'un coup** : voir [Réinitialiser Apollia (factory reset)](../troubleshooting/reinitialiser-apollia-factory-reset.md) pour la procédure de réinitialisation complète.

> **Référence technique :** [Briques-Memory-Engine](https://github.com/nidal-z/apollia-os/wiki/Briques-Memory-Engine) — types de mémoire, durées de rétention par défaut, limites.
