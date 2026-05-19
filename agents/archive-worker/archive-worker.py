"""archive-worker — Manipulation d'archives stdlib-only.

Worker Apollia OS standalone qui isole la manipulation d'archives (.zip, .tar,
.tar.gz, .tar.bz2, .tar.xz) du LLM director. Exécute des opérations
déterministes via la stdlib Python — zéro dépendance externe.

Skills A2A exposés :
- ``archive.list``       — Liste le contenu d'une archive sans extraction.
- ``archive.extract``    — Extrait avec garde-fous (path traversal, zip bomb, symlinks).
- ``archive.create``     — Crée une archive depuis fichiers/dossiers sources.
- ``archive.read-file``  — Lit un fichier précis depuis une archive (text ou binary/base64).

Contrainte plateforme v0.1.0 : Apollia ne propage pas ``skill_id`` au worker
Python — chaque payload doit donc inclure un champ ``op`` qui discrimine
l'opération demandée (``list`` | ``extract`` | ``create`` | ``read-file``).
À retirer si la propagation est ajoutée côté runtime.
"""

from __future__ import annotations

import base64
import datetime
import fnmatch
import json
import tarfile
import zipfile
from pathlib import Path
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.a2a import extract_a2a_payload, extract_skill_id


SUPPORTED_FORMATS: tuple[str, ...] = ("zip", "tar", "tar.gz", "tar.bz2", "tar.xz")
ALLOWED_SKILL_IDS: tuple[str, ...] = (
    "archive.list",
    "archive.extract",
    "archive.create",
    "archive.read-file",
)
DEFAULT_MAX_UNCOMPRESSED: int = 1_073_741_824  # 1 GiB
DEFAULT_MAX_READ_BYTES: int = 10_485_760       # 10 MiB


