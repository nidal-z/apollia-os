---
title: Installer Apollia sur Windows
slug: /operator-help/installation/install-on-windows
sidebar_position: 2
---

# Installer Apollia sur Windows

Chacun des fichiers ci-dessous est attaché à chaque publication sur la page de
publication, `https://github.com/Apollia-OS/apollia-os/releases`.

- **`Apollia-OS_<version>_x64_en-US.msi`** (recommandé) : installeur Windows standard avec entrées Démarrer + désinstallateur.
- **`Apollia-OS_<version>_x64-setup.exe`** : l'installeur NSIS. Un seul fichier, et il installe au lieu de s'exécuter sur place : il enregistre un désinstallateur dans **Paramètres > Applications**, exactement comme le `.msi`.
- **`Apollia-OS_<version>_x64_en-US-cuda.msi`** et **`Apollia-OS_<version>_x64-setup-cuda.exe`** : les deux mêmes installeurs avec une compilation CUDA du moteur d'inférence, pour une carte NVIDIA.
- **`apollia-os-windows-x86-cpu.zip`** et **`apollia-os-windows-x86-vulkan.zip`** : les deux bundles en ligne de commande, un par moteur, CPU ou Vulkan.

## Pré-requis

- Windows 10 22H2 / Windows 11.
- 4 Go de RAM libres minimum.
- Pour l'inférence sur GPU : un pilote compatible Vulkan, ou le pilote NVIDIA 550+ si vous prenez le bundle CUDA.
- **Pas besoin** d'installer Visual C++ Redistributable : le CRT est statiquement embarqué.

## Installation (MSI)

1. Téléchargez `Apollia-OS_<version>_x64_en-US.msi`.
2. Double-cliquez, suivez l'assistant. Il affiche le nom du produit, l'éditeur
   (Apollia), la version et la double licence MIT / Apache-2.0.
3. L'app apparaît dans le menu Démarrer.

L'installeur n'est pas encore signé par un certificat Authenticode : SmartScreen
signale donc un éditeur inconnu. Choisissez **Informations complémentaires**,
puis **Exécuter quand même**.

Le pare-feu Windows demandera à autoriser `apollia-os.exe`, le moteur d'inférence `llama-server.exe` et `apollia-runner-*.exe` (reconnaissance vocale) au premier lancement : **autorisez-les pour les réseaux privés** (ces composants communiquent avec le daemon en loopback 127.0.0.1).

L'installeur télécharge le runtime WebView2 si votre machine ne l'a pas déjà :
l'installation demande donc une connexion réseau sur une machine qui n'a jamais
fait tourner d'application WebView2. Windows 11 à jour le fournit d'origine.

## Installation (.exe)

L'installeur NSIS demande pour qui se fait l'installation. **Installer pour tous
les utilisateurs** la place dans `C:\Program Files\Apollia OS\`, au même endroit
que le `.msi`. **Installer pour moi uniquement** ne demande aucun droit
d'administrateur et la place sous votre profil utilisateur. Notez la destination
que l'assistant vous montre : la vérification ci-dessous la lit.

## Fermer l'application

<!-- claim:desktop-close-button-quits-outside-macos -->
La croix de fermeture quitte Apollia sous Windows, comme pour n'importe quelle
autre application Windows. Le runtime s'arrête avec elle, ainsi que les deux
processus d'arrière-plan qu'il détient : le moteur d'inférence
`llama-server.exe` et le runner de reconnaissance vocale
`apollia-runner-*.exe`. macOS se comporte différemment, la fermeture d'une
fenêtre n'y étant pas le même geste que quitter : l'app y reste résidente
derrière l'icône de la barre de menus.

<!-- claim:desktop-exit-stops-inference-engine -->
Quitter arrête toujours le moteur d'inférence, quelle que soit la surface
utilisée : la croix, le menu de la zone de notification ou le menu de
l'application. Rien ne reste à occuper de la mémoire vidéo ou un port loopback.
Pour le vérifier, depuis PowerShell après avoir quitté :

```powershell
Get-Process apollia-os, llama-server, apollia-runner-cpu -ErrorAction SilentlyContinue
```

La commande ne doit rien afficher.

<!-- claim:windows-no-console-window -->
Vous ne devriez jamais voir de fenêtre de terminal appartenant à Apollia. Les
processus d'arrière-plan sont des programmes console, et ils sont lancés avec le
drapeau Windows `CREATE_NO_WINDOW` pour qu'aucun n'ouvre sa propre console. Si
une fenêtre de terminal apparaît malgré tout, c'est un défaut à signaler plutôt
qu'une fenêtre à refermer à la main.

## Vérification

Depuis PowerShell. Le chemin ci-dessous est celui d'une installation pour tous
les utilisateurs ; sur une installation `.exe` pour vous seul, remplacez-le par
la destination que l'assistant vous a montrée.

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" --version
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor
```

