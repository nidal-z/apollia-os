---
title: Le runner sidecar ne démarre pas
slug: /operator-help/troubleshooting/the-runner-does-not-start
sidebar_position: 2
---

# Le runner sidecar ne démarre pas

Le **runner** (`apollia-runner-<backend>`) est le sidecar de reconnaissance vocale (STT, whisper). Le daemon le spawn au démarrage et communique avec lui en HTTP loopback. L'inférence LLM locale, elle, ne passe pas par le runner : elle est servie par le moteur embarqué `llama-server`. Si c'est le LLM local qui ne répond pas, voir plutôt [Le fournisseur d'IA ne répond pas](the-ai-provider-does-not-answer.md).

Si vous voyez des messages comme :

- `RUNNER_HANDSHAKE_TIMEOUT`
- `runner sidecar not available (spawn failed)`

...le sidecar STT n'a pas pu démarrer et la dictée vocale est indisponible. Voici les causes courantes.

## 1. Le binaire runner est absent

Le bundle d'installation doit contenir au minimum `apollia-runner-cpu` à côté du daemon.

**macOS :** `~/Applications/Apollia\ OS.app/Contents/Resources/apollia-runner-*`
**Linux :** dans l'AppImage `.AppDir/usr/bin/` ou à côté de `apollia-os` (paquet `.deb`)
**Windows :** `C:\Program Files\Apollia OS\apollia-runner-*.exe`

Vérifiez via :

```sh
apollia-os doctor
```

Si `apollia-runner-cpu` est absent : ré-installez Apollia (le bundle a été altéré).

## 2. Le driver GPU est manquant ou trop ancien

Le daemon a détecté votre GPU et tenté de spawner le runner STT correspondant (ex `apollia-runner-cuda`), mais les libs runtime sont absentes.

**Symptômes :**

- macOS : rarement bloquant (Metal est dans le système).
- Linux/Windows CUDA : `libcuda.so.1 not found` ou `nvcuda.dll not found`.
- Linux ROCm : `libhip.so not found`.
- Vulkan : `libvulkan.so.1 not found`.

**Fix :** mettez à jour le driver GPU, ou revenez au runner CPU en copiant `apollia-runner-cpu` à côté du binaire `apollia-os` (voir la page d'installation de votre système). Redémarrez ensuite : `apollia-os stop && apollia-os start`.

## 3. Pare-feu bloque la connexion loopback (Windows)

Le runner écoute sur `127.0.0.1:<port-auto>`. Si le Windows Defender Firewall a bloqué `apollia-runner-cuda.exe` au premier lancement, le daemon ne peut pas s'y connecter.

**Fix :** `Paramètres > Confidentialité et sécurité > Sécurité Windows > Pare-feu et protection réseau > Autoriser une application` puis cochez Apollia OS pour les réseaux privés.

## 4. Cold start lent (Apple Silicon)

Le premier spawn du runner Metal peut prendre 5 à 15 secondes (init MTLDevice). Si le handshake timeout déclenche : ré-essayez. Si récurrent, vérifiez que `xcode-select -p` retourne un chemin valide.

## 5. Recueillir des logs

```sh
apollia-os stop
APOLLIA_LOG=debug apollia-os start 2>&1 | tee /tmp/apollia.log
```

Cherchez :

- `supervisor.runner.spawned` (le démon l'a démarré)
- `supervisor.runner.spawn.failed` (il n'a pas démarré, et le démon continue
  sans STT locale)
- `runner.spawn.failed` (le binaire n'a pas pu être lancé du tout)
- `runner handshake timeout after 10s` (il a démarré mais n'a jamais annoncé son
  port)
- `runner.respawned` / `runner.respawn.failed` (il est mort et le démon a
  réessayé)

La sortie du runner lui-même est réémise sous la cible de log `runner` : son
stderr apparaît donc dans le même fichier.

Ouvrez une issue GitHub avec `apollia.log` + sortie de `apollia-os doctor --json`.