class _ArchiveError(Exception):
    """Domain error portant un code stable et un message humain."""

    def __init__(
        self,
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details


class ArchiveWorker(WorkerAgent):
    """Deterministic worker exposing 4 archive-manipulation skills."""

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "archive-worker",
            "version": "0.1.0",
            "description": (
                "Manipulation d'archives (.zip, .tar, .tar.gz, .tar.bz2, .tar.xz) : "
                "lister, extraire, créer, lire un fichier interne — 100 % stdlib, "
                "zéro dépendance externe."
            ),
            "execution_mode": "direct",
            "agent_type": "user",
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "tools_required": [],
            "dangerous_tools_allowed": False,
            "packages": [],
            "tags": ["archive", "zip", "tar", "compression", "extract", "worker"],
            "step_budget": {
                "max_steps": 1,
                "max_tool_calls": 5,
                "wall_clock_secs": 300,
            },
            "skills": [
                {
                    "id": "archive.list",
                    "name": "List archive contents",
                    "description": (
                        "Liste le contenu d'une archive sans extraction. "
                        "Retourne path, size, mtime, is_dir, is_symlink pour chaque entrée."
                    ),
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "archive_path": {
                            "type": "string",
                            "description": "Chemin absolu vers l'archive.",
                            "required": True,
                        },
                        "archive_format": {
                            "type": "string",
                            "description": (
                                "Format explicite si non détectable par extension : "
                                "zip|tar|tar.gz|tar.bz2|tar.xz."
                            ),
                            "required": False,
                        },
                    },
                },
                {
                    "id": "archive.extract",
                    "name": "Extract archive safely",
                    "description": (
                        "Extrait une archive avec garde-fous : path traversal, quota "
                        "anti zip-bomb, symlinks (opt-in). Filtre glob optionnel."
                    ),
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "archive_path": {
                            "type": "string",
                            "description": "Chemin absolu vers l'archive.",
                            "required": True,
                        },
                        "target_dir": {
                            "type": "string",
                            "description": "Dossier cible (créé si absent).",
                            "required": True,
                        },
                        "glob_filter": {
                            "type": "string",
                            "description": "Pattern glob pour filtrer (ex: '*.txt').",
                            "required": False,
                        },
                        "max_uncompressed_bytes": {
                            "type": "integer",
                            "description": (
                                "Quota anti zip-bomb. Défaut 1 073 741 824 (1 GiB)."
                            ),
                            "required": False,
                        },
                        "allow_symlinks": {
                            "type": "boolean",
                            "description": (
                                "Si true, extrait les symlinks. Défaut false : "
                                "symlinks ignorés."
                            ),
                            "required": False,
                        },
                        "archive_format": {
                            "type": "string",
                            "description": "Format explicite si non détectable.",
                            "required": False,
                        },
                    },
                },
                {
                    "id": "archive.create",
                    "name": "Create archive",
                    "description": (
                        "Crée une archive depuis fichiers/dossiers sources. "
                        "Format au choix, compression configurable."
                    ),
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "output_path": {
                            "type": "string",
                            "description": "Chemin de sortie de l'archive.",
                            "required": True,
                        },
                        "archive_format": {
                            "type": "string",
                            "description": "Format : zip|tar|tar.gz|tar.bz2|tar.xz.",
                            "required": True,
                        },
                        "sources": {
                            "type": "array",
                            "description": "Fichiers/dossiers à inclure (paths absolus).",
                            "required": True,
                        },
                        "base_dir": {
                            "type": "string",
                            "description": (
                                "Base pour les paths relatifs dans l'archive. "
                                "Défaut : parent commun des sources."
                            ),
                            "required": False,
                        },
                        "compression_level": {
                            "type": "integer",
                            "description": (
                                "Niveau de compression : 0-9 (zip) ou 1-9 (gz/bz2/xz)."
                            ),
                            "required": False,
                        },
                    },
                },
                {
                    "id": "archive.read-file",
                    "name": "Read single file from archive",
                    "description": (
                        "Lit un fichier précis depuis une archive sans tout extraire. "
                        "Mode text (UTF-8) ou binary (base64). Cap par défaut 10 MB."
                    ),
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "archive_path": {
                            "type": "string",
                            "description": "Chemin absolu vers l'archive.",
                            "required": True,
                        },
                        "entry_path": {
                            "type": "string",
                            "description": "Path à l'intérieur de l'archive.",
                            "required": True,
                        },
                        "mode": {
                            "type": "string",
                            "description": "'text' (défaut) ou 'binary' (base64).",
                            "required": False,
                        },
                        "encoding": {
                            "type": "string",
                            "description": "Encodage UTF-8 par défaut en mode text.",
                            "required": False,
                        },
                        "max_bytes": {
                            "type": "integer",
                            "description": (
                                "Lit au max N bytes. Défaut 10 485 760 (10 MiB)."
                            ),
                            "required": False,
                        },
                        "archive_format": {
                            "type": "string",
                            "description": "Format explicite si non détectable.",
                            "required": False,
                        },
                    },
                },
            ],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        skill_id = extract_skill_id(task)
        if skill_id is None:
            return AIPResult.failed(
                "MISSING_SKILL_ID",
                "archive-worker : aucun skill_id propagé par le runtime (worker multi-skills)",
                details={"expected": list(ALLOWED_SKILL_IDS)},
            )
        if skill_id not in ALLOWED_SKILL_IDS:
            return AIPResult.failed(
                "UNKNOWN_SKILL_ID",
                f"skill_id non reconnu : {skill_id!r}",
                details={"got": skill_id, "expected": list(ALLOWED_SKILL_IDS)},
            )

        payload = extract_a2a_payload(task)
        if not payload:
            return AIPResult.failed(
                "INVALID_PAYLOAD",
                "archive-worker : payload A2A vide ou non parseable",
            )

        try:
            if skill_id == "archive.list":
                result = _do_list(payload)
            elif skill_id == "archive.extract":
                result = _do_extract(payload, ctx)
            elif skill_id == "archive.create":
                result = _do_create(payload)
            else:  # "archive.read-file"
                result = _do_read_file(payload)
        except FileNotFoundError as exc:
            return AIPResult.failed("FILE_NOT_FOUND", str(exc))
        except _ArchiveError as exc:
            return AIPResult.failed(exc.code, exc.message, details=exc.details)
        except Exception as exc:  # last-resort safety net
            ctx.log("error", f"archive-worker {skill_id} failed: {exc}")
            return AIPResult.failed("EXECUTION_FAILED", f"Erreur inattendue : {exc}")

        return AIPResult.completed(json.dumps(result, ensure_ascii=False))


# ─── Operations ────────────────────────────────────────────────────────────


