# Changelog — archive-worker

Toutes les modifications notables de ce worker sont documentées ici.

Format inspiré de [Keep a Changelog](https://keepachangelog.com/), versioning [SemVer](https://semver.org/).

## [0.1.0] — 2026-05-18

### Added
- Release initiale.
- Skill `archive.list` : liste le contenu d'une archive sans extraction (path, size, mtime, is_dir, is_symlink).
- Skill `archive.extract` : extraction sécurisée avec garde-fous path traversal, quota anti zip-bomb, gestion symlinks opt-in, filtre glob optionnel.
- Skill `archive.create` : crée une archive depuis fichiers/dossiers sources, compression configurable, base_dir auto ou explicite.
- Skill `archive.read-file` : lit un fichier précis depuis une archive sans tout extraire, mode text (UTF-8) ou binary (base64).
- Formats supportés : `.zip`, `.tar`, `.tar.gz` / `.tgz`, `.tar.bz2` / `.tbz2`, `.tar.xz` / `.txz`.
- Eval suite avec 14 cas (happy paths × 4 skills, erreurs typées, safety checks).

### Notes
- Worker **déterministe pur** — n'utilise pas `ctx.llm` ni de boucle ReAct.
- Stateless — aucune mémoire entre appels.
- `packages = []` — **zéro dépendance externe**, 100 % stdlib Python (`zipfile`, `tarfile`, `pathlib`, `base64`, `fnmatch`, `datetime`). Aligné Principe 2 Apollia OS.
- Pas de validation Pydantic runtime — `schemas.py` documente le contrat via `TypedDict` stdlib uniquement.

### Limitations connues
- `.7z` et `.rar` non supportés (dépendances externes écartées).
- Archives chiffrées non supportées (retournent `PARSE_ERROR`).
- Détection symlink dans les zips créés sous Windows : best-effort.
- Streaming sur archives > 10 GiB non optimisé.

## [0.1.1] — 2026-05-19

### Changed
- **Breaking** : remplacé le workaround champ `op` payload par la lecture de `task.skill_id` (propagé par le runtime A2A depuis 2026-05-19). Le worker dispatche désormais sur le full skill_id (`archive.list`, etc.) via `extract_skill_id(task)`.
- Nouveaux codes d'erreur : `MISSING_SKILL_ID`, `UNKNOWN_SKILL_ID`. Les codes `MISSING_FIELD`/`INVALID_TYPE` (anciennement renvoyés pour `op` manquant/invalide) ne sont plus émis pour ce vecteur.
- Eval cases : champ `skill_id` top-level ajouté à chaque case ; champ `op` retiré des payloads.

### Removed
- Convention `op` dans le payload — caller invoque maintenant `ctx.a2a_invoke("archive.list", {...})` sans champ `op`.
