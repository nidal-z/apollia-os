---
sidebar_position: 6.5
title: Installer l'application de bureau
---

# Installer l'application de bureau

Ce guide s'adresse aux personnes qui souhaitent utiliser Apollia comme une
application de bureau classique : télécharger un installeur, l'installer, puis
la lancer. Pas de compilateur, pas de ligne de commande, pas de récupération
des sources. Si vous préférez compiler depuis les sources ou utiliser le
runtime en ligne de commande, suivez [Installer et exécuter le runtime](/how-to/install-and-run).

Apollia est local-first. L'application de bureau s'exécute sur votre machine,
y stocke ses données, et ne nécessite pas de compte pour démarrer.

## Plateformes prises en charge

Trois plateformes sont prises en charge. Les installeurs sont publiés sur la
page GitHub Releases du projet :

| Plateforme | Installeur | Remarques |
|---|---|---|
| macOS (Apple Silicon) | `.dmg` | Gatekeeper affiche un avertissement au premier lancement, sauf si la build est signée avec un Developer ID (voir ci-dessous). |
| Windows (x86-64) | `.msi` / `.exe` | SmartScreen avertit tant que la build n'est pas signée Authenticode. Nécessite le runtime WebView2, préinstallé sur les versions récentes de Windows et téléchargé par l'installeur dans le cas contraire. |
| Linux (x86-64) | `.AppImage` / `.deb` | Nécessite WebKitGTK, présent sur les distributions de bureau récentes. |

Le confinement des outils n'est pas uniforme entre les trois, et cette
différence n'est pas cosmétique : voir [ce qui est confiné et ce qui ne l'est
pas](/explanation/agent-trust-model). Sur Windows, il n'y en a aucun.

Si l'installeur correspondant à votre plateforme n'est pas attaché à la
release que vous consultez, compilez l'application depuis les sources avec
[Installer et exécuter le runtime](/how-to/install-and-run), qui couvre la
mise en route développeur sur les trois systèmes d'exploitation.

## Téléchargement

