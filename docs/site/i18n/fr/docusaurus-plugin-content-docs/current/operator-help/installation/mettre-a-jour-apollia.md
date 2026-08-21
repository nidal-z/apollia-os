---
title: Mettre à jour Apollia
sidebar_position: 8
---

# Mettre à jour Apollia

Apollia se met à jour depuis les versions publiées sur GitHub. Deux chemins, un
par surface. Aucun des deux ne touche à vos données.

## Depuis l'application

**Réglages > Système** affiche un panneau de mise à jour, également accessible
depuis **Réglages > À propos**.

1. Cliquez sur **Vérifier les mises à jour**. Apollia interroge la page des
   versions publiées et compare avec la vôtre.
2. Si une version plus récente existe, elle s'affiche avec ses notes.
3. Lancez l'installation. Une barre de progression suit le téléchargement.
4. Redémarrez l'application quand elle vous le demande.

## Depuis la ligne de commande

```sh
apollia-os update --check   # regarde s'il y a du neuf, sans rien installer
apollia-os update           # télécharge, vérifie, remplace
```

La mise à jour se déroule en trois temps, et chacun peut échouer sans conséquence :

- le binaire de votre plateforme est téléchargé dans un fichier temporaire ;
- sa somme de contrôle SHA256 est vérifiée. **En cas d'écart, l'opération
  s'arrête sans toucher au binaire en place** ;
- le remplacement est atomique. Vous n'obtenez jamais un binaire à moitié écrit.

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
- **Le remplacement échoue** sous Windows si l'application tourne encore. Fermez
  Apollia, puis relancez la commande.
- **Rien de neuf n'est proposé alors qu'une version existe** : l'artefact de
  votre plateforme n'est peut-être pas attaché à cette version. Regardez la page
  des versions publiées et installez à la main si besoin.
