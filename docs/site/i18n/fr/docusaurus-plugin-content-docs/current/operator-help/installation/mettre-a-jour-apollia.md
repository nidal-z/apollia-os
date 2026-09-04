---
title: Mettre à jour Apollia
slug: /operator-help/installation/update-apollia
sidebar_position: 8
---

# Mettre à jour Apollia

Apollia se met à jour depuis les versions publiées sur GitHub. Deux chemins, un
par surface. Aucun des deux ne touche à vos données.

## Quelles installations savent se mettre à jour

Toutes les installations n'ont pas de chemin de mise à jour, et savoir laquelle
est la vôtre évite une recherche.

- **L'application de bureau** se met à jour depuis le `.dmg` macOS, l'`.AppImage`
  Linux et les installeurs Windows. Un `.deb` Linux, celui en CUDA compris, n'a
  pas de mise à jour intégrée : Tauri remplace une AppImage sur place et n'a pas
  d'équivalent pour un paquet, donc réinstallez le `.deb` plus récent à la main.
- **La ligne de commande** se met à jour sur quatre couples : macOS Apple
  Silicon, Linux x86_64, Linux aarch64 et Windows x86_64. macOS Intel n'a aucune
  archive publiée, et la commande le dit au lieu de deviner un nom de fichier.
- Une installation Vulkan ou CUDA se met à jour depuis l'archive CPU de la même
  plateforme. C'est délibéré : le binaire `apollia-os` est identique entre les
  variantes de moteur d'une même plateforme, et la commande ne remplace que ce
  binaire.

## Depuis l'application

**Réglages > Système** affiche un panneau de mise à jour, également accessible
depuis **Réglages > À propos**.

1. Cliquez sur **Vérifier les mises à jour**. Apollia lit le manifeste de mise à
   jour attaché à la dernière version publiée et compare sa version à la vôtre.
2. Si une version plus récente existe, le panneau affiche son numéro. Le
   manifeste ne porte pas de journal des changements : ce qu'apporte cette
   version est sur la page de publication, pas dans le panneau.
3. Lancez l'installation. Une barre de progression suit le téléchargement.
4. Quittez Apollia et rouvrez-la une fois l'installation terminée.

## Depuis la ligne de commande

```sh
apollia-os update --check   # regarde s'il y a du neuf, sans rien installer
apollia-os update           # télécharge, vérifie, remplace
```

La seconde commande demande une confirmation, `[y/N]`, avant de télécharger quoi
que ce soit. Passez `--yes` pour y répondre d'avance, dans un script par exemple.

La mise à jour se déroule en trois temps, et chacun peut échouer sans conséquence :

- l'archive de release de votre plateforme (nommée par le contrat de bundle,
  `apollia-os-<preset>.tar.gz` ou `.zip`) est téléchargée dans un répertoire
  temporaire ;
- sa somme de contrôle SHA256 est vérifiée. **En cas d'écart, l'opération
  s'arrête sans toucher au binaire en place** ;
- le binaire `apollia-os` est extrait de l'archive et mis à la place de celui qui
  tourne. Le Python et les runners embarqués de votre installation restent tels
  quels.

Un verrou empêche deux mises à jour simultanées.

## Ce qui arrive à vos données

**Rien.** La mise à jour remplace un exécutable, pas votre répertoire
`~/.apollia`. Vos sessions, projets, agents, mémoire, journal d'audit et
`apollia.toml` sont conservés tels quels.

Un point à connaître avant de mettre à jour un poste que vous ne pourrez pas
réinstaller facilement : **il n'existe pas de chemin de retour arrière**.
`apollia-os update` ne prend pas de version cible, il installe la plus récente,
et rien n'est prévu pour réinstaller la précédente ni pour ramener vos bases à un
état antérieur. Une version plus ancienne relancée sur des données écrites par
une plus récente n'est pas un cas testé.

Si vous voulez pouvoir revenir en arrière, copiez `~/.apollia` avant la mise à
jour :

```sh
cp -R ~/.apollia ~/.apollia.backup-$(date +%F)
```

Sur Windows :

```powershell
Copy-Item -Recurse "$env:USERPROFILE\.apollia" "$env:USERPROFILE\.apollia.backup"
```

## Si ça ne marche pas

- **« SHA256 mismatch »** : le téléchargement a été corrompu ou interrompu.
  Relancez. Votre binaire en place n'a pas été modifié.
- **Le remplacement échoue sous Windows.** La ligne de commande remplace
  l'exécutable depuis lequel elle tourne elle-même, et Windows refuse d'écraser
  un fichier tenu ouvert par un processus en cours. Fermer l'application de
  bureau n'y change rien, le verrou étant tenu par la commande de mise à jour.
  Sous Windows, prenez plutôt l'installeur plus récent sur la page de
  publication.
- **« no release has been published yet »**, ou rien de proposé alors que vous
  voyez une version sur GitHub : les deux chemins lisent la dernière version
  *publiée*, ce qui écarte les brouillons et les préversions. Une version encore
  en brouillon, ou marquée préversion, leur est invisible tant qu'elle n'est pas
  publiée pour de bon.
- **Rien de neuf n'est proposé alors qu'une version publiée existe** : l'artefact
  de votre plateforme n'est peut-être pas attaché à cette version. Regardez la
  page de publication, `https://github.com/Apollia-OS/apollia-os/releases`, et
  installez à la main si besoin.
