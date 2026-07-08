# Installer Apollia sur macOS

Apollia est distribué pour macOS en deux formats :

- **`.dmg`** (recommandé) : application desktop complète, à glisser dans `Applications`.
- **`apollia-os-macos-silicon.tar.gz`** : bundle CLI seul (power users).

## Pré-requis

- macOS 13 (Ventura) ou plus récent.
- Apple Silicon (M1, M2, M3, M4). Le support Intel via Rosetta n'est pas officiel pour v0.1.0.
- 4 Go de RAM libres minimum (8 Go recommandé pour Mistral-7B).
- 10 Go d'espace disque pour le bundle + un modèle quantifié.

## Installation (DMG)

1. Téléchargez `Apollia-OS_<version>.dmg` depuis la page Releases.
2. Double-cliquez sur le fichier, glissez `Apollia OS.app` dans le dossier `Applications`.
3. Premier lancement : `Cmd+clic` sur l'icône puis `Ouvrir` (Gatekeeper bloque les apps non-signées).
4. L'app démarre le daemon `apollia-os` automatiquement et lance le sidecar runner Metal.

## Vérification

Depuis un terminal :

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os --version
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os doctor --json | jq .gpu
```

Vous devriez voir `vendor: Apple`, `recommended_backend: Metal`.

## Backends embarqués

Le bundle macOS contient :

- `apollia-runner-metal` : inférence GPU Apple Silicon (par défaut).
- `apollia-runner-cpu` : fallback universel.

Le daemon sélectionne automatiquement Metal au boot. Forcer le CPU :

```toml
# ~/.apollia/apollia.toml
[llm.runner]
backend = "cpu"
```

## Désinstallation

Glissez `Apollia OS.app` dans la corbeille. Données utilisateur :

```sh
rm -rf ~/.apollia
```
