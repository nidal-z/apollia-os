"""archive-worker — Manipulation d'archives stdlib-only.

Worker Apollia OS standalone qui isole la manipulation d'archives (.zip,
.tar, .tar.gz, .tar.bz2, .tar.xz) du LLM director. Exécute des opérations
déterministes via la stdlib Python — zéro dépendance externe.

Skills A2A exposés :

* ``archive.list``      — Liste le contenu d'une archive sans extraction.
* ``archive.extract``   — Extrait avec garde-fous (path traversal, zip bomb, symlinks).
* ``archive.create``    — Crée une archive depuis fichiers/dossiers sources.
* ``archive.read_file`` — Lit un fichier précis depuis une archive (text ou binary/base64).
"""

from __future__ import annotations

import base64
import datetime
import fnmatch
import tarfile
import zipfile
from pathlib import Path
from typing import Annotated, Any

from apollia import DomainError, agent, skill
from apollia.types import Ctx


SUPPORTED_FORMATS: tuple[str, ...] = ("zip", "tar", "tar.gz", "tar.bz2", "tar.xz")
DEFAULT_MAX_UNCOMPRESSED: int = 1_073_741_824  # 1 GiB
DEFAULT_MAX_READ_BYTES: int = 10_485_760  # 10 MiB


@agent(
    name="archive-worker",
    version="0.1.0",
    description=(
        "Manipulation d'archives (.zip, .tar, .tar.gz, .tar.bz2, .tar.xz) : "
        "lister, extraire, créer, lire un fichier interne — 100 % stdlib."
    ),
    tags=("archive", "zip", "tar", "compression", "extract", "worker"),
    agent_type="worker",
    step_budget={"max_steps": 1, "max_tool_calls": 5, "wall_clock_secs": 300},
)
class ArchiveWorker:
    """Worker déterministe — 4 skills archive."""

    @skill(
        "archive.list",
        description=(
            "List the contents of an archive without extracting. Returns "
            "{path, size, mtime, is_dir, is_symlink} per entry."
        ),
        examples=[
            {"archive_path": "/path/to/bundle.zip"},
        ],
    )
    async def list_archive(
        self,
        archive_path: Annotated[str, "Filesystem path to the archive file."],
        archive_format: Annotated[
            str | None,
            "Force the format: 'zip' | 'tar' | 'tar.gz' | 'tar.bz2' | 'tar.xz'. Omit to infer from extension.",
        ] = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Liste le contenu d'une archive."""
        fmt = _resolve_format(archive_format, archive_path)
        ap = Path(archive_path)
        if not ap.exists():
            raise DomainError("FILE_NOT_FOUND", f"archive introuvable : {archive_path}")

        entries: list[dict[str, Any]] = []
        total_uncompressed = 0
        compressed_size = ap.stat().st_size

        if fmt == "zip":
            try:
                with zipfile.ZipFile(archive_path, "r") as zf:
                    for info in zf.infolist():
                        entries.append(_zip_entry_dict(info))
                        total_uncompressed += info.file_size
            except zipfile.BadZipFile as exc:
                raise DomainError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
        else:
            try:
                with tarfile.open(archive_path, _tar_mode(fmt, "r")) as tf:
                    for member in tf.getmembers():
                        entries.append(_tar_entry_dict(member))
                        total_uncompressed += member.size
            except tarfile.TarError as exc:
                raise DomainError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

        return {
            "archive_path": archive_path,
            "format": fmt,
            "entries": entries,
            "total_entries": len(entries),
            "compressed_size": compressed_size,
            "uncompressed_size": total_uncompressed,
        }

    @skill(
        "archive.extract",
        description=(
            "Extract an archive safely with built-in guards: path traversal, "
            "anti zip-bomb quota, opt-in symlinks. Optional glob filter."
        ),
        examples=[
            {
                "archive_path": "/path/to/bundle.zip",
                "target_dir": "/tmp/unpacked",
                "glob_filter": "*.txt",
            },
        ],
    )
    async def extract_archive(
        self,
        archive_path: Annotated[str, "Filesystem path to the archive file."],
        target_dir: Annotated[str, "Directory where the archive is unpacked (created if missing)."],
        glob_filter: Annotated[
            str | None,
            "fnmatch pattern applied to entry names (e.g. '*.csv', 'docs/*'). Omit to extract everything.",
        ] = None,
        max_uncompressed_bytes: Annotated[
            int,
            "Total uncompressed size cap (anti zip-bomb). Entries exceeding the quota are skipped.",
        ] = DEFAULT_MAX_UNCOMPRESSED,
        allow_symlinks: Annotated[
            bool,
            "Allow extracting symlinks/hardlinks. Disabled by default for safety.",
        ] = False,
        archive_format: Annotated[
            str | None,
            "Force the format: 'zip' | 'tar' | 'tar.gz' | 'tar.bz2' | 'tar.xz'. Omit to infer from extension.",
        ] = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Extrait une archive en toute sécurité."""
        fmt = _resolve_format(archive_format, archive_path)
        if not Path(archive_path).exists():
            raise DomainError("FILE_NOT_FOUND", f"archive introuvable : {archive_path}")

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
                            ctx.logger.warning("refus path traversal", entry=name)
                            continue
                        if total_bytes + info.file_size > max_uncompressed_bytes:
                            skipped["quota"] += 1
                            continue
                        zf.extract(info, target)
                        if not info.is_dir():
                            total_bytes += info.file_size
                            extracted += 1
            except zipfile.BadZipFile as exc:
                raise DomainError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
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
                            ctx.logger.warning("refus path traversal", entry=name)
                            continue
                        if total_bytes + member.size > max_uncompressed_bytes:
                            skipped["quota"] += 1
                            continue
                        tf.extract(member, target, filter="data")
                        if not member.isdir():
                            total_bytes += member.size
                            extracted += 1
            except tarfile.TarError as exc:
                raise DomainError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

        return {
            "target_dir": str(target),
            "extracted_count": extracted,
            "skipped_count": sum(skipped.values()),
            "skipped_reasons": skipped,
            "total_uncompressed_bytes": total_bytes,
        }

    @skill(
        "archive.create",
        description=(
            "Create an archive from source files and/or directories. "
            "Directories are added recursively."
        ),
        examples=[
            {
                "output_path": "/tmp/bundle.zip",
                "archive_format": "zip",
                "sources": ["/path/to/dir", "/path/to/file.txt"],
            },
        ],
    )
    async def create_archive(
        self,
        output_path: Annotated[str, "Filesystem path of the archive to create."],
        archive_format: Annotated[
            str,
            "Archive format: 'zip' | 'tar' | 'tar.gz' | 'tar.bz2' | 'tar.xz'.",
        ],
        sources: Annotated[
            list[str],
            "List of file or directory paths to include. Directories are added recursively.",
        ],
        base_dir: Annotated[
            str | None,
            "Base directory used to compute archive-relative paths. Defaults to the common parent of sources.",
        ] = None,
        compression_level: Annotated[
            int | None,
            "Compression level (0-9 for zip / gz / bz2). Default 6 for zip, library default otherwise.",
        ] = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Crée une archive."""
        if archive_format not in SUPPORTED_FORMATS:
            raise DomainError(
                "UNSUPPORTED_FORMAT",
                f"Format non supporté : {archive_format!r}",
                details={"supported": list(SUPPORTED_FORMATS)},
            )
        if not sources:
            raise DomainError(
                "MISSING_FIELD",
                "Champ 'sources' requis (liste non vide de paths)",
                details={"field": "sources"},
            )

        src_paths = [Path(s).expanduser().resolve() for s in sources]
        base = (
            Path(base_dir).expanduser().resolve()
            if base_dir
            else _common_parent(src_paths)
        )

        out = Path(output_path).expanduser().resolve()
        out.parent.mkdir(parents=True, exist_ok=True)

        entries_count = 0
        uncompressed_size = 0

        if archive_format == "zip":
            level = 6 if compression_level is None else int(compression_level)
            with zipfile.ZipFile(
                out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=level
            ) as zf:
                for src in src_paths:
                    if not src.exists():
                        raise DomainError("FILE_NOT_FOUND", f"source introuvable : {src}")
                    if src.is_file():
                        zf.write(src, _rel_to(src, base))
                        entries_count += 1
                        uncompressed_size += src.stat().st_size
                    else:
                        for sub in src.rglob("*"):
                            if sub.is_file():
                                zf.write(sub, _rel_to(sub, base))
                                entries_count += 1
                                uncompressed_size += sub.stat().st_size
        else:
            mode = _tar_mode(archive_format, "w")
            kwargs: dict[str, Any] = {}
            if compression_level is not None and archive_format != "tar":
                kwargs["compresslevel"] = int(compression_level)
            with tarfile.open(out, mode, **kwargs) as tf:
                for src in src_paths:
                    if not src.exists():
                        raise DomainError("FILE_NOT_FOUND", f"source introuvable : {src}")
                    tf.add(src, arcname=_rel_to(src, base), recursive=True)
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
            "format": archive_format,
            "entries_count": entries_count,
            "compressed_size": out.stat().st_size,
            "uncompressed_size": uncompressed_size,
        }

    @skill(
        "archive.read_file",
        description=(
            "Read a single file from an archive without extracting everything. "
            "text mode returns UTF-8 string; binary mode returns base64."
        ),
        examples=[
            {
                "archive_path": "/path/to/bundle.zip",
                "entry_path": "docs/README.md",
                "mode": "text",
            },
        ],
    )
    async def read_file(
        self,
        archive_path: Annotated[str, "Filesystem path to the archive file."],
        entry_path: Annotated[str, "Path of the file inside the archive (use archive.list to discover entries)."],
        mode: Annotated[
            str,
            "'text' (decode as UTF-8 string) | 'binary' (return base64-encoded bytes).",
        ] = "text",
        encoding: Annotated[
            str,
            "Text encoding when mode='text' (default 'utf-8'). Ignored in binary mode.",
        ] = "utf-8",
        max_bytes: Annotated[
            int,
            "Maximum bytes read before raising TOO_LARGE. Default 10 MiB.",
        ] = DEFAULT_MAX_READ_BYTES,
        archive_format: Annotated[
            str | None,
            "Force the format: 'zip' | 'tar' | 'tar.gz' | 'tar.bz2' | 'tar.xz'. Omit to infer from extension.",
        ] = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Lit un fichier interne à l'archive."""
        if mode not in ("text", "binary"):
            raise DomainError(
                "INVALID_TYPE",
                f"mode doit être 'text' ou 'binary'. Reçu : {mode!r}",
                details={"field": "mode"},
            )
        fmt = _resolve_format(archive_format, archive_path)
        if not Path(archive_path).exists():
            raise DomainError("FILE_NOT_FOUND", f"archive introuvable : {archive_path}")

        raw: bytes
        if fmt == "zip":
            try:
                with zipfile.ZipFile(archive_path, "r") as zf:
                    try:
                        with zf.open(entry_path, "r") as fh:
                            raw = fh.read(max_bytes + 1)
                    except KeyError as exc:
                        raise DomainError(
                            "ENTRY_NOT_FOUND",
                            f"entry introuvable : {entry_path}",
                            details={"entry_path": entry_path},
                        ) from exc
            except zipfile.BadZipFile as exc:
                raise DomainError("PARSE_ERROR", f"Zip corrompu : {exc}") from exc
        else:
            try:
                with tarfile.open(archive_path, _tar_mode(fmt, "r")) as tf:
                    try:
                        member = tf.getmember(entry_path)
                    except KeyError as exc:
                        raise DomainError(
                            "ENTRY_NOT_FOUND",
                            f"entry introuvable : {entry_path}",
                            details={"entry_path": entry_path},
                        ) from exc
                    fh = tf.extractfile(member)
                    if fh is None:
                        raise DomainError(
                            "ENTRY_NOT_FOUND",
                            f"entry n'est pas un fichier régulier : {entry_path}",
                            details={"entry_path": entry_path},
                        )
                    raw = fh.read(max_bytes + 1)
            except tarfile.TarError as exc:
                raise DomainError("PARSE_ERROR", f"Tar corrompu : {exc}") from exc

        truncated = len(raw) > max_bytes
        if truncated:
            raw = raw[:max_bytes]

        out: dict[str, Any] = {
            "entry_path": entry_path,
            "mode": mode,
            "size_bytes": len(raw),
            "truncated": truncated,
        }
        if mode == "text":
            try:
                out["content"] = raw.decode(encoding)
            except UnicodeDecodeError as exc:
                raise DomainError(
                    "PARSE_ERROR",
                    f"Décodage {encoding} impossible : {exc}",
                    details={"encoding": encoding},
                ) from exc
        else:
            out["content_base64"] = base64.b64encode(raw).decode("ascii")
        return out


# ─── Helpers ─────────────────────────────────────────────────────────────


def _resolve_format(explicit: str | None, archive_path: str) -> str:
    fmt = explicit or _detect_format(archive_path)
    if fmt not in SUPPORTED_FORMATS:
        raise DomainError(
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
    if fmt == "tar":
        return f"{action}:"
    if fmt == "tar.gz":
        return f"{action}:gz"
    if fmt == "tar.bz2":
        return f"{action}:bz2"
    if fmt == "tar.xz":
        return f"{action}:xz"
    raise DomainError("UNSUPPORTED_FORMAT", f"format tar invalide : {fmt!r}")


def _is_path_safe(target_dir: Path, entry_name: str) -> bool:
    candidate = (target_dir / entry_name).resolve()
    try:
        candidate.relative_to(target_dir.resolve())
        return True
    except ValueError:
        return False


def _zip_is_symlink(info: zipfile.ZipInfo) -> bool:
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