`doctor` passe huit vérifications : le répertoire de données, le fichier de
configuration, les deux bases, le répertoire des modèles, Python, la posture de
bac à sable et la socket du runtime. Il ne détecte **pas** votre GPU. La commande
qui rapporte le matériel détecté est une autre, et elle a besoin du daemon
démarré :

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" model hardware --json
```

Elle répond la RAM totale, le processeur et l'accélérateur détecté. Demandez
`--json` : le rendu texte de cette commande n'affiche rien aujourd'hui.

## Ce que Windows ne confine pas

<!-- claim:windows-has-no-tool-sandbox -->
Sous Linux, un appel d'outil s'exécute dans des espaces de noms avec des limites
de ressources. Sous Windows il n'y a **aucun confinement** : ni espaces de noms,
ni limites de ressources, le mécanisme Unix n'ayant pas d'équivalent Windows et
la fonction qui l'applique ne faisant rien sur cette plateforme. Un outil lancé
par un agent a les mêmes droits sur votre machine que l'application elle-même.

Cela ne rend pas Windows inutilisable, et cela change ce que vous devriez
déléguer. Les règles de permission et les demandes d'approbation s'appliquent
toujours, et elles sont ici la seule barrière : traitez un « toujours autoriser »
sous Windows comme une autorisation plus large que le même choix sous Linux.

Une conséquence pratique : `bash_executor` exige un shell POSIX dans le `PATH`,
via Git Bash, WSL ou MSYS2, et échoue sans lui.

## Accélération GPU

Deux accélérations, et Windows y répond différemment.

**L'inférence LLM locale tourne déjà sur le GPU.** Les installeurs embarquent une
compilation Vulkan du moteur `llama-server`, et les installeurs `-cuda` une
compilation CUDA pour les cartes NVIDIA ; le bundle en ligne de commande
`apollia-os-windows-x86-vulkan.zip` porte la version Vulkan. Vulkan pilote
indifféremment les cartes NVIDIA, AMD et Intel. Rien à installer et rien à
régler : avec un pilote qui fonctionne, le moteur utilise la carte. Le bundle
`-cpu` est celui à prendre sur une machine sans pilote graphique.

**La dictée reste sur le processeur.** La reconnaissance vocale tourne dans le
sidecar `apollia-runner`, bâti sur whisper, et whisper n'a pas de backend
Vulkan : le binaire `apollia-runner-vulkan.exe` livré dans l'archive Vulkan est
octet pour octet celui du CPU. Le copier dans le répertoire d'installation ne
change rien. Aucun artefact Windows publié aujourd'hui ne porte de runner de
reconnaissance vocale accéléré par le GPU.

## Ce qui change sur Windows

Windows est une plateforme supportée, mais deux points la distinguent des deux
autres et méritent d'être connus avant de confier une tâche à un agent.

**Aucun confinement des outils.** Sur Linux, une commande lancée par un agent
s'exécute dans des espaces de noms isolés et avec des limites de ressources ; sur
macOS, avec des limites de ressources. Sur Windows, ni l'un ni l'autre : une
commande lancée par un agent tourne avec exactement vos droits, sur vos fichiers,
sans plafond de mémoire ni de temps processeur. La contrepartie pratique : ne
faites tourner sur Windows que des agents dont vous avez lu le code, et gardez
l'approbation manuelle active dans le chat.

**L'outil shell exige un shell POSIX.** `bash_executor` cherche un `sh` dans
votre `PATH`. Sans Git Bash, WSL ou MSYS2 installé, tout agent qui utilise cet
outil échoue. Les autres outils, fichiers, web et Python, fonctionnent
normalement.

## Mettre à jour

Voir [Mettre à jour Apollia](./mettre-a-jour-apollia.md).

## Désinstallation

`Paramètres > Applications > Apollia OS > Désinstaller`, pour le `.msi` comme
pour le `.exe`. Données utilisateur :

```powershell
Remove-Item -Recurse "$env:USERPROFILE\.apollia"
```
