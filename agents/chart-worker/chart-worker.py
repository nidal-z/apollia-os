"""chart-worker — Génération de charts (PNG/SVG) depuis JSON.

Worker Apollia OS standalone qui décharge le LLM director de la génération
de code matplotlib à la volée. Le caller fournit ``{type, data, title, ...}``
et reçoit un path ou base64 du chart rendu.

Cinq skills A2A déterministes :
- ``chart.bar``     — bar chart (vertical/horizontal, grouped/stacked)
- ``chart.line``    — line chart (datetime/number/category, markers, line_style)
- ``chart.pie``     — pie / donut (explode, hole_size, show_percent)
- ``chart.scatter`` — scatter / bubble (+ régression linéaire optionnelle)
- ``chart.heatmap`` — heatmap (8 colormaps, show_values overlay)

Dispatch multi-skills : le SDK route ``task.skill_id`` vers la méthode
``@skill`` correspondante (ex. ``chart.bar``). Les paramètres sont
extraits depuis la signature typée.

Trois thèmes intégrés : ``default``, ``dark``, ``minimal``.
Deux modes de sortie : ``file`` (écrit sur disque) ou ``base64`` (retourne
le contenu encodé dans le JSON).
"""

from __future__ import annotations

import base64
import datetime
import io
import json
import re
from pathlib import Path
from typing import Annotated, Any

# Headless rendering — must happen BEFORE importing pyplot. matplotlib
# itself is an optional runtime dep (declared in the agent.toml packages
# block); import lazily so ``apollia inspect`` works without it.
try:
    import matplotlib

    matplotlib.use("Agg")
except ImportError:  # pragma: no cover - matplotlib only required at runtime
    matplotlib = None  # type: ignore[assignment]

from apollia import DomainError, agent, skill
from apollia.types import Ctx

# Resolve ``schemas`` from the worker dir even when (a) loaded under bare
# ``importlib`` (eval runners don't set sys.path) and (b) another agent has
# already populated ``sys.modules['schemas']`` from its own folder. We push
# our dir to the FRONT of sys.path and purge any stale ``schemas`` entry —
# same pattern as veille-ia.
import sys
from pathlib import Path
_WORKER_DIR = str(Path(__file__).resolve().parent)
if _WORKER_DIR in sys.path:
    sys.path.remove(_WORKER_DIR)
sys.path.insert(0, _WORKER_DIR)
sys.modules.pop("schemas", None)

# Canonical input TypedDicts — drive the generated JSON Schema so mid-market
# LLMs (Mistral Small, Haiku, Llama 70B) see the exact field shape and don't
# need the structure pasted into the system prompt.
from schemas import (  # type: ignore[import-not-found]  # noqa: E402
    BarSeries,
    LineSeries,
    PieSlice,
    ScatterSeries,
)
ALLOWED_FORMATS: tuple[str, ...] = ("png", "svg")
ALLOWED_THEMES: tuple[str, ...] = ("default", "dark", "minimal")
ALLOWED_COLORMAPS: tuple[str, ...] = (
    "viridis", "plasma", "magma", "inferno",
    "Blues", "Reds", "Greens", "RdBu",
)
ALLOWED_BAR_ORIENTATIONS: tuple[str, ...] = ("vertical", "horizontal")
ALLOWED_BAR_STYLES: tuple[str, ...] = ("grouped", "stacked")
ALLOWED_X_TYPES: tuple[str, ...] = ("auto", "number", "datetime", "category")
ALLOWED_LINE_STYLES: tuple[str, ...] = ("solid", "dashed", "dotted")
ALLOWED_MARKER_STYLES_SCATTER: dict[str, str] = {
    "circle": "o", "square": "s", "triangle": "^",
    "diamond": "D", "x": "x", "plus": "+",
}

MAX_DATA_POINTS: int = 1_000_000
MAX_DIMENSION_INCHES: float = 1000.0

# Default palette per theme (used when colors not provided)
PALETTE_DEFAULT = ["#1F4E78", "#70AD47", "#ED7D31", "#FFC000", "#5B9BD5", "#A5A5A5", "#264478", "#9E480E"]
PALETTE_DARK = ["#4FC3F7", "#81C784", "#FFB74D", "#FFD54F", "#BA68C8", "#E0E0E0", "#4DD0E1", "#FF8A65"]
PALETTE_MINIMAL = ["#88C0D0", "#A3BE8C", "#EBCB8B", "#D08770", "#B48EAD", "#81A1C1", "#BF616A", "#5E81AC"]


class _ChartError(DomainError):
    """Legacy alias — subclass of :class:`DomainError` for typed dispatch."""


