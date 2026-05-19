# archive-worker

> Manipulation d'archives (.zip, .tar, .tar.gz, .tar.bz2, .tar.xz) : lister, extraire, créer, lire un fichier interne — 100 % stdlib, zéro dépendance externe.

Worker Apollia OS standalone. Expose quatre skills A2A invocables par n'importe quel agent (custom director, Chat Libre via outils virtuels `a2a:*`, autres workers chaînés).

Décharge le LLM director de la génération de code `openpyxl`/`pypdf` ad-hoc : l'agent passe une payload typée, le worker exécute la logique Python déterministe, le director reçoit du JSON propre.

## Skills exposés

| Skill ID | Description | Input principal |
|---|---|---|
| `archive.list` | Liste le contenu d'une archive sans extraction. | `archive_path` |
| `archive.extract` | Extrait avec garde-fous path traversal + zip bomb + symlinks. | `archive_path`, `target_dir` |
| `archive.create` | Crée une archive depuis sources. | `output_path`, `archive_format`, `sources` |
| `archive.read-file` | Lit un fichier précis depuis une archive (text ou base64). | `archive_path`, `entry_path` |

**Formats supportés** : `.zip`, `.tar`, `.tar.gz` / `.tgz`, `.tar.bz2` / `.tbz2`, `.tar.xz` / `.txz`.

## Dispatch multi-skills

Depuis Apollia v0.1.0 (2026-05-19), le runtime propage le `skill_id` invoqué dans `AIPTask.skill_id`. Le worker dispatche directement sur le full skill_id (`archive.list`, `archive.extract`, `archive.create`, `archive.read-file`) via `apollia.utils.a2a.extract_skill_id(task)`.

Côté caller : aucune convention spéciale dans le payload — invoque simplement `ctx.a2a_invoke("archive.list", {"archive_path": "..."})`. Le runtime se charge du reste.

## Installation

```bash
apollia agent install ./archive-worker
```

Vérification :

```bash
apollia agent list | grep archive-worker
```

Au premier passage à `Active`, Apollia crée un venv dédié `~/.apollia/venvs/archive-worker/venv/`. Le worker n'ayant **aucune dépendance Python externe** (`packages = []`), le venv est quasi instantané.

## Usage

### Depuis un director custom (Python)

```python
import json

# 1) Lister
result = await ctx.a2a_invoke(
    "archive.list",
    {"archive_path": "/data/rapport.zip"},
    timeout_secs=60,
)
listing = json.loads(result["result"]["text"])
# listing["entries"] → [{"path": "...", "size": ..., "is_dir": ..., ...}, ...]

# 2) Extraire avec filtre + quota
await ctx.a2a_invoke(
    "archive.extract",
    {
        "archive_path": "/data/dump.tar.gz",
        "target_dir": "/sandbox/extracted/",
        "glob_filter": "*.csv",
        "max_uncompressed_bytes": 256 * 1024 * 1024,  # 256 MiB
    },
    timeout_secs=120,
)

# 3) Créer un livrable .zip
await ctx.a2a_invoke(
    "archive.create",
    {
        "output_path": "/sandbox/livraison.zip",
        "archive_format": "zip",
        "sources": ["/sandbox/rapport.pdf", "/sandbox/data/"],
        "compression_level": 9,
    },
)

# 4) Lire un fichier sans tout extraire
result = await ctx.a2a_invoke(
    "archive.read-file",
    {
        "archive_path": "/data/backup.tar.gz",
        "entry_path": "config/settings.json",
        "mode": "text",
    },
)
content = json.loads(result["result"]["text"])["content"]
```

### Depuis Chat Libre

Les skills deviennent des outils virtuels `a2a:archive.list`, `a2a:archive.extract`, `a2a:archive.create`, `a2a:archive.read-file` directement utilisables par le LLM :

> "Utilise `a2a:archive.list` avec `{"archive_path": "/Users/.../backup.zip"}`."

### Eval

```bash
python3 archive-worker/eval/run-eval.py
```

Le runner crée des fixtures temporaires (zips/tars d'exemple), exécute 14 cas et reporte le pass rate. Aucun mock Apollia requis — le worker est purement déterministe.

## Configuration

Aucune configuration externe requise. Tous les paramètres se passent dans la payload de l'appel A2A. Aucune variable d'environnement, aucune entrée `apollia.toml`.

## Garde-fous sécurité

- **Path traversal** : toute entry dont le chemin résout en dehors de `target_dir` est ignorée et comptée dans `skipped_reasons.path_traversal`.
- **Zip bomb** : `max_uncompressed_bytes` (défaut 1 GiB) plafonne le volume décompressé total.
- **Symlinks** : ignorés par défaut, opt-in explicite via `allow_symlinks: true`.
- **Tar filter** : `tarfile.extract(..., filter="data")` (Python 3.12+) sanitise paths/permissions/devices.
- **Aucun accès réseau** (`network_allowlist: null`).
- **Aucun outil dangereux** (`dangerous_tools_allowed: false`).

## Limitations connues

- Archives chiffrées (zip password, tar.enc) : non supportées, retournent `PARSE_ERROR`.
- `.7z` : non supporté en v0.1.0 (nécessiterait `py7zr`, dépendance externe écartée).
- `.rar` : non supporté (lib propriétaire).
- Détection symlink sur les zips créés sous Windows : best-effort (Windows ne pose pas le mode Unix `0o120000`).
- Streaming sur archives > 10 GiB : non optimisé en v0.1.0.
- Préservation des permissions Unix : tar oui (avec filter="data"), zip best-effort.

## Sécurité

- `dangerous_tools_allowed = false`
- Aucun accès réseau direct.
- Validations internes : path traversal, zip bomb quota, symlinks, formats whitelist.

## License

MIT © Apollia OS

Voir [`CHANGELOG.md`](./CHANGELOG.md) pour l'historique des versions.
