---
title: Installer Apollia sur macOS
slug: /operator-help/installation/install-on-macos
sidebar_position: 1
---

# Installer Apollia sur macOS

Apollia est distribué pour macOS depuis la page de publication,
`https://github.com/Apollia-OS/apollia-os/releases`, qui attache trois fichiers
pour cette plateforme :

- **`Apollia-OS_<version>_aarch64.dmg`** (recommandé) : application desktop complète, à glisser dans `Applications`.
- **`apollia-os-macos-silicon.tar.gz`** : bundle CLI seul (power users).
- **`Apollia-OS.app.tar.gz`** : la charge que le mécanisme de mise à jour intégré télécharge, avec sa signature `.sig`. Ce n'est pas un format d'installation ; ignorez-le pour une première installation.

## Pré-requis

- macOS 13 (Ventura) ou plus récent.
- Apple Silicon (M1, M2, M3, M4). Le support Intel via Rosetta n'est pas officiel pour v0.1.0.
- 4 Go de RAM libres minimum. La liste curée proposée à l'accueil est Qwen3 en quatre tailles, 4B, 8B, 14B et 30B-A3B ; 8 Go est un plancher confortable pour le 8B.
- 10 Go d'espace disque pour le bundle + un modèle quantifié.

## Installation (DMG)

1. Téléchargez `Apollia-OS_<version>_aarch64.dmg` depuis la page de publication ci-dessus.
2. Double-cliquez sur le fichier, glissez `Apollia OS.app` dans votre dossier `Applications` **personnel**, celui qui est sous votre répertoire d'accueil. Les commandes de vérification ci-dessous utilisent ce chemin ; l'installation dans le `/Applications` système fonctionne aussi, il faut alors les adapter.
3. Premier lancement : la publication est signée et notariée avec un Apple Developer ID quand la chaîne dispose des secrets de signature, et signée en ad hoc sinon. Une compilation ad hoc est refusée au premier double-clic. Faites un clic droit (ou Contrôle-clic) sur l'icône, choisissez **Ouvrir**, puis confirmez **Ouvrir** dans la boîte de dialogue ; macOS retient le choix. Si elle reste bloquée, retirez une fois l'attribut de quarantaine depuis un terminal :

   ```sh
   xattr -dr com.apple.quarantine ~/Applications/Apollia\ OS.app
   ```

4. L'app démarre le daemon `apollia-os` automatiquement. Le daemon sert l'inférence LLM locale via le moteur embarqué `llama-server` et lance le runner de reconnaissance vocale (STT).

## Vérification

Depuis un terminal :

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os --version
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os doctor
```

`doctor` vérifie le répertoire de données, le fichier de configuration, les deux
bases, le répertoire des modèles, Python, la posture de bac à sable et la socket
du runtime. Il ne détecte **pas** votre GPU. La commande qui rapporte le matériel
détecté est une autre, et elle a besoin du daemon démarré :

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os model hardware --json
```

Elle répond la RAM totale, le processeur et l'accélérateur détecté, sondé sur la
machine. Demandez `--json` : le rendu texte de cette commande n'affiche rien
aujourd'hui.

## Fermer l'application

Fermer la fenêtre par le bouton rouge la masque et laisse Apollia active
derrière l'icône de la barre de menus, ce qui est la convention macOS : le
daemon, le moteur `llama-server` et le runner restent en place, et l'icône de la
barre de menus rouvre la fenêtre. Quitter réellement passe par `Cmd+Q`, le menu
de l'application ou **Quitter** dans l'icône de la barre de menus, ce qui arrête
tous les processus d'arrière-plan.

Windows et Linux diffèrent ici : la croix y quitte directement.

## Composants embarqués

Le bundle macOS contient :

- `llama-server` : le moteur d'inférence LLM local (modèles GGUF, accélération Metal). Le daemon le lance et le supervise automatiquement.
- `apollia-runner-metal` et `apollia-runner-cpu` : le runner de reconnaissance vocale (STT, whisper), avec accélération Metal ou repli CPU.

Le daemon sélectionne automatiquement l'accélération Metal au démarrage.

## Mettre à jour

Voir [Mettre à jour Apollia](./mettre-a-jour-apollia.md).

## Désinstallation

Glissez `Apollia OS.app` dans la corbeille. Données utilisateur :

```sh
rm -rf ~/.apollia
```