def _do_list(payload: dict[str, Any]) -> dict[str, Any]:
    archive_path = _require_str(payload, "archive_path")
    fmt = _resolve_format(payload, archive_path)
    if not Path(archive_path).exists():
        raise FileNotFoundError(f"archive introuvable : {archive_path}")

    entries: list[dict[str, Any]] = []
    total_uncompressed = 0
    compressed_size = Path(archive_path).stat().st_size

    if fmt == "zip":
        try:
            with zipfile.ZipFile(archive_path, "r") as zf:
                for info in zf.infolist():
                    entries.append(_zip_entry_dict(info))
                    total_uncompressed += info.file_size
        except zipfile.BadZipFile as exc:
            raise _ArchiveError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
    else:
        try:
            with tarfile.open(archive_path, _tar_mode(fmt, "r")) as tf:
                for member in tf.getmembers():
                    entries.append(_tar_entry_dict(member))
                    total_uncompressed += member.size
        except tarfile.TarError as exc:
            raise _ArchiveError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

    return {
        "archive_path": archive_path,
        "format": fmt,
        "entries": entries,
        "total_entries": len(entries),
        "compressed_size": compressed_size,
        "uncompressed_size": total_uncompressed,
    }


def _do_extract(payload: dict[str, Any], ctx: Any) -> dict[str, Any]:
    archive_path = _require_str(payload, "archive_path")
    target_dir = _require_str(payload, "target_dir")
    glob_filter = payload.get("glob_filter")
    max_uncompressed = int(payload.get("max_uncompressed_bytes") or DEFAULT_MAX_UNCOMPRESSED)
    allow_symlinks = bool(payload.get("allow_symlinks", False))
    fmt = _resolve_format(payload, archive_path)

    if not Path(archive_path).exists():
        raise FileNotFoundError(f"archive introuvable : {archive_path}")

    target = Path(target_dir).expanduser().resolve()
    target.mkdir(parents=True, exist_ok=True)

    extracted = 0
    skipped = {"path_traversal": 0, "symlink": 0, "quota": 0, "glob_mismatch": 0}
    total_bytes = 0

    def _glob_ok(name: str) -> bool:
        return True if not glob_filter else fnmatch.fnmatch(name, glob_filter)

    if fmt == "zip":
        try:
            with zipfile.ZipFile(archive_path, "r") as zf:
                for info in zf.infolist():
                    name = info.filename
                    is_sym = _zip_is_symlink(info)
                    if not _glob_ok(name):
                        skipped["glob_mismatch"] += 1
                        continue
                    if is_sym and not allow_symlinks:
                        skipped["symlink"] += 1
                        continue
                    if not _is_path_safe(target, name):
                        skipped["path_traversal"] += 1
                        ctx.log("warn", f"refus path traversal : {name}")
                        continue
                    if total_bytes + info.file_size > max_uncompressed:
                        skipped["quota"] += 1
                        continue
                    zf.extract(info, target)
                    if not info.is_dir():
                        total_bytes += info.file_size
                        extracted += 1
        except zipfile.BadZipFile as exc:
            raise _ArchiveError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
    else:
        try:
            with tarfile.open(archive_path, _tar_mode(fmt, "r")) as tf:
                for member in tf.getmembers():
                    name = member.name
                    is_sym = member.issym() or member.islnk()
                    if not _glob_ok(name):
                        skipped["glob_mismatch"] += 1
                        continue
                    if is_sym and not allow_symlinks:
                        skipped["symlink"] += 1
                        continue
                    if not _is_path_safe(target, name):
                        skipped["path_traversal"] += 1
                        ctx.log("warn", f"refus path traversal : {name}")
                        continue
                    if total_bytes + member.size > max_uncompressed:
                        skipped["quota"] += 1
                        continue
                    # Python 3.12+ : 'data' filter sanitise les paths/permissions.
                    tf.extract(member, target, filter="data")
                    if not member.isdir():
                        total_bytes += member.size
                        extracted += 1
        except tarfile.TarError as exc:
            raise _ArchiveError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

    return {
        "target_dir": str(target),
        "extracted_count": extracted,
        "skipped_count": sum(skipped.values()),
        "skipped_reasons": skipped,
        "total_uncompressed_bytes": total_bytes,
    }


