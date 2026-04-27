# Configurer la rétention de mémoire

> Pour les operators qui veulent contrôler combien de temps leurs IA gardent les informations, et libérer de l'espace automatiquement.

## Prérequis

- Au moins un agent actif ayant déjà généré de la mémoire.
- Vous savez quel type d'information vous voulez garder longtemps, et lequel oublier vite.

## Configuration via le manifest agent

La durée de rétention des entrées de mémoire épisodique se configure dans le **manifest de l'agent**, via le champ `episodic_retention_days`. Cette valeur est définie par le développeur de l'agent, pas depuis l'interface Settings.

Pour les agents que vous développez ou personnalisez, modifiez ce champ dans votre fichier `manifest()` Python :

```python
def manifest(self) -> dict:
    return {
        "name": "mon-agent",
        # ...
        "memory_config": {
            "episodic_retention_days": 30  # 0 = jamais supprimé
        }
    }
```

> **⚠️ Non disponible dans cette version :** la configuration graphique des durées de rétention via des curseurs dans Settings (Épisodique / Sémantique / Procédural) n'est pas encore disponible. L'interface Settings → Mémoire affiche uniquement les préférences utilisateur, sans contrôle de rétention par type.

## Purge manuelle

Pour supprimer des entrées de mémoire expirées ou indésirables :

1. Dans la sidebar, cliquez sur **Mémoire**, puis ouvrez l'onglet **Mémoire**.

2. Utilisez le sélecteur de namespace pour choisir l'espace mémoire de l'agent concerné.

3. Supprimez les entrées individuellement en cliquant sur la croix en bout de ligne.

## Vérification

L'entrée supprimée n'apparaît plus dans la liste. Une recherche par mot-clé sur cette entrée ne retourne plus de résultat.

## Si ça ne marche pas

- **Vous ne voyez pas l'espace mémoire d'un agent** : vérifiez le namespace dans le manifest de l'agent (champ `memory_namespace`).
- **La suppression échoue** : l'agent est peut-être en train d'écrire ; attendez quelques secondes et réessayez.

> **Référence technique :** [Briques-Memory-Engine](https://github.com/nidal-z/apollia-os/wiki/Briques-Memory-Engine) — stratégies de rétention, compromis entre coût et richesse de contexte.