1. Ouvrez la page des releases : `https://github.com/Apollia-OS/apollia-os/releases`.
2. Choisissez la dernière release.
3. Dans **Assets**, téléchargez le fichier correspondant à votre plateforme :
   - macOS : le `.dmg`
   - Windows : le `.msi` (ou l'installeur `.exe`)
   - Linux : le `.AppImage` ou le `.deb`

Les noms de fichiers portent le nom du produit avec une espace, exactement
tels que le bundler les écrit :

<!-- release-artifacts:begin - genere depuis packaging/artifacts.json par docs/site/regen.sh ; ne pas editer a la main -->
| Plateforme | Fichiers sur la page de release |
|---|---|
| macOS (Apple Silicon) | `Apollia OS_0.1.0-1_aarch64.dmg` |
| Linux (x86-64) | `Apollia OS_0.1.0-1_amd64.AppImage`, `Apollia OS_0.1.0-1_amd64.deb` |
| Windows (x86-64) | `Apollia OS_0.1.0-1_x64_en-US.msi`, `Apollia OS_0.1.0-1_x64-setup.exe` |
| Linux (x86-64), moteur CUDA | `Apollia OS_0.1.0-1_amd64-cuda.AppImage`, `Apollia OS_0.1.0-1_amd64-cuda.deb` |
| Windows (x86-64), moteur CUDA | `Apollia OS_0.1.0-1_x64_en-US-cuda.msi`, `Apollia OS_0.1.0-1_x64-setup-cuda.exe` |
<!-- release-artifacts:end -->

Chaque release fournit également un fichier `SHA256SUMS`. Pour vérifier que
votre téléchargement est intact, comparez son empreinte à ce fichier.

```sh
# macOS / Linux
shasum -a 256 <downloaded-file>
```

```powershell
# Windows (PowerShell)
Get-FileHash .\<downloaded-file> -Algorithm SHA256
```

Chaque fichier publié porte aussi une signature Sigstore détachée (`.sig`) et
son certificat de signature (`.pem`), produits par le pipeline de release avec
`cosign` sans clé. Pour vérifier l'origine d'un téléchargement, et pas
seulement son intégrité :

```sh
cosign verify-blob <file> \
  --certificate <file>.pem --signature <file>.sig \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Apollia-OS/'
```

`cosign` est un outil séparé du projet Sigstore ; le runtime ne l'embarque
pas, et `apollia-os update` ne vérifie que l'empreinte SHA256.

## Installer et lancer

### macOS

1. Double-cliquez sur le `.dmg` téléchargé.
2. Glissez **Apollia OS** dans le dossier **Applications**.
3. Éjectez l'image disque, puis ouvrez **Apollia OS** depuis Applications ou
   Launchpad.

Une build macOS est signée et notarisée avec un Apple Developer ID lorsque le
pipeline de release dispose des secrets de signature, et signée ad hoc dans le
cas contraire. Si la vôtre est signée ad hoc, le premier lancement peut
indiquer que l'application "ne peut pas être ouverte car le développeur ne
peut pas être vérifié". Pour l'ouvrir malgré tout :

- Faites un clic droit (ou Ctrl-clic) sur l'icône de l'application et
  choisissez **Ouvrir**, puis confirmez **Ouvrir** dans la boîte de dialogue.
  macOS mémorise ce choix pour les lancements suivants.
- Si l'application reste bloquée, autorisez-la une fois depuis le Terminal :

  ```sh
  xattr -dr com.apple.quarantine "/Applications/Apollia OS.app"
  ```

Apollia OS nécessite macOS 13 (Ventura) ou une version plus récente.

### Windows

1. Double-cliquez sur l'installeur `.msi` (ou `.exe`).
2. Suivez les étapes de l'installeur, puis lancez **Apollia OS** depuis le
   menu Démarrer.

Windows SmartScreen peut avertir que l'éditeur n'est pas reconnu. L'installeur
indique Apollia comme éditeur, mais il n'est pas encore signé avec un
certificat Authenticode, ce que vérifie SmartScreen. Choisissez **Plus
d'infos**, puis **Exécuter quand même** pour continuer.

L'application affiche son interface avec Microsoft Edge WebView2. Il est
préinstallé sur les systèmes Windows 11 récents et les systèmes Windows 10 à
jour. Là où il est absent, l'installeur le télécharge, ce qui nécessite alors
une connexion réseau. Si l'application signale l'absence du runtime WebView2,
installez le "Evergreen Bootstrapper" depuis la page de téléchargement du
runtime WebView2 de Microsoft, puis relancez l'application.

Le bouton de fermeture de la fenêtre quitte l'application sur Windows, ce qui
arrête le runtime ainsi que les processus d'inférence et de reconnaissance
vocale en arrière-plan. Utilisez l'icône dans la zone de notification pour la
garder active tout en masquant la fenêtre. macOS fait exception : fermer une
fenêtre laisse l'application résidente derrière l'icône de la barre de menus.

### Linux

Le `.AppImage` est portable et ne nécessite aucune installation. Son nom de
fichier contient une espace, protégez-la par des guillemets :

```sh
chmod +x "Apollia OS_"*.AppImage
"./Apollia OS_"*.AppImage
```

Le `.deb` s'installe pour l'ensemble du système sur Debian et Ubuntu :

```sh
sudo apt install "./Apollia OS_"*.deb
```

Après avoir installé le `.deb`, lancez **Apollia OS** depuis le menu des
applications de votre bureau. L'application repose sur le runtime système
WebKitGTK ; sur un système minimal, installez `libwebkit2gtk-4.1-0` si la
fenêtre ne s'ouvre pas.

## Premier lancement

Au premier lancement, l'application crée son répertoire de données et vous
guide à travers un court parcours d'accueil (choix d'un backend de modèle,
octroi des permissions). Vous pouvez démarrer avec un backend cloud ou pointer
l'application vers un fichier de modèle local que vous possédez déjà. Pour
<!-- claim:desktop-downloads-models-in-app -->
exécuter une inférence entièrement locale, téléchargez un GGUF depuis
l'application elle-même : le parcours d'accueil le propose, et Réglages,
Model Hub fait de même par la suite. Déposer un fichier `.gguf` à la main dans
`~/.apollia/models/` fonctionne aussi, avant ou après le parcours d'accueil.

## Où l'application stocke vos données

Tout ce que l'application conserve se trouve dans un unique répertoire de
votre dossier personnel :

| Chemin | Contenu |
|---|---|
| `~/.apollia/` | La racine des données de l'application, créée au premier lancement. |
| `~/.apollia/models/` | Les fichiers de modèle `.gguf` locaux que vous fournissez. |
| `~/.apollia/api-token` | Le jeton porteur pour l'API HTTP locale. |

Rien ne quitte votre machine, sauf si vous configurez explicitement un backend
cloud ou un connecteur. Pour réinitialiser l'application à un état vierge,
quittez-la et supprimez `~/.apollia/` (cela supprime vos agents, votre mémoire
et votre configuration, alors sauvegardez d'abord ce dossier si vous y
tenez).

Sur Windows, la racine des données est le dossier `.apollia` situé dans votre
profil utilisateur (`%USERPROFILE%\.apollia`).

## Prochaines étapes

- Comprendre le runtime derrière l'application : [Installer et exécuter le runtime](/how-to/install-and-run).
- Accélérer l'inférence locale : [Accélérer l'inférence locale](/how-to/accelerate-local-inference).
- Écrire votre propre agent : [Votre premier agent](/tutorials/your-first-agent).