def _do_create(payload: dict[str, Any]) -> dict[str, Any]:
    output_path = _require_str(payload, "output_path")
    fmt = _require_str(payload, "archive_format")
    if fmt not in SUPPORTED_FORMATS:
        raise _ArchiveError(
            "UNSUPPORTED_FORMAT",
            f"Format non supporté : {fmt!r}",
            details={"supported": list(SUPPORTED_FORMATS)},
        )

    sources = payload.get("sources")
    if not isinstance(sources, list) or not sources:
        raise _ArchiveError(
            "MISSING_FIELD",
            "Champ 'sources' requis (liste non vide de paths)",
            details={"field": "sources"},
        )

    base_dir_raw = payload.get("base_dir")
    src_paths = [Path(s).expanduser().resolve() for s in sources]
    base = (
        Path(base_dir_raw).expanduser().resolve()
        if base_dir_raw
        else _common_parent(src_paths)
    )

    compression_level = payload.get("compression_level")
    out = Path(output_path).expanduser().resolve()
    out.parent.mkdir(parents=True, exist_ok=True)

    entries_count = 0
    uncompressed_size = 0

    if fmt == "zip":
        level = 6 if compression_level is None else int(compression_level)
        with zipfile.ZipFile(
            out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=level
        ) as zf:
            for src in src_paths:
                if not src.exists():
                    raise FileNotFoundError(f"source introuvable : {src}")
                if src.is_file():
                    arcname = _rel_to(src, base)
                    zf.write(src, arcname)
                    entries_count += 1
                    uncompressed_size += src.stat().st_size
                else:
                    for sub in src.rglob("*"):
                        if sub.is_file():
                            arcname = _rel_to(sub, base)
                            zf.write(sub, arcname)
                            entries_count += 1
                            uncompressed_size += sub.stat().st_size
    else:
        mode = _tar_mode(fmt, "w")
        kwargs: dict[str, Any] = {}
        if compression_level is not None and fmt != "tar":
            kwargs["compresslevel"] = int(compression_level)
        with tarfile.open(out, mode, **kwargs) as tf:
            for src in src_paths:
                if not src.exists():
                    raise FileNotFoundError(f"source introuvable : {src}")
                arcname = _rel_to(src, base)
                tf.add(src, arcname=arcname, recursive=True)
                if src.is_file():
                    entries_count += 1
                    uncompressed_size += src.stat().st_size
                else:
                    for sub in src.rglob("*"):
                        if sub.is_file():
                            entries_count += 1
                            uncompressed_size += sub.stat().st_size

    return {
        "output_path": str(out),
        "format": fmt,
        "entries_count": entries_count,
        "compressed_size": out.stat().st_size,
        "uncompressed_size": uncompressed_size,
    }


def _do_read_file(payload: dict[str, Any]) -> dict[str, Any]:
    archive_path = _require_str(payload, "archive_path")
    entry_path = _require_str(payload, "entry_path")
    mode = payload.get("mode", "text")
    encoding = payload.get("encoding", "utf-8")
    max_bytes = int(payload.get("max_bytes") or DEFAULT_MAX_READ_BYTES)
    fmt = _resolve_format(payload, archive_path)

    if mode not in ("text", "binary"):
        raise _ArchiveError(
            "INVALID_TYPE",
            f"mode doit être 'text' ou 'binary'. Reçu : {mode!r}",
            details={"field": "mode"},
        )
    if not Path(archive_path).exists():
        raise FileNotFoundError(f"archive introuvable : {archive_path}")

    raw: bytes
    if fmt == "zip":
        try:
            with zipfile.ZipFile(archive_path, "r") as zf:
                try:
                    with zf.open(entry_path, "r") as fh:
                        raw = fh.read(max_bytes + 1)
                except KeyError as exc:
                    raise _ArchiveError(
                        "ENTRY_NOT_FOUND",
                        f"entry introuvable dans l'archive : {entry_path}",
                        details={"entry_path": entry_path},
                    ) from exc
        except zipfile.BadZipFile as exc:
            raise _ArchiveError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
    else:
        try:
            with tarfile.open(archive_path, _tar_mode(fmt, "r")) as tf:
                try:
                    member = tf.getmember(entry_path)
                except KeyError as exc:
                    raise _ArchiveError(
                        "ENTRY_NOT_FOUND",
                        f"entry introuvable dans l'archive : {entry_path}",
                        details={"entry_path": entry_path},
                    ) from exc
                fh = tf.extractfile(member)
                if fh is None:
                    raise _ArchiveError(
                        "ENTRY_NOT_FOUND",
                        f"entry n'est pas un fichier régulier : {entry_path}",
                        details={"entry_path": entry_path},
                    )
                raw = fh.read(max_bytes + 1)
        except tarfile.TarError as exc:
            raise _ArchiveError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

    truncated = len(raw) > max_bytes
    if truncated:
        raw = raw[:max_bytes]

    result: dict[str, Any] = {
        "entry_path": entry_path,
        "mode": mode,
        "size_bytes": len(raw),
        "truncated": truncated,
    }
    if mode == "text":
        try:
            result["content"] = raw.decode(encoding)
        except UnicodeDecodeError as exc:
            raise _ArchiveError(
                "PARSE_ERROR",
                f"Décodage {encoding} impossible : {exc}",
                details={"encoding": encoding},
            ) from exc
    else:
        result["content_base64"] = base64.b64encode(raw).decode("ascii")
    return result


