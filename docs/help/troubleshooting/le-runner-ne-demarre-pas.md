# Le runner sidecar ne démarre pas

Depuis v0.1.0, Apollia exécute l'inférence locale dans un process séparé : le **runner** (`apollia-runner-<backend>`). Le daemon le spawn au démarrage et communique avec lui en HTTP loopback.

Si vous voyez des messages comme :

- `RUNNER_HANDSHAKE_TIMEOUT`
- `runner sidecar not available (Phase 4.5 spawn failed)`
- `no LLM backends configured`

...le sidecar n'a pas pu démarrer. Voici les causes courantes.

## 1. Le binaire runner est absent

Le bundle d'installation doit contenir au minimum `apollia-runner-cpu` à côté du daemon.

**macOS :** `~/Applications/Apollia\ OS.app/Contents/Resources/apollia-runner-*`
**Linux :** dans l'AppImage `.AppDir/usr/bin/` ou à côté de `apollia-os` (paquet `.deb`)
**Windows :** `C:\Program Files\Apollia OS\apollia-runner-*.exe`

Vérifiez via :

```sh
apollia-os doctor --json | jq .runner
```

Si `apollia-runner-cpu` est absent : ré-installez Apollia (le bundle a été altéré).

## 2. Le driver GPU est manquant ou trop ancien

Le daemon a détecté votre GPU et tenté de spawner le runner correspondant (ex `apollia-runner-cuda`), mais les libs runtime sont absentes.

**Symptômes :**

- macOS : rarement bloquant (Metal est dans le système).
- Linux/Windows CUDA : `libcuda.so.1 not found` ou `nvcuda.dll not found`.
- Linux ROCm : `libhip.so not found`.
- Vulkan : `libvulkan.so.1 not found`.

**Fix :** mettez à jour le driver GPU ou forcez le backend CPU dans `~/.apollia/apollia.toml` :

```toml
[llm.runner]
backend = "cpu"
```

Redémarrez : `apollia-os stop && apollia-os start`.

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

- `RunnerSupervisor: spawning <binary>` (le daemon trouve le binaire)
- `runner handshake ok` (succès)
- `runner exited prematurely` (le runner a crashé — joindre les lignes précédentes au rapport)

Ouvrez une issue GitHub avec `apollia.log` + sortie de `apollia-os doctor --json`.
