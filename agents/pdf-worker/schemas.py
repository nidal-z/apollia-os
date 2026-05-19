"""Schémas I/O documentaires pour pdf-worker.

``TypedDict`` stdlib pour documenter le contrat A2A des 6 skills. Aucune
validation runtime — validation manuelle dans ``pdf-worker.py``.

Convention multi-skills : chaque payload inclut ``op`` qui discrimine
l'opération (limitation v0.1.0 — skill_id non propagé côté Python).
"""

from __future__ import annotations

from typing import Any, Literal, NotRequired, TypedDict


PageSize = Literal["A4", "Letter", "Legal", "A3", "A5"]
Orientation = Literal["portrait", "landscape"]
FieldType = Literal[
    "text", "checkbox", "radio", "pushbutton",
    "dropdown", "listbox", "signature", "choice", "button", "unknown",
]


# ─── pdf.read-text ─────────────────────────────────────────────────────────


class ReadTextInput(TypedDict):
    path: str
    page_range: NotRequired[str]
    max_chars_per_page: NotRequired[int]
    include_metadata: NotRequired[bool]


class PdfMetadata(TypedDict, total=False):
    title: str
    author: str
    subject: str
    creator: str
    producer: str
    creation_date: str
    modification_date: str


class PageReadOutput(TypedDict):
    page_num: int
    text: str
    char_count: int
    truncated: bool


class ReadTextOutput(TypedDict, total=False):
    path: str
    total_pages: int
    pages: list[PageReadOutput]
    metadata: PdfMetadata


# ─── pdf.read-tables ───────────────────────────────────────────────────────


class ReadTablesInput(TypedDict):
    path: str
    page_range: NotRequired[str]
    table_settings: NotRequired[dict[str, Any]]
    include_headers: NotRequired[bool]


class TableExtract(TypedDict, total=False):
    page_num: int
    table_index: int
    rows: list[list[str | None]]
    rows_count: int
    cols_count: int
    headers: list[str | None]


class ReadTablesOutput(TypedDict):
    path: str
    tables: list[TableExtract]
    total_tables: int


# ─── pdf.read-forms ────────────────────────────────────────────────────────


class ReadFormsInput(TypedDict):
    path: str


class FormField(TypedDict, total=False):
    name: str
    type: FieldType
    value: str
    options: list[str]
    readonly: bool
    required: bool
    max_length: int


class ReadFormsOutput(TypedDict):
    path: str
    fields: list[FormField]
    total_fields: int


# ─── pdf.render-from-markdown ──────────────────────────────────────────────


class MarginsCm(TypedDict, total=False):
    top: float
    bottom: float
    left: float
    right: float


class RenderFromMarkdownInput(TypedDict, total=False):
    output_path: str
    markdown: str
    markdown_path: str
    page_size: PageSize
    orientation: Orientation
    margins_cm: MarginsCm
    title: str
    author: str
    subject: str
    overwrite: bool


class RenderFromMarkdownOutput(TypedDict):
    output_path: str
    pages_count: int
    file_size_bytes: int


# ─── pdf.merge ─────────────────────────────────────────────────────────────


class MergeInput(TypedDict):
    paths: list[str]
    output_path: str
    overwrite: NotRequired[bool]
    metadata_from: NotRequired[int]


class MergeOutput(TypedDict):
    output_path: str
    merged_count: int
    total_pages: int
    file_size_bytes: int


# ─── pdf.split ─────────────────────────────────────────────────────────────


class SplitRangeSpec(TypedDict):
    name: str
    pages: str  # ex "1-5,7,10-12"


class SplitInput(TypedDict):
    path: str
    output_dir: str
    ranges: NotRequired[list[SplitRangeSpec]]
    overwrite: NotRequired[bool]


class SplitEntry(TypedDict):
    output_path: str
    pages_count: int
    file_size_bytes: int
    page_range: str


class SplitOutput(TypedDict):
    output_dir: str
    splits: list[SplitEntry]
    total_splits: int


# ─── Codes d'erreur ────────────────────────────────────────────────────────


ErrorCode = Literal[
    "INVALID_PAYLOAD",
    "MISSING_FIELD",
    "INVALID_TYPE",
    "INVALID_DATA",
    "FILE_NOT_FOUND",
    "UNSUPPORTED_FORMAT",
    "UNSUPPORTED_FEATURE",
    "OUTPUT_EXISTS",
    "TOO_LARGE",
    "PARSE_ERROR",
    "ENCRYPTED_PDF",
    "INVALID_PAGE_RANGE",
    "PAGE_OUT_OF_RANGE",
    "MARKDOWN_PARSE_ERROR",
    "RENDER_ERROR",
    "EXECUTION_FAILED",
]


class ErrorDetails(TypedDict, total=False):
    field: str
    expected: list[Any]
    got: Any
    path: str
    output_path: str
    size_bytes: int
    limit_bytes: int
    index: int
    page: int
    total_pages: int
    token: str
    feature: str
    reason: str