# ─── Helpers ───────────────────────────────────────────────────────────────


def _require_str(payload: dict[str, Any], field: str) -> str:
    value = payload.get(field)
    if not isinstance(value, str) or not value:
        raise _ArchiveError(
            "MISSING_FIELD",
            f"Champ '{field}' requis (string non vide)",
            details={"field": field},
        )
    return value


def _resolve_format(payload: dict[str, Any], archive_path: str) -> str:
    fmt = payload.get("archive_format") or _detect_format(archive_path)
    if fmt not in SUPPORTED_FORMATS:
        raise _ArchiveError(
            "UNSUPPORTED_FORMAT",
            f"Format non supporté : {fmt!r}",
            details={"detected": fmt, "supported": list(SUPPORTED_FORMATS)},
        )
    return fmt


def _detect_format(path: str) -> str | None:
    p = path.lower()
    if p.endswith(".zip"):
        return "zip"
    if p.endswith(".tar.gz") or p.endswith(".tgz"):
        return "tar.gz"
    if p.endswith(".tar.bz2") or p.endswith(".tbz2"):
        return "tar.bz2"
    if p.endswith(".tar.xz") or p.endswith(".txz"):
        return "tar.xz"
    if p.endswith(".tar"):
        return "tar"
    return None


def _tar_mode(fmt: str, action: str) -> str:
    """Map archive format + action ('r'|'w') to a tarfile.open() mode."""
    if fmt == "tar":
        return f"{action}:"
    if fmt == "tar.gz":
        return f"{action}:gz"
    if fmt == "tar.bz2":
        return f"{action}:bz2"
    if fmt == "tar.xz":
        return f"{action}:xz"
    raise _ArchiveError("UNSUPPORTED_FORMAT", f"format tar invalide : {fmt!r}")


def _is_path_safe(target_dir: Path, entry_name: str) -> bool:
    """True if (target_dir / entry_name) resolves inside target_dir."""
    candidate = (target_dir / entry_name).resolve()
    try:
        candidate.relative_to(target_dir.resolve())
        return True
    except ValueError:
        return False


def _zip_is_symlink(info: zipfile.ZipInfo) -> bool:
    # Unix mode in upper 16 bits of external_attr ; symlinks = 0o120000.
    return (info.external_attr >> 16) & 0o170000 == 0o120000


def _zip_entry_dict(info: zipfile.ZipInfo) -> dict[str, Any]:
    try:
        mtime = datetime.datetime(*info.date_time).isoformat()
    except ValueError:
        mtime = None
    return {
        "path": info.filename,
        "size": info.file_size,
        "mtime": mtime,
        "is_dir": info.is_dir(),
        "is_symlink": _zip_is_symlink(info),
    }


def _tar_entry_dict(member: tarfile.TarInfo) -> dict[str, Any]:
    return {
        "path": member.name,
        "size": member.size,
        "mtime": datetime.datetime.fromtimestamp(
            member.mtime, tz=datetime.timezone.utc
        ).isoformat(),
        "is_dir": member.isdir(),
        "is_symlink": member.issym() or member.islnk(),
    }


def _common_parent(paths: list[Path]) -> Path:
    if not paths:
        return Path("/")
    common = paths[0] if paths[0].is_dir() else paths[0].parent
    for p in paths[1:]:
        candidate = p if p.is_dir() else p.parent
        while True:
            try:
                candidate.relative_to(common)
                break
            except ValueError:
                if common == common.parent:
                    return common
                common = common.parent
    return common


def _rel_to(p: Path, base: Path) -> str:
    try:
        return str(p.relative_to(base))
    except ValueError:
        return p.name


# Module-level — requis par le loader runtime (crates/apollia-aip/src/loader.rs).
agent = ArchiveWorker()
