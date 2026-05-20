"""Schémas I/O documentaires pour docx-worker.

Ce module utilise ``TypedDict`` (stdlib) pour documenter le contrat A2A des
5 skills exposés ET pour **structurer les `input_schema`** générés par le
SDK (les LLM mid-market voient ainsi la forme exacte des dicts d'entrée
— ex. `sections[i]`, `blocks[i]` polymorphe via union, `named_styles` —
sans avoir besoin de la structure dans le system prompt). Aligné Principe 2
(zéro dépendance externe : tout provient de la stdlib).

**Pas de `from __future__ import annotations`** : sous PEP 563 les
annotations sont des strings et ``TypedDict.__required_keys__`` ne peut
pas détecter ``NotRequired[T]`` à la création de classe — ce qui rendrait
tous les champs requis dans le JSON Schema final.

Headers/footers placeholders (10) :
    {page}, {total_pages}                   — champs Word PAGE/NUMPAGES
    {date}, {time}, {datetime}              — formatables via :strftime
    {author}, {title}, {subject}            — core_properties du doc
    {filename}                              — basename du output_path
    {section_num}                           — index 1-based de la section
"""

from typing import Any, Literal, NotRequired, TypedDict, Union


# ─── Style spec ────────────────────────────────────────────────────────────


class FontSpec(TypedDict, total=False):
    """Spec d'une police. Tous les champs optionnels."""

    name: str
    size_pt: float
    bold: bool
    italic: bool
    underline: bool
    strike: bool
    color: str  # hex 'RRGGBB' ou 'AARRGGBB' (préfixe '#' toléré)
    highlight: Literal[
        "yellow", "green", "cyan", "magenta", "blue", "red",
        "darkblue", "darkcyan", "darkgreen", "darkmagenta",
        "darkred", "darkyellow", "darkgray", "lightgray",
        "black", "white",
    ]


class ParagraphFormatSpec(TypedDict, total=False):
    alignment: Literal["left", "center", "right", "justify"]
    space_before_pt: float
    space_after_pt: float
    line_spacing: float  # multiplicateur (1.0 = simple, 1.5 = 1.5x, 2.0 = double)
    first_line_indent_cm: float
    left_indent_cm: float
    right_indent_cm: float


class BorderSideSpec(TypedDict, total=False):
    style: Literal["single", "double", "thick", "dashed", "dotted", "none"]
    size_pt: float
    color: str


class TableBordersSpec(TypedDict, total=False):
    top: BorderSideSpec
    bottom: BorderSideSpec
    left: BorderSideSpec
    right: BorderSideSpec
    inside_h: BorderSideSpec
    inside_v: BorderSideSpec


class CellShadingSpec(TypedDict, total=False):
    color: str
    pattern: Literal["clear", "solid"]


class TableStyleSpec(TypedDict, total=False):
    borders: TableBordersSpec
    cell_shading: CellShadingSpec
    alignment: Literal["left", "center", "right"]
    autofit: bool


class StyleSpec(TypedDict, total=False):
    """Style nommé réutilisable. Tous les champs optionnels."""

    font: FontSpec
    paragraph: ParagraphFormatSpec
    table: TableStyleSpec


# ─── Block specs (write/append) ────────────────────────────────────────────


class RunSpec(TypedDict, total=False):
    text: str
    font: FontSpec


class ParagraphBlock(TypedDict):
    """Paragraphe — ``text`` OU ``runs`` (mutuellement exclusif côté code)."""

    type: Literal["paragraph"]
    text: NotRequired[str]
    runs: NotRequired[list[RunSpec]]
    style: NotRequired[str]
    alignment: NotRequired[Literal["left", "center", "right", "justify"]]


class HeadingBlock(TypedDict):
    """Titre Word (Heading 1..9)."""

    type: Literal["heading"]
    text: str
    level: int  # 1-9
    style: NotRequired[str]


class CellMergeSpec(TypedDict, total=False):
    cols: int  # nombre de colonnes à merger (gridSpan)


class TableCellSpec(TypedDict, total=False):
    text: str
    style: str
    alignment: Literal["left", "center", "right", "justify"]
    merge: CellMergeSpec


class CellStyleApply(TypedDict, total=False):
    row: int
    col: int
    style: str


class TableBlock(TypedDict):
    """Tableau."""

    type: Literal["table"]
    rows: list[list[Any]]  # liste de listes — chaque cell = str ou TableCellSpec
    style: NotRequired[str]
    column_widths_cm: NotRequired[list[float]]
    header_row: NotRequired[bool]
    cell_styles: NotRequired[list[CellStyleApply]]


class ListBlock(TypedDict):
    """Liste à puces / numérotée."""

    type: Literal["list"]
    items: list[Any]  # str ou {text, items: [...]} pour nested
    list_type: NotRequired[Literal["bullet", "number"]]
    style: NotRequired[str]


class ImageBlock(TypedDict):
    """Image embarquée. Le chemin doit exister sur le disque local."""

    type: Literal["image"]
    path: str
    width_cm: NotRequired[float]
    height_cm: NotRequired[float]
    alignment: NotRequired[Literal["left", "center", "right"]]


class PageBreakBlock(TypedDict):
    """Saut de page."""

    type: Literal["page_break"]


