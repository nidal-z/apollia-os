---
sidebar_position: 6.5
title: Installer Apollia Desktop
description: Installer Apollia Desktop, l'application de bureau, sur macOS, Windows ou Linux, et aller du téléchargement à une première conversation qui fonctionne.
---

# Installer Apollia Desktop

Ce guide s'adresse aux personnes qui souhaitent utiliser Apollia Desktop,
l'application de bureau, comme n'importe quelle autre : télécharger un installeur, l'installer, puis
la lancer. Pas de compilateur, pas de ligne de commande, pas de récupération
des sources. Si vous préférez compiler depuis les sources ou utiliser le
runtime en ligne de commande, suivez [Installer et exécuter le runtime](/how-to/install-and-run).

Apollia est local-first. Apollia Desktop s'exécute sur votre machine,
y stocke ses données, et ne nécessite pas de compte pour démarrer.

## Quel fichier porte quel nom

L'application s'appelle **Apollia Desktop** : c'est le nom que portent sa
fenêtre, ses menus et cette documentation. Les fichiers qui l'installent, et ce
qu'ils déposent sur le disque, portent encore le nom du moteur qu'elle
embarque, Apollia OS.

| Ce que vous voyez | Son nom |
|---|---|
| L'application : fenêtre, menus, aide | Apollia Desktop |
| Les installeurs de la page de publication | `Apollia-OS_<version>_...` |
| Le fichier de l'application sous macOS | `Apollia OS.app` |
| L'entrée du menu Démarrer et le dossier d'installation sous Windows | `Apollia OS` |
| L'entrée du menu des applications et le dossier de ressources sous Linux | `Apollia OS`, `/usr/lib/Apollia OS/` |
| La ligne de commande livrée à l'intérieur | `apollia-os` |

## Plateformes prises en charge

Trois plateformes sont prises en charge. Les installeurs sont publiés sur la
page GitHub Releases du projet :

| Plateforme | Installeur | Remarques |
|---|---|---|
| macOS (Apple Silicon) | `.dmg` | Gatekeeper affiche un avertissement au premier lancement, sauf si la build est signée avec un Developer ID (voir ci-dessous). |
| Windows (x86-64) | `.msi` / `.exe` | SmartScreen avertit quand la build ne porte pas de signature Authenticode (voir ci-dessous). Nécessite le runtime WebView2, préinstallé sur les versions récentes de Windows et téléchargé par l'installeur dans le cas contraire. |
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
| macOS (Apple Silicon) | `Apollia-OS_0.3.1_aarch64.dmg` |
| Linux (x86-64) | `Apollia-OS_0.3.1_amd64.AppImage`, `Apollia-OS_0.3.1_amd64.deb` |
| Windows (x86-64) | `Apollia-OS_0.3.1_x64_en-US.msi`, `Apollia-OS_0.3.1_x64-setup.exe` |
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

Chaque fichier publié porte aussi un paquet Sigstore (`.cosign.bundle`), qui
réunit sa signature et son certificat de signature, produit par le pipeline de
release avec `cosign` sans clé. Pour vérifier l'origine d'un téléchargement, et
pas seulement son intégrité :

```sh
cosign verify-blob <file> \
  --bundle <file>.cosign.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Apollia-OS/'
```

`cosign` est un outil séparé du projet Sigstore ; le runtime ne l'embarque
pas, et `apollia-os update` ne vérifie que l'empreinte SHA256.

## Installer et lancer

### macOS

1. Double-cliquez sur le `.dmg` téléchargé.
2. Glissez **Apollia OS**, le nom que porte le fichier de l'application, dans
   le dossier **Applications**.
3. Éjectez l'image disque, puis ouvrez-la depuis Applications ou
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

Apollia Desktop nécessite macOS 13 (Ventura) ou une version plus récente. Les
modèles au format GGUF tournent sur le moteur llama.cpp embarqué sur toutes les
versions prises en charge ; les modèles au format MLX, servis par le moteur MLX
que livre le bundle macOS, demandent macOS 14 ou plus récent.

### Windows

1. Double-cliquez sur l'installeur `.msi` (ou `.exe`).
2. Suivez les étapes de l'installeur, puis lancez l'application, sous l'entrée **Apollia OS**, depuis le
   menu Démarrer.

