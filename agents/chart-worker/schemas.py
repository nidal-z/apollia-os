"""Schémas I/O documentaires pour chart-worker.

Ce module utilise ``TypedDict`` (stdlib) pour documenter le contrat A2A des
5 skills exposés et **structurer les `input_schema`** générés par le SDK
(étape B optimisation : les LLM mid-market voient maintenant la forme exacte
des dicts attendus dans `series[i]`, `data[i]`, etc.). Aligné Principe 2
(zéro dépendance externe — `typing.TypedDict` est stdlib).

Modes de sortie : ``file`` (écrit sur disque, retourne path) ou ``base64``
(retourne content_base64 dans le JSON, pas d'écriture disque).

**Convention PEP 655 + PEP 563** : ce module n'utilise PAS
``from __future__ import annotations`` car ``TypedDict.__required_keys__``
est calculé à la création de la classe en inspectant les annotations brutes.
Sous PEP 563 (stringified annotations), ``NotRequired[T]`` n'est pas
détecté et tous les champs deviennent requis. Le SDK utilise
``__required_keys__`` pour produire le bon ``"required": [...]``.
"""

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


BarOrientation = Literal["vertical", "horizontal"]
BarStyle = Literal["grouped", "stacked"]


class BarPoint(TypedDict):
    """One {x, y} point used when bar data is provided as labelled values."""

    x: Any
    y: float


class BarSeries(TypedDict):
    """One bar group in a bar chart.

    ``data`` is either a flat ``list[number]`` (paired with ``categories``
    at the skill level) OR a ``list[{x, y}]`` where ``x`` becomes the
    category label. ``color`` is optional; when omitted the theme palette
    is used.
    """

    name: str
    data: list[Any]  # list[float] OU list[BarPoint]
    color: NotRequired[str]  # hex '#RRGGBB' ou 'RRGGBB'


# Back-compat alias (older code referenced ``BarSeriesSpec``).
BarSeriesSpec = BarSeries


# ─── chart.line ────────────────────────────────────────────────────────────


LineStyle = Literal["solid", "dashed", "dotted"]
LineMarker = Literal["o", "s", "^", "D", "x", "+", "none"]
XType = Literal["auto", "number", "datetime", "category"]


class LinePoint(TypedDict):
    """One {x, y} point in a line series. ``x`` may be number, ISO datetime, or category."""

    x: Any
    y: float


class LineSeries(TypedDict):
    """One line in a line chart."""

    name: str
    data: list[LinePoint]
    color: NotRequired[str]
    line_style: NotRequired[LineStyle]
    line_width: NotRequired[float]
    marker: NotRequired[LineMarker]


LineSeriesSpec = LineSeries  # back-compat alias


# ─── chart.pie ─────────────────────────────────────────────────────────────


class PieSlice(TypedDict):
    """One slice of a pie / donut chart. ``value`` must be non-negative."""

    label: str
    value: float


# ─── chart.scatter ─────────────────────────────────────────────────────────


ScatterMarkerStyle = Literal["circle", "square", "triangle", "diamond", "x", "plus"]


class ScatterPoint(TypedDict):
    """One scatter point. ``size`` overrides the series ``marker_size`` for bubble charts."""

    x: Any
    y: Any
    size: NotRequired[float]


class ScatterSeries(TypedDict):
    """One scatter series."""

    name: str
    data: list[ScatterPoint]
    color: NotRequired[str]
    marker_style: NotRequired[ScatterMarkerStyle]


ScatterSeriesSpec = ScatterSeries  # back-compat alias


# ─── chart.heatmap ─────────────────────────────────────────────────────────


Colormap = Literal[
    "viridis", "plasma", "magma", "inferno",
    "Blues", "Reds", "Greens", "RdBu",
]


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