@agent(
    name="chart-worker",
    version="0.1.0",
    description=(
        "Génération de charts (PNG/SVG) depuis JSON : bar, line, pie, "
        "scatter, heatmap. Styles avancés (3 thèmes), régression linéaire."
    ),
    tags=("chart", "dataviz", "matplotlib", "png", "svg", "worker"),
    agent_type="worker",
    step_budget={"max_steps": 1, "max_tool_calls": 5, "wall_clock_secs": 300},
)
class ChartWorker:
    """5 skills chart-generation (decorator-first)."""

    @skill(
        "chart.bar",
        description=(
            "Generate a bar chart (PNG/SVG) from data series. Each series "
            "produces one colored bar group; categories label the X axis."
        ),
        examples=[
            {
                "series": [
                    {"name": "Q1", "data": [10, 20, 30]},
                    {"name": "Q2", "data": [15, 25, 35]},
                ],
                "categories": ["Product A", "Product B", "Product C"],
                "title": "Quarterly Sales",
                "output_path": "/tmp/sales.png",
            },
        ],
    )
    async def bar(
        self,
        series: list[BarSeries],
        output_path: Annotated[
            str | None,
            "Filesystem path to write the chart. Required when output_mode='file'.",
        ] = None,
        output_mode: Annotated[
            str,
            "'file' (writes to output_path) | 'base64' (returns inline in response).",
        ] = "file",
        format: Annotated[
            str | None,
            "'png' (default) | 'svg'. Inferred from output_path extension when omitted.",
        ] = None,
        title: Annotated[str | None, "Chart title shown at the top."] = None,
        xlabel: Annotated[str | None, "X axis label."] = None,
        ylabel: Annotated[str | None, "Y axis label."] = None,
        width_inches: float = 10.0,
        height_inches: float = 6.0,
        dpi: int = 150,
        theme: Annotated[
            str,
            "Visual theme: 'default' | 'dark' | 'minimal'.",
        ] = "default",
        legend: bool = True,
        grid: bool = True,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        font_size: int = 11,
        title_font_size: int = 14,
        categories: Annotated[
            list[Any] | None,
            "X-axis category labels (one per index in series[*].data when data is list[number]).",
        ] = None,
        orientation: Annotated[
            str,
            "'vertical' (bars upward, default) | 'horizontal' (bars rightward).",
        ] = "vertical",
        bar_style: Annotated[
            str,
            "'grouped' (default, side-by-side) | 'stacked' (cumulative).",
        ] = "grouped",
        value_labels: Annotated[
            bool,
            "Display numeric value labels on top of each bar.",
        ] = False,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Bar chart."""
        return _do_bar(
            _gather_chart_payload(
                locals(),
                extra_keys=("series", "categories", "orientation", "bar_style", "value_labels"),
            )
        )

    @skill(
        "chart.line",
        description=(
            "Generate a line chart (PNG/SVG). Supports datetime, numeric, "
            "or categorical X axes, markers, dashed/dotted styles."
        ),
        examples=[
            {
                "series": [
                    {
                        "name": "Revenue",
                        "data": [
                            {"x": "2026-01-01", "y": 100},
                            {"x": "2026-02-01", "y": 130},
                            {"x": "2026-03-01", "y": 175},
                        ],
                    },
                ],
                "x_type": "datetime",
                "title": "Monthly Revenue",
                "output_path": "/tmp/revenue.png",
            },
        ],
    )
    async def line(
        self,
        series: list[LineSeries],
        output_path: Annotated[
            str | None,
            "Filesystem path to write the chart. Required when output_mode='file'.",
        ] = None,
        output_mode: Annotated[
            str,
            "'file' (writes to output_path) | 'base64' (returns inline in response).",
        ] = "file",
        format: Annotated[
            str | None,
            "'png' (default) | 'svg'. Inferred from output_path extension when omitted.",
        ] = None,
        title: Annotated[str | None, "Chart title shown at the top."] = None,
        xlabel: Annotated[str | None, "X axis label."] = None,
        ylabel: Annotated[str | None, "Y axis label."] = None,
        width_inches: float = 10.0,
        height_inches: float = 6.0,
        dpi: int = 150,
        theme: Annotated[
            str,
            "Visual theme: 'default' | 'dark' | 'minimal'.",
        ] = "default",
        legend: bool = True,
        grid: bool = True,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        font_size: int = 11,
        title_font_size: int = 14,
        x_type: Annotated[
            str,
            "X-axis interpretation: 'auto' (detect) | 'number' | 'datetime' (ISO 8601) | 'category'.",
        ] = "auto",
        y_log_scale: Annotated[bool, "Use logarithmic scale on Y axis."] = False,
        area_fill: Annotated[bool, "Fill the area below each line with translucent color."] = False,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Line chart."""
        return _do_line(
            _gather_chart_payload(
                locals(),
                extra_keys=("series", "x_type", "y_log_scale", "area_fill"),
            )
        )

    @skill(
        "chart.pie",
        description=(
            "Generate a pie or donut chart (PNG/SVG). Supports explode, "
            "percent/value labels, donut hole sizing, custom start angle."
        ),
        examples=[
            {
                "data": [
                    {"label": "Chrome", "value": 65.2},
                    {"label": "Safari", "value": 18.7},
                    {"label": "Firefox", "value": 8.5},
                    {"label": "Other", "value": 7.6},
                ],
                "title": "Browser Market Share",
                "output_path": "/tmp/browsers.png",
            },
        ],
    )
    async def pie(
        self,
        data: list[PieSlice],
        output_path: Annotated[
            str | None,
            "Filesystem path to write the chart. Required when output_mode='file'.",
        ] = None,
        output_mode: Annotated[
            str,
            "'file' (writes to output_path) | 'base64' (returns inline in response).",
        ] = "file",
        format: Annotated[
            str | None,
            "'png' (default) | 'svg'. Inferred from output_path extension when omitted.",
        ] = None,
        title: Annotated[str | None, "Chart title shown at the top."] = None,
        xlabel: Annotated[str | None, "X axis label (rarely used on pies)."] = None,
        ylabel: Annotated[str | None, "Y axis label (rarely used on pies)."] = None,
        width_inches: float = 10.0,
        height_inches: float = 6.0,
        dpi: int = 150,
        theme: Annotated[
            str,
            "Visual theme: 'default' | 'dark' | 'minimal'.",
        ] = "default",
        legend: bool = True,
        grid: bool = True,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        font_size: int = 11,
        title_font_size: int = 14,
        donut: Annotated[bool, "Render as a donut (hollow center) instead of full pie."] = False,
        hole_size: Annotated[
            float | None,
            "Donut hole radius in [0.0, 0.9]. Default 0.4 when donut=True.",
        ] = None,
        colors: Annotated[
            list[str] | None,
            "Hex color list ('#RRGGBB') — must match len(data) when provided.",
        ] = None,
        show_percent: Annotated[bool, "Annotate each slice with its percentage."] = True,
        show_values: Annotated[bool, "Annotate each slice with its raw value."] = False,
        explode: Annotated[
            list[float] | None,
            "Per-slice radial offset (0.0 flush, 0.1 typical pull-out). Length must match data.",
        ] = None,
        start_angle: Annotated[float, "Rotation of the first slice in degrees (90 = top)."] = 90.0,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Pie / donut chart."""
        return _do_pie(
            _gather_chart_payload(
                locals(),
                extra_keys=(
                    "data", "donut", "hole_size", "colors", "show_percent",
                    "show_values", "explode", "start_angle",
                ),
            )
        )

    @skill(
        "chart.scatter",
        description=(
            "Generate a scatter or bubble chart (PNG/SVG). Per-point sizing "
            "via data[i].size; optional linear regression with R² annotated."
        ),
        examples=[
            {
                "series": [
                    {
                        "name": "Sample A",
                        "data": [
                            {"x": 1.0, "y": 2.1},
                            {"x": 2.0, "y": 4.3},
                            {"x": 3.0, "y": 5.9},
                            {"x": 4.0, "y": 8.2},
                        ],
                    },
                ],
                "regression": True,
                "title": "Correlation",
                "output_path": "/tmp/scatter.png",
            },
        ],
    )
    async def scatter(
        self,
        series: list[ScatterSeries],
        output_path: Annotated[
            str | None,
            "Filesystem path to write the chart. Required when output_mode='file'.",
        ] = None,
        output_mode: Annotated[
            str,
            "'file' (writes to output_path) | 'base64' (returns inline in response).",
        ] = "file",
        format: Annotated[
            str | None,
            "'png' (default) | 'svg'. Inferred from output_path extension when omitted.",
        ] = None,
        title: Annotated[str | None, "Chart title shown at the top."] = None,
        xlabel: Annotated[str | None, "X axis label."] = None,
        ylabel: Annotated[str | None, "Y axis label."] = None,
        width_inches: float = 10.0,
        height_inches: float = 6.0,
        dpi: int = 150,
        theme: Annotated[
            str,
            "Visual theme: 'default' | 'dark' | 'minimal'.",
        ] = "default",
        legend: bool = True,
        grid: bool = True,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        font_size: int = 11,
        title_font_size: int = 14,
        x_type: Annotated[str, "X-axis: 'number' (default) | 'datetime' (ISO 8601)."] = "number",
        y_type: Annotated[str, "Y-axis: 'number' (default) | 'datetime' (ISO 8601)."] = "number",
        regression: Annotated[
            bool,
            "Overlay a least-squares linear fit and annotate R² (numeric axes only).",
        ] = False,
        marker_size: Annotated[float, "Default marker area in points² (overridden by data[i].size)."] = 50.0,
        alpha: Annotated[float, "Marker opacity in [0.0, 1.0]."] = 0.7,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Scatter / bubble chart."""
        return _do_scatter(
            _gather_chart_payload(
                locals(),
                extra_keys=("series", "x_type", "y_type", "regression", "marker_size", "alpha"),
            )
        )

    @skill(
        "chart.heatmap",
        description=(
            "Generate a heatmap (PNG/SVG) from a 2D numeric matrix. Supports "
            "8 colormaps, optional value overlay, colorbar."
        ),
        examples=[
            {
                "matrix": [
                    [1.0, 0.8, 0.3],
                    [0.8, 1.0, 0.5],
                    [0.3, 0.5, 1.0],
                ],
                "row_labels": ["A", "B", "C"],
                "col_labels": ["A", "B", "C"],
                "show_values": True,
                "title": "Correlation Matrix",
                "output_path": "/tmp/heatmap.png",
            },
        ],
    )
    async def heatmap(
        self,
        matrix: Annotated[
            list[list[float]],
            "2D matrix of numbers (rows × cols). All rows must have the same length; no NaN/Inf.",
        ],
        output_path: Annotated[
            str | None,
            "Filesystem path to write the chart. Required when output_mode='file'.",
        ] = None,
        output_mode: Annotated[
            str,
            "'file' (writes to output_path) | 'base64' (returns inline in response).",
        ] = "file",
        format: Annotated[
            str | None,
            "'png' (default) | 'svg'. Inferred from output_path extension when omitted.",
        ] = None,
        title: Annotated[str | None, "Chart title shown at the top."] = None,
        xlabel: Annotated[str | None, "X axis label."] = None,
        ylabel: Annotated[str | None, "Y axis label."] = None,
        width_inches: float = 10.0,
        height_inches: float = 6.0,
        dpi: int = 150,
        theme: Annotated[
            str,
            "Visual theme: 'default' | 'dark' | 'minimal'.",
        ] = "default",
        legend: bool = True,
        grid: bool = True,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        font_size: int = 11,
        title_font_size: int = 14,
        row_labels: Annotated[
            list[str] | None,
            "Y-axis row labels — length must equal matrix row count.",
        ] = None,
        col_labels: Annotated[
            list[str] | None,
            "X-axis column labels — length must equal matrix column count.",
        ] = None,
        colormap: Annotated[
            str,
            "Matplotlib colormap: 'viridis' | 'plasma' | 'magma' | 'inferno' | 'Blues' | 'Reds' | 'Greens' | 'RdBu'.",
        ] = "viridis",
        vmin: Annotated[float | None, "Lower bound for colormap normalization (default: matrix min)."] = None,
        vmax: Annotated[float | None, "Upper bound for colormap normalization (default: matrix max)."] = None,
        show_values: Annotated[bool, "Overlay each cell with its formatted numeric value."] = False,
        value_format: Annotated[
            str,
            "Python format spec applied to each cell value (e.g. '{:.2f}', '{:.0%}').",
        ] = "{:.2f}",
        colorbar: Annotated[bool, "Display the colorbar legend on the right."] = True,
        colorbar_label: Annotated[str | None, "Optional label rendered alongside the colorbar."] = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Heatmap."""
        return _do_heatmap(
            _gather_chart_payload(
                locals(),
                extra_keys=(
                    "matrix", "row_labels", "col_labels", "colormap", "vmin", "vmax",
                    "show_values", "value_format", "colorbar", "colorbar_label",
                ),
            )
        )


_COMMON_PAYLOAD_KEYS: tuple[str, ...] = (
    "output_path", "output_mode", "format", "title", "xlabel", "ylabel",
    "width_inches", "height_inches", "dpi", "theme", "legend", "grid",
    "overwrite", "font_size", "title_font_size",
)


def _gather_chart_payload(local_vars: dict[str, Any], extra_keys: tuple[str, ...]) -> dict[str, Any]:
    """Assemble a payload dict from a handler's ``locals()`` snapshot."""
    payload: dict[str, Any] = {}
    for k in _COMMON_PAYLOAD_KEYS:
        if k in local_vars:
            payload[k] = local_vars[k]
    for k in extra_keys:
        if k in local_vars:
            payload[k] = local_vars[k]
    return payload


# ─── Operation: bar ────────────────────────────────────────────────────────


def _do_bar(payload: dict[str, Any]) -> dict[str, Any]:
    common = _parse_common(payload)
    series_spec = _require_list(payload, "series")
    if not series_spec:
        raise _ChartError("MISSING_FIELD", "series ne peut pas être vide", details={"field": "series"})

    orientation = payload.get("orientation", "vertical")
    if orientation not in ALLOWED_BAR_ORIENTATIONS:
        raise _ChartError(
            "INVALID_TYPE",
            f"orientation invalide : {orientation!r}. Valeurs : {list(ALLOWED_BAR_ORIENTATIONS)}",
            details={"field": "orientation"},
        )
    bar_style = payload.get("bar_style", "grouped")
    if bar_style not in ALLOWED_BAR_STYLES:
        raise _ChartError(
            "INVALID_TYPE",
            f"bar_style invalide : {bar_style!r}. Valeurs : {list(ALLOWED_BAR_STYLES)}",
            details={"field": "bar_style"},
        )
    value_labels = bool(payload.get("value_labels", False))
    categories = payload.get("categories")

    # Normalize series : derive x-values and y-values per series
    palette = _palette(common["theme"])
    normalized: list[dict[str, Any]] = []
    total_points = 0
    n_cats: int | None = None
    cat_labels: list[str] | None = None

    for s_idx, spec in enumerate(series_spec):
        if not isinstance(spec, dict):
            raise _ChartError("INVALID_DATA", f"series[{s_idx}] doit être un objet", details={"index": s_idx})
        name = str(spec.get("name", f"Series {s_idx + 1}"))
        data = spec.get("data")
        if not isinstance(data, list) or not data:
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data doit être une liste non vide", details={"index": s_idx})

        if all(isinstance(v, (int, float)) for v in data):
            # data is [number, number, ...]
            xs = list(range(len(data)))
            ys = [float(v) for v in data]
            if cat_labels is None:
                cat_labels = (
                    [str(c) for c in categories]
                    if categories
                    else [f"Cat {i + 1}" for i in range(len(data))]
                )
                n_cats = len(cat_labels)
            if len(ys) != n_cats:
                raise _ChartError(
                    "INVALID_DATA",
                    f"series[{s_idx}] a {len(ys)} valeurs mais {n_cats} catégories attendues",
                    details={"index": s_idx, "expected": n_cats, "got": len(ys)},
                )
        elif all(isinstance(v, dict) for v in data):
            xs_labels: list[str] = []
            ys = []
            for d in data:
                if "x" not in d or "y" not in d:
                    raise _ChartError("INVALID_DATA", f"series[{s_idx}].data[i] doit contenir 'x' et 'y'", details={"index": s_idx})
                xs_labels.append(str(d["x"]))
                ys.append(_to_float(d["y"], f"series[{s_idx}].data[i].y"))
            if cat_labels is None:
                cat_labels = xs_labels
                n_cats = len(cat_labels)
            elif xs_labels != cat_labels:
                # Different categories per series : enforce same set
                raise _ChartError(
                    "INVALID_DATA",
                    f"series[{s_idx}] a des x différents des autres séries",
                    details={"index": s_idx, "expected": cat_labels, "got": xs_labels},
                )
        else:
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data doit être [number] ou [{{x,y}}]", details={"index": s_idx})

        color = spec.get("color")
        if color:
            color = _validate_hex_color_str(color, where=f"series[{s_idx}].color")
        else:
            color = palette[s_idx % len(palette)]

        if any(_is_nan_or_inf(v) for v in ys):
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data contient NaN ou Inf", details={"index": s_idx})

        normalized.append({"name": name, "values": ys, "color": color})
        total_points += len(ys)

    _check_data_points_cap(total_points)

    import numpy as np

    fig, ax = _new_figure(common)
    n_series = len(normalized)
    indices = np.arange(n_cats or 0)
    bar_width = 0.8 / n_series if bar_style == "grouped" else 0.8

    if bar_style == "grouped":
        for s_idx, s in enumerate(normalized):
            offsets = indices + (s_idx - (n_series - 1) / 2) * bar_width
            if orientation == "vertical":
                bars = ax.bar(offsets, s["values"], bar_width, label=s["name"], color=s["color"])
            else:
                bars = ax.barh(offsets, s["values"], bar_width, label=s["name"], color=s["color"])
            if value_labels:
                _annotate_bars(ax, bars, orientation)
    else:  # stacked
        bottoms = [0.0] * (n_cats or 0)
        for s in normalized:
            if orientation == "vertical":
                bars = ax.bar(indices, s["values"], bar_width, bottom=bottoms, label=s["name"], color=s["color"])
            else:
                bars = ax.barh(indices, s["values"], bar_width, left=bottoms, label=s["name"], color=s["color"])
            if value_labels:
                _annotate_bars(ax, bars, orientation)
            bottoms = [b + v for b, v in zip(bottoms, s["values"])]

    if orientation == "vertical":
        ax.set_xticks(indices)
        ax.set_xticklabels(cat_labels or [])
    else:
        ax.set_yticks(indices)
        ax.set_yticklabels(cat_labels or [])

    _apply_common_decorations(fig, ax, common)
    return _save_chart(fig, common, total_points)


def _annotate_bars(ax: Any, bars: Any, orientation: str) -> None:
    for bar in bars:
        if orientation == "vertical":
            h = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2.0,
                h,
                f"{h:g}",
                ha="center", va="bottom", fontsize=9,
            )
        else:
            w = bar.get_width()
            ax.text(
                w,
                bar.get_y() + bar.get_height() / 2.0,
                f" {w:g}",
                ha="left", va="center", fontsize=9,
            )


# ─── Operation: line ───────────────────────────────────────────────────────


def _do_line(payload: dict[str, Any]) -> dict[str, Any]:
    common = _parse_common(payload)
    series_spec = _require_list(payload, "series")
    if not series_spec:
        raise _ChartError("MISSING_FIELD", "series ne peut pas être vide", details={"field": "series"})

    x_type = payload.get("x_type", "auto")
    if x_type not in ALLOWED_X_TYPES:
        raise _ChartError("INVALID_TYPE", f"x_type invalide : {x_type!r}", details={"field": "x_type"})

    y_log_scale = bool(payload.get("y_log_scale", False))
    area_fill = bool(payload.get("area_fill", False))

    palette = _palette(common["theme"])
    total_points = 0
    parsed_series: list[dict[str, Any]] = []

    for s_idx, spec in enumerate(series_spec):
        if not isinstance(spec, dict):
            raise _ChartError("INVALID_DATA", f"series[{s_idx}] doit être un objet", details={"index": s_idx})
        name = str(spec.get("name", f"Series {s_idx + 1}"))
        data = spec.get("data")
        if not isinstance(data, list) or not data:
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data doit être une liste non vide", details={"index": s_idx})

        xs_raw, ys = [], []
        for d in data:
            if not isinstance(d, dict) or "x" not in d or "y" not in d:
                raise _ChartError("INVALID_DATA", f"series[{s_idx}].data[i] doit être {{x, y}}", details={"index": s_idx})
            xs_raw.append(d["x"])
            ys.append(_to_float(d["y"], f"series[{s_idx}].data[i].y"))

        # Determine x_type if auto
        effective_x_type = x_type
        if effective_x_type == "auto":
            if all(isinstance(v, (int, float)) for v in xs_raw):
                effective_x_type = "number"
            elif all(_looks_like_datetime(v) for v in xs_raw):
                effective_x_type = "datetime"
            else:
                effective_x_type = "category"

        if effective_x_type == "number":
            xs = [float(v) for v in xs_raw]
        elif effective_x_type == "datetime":
            xs = [_parse_datetime(v, f"series[{s_idx}].data[i].x") for v in xs_raw]
        else:  # category
            xs = [str(v) for v in xs_raw]

        color = spec.get("color")
        if color:
            color = _validate_hex_color_str(color, where=f"series[{s_idx}].color")
        else:
            color = palette[s_idx % len(palette)]

        line_style = spec.get("line_style", "solid")
        if line_style not in ALLOWED_LINE_STYLES:
            raise _ChartError("INVALID_TYPE", f"line_style invalide : {line_style!r}", details={"field": "line_style"})
        ls_map = {"solid": "-", "dashed": "--", "dotted": ":"}
        line_width = float(spec.get("line_width", 1.8))
        marker_raw = spec.get("marker", "none")
        marker = None if marker_raw == "none" else marker_raw

        if any(_is_nan_or_inf(v) for v in ys):
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data contient NaN/Inf en y", details={"index": s_idx})

        parsed_series.append({
            "name": name, "xs": xs, "ys": ys, "color": color,
            "linestyle": ls_map[line_style], "linewidth": line_width, "marker": marker,
            "x_type": effective_x_type,
        })
        total_points += len(ys)

    _check_data_points_cap(total_points)

    fig, ax = _new_figure(common)

    for s in parsed_series:
        ax.plot(
            s["xs"], s["ys"],
            label=s["name"], color=s["color"],
            linestyle=s["linestyle"], linewidth=s["linewidth"],
            marker=s["marker"], markersize=6 if s["marker"] else 0,
        )
        if area_fill:
            ax.fill_between(s["xs"], s["ys"], alpha=0.2, color=s["color"])

    if y_log_scale:
        ax.set_yscale("log")

    # Datetime axis formatting
    if any(s["x_type"] == "datetime" for s in parsed_series):
        import matplotlib.dates as mdates
        ax.xaxis.set_major_formatter(mdates.DateFormatter("%Y-%m-%d"))
        fig.autofmt_xdate()

    _apply_common_decorations(fig, ax, common)
    return _save_chart(fig, common, total_points)


# ─── Operation: pie ────────────────────────────────────────────────────────


def _do_pie(payload: dict[str, Any]) -> dict[str, Any]:
    common = _parse_common(payload)
    data = _require_list(payload, "data")
    if not data:
        raise _ChartError("MISSING_FIELD", "data ne peut pas être vide", details={"field": "data"})

    labels: list[str] = []
    values: list[float] = []
    for i, d in enumerate(data):
        if not isinstance(d, dict) or "label" not in d or "value" not in d:
            raise _ChartError("INVALID_DATA", f"data[{i}] doit être {{label, value}}", details={"index": i})
        v = _to_float(d["value"], f"data[{i}].value")
        if v < 0:
            raise _ChartError(
                "INVALID_DATA",
                f"data[{i}].value < 0 ({v}) — pie ne supporte pas les valeurs négatives",
                details={"index": i, "value": v},
            )
        labels.append(str(d["label"]))
        values.append(v)

    if sum(values) <= 0:
        raise _ChartError("INVALID_DATA", "Somme des values doit être > 0", details={"sum": sum(values)})

    _check_data_points_cap(len(values))

    donut = bool(payload.get("donut", False))
    hole_size = float(payload.get("hole_size", 0.4 if donut else 0.0))
    if not 0.0 <= hole_size <= 0.9:
        raise _ChartError("INVALID_DATA", "hole_size doit être entre 0 et 0.9", details={"field": "hole_size"})

    colors_spec = payload.get("colors")
    if colors_spec:
        if not isinstance(colors_spec, list) or len(colors_spec) != len(values):
            raise _ChartError(
                "INVALID_DATA",
                "colors doit avoir la même longueur que data",
                details={"expected": len(values), "got": len(colors_spec) if isinstance(colors_spec, list) else None},
            )
        colors = [_validate_hex_color_str(c, where="colors[i]") for c in colors_spec]
    else:
        palette = _palette(common["theme"])
        colors = [palette[i % len(palette)] for i in range(len(values))]

    explode = payload.get("explode")
    if explode is not None:
        if not isinstance(explode, list) or len(explode) != len(values):
            raise _ChartError("INVALID_DATA", "explode doit avoir la même longueur que data", details={"field": "explode"})
        explode = [float(e) for e in explode]

    start_angle = float(payload.get("start_angle", 90))
    show_percent = bool(payload.get("show_percent", True))
    show_values = bool(payload.get("show_values", False))

    def _autopct(pct: float) -> str:
        if show_percent and show_values:
            absolute = pct * sum(values) / 100.0
            return f"{pct:.1f}%\n({absolute:g})"
        if show_percent:
            return f"{pct:.1f}%"
        if show_values:
            return f"{pct * sum(values) / 100.0:g}"
        return ""

    fig, ax = _new_figure(common)
    wedge_kwargs: dict[str, Any] = {"width": (1 - hole_size) if donut else None} if donut else {}

    ax.pie(
        values,
        labels=labels,
        colors=colors,
        explode=explode,
        startangle=start_angle,
        autopct=_autopct if (show_percent or show_values) else None,
        wedgeprops=wedge_kwargs,
    )
    ax.set_aspect("equal")

    _apply_common_decorations(fig, ax, common, suppress_axes=True)
    return _save_chart(fig, common, len(values))


# ─── Operation: scatter ────────────────────────────────────────────────────


def _do_scatter(payload: dict[str, Any]) -> dict[str, Any]:
    common = _parse_common(payload)
    series_spec = _require_list(payload, "series")
    if not series_spec:
        raise _ChartError("MISSING_FIELD", "series ne peut pas être vide", details={"field": "series"})

    x_type = payload.get("x_type", "number")
    y_type = payload.get("y_type", "number")
    for k, v in (("x_type", x_type), ("y_type", y_type)):
        if v not in ("number", "datetime"):
            raise _ChartError("INVALID_TYPE", f"{k} invalide : {v!r}. Valeurs : ['number', 'datetime']", details={"field": k})

    regression = bool(payload.get("regression", False))
    default_marker_size = float(payload.get("marker_size", 50))
    alpha = float(payload.get("alpha", 0.7))
    if not 0.0 <= alpha <= 1.0:
        raise _ChartError("INVALID_DATA", "alpha doit être entre 0 et 1", details={"field": "alpha"})

    palette = _palette(common["theme"])
    parsed: list[dict[str, Any]] = []
    total_points = 0

    for s_idx, spec in enumerate(series_spec):
        if not isinstance(spec, dict):
            raise _ChartError("INVALID_DATA", f"series[{s_idx}] doit être un objet", details={"index": s_idx})
        name = str(spec.get("name", f"Series {s_idx + 1}"))
        data = spec.get("data")
        if not isinstance(data, list) or not data:
            raise _ChartError("INVALID_DATA", f"series[{s_idx}].data doit être une liste non vide", details={"index": s_idx})

        xs_raw, ys_raw, sizes = [], [], []
        for d in data:
            if not isinstance(d, dict) or "x" not in d or "y" not in d:
                raise _ChartError("INVALID_DATA", f"series[{s_idx}].data[i] doit être {{x, y}}", details={"index": s_idx})
            xs_raw.append(d["x"])
            ys_raw.append(d["y"])
            sizes.append(float(d.get("size", default_marker_size)))

        xs = (
            [_parse_datetime(v, f"series[{s_idx}].data[i].x") for v in xs_raw]
            if x_type == "datetime"
            else [_to_float(v, f"series[{s_idx}].data[i].x") for v in xs_raw]
        )
        ys = (
            [_parse_datetime(v, f"series[{s_idx}].data[i].y") for v in ys_raw]
            if y_type == "datetime"
            else [_to_float(v, f"series[{s_idx}].data[i].y") for v in ys_raw]
        )

        color = spec.get("color")
        if color:
            color = _validate_hex_color_str(color, where=f"series[{s_idx}].color")
        else:
            color = palette[s_idx % len(palette)]

        marker_style = spec.get("marker_style", "circle")
        if marker_style not in ALLOWED_MARKER_STYLES_SCATTER:
            raise _ChartError(
                "INVALID_TYPE",
                f"marker_style invalide : {marker_style!r}. Valeurs : {list(ALLOWED_MARKER_STYLES_SCATTER)}",
                details={"field": "marker_style"},
            )

        parsed.append({
            "name": name, "xs": xs, "ys": ys, "sizes": sizes,
            "color": color, "marker": ALLOWED_MARKER_STYLES_SCATTER[marker_style],
        })
        total_points += len(ys)

    _check_data_points_cap(total_points)

    import numpy as np

    fig, ax = _new_figure(common)

    for s in parsed:
        ax.scatter(
            s["xs"], s["ys"],
            s=s["sizes"], c=s["color"], marker=s["marker"],
            alpha=alpha, label=s["name"], edgecolors="none",
        )

        if regression and x_type == "number" and y_type == "number" and len(s["xs"]) >= 2:
            xs_arr = np.array(s["xs"], dtype=float)
            ys_arr = np.array(s["ys"], dtype=float)
            slope, intercept = np.polyfit(xs_arr, ys_arr, 1)
            xs_line = np.linspace(xs_arr.min(), xs_arr.max(), 50)
            ys_line = slope * xs_line + intercept
            # R² calculation
            ss_res = float(np.sum((ys_arr - (slope * xs_arr + intercept)) ** 2))
            ss_tot = float(np.sum((ys_arr - ys_arr.mean()) ** 2))
            r2 = 1.0 - ss_res / ss_tot if ss_tot > 0 else 0.0
            ax.plot(
                xs_line, ys_line,
                color=s["color"], linestyle="--", linewidth=1.2,
                label=f"{s['name']} (R²={r2:.3f})",
            )

    if x_type == "datetime" or y_type == "datetime":
        import matplotlib.dates as mdates
        if x_type == "datetime":
            ax.xaxis.set_major_formatter(mdates.DateFormatter("%Y-%m-%d"))
        if y_type == "datetime":
            ax.yaxis.set_major_formatter(mdates.DateFormatter("%Y-%m-%d"))
        fig.autofmt_xdate()

    _apply_common_decorations(fig, ax, common)
    return _save_chart(fig, common, total_points)


# ─── Operation: heatmap ────────────────────────────────────────────────────


def _do_heatmap(payload: dict[str, Any]) -> dict[str, Any]:
    common = _parse_common(payload)
    matrix = payload.get("matrix")
    if not isinstance(matrix, list) or not matrix:
        raise _ChartError("MISSING_FIELD", "matrix requise (liste non vide)", details={"field": "matrix"})
    if not all(isinstance(row, list) and row for row in matrix):
        raise _ChartError("INVALID_DATA", "matrix doit être une liste de listes non vides", details={"field": "matrix"})

    ncols = len(matrix[0])
    if not all(len(row) == ncols for row in matrix):
        raise _ChartError("INVALID_DATA", "Toutes les lignes de matrix doivent avoir la même longueur", details={"field": "matrix"})

    # Validate numbers + NaN/Inf
    flat: list[float] = []
    for r_idx, row in enumerate(matrix):
        for c_idx, v in enumerate(row):
            f = _to_float(v, f"matrix[{r_idx}][{c_idx}]")
            if _is_nan_or_inf(f):
                raise _ChartError("INVALID_DATA", f"matrix[{r_idx}][{c_idx}] est NaN ou Inf", details={"row": r_idx, "col": c_idx})
            flat.append(f)

    _check_data_points_cap(len(flat))

    colormap = payload.get("colormap", "viridis")
    if colormap not in ALLOWED_COLORMAPS:
        raise _ChartError(
            "INVALID_COLORMAP",
            f"colormap non whitelistée : {colormap!r}. Valeurs : {list(ALLOWED_COLORMAPS)}",
            details={"field": "colormap"},
        )

    row_labels = payload.get("row_labels")
    col_labels = payload.get("col_labels")
    if row_labels is not None and len(row_labels) != len(matrix):
        raise _ChartError("INVALID_DATA", f"row_labels doit avoir {len(matrix)} éléments", details={"expected": len(matrix), "got": len(row_labels)})
    if col_labels is not None and len(col_labels) != ncols:
        raise _ChartError("INVALID_DATA", f"col_labels doit avoir {ncols} éléments", details={"expected": ncols, "got": len(col_labels)})

    vmin = payload.get("vmin")
    vmax = payload.get("vmax")
    show_values = bool(payload.get("show_values", False))
    value_format = payload.get("value_format", "{:.2f}")
    colorbar = bool(payload.get("colorbar", True))
    colorbar_label = payload.get("colorbar_label")

    import numpy as np

    arr = np.array(matrix, dtype=float)

    fig, ax = _new_figure(common)
    im = ax.imshow(
        arr,
        cmap=colormap,
        vmin=vmin if vmin is not None else None,
        vmax=vmax if vmax is not None else None,
        aspect="auto",
    )

    if col_labels:
        ax.set_xticks(range(ncols))
        ax.set_xticklabels(col_labels, rotation=45, ha="right")
    if row_labels:
        ax.set_yticks(range(len(matrix)))
        ax.set_yticklabels(row_labels)

    if show_values:
        for r in range(len(matrix)):
            for c in range(ncols):
                val = arr[r, c]
                try:
                    text = value_format.format(val)
                except Exception as exc:
                    raise _ChartError("INVALID_TYPE", f"value_format invalide : {exc}", details={"value_format": value_format}) from exc
                # Auto contrast : white text on dark cells
                norm_val = (val - arr.min()) / (arr.max() - arr.min()) if arr.max() != arr.min() else 0.5
                text_color = "white" if norm_val < 0.5 else "black"
                ax.text(c, r, text, ha="center", va="center", color=text_color, fontsize=9)

    if colorbar:
        cbar = fig.colorbar(im, ax=ax)
        if colorbar_label:
            cbar.set_label(str(colorbar_label))

    _apply_common_decorations(fig, ax, common, suppress_legend=True, suppress_grid=True)
    return _save_chart(fig, common, len(flat))


# ─── Common helpers ────────────────────────────────────────────────────────


def _parse_common(payload: dict[str, Any]) -> dict[str, Any]:
    """Parse and validate common cross-skill fields."""
    output_mode = payload.get("output_mode", "file")
    if output_mode not in ("file", "base64"):
        raise _ChartError(
            "INVALID_TYPE",
            f"output_mode invalide : {output_mode!r}. Valeurs : ['file', 'base64']",
            details={"field": "output_mode"},
        )

    output_path = payload.get("output_path")
    format_explicit = payload.get("format")

    if output_mode == "file":
        if not isinstance(output_path, str) or not output_path:
            raise _ChartError(
                "MISSING_FIELD",
                "output_path requis en mode 'file'",
                details={"field": "output_path"},
            )
        fmt = _resolve_format(output_path, format_explicit)
        _validate_chart_extension(output_path)
    else:
        # base64 mode — output_path silently ignored
        fmt = format_explicit if format_explicit else "png"
        if fmt not in ALLOWED_FORMATS:
            raise _ChartError(
                "INVALID_FORMAT",
                f"format invalide : {fmt!r}. Valeurs : {list(ALLOWED_FORMATS)}",
                details={"field": "format"},
            )

    theme = payload.get("theme", "default")
    if theme not in ALLOWED_THEMES:
        raise _ChartError(
            "INVALID_STYLE",
            f"theme invalide : {theme!r}. Valeurs : {list(ALLOWED_THEMES)}",
            details={"field": "theme"},
        )

    width = float(payload.get("width_inches", 10.0))
    height = float(payload.get("height_inches", 6.0))
    if not 1.0 <= width <= MAX_DIMENSION_INCHES or not 1.0 <= height <= MAX_DIMENSION_INCHES:
        raise _ChartError(
            "INVALID_DATA",
            f"width/height doivent être dans [1, {MAX_DIMENSION_INCHES}]",
            details={"width": width, "height": height},
        )

    dpi = int(payload.get("dpi", 150))
    if not 50 <= dpi <= 600:
        raise _ChartError("INVALID_DATA", "dpi doit être dans [50, 600]", details={"dpi": dpi})

    overwrite = bool(payload.get("overwrite", False))
    if output_mode == "file":
        out = Path(output_path).expanduser().resolve()
        if out.exists() and not overwrite:
            raise _ChartError(
                "OUTPUT_EXISTS",
                f"Fichier de sortie existe déjà (overwrite=false) : {out}",
                details={"output_path": str(out)},
            )
        out.parent.mkdir(parents=True, exist_ok=True)
        resolved_output = str(out)
    else:
        resolved_output = None

    return {
        "output_mode": output_mode,
        "output_path": resolved_output,
        "format": fmt,
        "theme": theme,
        "title": payload.get("title"),
        "xlabel": payload.get("xlabel"),
        "ylabel": payload.get("ylabel"),
        "width_inches": width,
        "height_inches": height,
        "dpi": dpi,
        "legend": bool(payload.get("legend", True)),
        "grid": bool(payload.get("grid", True)),
        "font_size": int(payload.get("font_size", 11)),
        "title_font_size": int(payload.get("title_font_size", 14)),
    }


def _new_figure(common: dict[str, Any]) -> tuple[Any, Any]:
    import matplotlib.pyplot as plt

    plt.rcdefaults()
    plt.rcParams["font.size"] = common["font_size"]

    theme = common["theme"]
    if theme == "dark":
        plt.style.use("dark_background")
        plt.rcParams["figure.facecolor"] = "#0F1419"
        plt.rcParams["axes.facecolor"] = "#0F1419"
    elif theme == "minimal":
        plt.rcParams["axes.spines.top"] = False
        plt.rcParams["axes.spines.right"] = False
        plt.rcParams["grid.color"] = "#E0E0E0"
        plt.rcParams["grid.linewidth"] = 0.5

    fig, ax = plt.subplots(figsize=(common["width_inches"], common["height_inches"]))
    return fig, ax


def _apply_common_decorations(
    fig: Any, ax: Any, common: dict[str, Any],
    suppress_axes: bool = False,
    suppress_legend: bool = False,
    suppress_grid: bool = False,
) -> None:
    if common.get("title"):
        ax.set_title(common["title"], fontsize=common["title_font_size"])
    if not suppress_axes and common.get("xlabel"):
        ax.set_xlabel(common["xlabel"])
    if not suppress_axes and common.get("ylabel"):
        ax.set_ylabel(common["ylabel"])
    if common.get("legend") and not suppress_legend:
        handles, labels = ax.get_legend_handles_labels()
        if handles:
            ax.legend()
    if common.get("grid") and not suppress_grid:
        ax.grid(True, alpha=0.3)

    fig.tight_layout()


def _save_chart(fig: Any, common: dict[str, Any], data_points: int) -> dict[str, Any]:
    import matplotlib.pyplot as plt

    fmt = common["format"]
    save_kwargs: dict[str, Any] = {"format": fmt, "bbox_inches": "tight"}
    if fmt == "png":
        save_kwargs["dpi"] = common["dpi"]

    if common["output_mode"] == "file":
        fig.savefig(common["output_path"], **save_kwargs)
        plt.close(fig)
        size_bytes = Path(common["output_path"]).stat().st_size
        return {
            "output_path": common["output_path"],
            "format": fmt,
            "width_inches": common["width_inches"],
            "height_inches": common["height_inches"],
            "dpi": common["dpi"],
            "data_points_count": data_points,
            "file_size_bytes": size_bytes,
        }
    else:
        buf = io.BytesIO()
        fig.savefig(buf, **save_kwargs)
        plt.close(fig)
        content = buf.getvalue()
        return {
            "content_base64": base64.b64encode(content).decode("ascii"),
            "format": fmt,
            "width_inches": common["width_inches"],
            "height_inches": common["height_inches"],
            "dpi": common["dpi"],
            "data_points_count": data_points,
        }


def _palette(theme: str) -> list[str]:
    if theme == "dark":
        return PALETTE_DARK
    if theme == "minimal":
        return PALETTE_MINIMAL
    return PALETTE_DEFAULT


# ─── Validation primitives ─────────────────────────────────────────────────


def _require_list(payload: dict[str, Any], field: str) -> list[Any]:
    value = payload.get(field)
    if not isinstance(value, list):
        raise _ChartError(
            "MISSING_FIELD",
            f"Champ '{field}' requis (liste)",
            details={"field": field},
        )
    return value


def _to_float(v: Any, where: str) -> float:
    if isinstance(v, bool):
        raise _ChartError("INVALID_DATA", f"{where} doit être number, reçu bool", details={"where": where})
    if isinstance(v, (int, float)):
        return float(v)
    raise _ChartError("INVALID_DATA", f"{where} doit être number, reçu {type(v).__name__}", details={"where": where})


def _is_nan_or_inf(v: float) -> bool:
    return v != v or v == float("inf") or v == -float("inf")


def _validate_chart_extension(path: str) -> None:
    p = path.lower()
    if not any(p.endswith(f".{ext}") for ext in ALLOWED_FORMATS):
        raise _ChartError(
            "INVALID_FORMAT",
            f"Extension non supportée : {path!r}. Valeurs : {[f'.{e}' for e in ALLOWED_FORMATS]}",
            details={"path": path},
        )


def _resolve_format(output_path: str, explicit: Any) -> str:
    if explicit:
        if explicit not in ALLOWED_FORMATS:
            raise _ChartError(
                "INVALID_FORMAT",
                f"format invalide : {explicit!r}. Valeurs : {list(ALLOWED_FORMATS)}",
                details={"field": "format"},
            )
        return explicit
    p = output_path.lower()
    for ext in ALLOWED_FORMATS:
        if p.endswith(f".{ext}"):
            return ext
    raise _ChartError(
        "INVALID_FORMAT",
        f"Impossible d'inférer format depuis output_path : {output_path!r}",
        details={"path": output_path},
    )


def _validate_hex_color_str(color: Any, where: str) -> str:
    if not isinstance(color, str):
        raise _ChartError(
            "INVALID_STYLE",
            f"{where} doit être hex string, reçu : {type(color).__name__}",
            details={"where": where},
        )
    c = color.lstrip("#").upper()
    if len(c) == 6 and all(ch in "0123456789ABCDEF" for ch in c):
        return "#" + c
    if len(c) == 8 and all(ch in "0123456789ABCDEF" for ch in c):
        # Drop alpha for matplotlib (it accepts #RRGGBB or rgba tuple — keep simple)
        return "#" + c[2:]
    raise _ChartError(
        "INVALID_STYLE",
        f"{where} : hex invalide {color!r} (attendu RRGGBB ou AARRGGBB)",
        details={"where": where, "got": color},
    )


_DATETIME_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?$"
)


def _looks_like_datetime(v: Any) -> bool:
    return isinstance(v, str) and bool(_DATETIME_RE.match(v))


def _parse_datetime(v: Any, where: str) -> datetime.datetime:
    if isinstance(v, datetime.datetime):
        return v
    if isinstance(v, datetime.date):
        return datetime.datetime(v.year, v.month, v.day)
    if not isinstance(v, str):
        raise _ChartError(
            "UNSUPPORTED_X_TYPE",
            f"{where} : datetime attendu, reçu {type(v).__name__}",
            details={"where": where},
        )
    candidate = v.replace("Z", "+00:00")
    try:
        return datetime.datetime.fromisoformat(candidate)
    except ValueError as exc:
        raise _ChartError(
            "UNSUPPORTED_X_TYPE",
            f"{where} : datetime ISO 8601 invalide : {v!r}",
            details={"where": where, "got": v},
        ) from exc


def _check_data_points_cap(total: int) -> None:
    if total > MAX_DATA_POINTS:
        raise _ChartError(
            "TOO_MANY_POINTS",
            f"Total data points trop élevé : {total} > {MAX_DATA_POINTS}",
            details={"got": total, "limit": MAX_DATA_POINTS},
        )


# (Module-level ``agent`` singleton exposé automatiquement par ``@agent``.)
