# Installer Apollia sur Windows

Apollia est distribué pour Windows x86_64 sous trois formats :

- **`.msi`** (recommandé) : installeur Windows standard avec entrées Démarrer + désinstallateur.
- **`.exe` (NSIS)** : installeur portable single-file.
- **`apollia-os-windows-x86-*.zip`** : bundles CLI par accélérateur (CPU, CUDA, Vulkan).

## Pré-requis

- Windows 10 22H2 / Windows 11.
- 4 Go de RAM libres minimum.
- Pour GPU : driver NVIDIA 550+ (CUDA) ou driver Vulkan-capable.
- **Pas besoin** d'installer Visual C++ Redistributable : le CRT est statiquement embarqué.

## Installation (MSI)

1. Téléchargez `Apollia-OS_<version>_x64.msi`.
2. Double-cliquez, suivez l'assistant.
3. L'app apparaît dans le menu Démarrer.

Le pare-feu Windows demandera à autoriser `apollia-os.exe`, le moteur d'inférence `llama-server.exe` et `apollia-runner-*.exe` (reconnaissance vocale) au premier lancement : **autorisez-les pour les réseaux privés** (ces composants communiquent avec le daemon en loopback 127.0.0.1).

## Vérification

Depuis PowerShell :

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" --version
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor
```

`doctor` passe huit vérifications : le répertoire de données, le fichier de
configuration, les deux bases, le répertoire des modèles, Python, la posture de
bac à sable et la socket du runtime. Il ne détecte **pas** votre GPU, et aucune
commande ne le rapporte : le périphérique d'inférence se configure au lieu de se
sonder. Voir la section ci-dessous.

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

L'inférence LLM locale passe par le moteur embarqué `llama-server`, livré avec le bundle. La reconnaissance vocale (STT) utilise le runner `apollia-runner`, dont l'installeur MSI embarque la variante CPU. Pour accélérer la dictée sur GPU CUDA / Vulkan :

1. Téléchargez `apollia-os-windows-x86-cuda.zip` (ou vulkan).
2. Décompressez et copiez `apollia-runner-cuda.exe` (ou `apollia-runner-vulkan.exe`) dans `C:\Program Files\Apollia OS\`.
3. Relancez l'app.

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

`Paramètres > Applications > Apollia OS > Désinstaller`. Données utilisateur :

```powershell
Remove-Item -Recurse "$env:USERPROFILE\.apollia"
```