Windows SmartScreen peut avertir que l'éditeur n'est pas reconnu. L'installeur
indique Apollia comme éditeur, mais SmartScreen vérifie une signature
Authenticode, que la chaîne de publication n'appose que lorsqu'elle dispose du
certificat de signature. Sur une build non signée, choisissez **Plus
d'infos**, puis **Exécuter quand même** pour continuer.

L'application affiche son interface avec Microsoft Edge WebView2. Il est
préinstallé sur les systèmes Windows 11 récents et les systèmes Windows 10 à
jour. Là où il est absent, l'installeur le télécharge, ce qui nécessite alors
une connexion réseau. Si l'application signale l'absence du runtime WebView2,
installez le "Evergreen Bootstrapper" depuis la page de téléchargement du
runtime WebView2 de Microsoft, puis relancez l'application.

Le bouton de fermeture de la fenêtre quitte l'application sur Windows, ce qui
arrête le runtime ainsi que les processus d'inférence et de reconnaissance
vocale en arrière-plan. L'icône de la zone de notification ouvre la fenêtre,
montre les validations en attente et quitte ; elle ne masque pas la fenêtre en
gardant le runtime actif, réduisez donc la fenêtre plutôt. Pour que les agents
continuent sans aucune fenêtre, activez le mode développeur et utilisez
l'interrupteur de **Réglages > Développeur** qui installe le runtime comme
service de votre session. macOS fait exception : fermer une fenêtre laisse
l'application résidente derrière l'icône de la barre de menus.

### Linux

Le `.AppImage` est portable et ne nécessite aucune installation. Son nom de
fichier contient une espace, protégez-la par des guillemets :

```sh
chmod +x Apollia-OS_*.AppImage
./Apollia-OS_*.AppImage
```

Le `.deb` s'installe pour l'ensemble du système sur Debian et Ubuntu :

```sh
sudo apt install ./Apollia-OS_*.deb
```

Après avoir installé le `.deb`, lancez l'application, sous l'entrée **Apollia OS**, depuis le menu des
applications de votre bureau. L'application repose sur le runtime système
WebKitGTK ; sur un système minimal, installez `libwebkit2gtk-4.1-0` si la
fenêtre ne s'ouvre pas.

## Premier lancement

Au premier lancement, l'application crée son répertoire de données,
`~/.apollia/`, et ouvre une fenêtre de premier lancement. Elle compte sept
étapes ; le haut de la fenêtre indique où vous en êtes, et **Configurer plus
tard**, en haut à droite, la ferme à toute étape.

1. **Langue** : français ou anglais. Tout ce qui suit cette étape s'y conforme.
2. **IA** : où tourne l'IA de vos agents. C'est la seule étape que l'on ne peut
   pas passer.
   - **Sur ce poste** : Apollia liste les fichiers `.gguf` qu'il trouve déjà
     (dans `~/.apollia/models/`, `~/Downloads/` et le dossier des modèles de
     LM Studio) et une liste de **Modèles recommandés**, dimensionnés pour votre
     machine. Un clic sur l'un d'eux lance son téléchargement ; vous pouvez
     poursuivre les étapes suivantes pendant qu'il arrive. **Rechercher sur
     HuggingFace** cherche un autre modèle.
   - **Utiliser un serveur** : l'adresse d'un serveur compatible OpenAI que
     votre organisation exploite ou que vous utilisez déjà (Ollama, vLLM,
     LM Studio, une passerelle LiteLLM), sa clé s'il en demande une, puis
     **Tester** et le modèle que vos agents utiliseront.
3. **Recherche, voix** : deux modèles plus petits, facultatifs. L'un permet à
   vos agents de retrouver un document par son sens ; l'autre transcrit votre
   voix pour la dictée. Chacun est téléchargé ou importé depuis un fichier, puis
   essayé sur place.
4. **Qui je suis** : le modèle choisi répond à une question, pour que vous le
   voyiez fonctionner, puis un court formulaire recueille ce que vos agents
   doivent savoir de vous. Chaque champ est facultatif.
5. **Premier agent** : installer un paquet reçu (un fichier `.apo`), créer un
   agent en décrivant votre besoin en quelques phrases, ou partir d'un exemple
   proposé par Apollia. Cette étape peut être passée.
6. **Connexion** : affichée seulement quand cet agent a besoin d'une connexion
   que vous n'avez pas encore, comme une boîte mail ou un dossier. Elle peut
   être passée.
