"""Schémas I/O documentaires pour archive-worker.

Ce module utilise ``TypedDict`` (stdlib) pour documenter le contrat A2A des
4 skills exposés. Aucune validation runtime n'est effectuée par ces classes —
la validation est faite manuellement dans ``archive-worker.py`` (run/_do_*).

Le choix de ``TypedDict`` plutôt que Pydantic préserve l'absence totale de
dépendance externe (alignement Principe 2 d'Apollia OS).

Dispatch multi-skills : depuis 2026-05-19, le runtime Apollia propage
``skill_id`` dans ``AIPTask`` (lisible côté Python via
``apollia.utils.a2a.extract_skill_id(task)``). Le worker dispatche sur le
full skill id (``"archive.list"``, etc.) — plus de champ ``op`` dans les
payloads.
"""

from __future__ import annotations

from typing import Any, Literal, NotRequired, TypedDict


# ─── archive.list ──────────────────────────────────────────────────────────


class ListInput(TypedDict):
    """Payload d'entrée du skill ``archive.list``."""

    archive_path: str
    archive_format: NotRequired[str]


class ArchiveEntry(TypedDict):
    """Une entrée d'archive — fichier ou répertoire."""

    path: str
    size: int
    mtime: str | None  # ISO 8601, None si timestamp invalide
    is_dir: bool
    is_symlink: bool


class ListOutput(TypedDict):
    """Sortie du skill ``archive.list``."""

    archive_path: str
    format: str
    entries: list[ArchiveEntry]
    total_entries: int
    compressed_size: int
    uncompressed_size: int


# ─── archive.extract ───────────────────────────────────────────────────────


class ExtractInput(TypedDict):
    """Payload d'entrée du skill ``archive.extract``."""

    archive_path: str
    target_dir: str
    glob_filter: NotRequired[str]
    max_uncompressed_bytes: NotRequired[int]
    allow_symlinks: NotRequired[bool]
    archive_format: NotRequired[str]


class ExtractSkippedReasons(TypedDict):
    """Compteurs des entries non extraites, par raison."""

    path_traversal: int
    symlink: int
    quota: int
    glob_mismatch: int


class ExtractOutput(TypedDict):
    """Sortie du skill ``archive.extract``."""

    target_dir: str
    extracted_count: int
    skipped_count: int
    skipped_reasons: ExtractSkippedReasons
    total_uncompressed_bytes: int


# ─── archive.create ────────────────────────────────────────────────────────


class CreateInput(TypedDict):
    """Payload d'entrée du skill ``archive.create``."""

    output_path: str
    archive_format: Literal["zip", "tar", "tar.gz", "tar.bz2", "tar.xz"]
    sources: list[str]
    base_dir: NotRequired[str]
    compression_level: NotRequired[int]


class CreateOutput(TypedDict):
    """Sortie du skill ``archive.create``."""

    output_path: str
    format: str
    entries_count: int
    compressed_size: int
    uncompressed_size: int


# ─── archive.read-file ─────────────────────────────────────────────────────


class ReadFileInput(TypedDict):
    """Payload d'entrée du skill ``archive.read-file``."""

    archive_path: str
    entry_path: str
    mode: NotRequired[Literal["text", "binary"]]
    encoding: NotRequired[str]
    max_bytes: NotRequired[int]
    archive_format: NotRequired[str]


class ReadFileOutput(TypedDict, total=False):
    """Sortie du skill ``archive.read-file``.

    ``content`` est présent en mode 'text', ``content_base64`` en mode 'binary'.
    Les autres champs sont toujours présents.
    """

    entry_path: str
    mode: Literal["text", "binary"]
    size_bytes: int
    truncated: bool
    content: str             # mode == 'text'
    content_base64: str      # mode == 'binary'


# ─── Erreurs ───────────────────────────────────────────────────────────────


ErrorCode = Literal[
    "MISSING_SKILL_ID",
    "UNKNOWN_SKILL_ID",
    "INVALID_PAYLOAD",
    "MISSING_FIELD",
    "INVALID_TYPE",
    "FILE_NOT_FOUND",
    "UNSUPPORTED_FORMAT",
    "PARSE_ERROR",
    "ENTRY_NOT_FOUND",
    "EXECUTION_FAILED",
]


class ErrorDetails(TypedDict, total=False):
    """Détails optionnels d'une erreur (forme libre selon code)."""

    field: str
    expected: list[Any]
    got: Any
    detected: str | None
    supported: list[str]
    encoding: str
    entry_path: str
