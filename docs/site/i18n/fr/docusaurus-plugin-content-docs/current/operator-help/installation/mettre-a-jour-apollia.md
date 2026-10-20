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
  Linux et les installeurs Windows. Un `.deb` Linux n'a pas de mise à jour
  intégrée : Tauri remplace une AppImage sur place et n'a pas d'équivalent pour
  un paquet, donc réinstallez le `.deb` plus récent à la main.
- **Une installation faite depuis un ancien installeur `-cuda`** se met à jour
  comme les autres, vers l'installeur unique de son système ; un `-cuda.deb` se
  remplace à la main par le `.deb`. Cet installeur ne porte plus le moteur pour
  les cartes NVIDIA : après la mise à jour, l'écran IA le propose, **Accélérer
  avec votre carte NVIDIA**, et un clic le ramène.
- **La ligne de commande** se met à jour sur quatre couples : macOS Apple
  Silicon, Linux x86_64, Linux aarch64 et Windows x86_64. macOS Intel n'a aucune
  archive publiée, et la commande le dit au lieu de deviner un nom de fichier.
- Une installation Vulkan se met à jour depuis l'archive CPU de la même
  plateforme. C'est délibéré : le binaire `apollia-os` est identique entre les
  variantes de moteur d'une même plateforme, et la commande ne remplace que ce
  binaire.

## Depuis l'application

**Réglages > Général** s'ouvre sur le bloc **Mise à jour de l'application** : la
version installée, le panneau de mise à jour et le **Canal de mise à jour**,
**Stable** ou **Préversion**.

1. Cliquez sur **Vérifier les mises à jour**. Apollia lit le manifeste de mise à
   jour attaché à la dernière version publiée et compare sa version à la vôtre.
2. Si une version plus récente existe, le panneau affiche son numéro, et sous
   **Nouveautés** les notes de cette version quand la publication en porte.
   Sinon, ce qu'apporte cette version est sur la page de publication.
3. Cliquez sur **Installer et relancer**. Une barre de progression suit le téléchargement.
4. Une fois l'installation terminée, Apollia se ferme et se rouvre sur la nouvelle version.

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

**Rien n'est supprimé.** La mise à jour remplace un exécutable, pas votre
répertoire `~/.apollia`. Vos sessions, sources, agents, mémoire, journal
d'audit et `apollia.toml` sont conservés. Une nouvelle version peut migrer les
bases de ce répertoire à son premier démarrage, comme le fait la 0.3.0.

Un point à connaître avant de mettre à jour un poste que vous ne pourrez pas
réinstaller facilement : **il n'existe pas de chemin de retour arrière**.
`apollia-os update` ne prend pas de version cible, il installe la plus récente,
et rien n'est prévu pour réinstaller la précédente ni pour ramener vos bases à un
état antérieur. Une fois que la 0.3.0 les a migrées, la 0.2.0 refuse d'ouvrir
une base dont le schéma est plus récent que le sien.

Si vous voulez pouvoir revenir en arrière, copiez `~/.apollia` avant la mise à
jour :

```sh
cp -R ~/.apollia ~/.apollia.backup-$(date +%F)
```

Sur Windows :

```powershell
Copy-Item -Recurse "$env:USERPROFILE\.apollia" "$env:USERPROFILE\.apollia.backup"
```

## Depuis la 0.2.0-preview {#coming-from-0-2-0-preview}

Les installeurs et l'application installée gardent le nom Apollia OS : la 0.3.0
met donc à jour sur place une installation 0.2.0 au lieu de s'installer à côté ;
la fenêtre et l'interface disent Apollia Desktop. Une installation faite depuis
un ancien installeur `-cuda` est amenée à l'installeur standard de son système,
et l'écran IA propose ensuite le moteur accéléré NVIDIA. Copiez d'abord
`~/.apollia`, comme ci-dessus, si vous risquez de revenir à la 0.2.0.

À son premier démarrage, la 0.3.0 convertit ce que la 0.2.0 a laissé, et
quelques points demandent un geste de votre part :

- **Fournisseurs cloud.** Les backends cloud Anthropic, Mistral et OpenAI sont
  supprimés, et le modèle par défaut passe au moteur local quand il y en a un.
  Un serveur que vous exploitez vous-même est conservé. Pour continuer à
  joindre un modèle hébergé, placez devant lui une passerelle que vous exploitez
  (LiteLLM, par exemple) et déclarez son adresse dans l'écran IA, comme dans
  [Connecter un modèle distant](connecter-un-modele-distant.md).
- **Projets.** Chaque projet devient des sources regroupées sous son nom, son
  dossier de travail une connexion de dossier accordée à aucun agent, et ses
  conversations gardent leur agent. Ce qui n'a pas pu être importé est listé
  dans un rapport, affiché une fois dans l'application et lisible avec
  `apollia-os source report`. Accordez les dossiers importés aux agents qui en
  ont besoin, sur leur fiche. Voir
  [Ce que sont devenus les projets](../projets/what-became-of-projects.md).
- **Opérateur et Builder.** Ils sont remplacés par un seul interrupteur, le
  mode développeur, dans **Réglages > Développeur**, désactivé par défaut. Un
  ancien choix Builder l'active.
- **Dossiers.** Un agent sans dossier accordé n'atteint plus votre dossier
  personnel. Accordez-lui une connexion de dossier, en lecture ou en écriture,
  sur sa fiche.
- **Consentements permanents.** Les règles limitées à un projet ou à une
  session sont converties une fois et journalisées ; une autorisation
  permanente sur un exécuteur de code, ou sur un outil de famille inconnue, est
  retirée et sera demandée à nouveau.
- **Agents écrits en Python.** Un agent ne s'écrit plus en Python : un agent
  qui l'était se réécrit en paquet, sa mission et ses accès comme une fiche, son
  code comme des outils et des workflows.
- **Configuration.** Seul `~/.apollia/apollia.toml` est lu. Un fichier laissé
  dans le répertoire de travail ou sous `~/.config/apollia` est ignoré, et
  `apollia-os doctor` le nomme avec la commande qui le déplace. Les clés que la
  0.3.0 a retirées se chargent encore, avec un avertissement, et ne font rien.

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
  publiée pour de bon. Dans l'application, le canal de mise à jour
  **Préversion** reçoit aussi les versions d'essai.
- **Rien de neuf n'est proposé alors qu'une version publiée existe** : l'artefact
  de votre plateforme n'est peut-être pas attaché à cette version. Regardez la
  page de publication, `https://github.com/Apollia-OS/apollia-os/releases`, et
  installez à la main si besoin.
