# Setup — archive-worker v0.1.0

## Prérequis

- Apollia OS **v0.1.0+** installé et fonctionnel (`apollia --version`)
- Python 3.13 (bundled avec Apollia — rien à installer côté utilisateur)

### Dépendances natives système

Aucune. Le worker utilise exclusivement la stdlib Python (`zipfile`, `tarfile`, `pathlib`, `base64`, `fnmatch`, `datetime`). Pas de binaire système externe, pas de package PyPI à installer.

## Installation

```bash
apollia agent install ./archive-worker
```

Cette commande :
1. Valide `agent.toml` et le code Python (duck typing : `agent` au niveau module).
2. Copie le dossier vers `~/.apollia/agents/packages/archive-worker/`.
3. Enregistre le worker dans `~/.apollia/agents.db` (auto-load au prochain boot).

Au premier passage à l'état `Active`, le runtime crée un venv dédié :

```
~/.apollia/venvs/archive-worker/venv/
```

Avec `packages = []`, l'installation pip est instantanée (aucun paquet à télécharger).

## Vérification

```bash
apollia agent list | grep archive-worker
```

État attendu : `Active`. Si `Degraded` ou `Failed`, voir Troubleshooting.

Test rapide via eval :

```bash
python3 ~/.apollia/agents/packages/archive-worker/eval/run-eval.py
```

Sortie attendue : `14/14 cases passed (100%)`.

## Désinstallation

```bash
apollia agent uninstall archive-worker
```

Cela retire :
- L'enregistrement SQLite (`agents.db`, `packages.db`)
- Le dossier `~/.apollia/agents/packages/archive-worker/`

Le venv (`~/.apollia/venvs/archive-worker/`) n'est pas supprimé automatiquement. À retirer manuellement si besoin :

```bash
rm -rf ~/.apollia/venvs/archive-worker
```

## Troubleshooting

### Le worker reste `Initializing` longtemps

Improbable pour ce worker (pas de packages à installer). Si bloqué > 30 s :

```bash
tail -f ~/.apollia/logs/runtime.log | grep archive-worker
```

Vérifier les droits sur `~/.apollia/agents/` et `~/.apollia/venvs/`.

### `INVALID_PAYLOAD` à chaque appel

Le director appelant doit utiliser `ctx.a2a_invoke(skill_id, payload_dict, ...)` avec un `payload_dict` non vide. Si le payload est passé sous forme de string, le runtime ne le wrappe pas correctement en `DataPart`.

### `MISSING_SKILL_ID`

Le runtime n'a pas propagé de `skill_id` jusqu'au worker. C'est typiquement le cas si l'agent est invoqué hors A2A (CLI/trigger/REST direct) — un worker multi-skills exige le `skill_id` pour dispatcher. Invoquer via `ctx.a2a_invoke("archive.list", payload)` pour que le runtime propage automatiquement.

### `UNKNOWN_SKILL_ID`

Le `skill_id` reçu n'est pas dans `("archive.list", "archive.extract", "archive.create", "archive.read-file")`. Vérifier l'orthographe côté caller.

### `UNSUPPORTED_FORMAT`

Le worker supporte uniquement `.zip`, `.tar`, `.tar.gz`, `.tar.bz2`, `.tar.xz`. Pour `.7z` ou `.rar`, le worker retourne ce code volontairement (dépendances externes écartées en v0.1.0).

Si l'extension n'est pas reconnue, passer le format explicitement via `archive_format` :

```python
await ctx.a2a_invoke("archive.list", {"archive_path": "/tmp/archive.bin", "archive_format": "zip"})
```

### `PARSE_ERROR`

Archive corrompue, chiffrée, ou tronquée. Vérifier l'intégrité :

```bash
unzip -t /tmp/archive.zip
tar -tf /tmp/archive.tar.gz
```

### Symlinks ignorés sur l'extraction

Comportement par défaut (sécurité). Pour les conserver :

```python
await ctx.a2a_invoke("archive.extract", {"archive_path": "...", "target_dir": "...", "allow_symlinks": True})
```

### Quota atteint en extraction (`skipped_reasons.quota > 0`)

`max_uncompressed_bytes` (défaut 1 GiB) bloque les zip bombs. Augmenter pour archives légitimement grosses :

```python
await ctx.a2a_invoke("archive.extract", {"archive_path": "...", "target_dir": "...", "max_uncompressed_bytes": 10737418240})  # 10 GiB
```

### Logs détaillés

```bash
RUST_LOG=apollia=debug apollia-os start --foreground 2>&1 | grep archive-worker
```