class SectionBreakBlock(TypedDict):
    """Saut de section (changement de page setup)."""

    type: Literal["section_break"]
    break_type: NotRequired[Literal["next_page", "continuous", "odd_page", "even_page"]]


# Polymorphic block union — discriminé par ``type``. Le SDK rend cette union
# en ``anyOf`` dans le JSON Schema, exposant toutes les variantes possibles
# avec leurs champs au LLM.
DocxBlock = Union[
    ParagraphBlock,
    HeadingBlock,
    TableBlock,
    ListBlock,
    ImageBlock,
    PageBreakBlock,
    SectionBreakBlock,
]


# ─── docx.read ─────────────────────────────────────────────────────────────


class ReadInput(TypedDict):
    path: str
    include_runs: NotRequired[bool]
    include_styles: NotRequired[bool]
    max_blocks: NotRequired[int]


class SectionMarginsRead(TypedDict):
    top: float | None
    bottom: float | None
    left: float | None
    right: float | None


class SectionRead(TypedDict, total=False):
    orientation: Literal["portrait", "landscape"]
    margins_cm: SectionMarginsRead
    paper_size: str | None
    header_text: str | None
    footer_text: str | None
    blocks: list[dict[str, Any]]


class Hyperlink(TypedDict):
    text: str
    url: str


class ReadOutput(TypedDict):
    path: str
    sections: list[SectionRead]
    styles_used: list[str]
    hyperlinks: list[Hyperlink]
    total_blocks: int
    truncated: bool


# ─── docx.write ────────────────────────────────────────────────────────────


class MarginsCm(TypedDict, total=False):
    top: float
    bottom: float
    left: float
    right: float


class DocumentSetup(TypedDict, total=False):
    orientation: Literal["portrait", "landscape"]
    margins_cm: MarginsCm
    paper_size: Literal["A4", "Letter", "Legal", "A3", "A5"]


class HeaderFooterSpec(TypedDict, total=False):
    """Header ou footer — 3 zones (left, center, right).

    10 placeholders disponibles dans chaque zone :
    - {page} {total_pages} (champs Word)
    - {date} {time} {datetime} (formatables : {date:%d/%m/%Y})
    - {author} {title} {subject} {filename} {section_num}
    """

    left: str
    center: str
    right: str


class SectionWriteSpec(TypedDict):
    """Spec d'une section à écrire.

    ``blocks`` est requis ; la page setup (orientation, marges, paper_size,
    header/footer) est entièrement optionnelle.
    """

    blocks: list[DocxBlock]
    orientation: NotRequired[Literal["portrait", "landscape"]]
    margins_cm: NotRequired[MarginsCm]
    paper_size: NotRequired[Literal["A4", "Letter", "Legal", "A3", "A5"]]
    header: NotRequired[HeaderFooterSpec]
    footer: NotRequired[HeaderFooterSpec]


class WriteInput(TypedDict):
    output_path: str
    sections: list[SectionWriteSpec]
    named_styles: NotRequired[dict[str, StyleSpec]]
    document_setup: NotRequired[DocumentSetup]
    overwrite: NotRequired[bool]


class WriteOutput(TypedDict):
    output_path: str
    sections_count: int
    blocks_count: int
    styles_applied_count: int


# ─── docx.append-section ───────────────────────────────────────────────────


class AppendSectionInput(TypedDict):
    path: str
    blocks: list[dict[str, Any]]
    named_styles: NotRequired[dict[str, StyleSpec]]
    section_break_before: NotRequired[bool]
    new_section_setup: NotRequired[SectionWriteSpec]


class AppendSectionOutput(TypedDict):
    path: str
    blocks_appended: int
    new_section_started: bool


# ─── docx.extract-tables ───────────────────────────────────────────────────


class ExtractTablesInput(TypedDict):
    path: str
    include_headers: NotRequired[bool]


class TableExtract(TypedDict, total=False):
    index: int
    rows: list[list[str]]
    rows_count: int
    cols_count: int
    headers: list[str]


class ExtractTablesOutput(TypedDict):
    path: str
    tables: list[TableExtract]
    total_tables: int


# ─── docx.render-from-template ─────────────────────────────────────────────


class RenderFromTemplateInput(TypedDict):
    template_path: str
    output_path: str
    context: dict[str, Any]
    strict_undefined: NotRequired[bool]
    overwrite: NotRequired[bool]


class RenderFromTemplateOutput(TypedDict):
    output_path: str
    template_path: str
    variables_used: list[str]
    undefined_variables: list[str]


# ─── Codes d'erreur ────────────────────────────────────────────────────────


ErrorCode = Literal[
    "INVALID_PAYLOAD",
    "MISSING_FIELD",
    "INVALID_TYPE",
    "FILE_NOT_FOUND",
    "UNSUPPORTED_FORMAT",
    "OUTPUT_EXISTS",
    "TOO_LARGE",
    "PARSE_ERROR",
    "INVALID_STYLE",
    "STYLE_NOT_FOUND",
    "INVALID_BLOCK",
    "IMAGE_NOT_FOUND",
    "IMAGE_UNSUPPORTED",
    "TEMPLATE_RENDER_ERROR",
    "UNDEFINED_VARIABLES",
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
    style_id: str
    available: list[str]
    block: dict[str, Any]
    undefined: list[str]
    reason: str