7. **Première tâche** : la fenêtre se ferme sur votre agent au travail, une
   conversation où son premier exemple est déjà écrit, ou, pour l'exemple qui
   qualifie des demandes, un lot sur un classeur d'exemple. Sans agent, c'est
   une conversation avec Apollia, l'agent intégré, qui s'ouvre.

Chaque étape est détaillée dans
[Configurer votre profil au premier lancement](/operator-help/installation/set-up-your-profile).
Le parcours reprend là où vous l'avez laissé après un redémarrage, et il se
rejoue depuis la **Zone de danger** de **Réglages > Général**.

<!-- claim:desktop-downloads-models-in-app -->
Pour une inférence entièrement locale, téléchargez un modèle depuis
l'application elle-même : le premier lancement le propose, et l'écran IA,
**Découvrir des modèles**, fait de même par la suite. Déposer un fichier
`.gguf` à la main dans `~/.apollia/models/` fonctionne aussi, avant ou après le
premier lancement. Apollia choisit le moteur d'après le format du modèle : GGUF
sur llama.cpp, MLX sur le moteur MLX des puces Apple Silicon.

## L'interpréteur Python sur lequel tournent les agents

Apollia est livré avec son propre Python 3.13, et c'est lui qui fait tourner les
agents : rien n'a besoin d'être installé sur la machine, et aucun droit
d'administrateur n'est requis. Les interpréteurs déjà présents sur la machine ne
sont pas consultés, quelle que soit leur version.

Si votre environnement en exige un autre, nommez-le dans les réglages, sous
**Système > Matériel** avec le mode développeur activé, ou dans
`apollia.toml` :

```toml
[tools]
python_interpreter = "/opt/corp/python3.13/bin/python3"
```

Il est vérifié avant d'être accepté : il doit exister, démarrer, et être un
Python 3.13, car la bibliothèque standard livrée avec Apollia ne peut pas être
lue par une autre version mineure. S'il cesse ensuite de satisfaire à cela,
après une mise à jour ou une désinstallation, l'interpréteur livré reprend la
main et le journal dit pourquoi, plutôt que de laisser les outils de l'agent en
échec.

La même clé s'écrit depuis un terminal, avec la même vérification :

```sh
apollia-os config set tools.python_interpreter /opt/corp/python3.13/bin/python3
```

## Où l'application stocke vos données

Tout ce que l'application conserve se trouve dans un unique répertoire de
votre dossier personnel :

| Chemin | Contenu |
|---|---|
| `~/.apollia/` | La racine des données de l'application, créée au premier lancement. |
| `~/.apollia/apollia.toml` | Le fichier de configuration, celui que toutes les surfaces lisent et écrivent. |
| `~/.apollia/models/` | Les modèles locaux, téléchargés depuis l'application ou déposés là à la main. |
| `~/.apollia/api-token` | Le jeton porteur pour l'API HTTP locale. |

Rien ne quitte votre machine, sauf si vous déclarez explicitement un serveur
ou ajoutez une connexion. Pour réinitialiser l'application à un état vierge,
quittez-la et supprimez `~/.apollia/` (cela supprime vos agents, votre mémoire
et votre configuration, alors sauvegardez d'abord ce dossier si vous y
tenez).

Sur Windows, la racine des données est le dossier `.apollia` situé dans votre
profil utilisateur (`%USERPROFILE%\.apollia`).

## Depuis la 0.2.0-preview {#coming-from-0-2-0-preview}

Les installeurs gardent le nom Apollia OS, donc la 0.3.0 remplace sur place une
0.2.0 installée : utilisez **Installer et relancer** dans l'application, ou
lancez le nouvel installeur. Les bases de données de `~/.apollia` sont migrées
au premier démarrage de la 0.3.0, et la 0.2.0 refuse ensuite de les ouvrir :
copiez d'abord le dossier si vous risquez de revenir en arrière. Ce qui change
pour vous, et ce qu'il faut faire, se trouve dans
[Mettre à jour Apollia](/operator-help/installation/update-apollia#coming-from-0-2-0-preview).

## Prochaines étapes

- Comprendre le runtime derrière l'application : [Installer et exécuter le runtime](/how-to/install-and-run).
- Accélérer l'inférence locale : [Accélérer l'inférence locale](/how-to/accelerate-local-inference).
- Installer, créer ou ajuster un agent depuis l'application : [Installer un agent](/operator-help/agents/install-an-agent).
- Écrire votre propre paquet, en code : [Votre premier agent](/tutorials/your-first-agent).
