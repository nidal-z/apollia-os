"""Schémas I/O documentaires pour chart-worker.

Ce module utilise ``TypedDict`` (stdlib) pour documenter le contrat A2A des
5 skills exposés. Aucune validation runtime — la validation est manuelle
dans ``chart-worker.py``. Aligné Principe 2 (zéro dépendance pour les schemas).

Convention multi-skills : chaque payload inclut un champ ``op`` qui
discrimine l'opération. Tant qu'Apollia v0.1.0 ne propage pas ``skill_id``
côté Python, ce discriminateur reste obligatoire.

Modes de sortie : ``file`` (écrit sur disque, retourne path) ou ``base64``
(retourne content_base64 dans le JSON, pas d'écriture disque).
"""

from __future__ import annotations

from typing import Any, Literal, NotRequired, TypedDict


# ─── Common input fields (shared across all 5 skills) ──────────────────────


Theme = Literal["default", "dark", "minimal"]
ChartFormat = Literal["png", "svg"]
OutputMode = Literal["file", "base64"]


class CommonChartFields(TypedDict, total=False):
    """Champs communs à tous les skills chart.*."""

    output_path: str
    output_mode: OutputMode
    format: ChartFormat
    title: str
    xlabel: str
    ylabel: str
    width_inches: float
    height_inches: float
    dpi: int
    theme: Theme
    legend: bool
    grid: bool
    overwrite: bool
    font_size: int
    title_font_size: int


# ─── chart.bar ─────────────────────────────────────────────────────────────


class BarPoint(TypedDict):
    x: Any
    y: float


class BarSeriesSpec(TypedDict, total=False):
    name: str
    data: list[Any]  # list[float] OU list[BarPoint]
    color: str  # hex


BarOrientation = Literal["vertical", "horizontal"]
BarStyle = Literal["grouped", "stacked"]


class BarInput(TypedDict, total=False):
    series: list[BarSeriesSpec]
    categories: list[str]
    orientation: BarOrientation
    bar_style: BarStyle
    value_labels: bool
    # + CommonChartFields ...


# ─── chart.line ────────────────────────────────────────────────────────────


LineStyle = Literal["solid", "dashed", "dotted"]
LineMarker = Literal["o", "s", "^", "D", "x", "+", "none"]
XType = Literal["auto", "number", "datetime", "category"]


class LinePoint(TypedDict):
    x: Any
    y: float


class LineSeriesSpec(TypedDict, total=False):
    name: str
    data: list[LinePoint]
    color: str
    line_style: LineStyle
    line_width: float
    marker: LineMarker


class LineInput(TypedDict, total=False):
    series: list[LineSeriesSpec]
    x_type: XType
    y_log_scale: bool
    area_fill: bool


# ─── chart.pie ─────────────────────────────────────────────────────────────


class PieSlice(TypedDict):
    label: str
    value: float


class PieInput(TypedDict, total=False):
    data: list[PieSlice]
    donut: bool
    hole_size: float
    colors: list[str]
    show_percent: bool
    show_values: bool
    explode: list[float]
    start_angle: float


# ─── chart.scatter ─────────────────────────────────────────────────────────


ScatterMarkerStyle = Literal["circle", "square", "triangle", "diamond", "x", "plus"]


class ScatterPoint(TypedDict, total=False):
    x: Any
    y: Any
    size: float


class ScatterSeriesSpec(TypedDict, total=False):
    name: str
    data: list[ScatterPoint]
    color: str
    marker_style: ScatterMarkerStyle


class ScatterInput(TypedDict, total=False):
    series: list[ScatterSeriesSpec]
    x_type: Literal["number", "datetime"]
    y_type: Literal["number", "datetime"]
    regression: bool
    marker_size: float
    alpha: float


# ─── chart.heatmap ─────────────────────────────────────────────────────────


Colormap = Literal[
    "viridis", "plasma", "magma", "inferno",
    "Blues", "Reds", "Greens", "RdBu",
]


class HeatmapInput(TypedDict, total=False):
    matrix: list[list[float]]
    row_labels: list[str]
    col_labels: list[str]
    colormap: Colormap
    vmin: float
    vmax: float
    show_values: bool
    value_format: str
    colorbar: bool
    colorbar_label: str


# ─── Output ────────────────────────────────────────────────────────────────


class FileOutput(TypedDict):
    output_path: str
    format: ChartFormat
    width_inches: float
    height_inches: float
    dpi: int
    data_points_count: int
    file_size_bytes: int


class Base64Output(TypedDict):
    content_base64: str
    format: ChartFormat
    width_inches: float
    height_inches: float
    dpi: int
    data_points_count: int


# ─── Codes d'erreur ────────────────────────────────────────────────────────


ErrorCode = Literal[
    "INVALID_PAYLOAD",
    "MISSING_FIELD",
    "INVALID_TYPE",
    "INVALID_DATA",
    "INVALID_FORMAT",
    "INVALID_COLORMAP",
    "INVALID_STYLE",
    "UNSUPPORTED_X_TYPE",
    "TOO_MANY_POINTS",
    "OUTPUT_EXISTS",
    "EXECUTION_FAILED",
]


class ErrorDetails(TypedDict, total=False):
    field: str
    expected: list[Any]
    got: Any
    index: int
    width: float
    height: float
    dpi: int
    where: str
    sum: float
    output_path: str
    path: str
    limit: int
