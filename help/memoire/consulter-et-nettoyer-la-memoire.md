# Consulter et nettoyer la mémoire

> Pour les operators qui veulent voir ce que leurs IA ont retenu, et supprimer ce qui ne devrait plus être là.

## Prérequis

- Au moins un agent qui a déjà conversé ou exécuté une tâche.
- Ou des préférences utilisateur déjà saisies dans **Settings → Mémoire utilisateur**.

## Étapes

1. Dans la sidebar, cliquez sur **Mémoire**. La page liste tout ce que vos IA ont retenu, classé par type.
   `[SCREENSHOT: page Mémoire avec onglets Épisodique, Sémantique, Procédural, Utilisateur]`

2. Ouvrez l'onglet **Épisodique** pour voir les événements datés (par exemple : *"L'utilisateur a demandé un rapport hebdo le 12 mars"*).

3. Ouvrez l'onglet **Sémantique** pour voir les faits durables que l'agent a appris (par exemple : *"Le client préfère les bullet points"*).

4. Ouvrez l'onglet **Procédural** pour voir les méthodes que l'agent applique systématiquement (par exemple : *"Toujours envoyer le rapport en PDF"*).

5. Ouvrez l'onglet **Utilisateur** pour voir vos préférences globales, partagées entre tous les agents (langue, format, ton).

6. Pour retrouver une entrée précise, utilisez la **barre de recherche** en haut. Tapez quelques mots-clés (par exemple *"bullet points"*) — les entrées correspondantes remontent en tête.
   `[SCREENSHOT: barre de recherche avec terme tapé et résultats filtrés en dessous]`

7. Pour **supprimer une entrée** précise, cliquez sur la croix en bout de ligne, puis confirmez. L'entrée disparaît immédiatement et ne reviendra pas.

8. Pour **vider toute la mémoire d'un agent**, cliquez sur le bouton **Vider [Nom de l'agent]** en haut de l'onglet, puis tapez le nom pour confirmer. Cette action est irréversible.
   `[SCREENSHOT: dialog de confirmation Vider l'agent avec champ de saisie de confirmation]`

9. Pour modifier une préférence utilisateur, allez dans l'onglet **Utilisateur**, cliquez sur l'entrée, modifiez le texte, puis enregistrez.

## Vérification

L'entrée supprimée n'apparaît plus dans la liste, même après recharge de la page. Une recherche par mot-clé sur cette entrée ne retourne plus de résultat.

## Si ça ne marche pas

- **La page est vide** : aucun agent n'a encore généré de mémoire. Lancez une conversation et revenez.
- **La suppression échoue** : l'agent est en train d'écrire dans la mémoire, attendez quelques secondes et réessayez.
- **Vous voulez tout effacer d'un coup** : voir [troubleshooting/un-agent-est-bloque](../troubleshooting/un-agent-est-bloque.md) pour la procédure de réinitialisation complète.

> **Référence technique :** [Briques-Memory-Engine](https://github.com/nidal-z/apollia-os/wiki/Briques-Memory-Engine) — types de mémoire, durées de rétention par défaut, limites.
