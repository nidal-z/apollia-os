"""Schémas I/O documentaires pour xlsx-worker.

Ce module utilise ``TypedDict`` (stdlib) pour documenter le contrat A2A des
5 skills exposés. Aucune validation runtime — la validation est manuelle
dans ``xlsx-worker.py``. Aligné Principe 2 (zéro dépendance pour les schemas).

Convention multi-skills : chaque payload inclut un champ ``op`` qui
discrimine l'opération. Tant qu'Apollia v0.1.0 ne propage pas ``skill_id``
côté Python, ce discriminateur reste obligatoire.
"""

from __future__ import annotations

from typing import Any, Literal, NotRequired, TypedDict


# ─── Style spec ────────────────────────────────────────────────────────────


class FontSpec(TypedDict, total=False):
    """Spec d'une police de cellule. Tous les champs optionnels."""

    name: str
    size: float
    bold: bool
    italic: bool
    underline: bool
    strike: bool
    color: str  # hex 'RRGGBB' ou 'AARRGGBB' ou '#RRGGBB'


class FillSpec(TypedDict, total=False):
    color: str
    pattern: Literal["solid", "darkGray", "lightGray", "darkGrid"]


class SideSpec(TypedDict, total=False):
    style: Literal["thin", "medium", "thick", "dashed", "dotted", "double"]
    color: str


class BorderSpec(TypedDict, total=False):
    top: SideSpec
    bottom: SideSpec
    left: SideSpec
    right: SideSpec


class AlignmentSpec(TypedDict, total=False):
    horizontal: Literal["general", "left", "center", "right", "justify"]
    vertical: Literal["top", "center", "bottom"]
    wrap_text: bool
    indent: int


class StyleSpec(TypedDict, total=False):
    """Style nommé réutilisable. Tous les champs optionnels."""

    font: FontSpec
    fill: FillSpec
    border: BorderSpec
    alignment: AlignmentSpec
    number_format: str  # ex: "0.00", "#,##0.00 €", "yyyy-mm-dd"


# ─── xlsx.read ─────────────────────────────────────────────────────────────


class ReadInput(TypedDict):
    path: str
    sheet_names: NotRequired[list[str]]
    read_mode: NotRequired[Literal["values", "formulas"]]
    max_rows: NotRequired[int]
    max_cols: NotRequired[int]
    include_types: NotRequired[bool]


CellTypeName = Literal[
    "string", "int", "float", "bool", "datetime", "date", "formula", "error", "null", "unknown"
]


class TypedCell(TypedDict):
    value: Any
    type: CellTypeName


class SheetReadOutput(TypedDict):
    name: str
    headers: list[str] | None
    rows: list[list[Any]]  # ou list[list[TypedCell]] si include_types
    total_rows: int
    total_cols: int
    merged_ranges: list[str]
    frozen_rows: int
    frozen_cols: int
    truncated: bool


class ReadOutput(TypedDict):
    path: str
    read_mode: Literal["values", "formulas"]
    sheets: list[SheetReadOutput]


# ─── xlsx.read-as-dataframe ────────────────────────────────────────────────


DTypeHint = Literal["int", "float", "str", "bool", "datetime"]


class ReadAsDataFrameInput(TypedDict):
    path: str
    sheet_name: NotRequired[str | int]
    header_row: NotRequired[int | None]
    skiprows: NotRequired[int]
    nrows: NotRequired[int]
    dtypes_hint: NotRequired[dict[str, DTypeHint]]
    na_values: NotRequired[list[str]]


class ReadAsDataFrameOutput(TypedDict):
    path: str
    sheet_name: str | int | None
    columns: list[str]
    dtypes: dict[str, str]
    records: list[dict[str, Any]]
    rows_count: int
    na_count: int


# ─── xlsx.write ────────────────────────────────────────────────────────────


class FreezeSpec(TypedDict, total=False):
    row: int  # nombre de lignes figées
    col: int  # nombre de colonnes figées


class StyleApplyEntry(TypedDict, total=False):
    range: str       # ex: "A1:C3", "A:C", "1:1"
    cell_ref: str    # ex: "B5" — alternative à range
    style: str       # référence à named_styles


ConditionalRule = Literal[
    "greater_than", "less_than", "equal_to", "not_equal_to",
    "between", "not_between",
    "contains_text", "not_contains_text",
    "begins_with", "ends_with",
]


class ConditionalFormatSpec(TypedDict, total=False):
    range: str
    rule: ConditionalRule
    value: Any              # pour rules unaires
    values: list[Any]       # pour between/not_between : [a, b]
    style: str              # référence à named_styles


class SheetWriteSpec(TypedDict, total=False):
    name: str                              # requis
    headers: list[str]                     # optionnel
    rows: list[list[Any]]                  # requis
    column_widths: dict[str, float]        # {col_letter: width}
    row_heights: dict[str, float]          # {"1": 30, ...} keys are stringified row indices
    freeze: FreezeSpec
    auto_filter: bool
    styles_apply: list[StyleApplyEntry]
    conditional_formatting: list[ConditionalFormatSpec]


class WriteInput(TypedDict):
    output_path: str
    sheets: list[SheetWriteSpec]
    named_styles: NotRequired[dict[str, StyleSpec]]
    overwrite: NotRequired[bool]


class WriteOutput(TypedDict):
    output_path: str
    sheets_count: int
    total_rows_written: int
    total_cells_written: int
    styles_applied_count: int
    conditional_rules_count: int


# ─── xlsx.append-rows ──────────────────────────────────────────────────────


class AppendRowsInput(TypedDict):
    path: str
    sheet_name: NotRequired[str]
    rows: list[list[Any]]


class AppendRowsOutput(TypedDict):
    path: str
    sheet_name: str
    rows_appended: int
    new_total_rows: int


# ─── xlsx.update-cells ─────────────────────────────────────────────────────


class CellUpdate(TypedDict):
    cell_ref: str
    value: Any


class UpdateCellsInput(TypedDict):
    path: str
    sheet_name: str
    updates: list[CellUpdate]


class CellUpdateError(TypedDict, total=False):
    cell_ref: str
    index: int
    reason: str


class UpdateCellsOutput(TypedDict):
    path: str
    sheet_name: str
    updated_count: int
    skipped_count: int
    errors: list[CellUpdateError]


# ─── Codes d'erreur ────────────────────────────────────────────────────────


ErrorCode = Literal[
    "INVALID_PAYLOAD",
    "MISSING_FIELD",
    "INVALID_TYPE",
    "FILE_NOT_FOUND",
    "UNSUPPORTED_FORMAT",
    "PARSE_ERROR",
    "SHEET_NOT_FOUND",
    "OUTPUT_EXISTS",
    "TOO_LARGE",
    "INVALID_STYLE",
    "STYLE_NOT_FOUND",
    "INVALID_RANGE",
    "INVALID_RULE",
    "DTYPE_COERCION_FAILED",
    "EXECUTION_FAILED",
]


class ErrorDetails(TypedDict, total=False):
    field: str
    expected: list[Any]
    got: Any
    path: str
    output_path: str
    sheet_name: str
    available: list[str]
    size_bytes: int
    limit_bytes: int
    row_count: int
    limit: int
    style_id: str
    range: str
    rule: str
    hint: dict[str, Any]
