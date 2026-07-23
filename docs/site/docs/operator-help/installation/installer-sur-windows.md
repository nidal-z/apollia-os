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
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor --json | ConvertFrom-Json | Select-Object -ExpandProperty gpu
```

## Accélération GPU

L'inférence LLM locale passe par le moteur embarqué `llama-server`, livré avec le bundle. La reconnaissance vocale (STT) utilise le runner `apollia-runner`, dont l'installeur MSI embarque la variante CPU. Pour accélérer la dictée sur GPU CUDA / Vulkan :

1. Téléchargez `apollia-os-windows-x86-cuda.zip` (ou vulkan).
2. Décompressez et copiez `apollia-runner-cuda.exe` (ou `apollia-runner-vulkan.exe`) dans `C:\Program Files\Apollia OS\`.
3. Relancez l'app.

## Désinstallation

`Paramètres > Applications > Apollia OS > Désinstaller`. Données utilisateur :

```powershell
Remove-Item -Recurse "$env:USERPROFILE\.apollia"
```
