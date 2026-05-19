"""pdf-worker — Manipulation de fichiers PDF.

Worker Apollia OS standalone qui couvre 3 axes :
- **Extraction** : texte (pypdf), tables (pdfplumber), forms AcroForm (pypdf)
- **Génération** : Markdown → PDF (markdown + reportlab)
- **Manipulation** : merge, split par plages

Six skills A2A déterministes :
- ``pdf.read-text``             — texte par page + metadata
- ``pdf.read-tables``           — tables via pdfplumber
- ``pdf.read-forms``            — champs AcroForm
- ``pdf.render-from-markdown``  — Markdown → PDF
- ``pdf.merge``                 — concaténer N PDFs
- ``pdf.split``                 — découper par plages ou page-par-page

Dispatch multi-skills : le runtime Apollia (≥ 2026-05-19) propage le
``skill_id`` invoqué dans ``AIPTask``. Le worker lit ``task.skill_id`` via
``apollia.utils.a2a.extract_skill_id(task)`` et dispatche sur le full
identifier (ex. ``pdf.read-text``).

Page range syntax : ``"1-5,7,10-12"`` — 1-based (convention métier),
converti en 0-based en interne pour pypdf/pdfplumber.
"""

from __future__ import annotations

import datetime
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.a2a import extract_a2a_payload, extract_skill_id


ALLOWED_SKILL_IDS: tuple[str, ...] = (
    "pdf.read-text",
    "pdf.read-tables",
    "pdf.read-forms",
    "pdf.render-from-markdown",
    "pdf.merge",
    "pdf.split",
)

MAX_FILE_BYTES: int = 100 * 1024 * 1024
ALLOWED_PAGE_SIZES: tuple[str, ...] = ("A4", "Letter", "Legal", "A3", "A5")
ALLOWED_ORIENTATIONS: tuple[str, ...] = ("portrait", "landscape")


class _PdfError(Exception):
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


class PdfWorker(WorkerAgent):
    """Deterministic worker exposing 6 PDF-manipulation skills."""

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "pdf-worker",
            "version": "0.1.0",
            "description": (
                "Manipulation de fichiers PDF : extraction (texte/tables/forms), "
                "génération (Markdown → PDF), merge et split par plages — préserve "
                "metadata, sous-ensemble Markdown riche."
            ),
            "execution_mode": "direct",
            "agent_type": "user",
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "tools_required": [],
            "dangerous_tools_allowed": False,
            "packages": ["pypdf==5.1.0", "pdfplumber==0.11.4", "reportlab==4.2.5", "markdown==3.7"],
            "tags": ["pdf", "pypdf", "pdfplumber", "reportlab", "markdown", "worker"],
            "step_budget": {
                "max_steps": 1,
                "max_tool_calls": 5,
                "wall_clock_secs": 300,
            },
            "skills": _build_skill_manifests(),
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        skill_id = extract_skill_id(task)
        if skill_id is None:
            return AIPResult.failed(
                "MISSING_SKILL_ID",
                "pdf-worker : aucun skill_id propagé par le runtime (worker multi-skills)",
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
                "pdf-worker : payload A2A vide ou non parseable",
            )

        try:
            if skill_id == "pdf.read-text":
                result = _do_read_text(payload)
            elif skill_id == "pdf.read-tables":
                result = _do_read_tables(payload)
            elif skill_id == "pdf.read-forms":
                result = _do_read_forms(payload)
            elif skill_id == "pdf.render-from-markdown":
                result = _do_render_from_markdown(payload)
            elif skill_id == "pdf.merge":
                result = _do_merge(payload)
            else:  # "pdf.split"
                result = _do_split(payload)
        except FileNotFoundError as exc:
            return AIPResult.failed("FILE_NOT_FOUND", str(exc))
        except _PdfError as exc:
            return AIPResult.failed(exc.code, exc.message, details=exc.details)
        except Exception as exc:  # last-resort safety net
            ctx.log("error", f"pdf-worker {skill_id} failed: {exc}")
            return AIPResult.failed("EXECUTION_FAILED", f"Erreur inattendue : {exc}")

        return AIPResult.completed(
            json.dumps(result, ensure_ascii=False, default=_json_default)
        )


# ─── Manifest skills builder ───────────────────────────────────────────────


def _build_skill_manifests() -> list[dict[str, Any]]:
    return [
        {
            "id": "pdf.read-text",
            "name": "Read text from PDF",
            "description": (
                "Extrait le texte d'un PDF page par page avec metadata. Page range "
                "1-based (ex: '1-5,7,10-12'). Cap 100k chars/page."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "path": {"type": "string", "description": "Chemin absolu vers le .pdf.", "required": True},
                "page_range": {"type": "string", "description": "1-based, ex: '1-5,7,10-12'. Toutes si absent.", "required": False},
                "max_chars_per_page": {"type": "integer", "description": "Défaut 100 000.", "required": False},
                "include_metadata": {"type": "boolean", "description": "Défaut true.", "required": False},
            },
        },
        {
            "id": "pdf.read-tables",
            "name": "Read tables from PDF",
            "description": (
                "Extrait les tables d'un PDF via pdfplumber. table_settings "
                "configurable (vertical/horizontal strategy)."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "path": {"type": "string", "description": "Chemin absolu vers le .pdf.", "required": True},
                "page_range": {"type": "string", "description": "1-based.", "required": False},
                "table_settings": {"type": "object", "description": "Forwardé à pdfplumber (vertical_strategy, etc.).", "required": False},
                "include_headers": {"type": "boolean", "description": "Défaut true — 1ère ligne = headers.", "required": False},
            },
        },
        {
            "id": "pdf.read-forms",
            "name": "Read AcroForm fields from PDF",
            "description": (
                "Extrait les champs AcroForm (text, checkbox, radio, dropdown, "
                "listbox, signature). Refuse XFA forms Adobe LiveCycle."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "path": {"type": "string", "description": "Chemin absolu vers le .pdf.", "required": True},
            },
        },
        {
            "id": "pdf.render-from-markdown",
            "name": "Render PDF from Markdown",
            "description": (
                "Génère un PDF depuis Markdown (markdown → HTML → reportlab "
                "Flowables) avec page_size, orientation, margins, metadata."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "output_path": {"type": "string", "description": "Chemin de sortie du .pdf.", "required": True},
                "markdown": {"type": "string", "description": "Texte Markdown brut. Un seul de {markdown, markdown_path}.", "required": False},
                "markdown_path": {"type": "string", "description": "Chemin .md. Un seul de {markdown, markdown_path}.", "required": False},
                "page_size": {"type": "string", "description": "'A4' (défaut), 'Letter', 'Legal', 'A3', 'A5'.", "required": False},
                "orientation": {"type": "string", "description": "'portrait' (défaut) ou 'landscape'.", "required": False},
                "margins_cm": {"type": "object", "description": "{top, bottom, left, right} défaut 2.5.", "required": False},
                "title": {"type": "string", "description": "PDF metadata Title.", "required": False},
                "author": {"type": "string", "description": "PDF metadata Author.", "required": False},
                "subject": {"type": "string", "description": "PDF metadata Subject.", "required": False},
                "overwrite": {"type": "boolean", "description": "Défaut false.", "required": False},
            },
        },
        {
            "id": "pdf.merge",
            "name": "Merge multiple PDFs",
            "description": (
                "Concatène N PDFs (≥2). Préserve bookmarks/TOC en best-effort. "
                "Metadata depuis le PDF désigné par metadata_from."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "paths": {"type": "array", "description": "Liste de chemins .pdf (≥2).", "required": True},
                "output_path": {"type": "string", "description": "Chemin de sortie.", "required": True},
                "overwrite": {"type": "boolean", "description": "Défaut false.", "required": False},
                "metadata_from": {"type": "integer", "description": "Index 0-based dans paths. Défaut 0.", "required": False},
            },
        },
        {
            "id": "pdf.split",
            "name": "Split PDF by page ranges",
            "description": (
                "Découpe un PDF par plages ou page-par-page. Nommage zero-padded "
                "ou via 'name' du range."
            ),
            "input_modes": ["text"],
            "output_modes": ["text"],
            "input_schema": {
                "path": {"type": "string", "description": "Chemin du .pdf à découper.", "required": True},
                "output_dir": {"type": "string", "description": "Dossier cible (créé si absent).", "required": True},
                "ranges": {"type": "array", "description": "[{name, pages: '1-5'}]. Si absent : page-par-page.", "required": False},
                "overwrite": {"type": "boolean", "description": "Défaut false.", "required": False},
            },
        },
    ]


# ─── Operation: read-text ──────────────────────────────────────────────────


def _do_read_text(payload: dict[str, Any]) -> dict[str, Any]:
    path = _require_str(payload, "path")
    _validate_pdf_path(path, must_exist=True)

    page_range_spec = payload.get("page_range")
    max_chars_per_page = int(payload.get("max_chars_per_page", 100_000))
    include_metadata = bool(payload.get("include_metadata", True))

    from pypdf import PdfReader

    try:
        reader = PdfReader(path)
    except Exception as exc:
        raise _PdfError("PARSE_ERROR", f"Impossible de lire le .pdf : {exc}") from exc

    if reader.is_encrypted:
        raise _PdfError("ENCRYPTED_PDF", f"PDF chiffré, non supporté en v0.1.0 : {path}", details={"path": path})

    total_pages = len(reader.pages)
    indices = _parse_page_range(page_range_spec, total_pages) if page_range_spec else list(range(total_pages))

    pages_out: list[dict[str, Any]] = []
    for idx in indices:
        try:
            raw_text = reader.pages[idx].extract_text() or ""
        except Exception as exc:
            raise _PdfError("PARSE_ERROR", f"Extraction texte échouée page {idx + 1} : {exc}") from exc

        truncated = len(raw_text) > max_chars_per_page
        text = raw_text[:max_chars_per_page] if truncated else raw_text
        pages_out.append({
            "page_num": idx + 1,
            "text": text,
            "char_count": len(text),
            "truncated": truncated,
        })

    result: dict[str, Any] = {
        "path": path,
        "total_pages": total_pages,
        "pages": pages_out,
    }

    if include_metadata:
        result["metadata"] = _extract_pdf_metadata(reader)

    return result


def _extract_pdf_metadata(reader: Any) -> dict[str, Any]:
    md = reader.metadata
    if not md:
        return {}
    out: dict[str, Any] = {}
    for k_pdf, k_out in (
        ("/Title", "title"),
        ("/Author", "author"),
        ("/Subject", "subject"),
        ("/Creator", "creator"),
        ("/Producer", "producer"),
        ("/CreationDate", "creation_date"),
        ("/ModDate", "modification_date"),
    ):
        try:
            v = md.get(k_pdf)
        except Exception:
            v = None
        if v is not None:
            out[k_out] = str(v)
    return out


# ─── Operation: read-tables ────────────────────────────────────────────────


def _do_read_tables(payload: dict[str, Any]) -> dict[str, Any]:
    path = _require_str(payload, "path")
    _validate_pdf_path(path, must_exist=True)

    page_range_spec = payload.get("page_range")
    table_settings = payload.get("table_settings")
    include_headers = bool(payload.get("include_headers", True))

    # Encryption check via pypdf (more reliable than pdfplumber)
    from pypdf import PdfReader
    try:
        pre_reader = PdfReader(path)
    except Exception as exc:
        raise _PdfError("PARSE_ERROR", f"Impossible de lire le .pdf : {exc}") from exc
    if pre_reader.is_encrypted:
        raise _PdfError("ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path})

    import pdfplumber

    try:
        pdf = pdfplumber.open(path)
    except Exception as exc:
        raise _PdfError("PARSE_ERROR", f"pdfplumber a échoué : {exc}") from exc

    try:
        total_pages = len(pdf.pages)
        indices = _parse_page_range(page_range_spec, total_pages) if page_range_spec else list(range(total_pages))

        tables_out: list[dict[str, Any]] = []
        for page_idx in indices:
            page = pdf.pages[page_idx]
            try:
                extracted = page.extract_tables(table_settings=table_settings) if table_settings else page.extract_tables()
            except Exception as exc:
                raise _PdfError(
                    "PARSE_ERROR",
                    f"Extraction tables échouée page {page_idx + 1} : {exc}",
                    details={"page": page_idx + 1},
                ) from exc

            for t_idx, table in enumerate(extracted or []):
                if not table:
                    continue
                rows = table
                rows_count = len(rows)
                cols_count = len(rows[0]) if rows else 0
                entry: dict[str, Any] = {
                    "page_num": page_idx + 1,
                    "table_index": t_idx,
                    "rows": rows,
                    "rows_count": rows_count,
                    "cols_count": cols_count,
                }
                if include_headers and rows:
                    entry["headers"] = rows[0]
                tables_out.append(entry)

        return {
            "path": path,
            "tables": tables_out,
            "total_tables": len(tables_out),
        }
    finally:
        pdf.close()


# ─── Operation: read-forms ─────────────────────────────────────────────────


def _do_read_forms(payload: dict[str, Any]) -> dict[str, Any]:
    path = _require_str(payload, "path")
    _validate_pdf_path(path, must_exist=True)

    from pypdf import PdfReader

    try:
        reader = PdfReader(path)
    except Exception as exc:
        raise _PdfError("PARSE_ERROR", f"Impossible de lire le .pdf : {exc}") from exc
    if reader.is_encrypted:
        raise _PdfError("ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path})

    # XFA detection
    try:
        root = reader.trailer.get("/Root")
        if root:
            acro = root.get_object().get("/AcroForm")
            if acro and acro.get_object().get("/XFA"):
                raise _PdfError(
                    "UNSUPPORTED_FEATURE",
                    "XFA forms (Adobe LiveCycle) non supportés en v0.1.0",
                    details={"feature": "XFA"},
                )
    except _PdfError:
        raise
    except Exception:
        # Non-fatal — continue with AcroForm extraction
        pass

    fields_out: list[dict[str, Any]] = []
    all_fields = reader.get_fields() or {}

    for name, field in all_fields.items():
        ft = field.get("/FT", "")
        # /FT values: /Tx (text), /Ch (choice), /Btn (button), /Sig (signature)
        ftype_map = {"/Tx": "text", "/Ch": "choice", "/Btn": "button", "/Sig": "signature"}
        ftype = ftype_map.get(str(ft), "unknown")
        flags = _safe_int(field.get("/Ff", 0))

        # Refine /Btn → checkbox/radio/pushbutton
        if str(ft) == "/Btn":
            if flags & (1 << 15):
                ftype = "radio"
            elif flags & (1 << 16):
                ftype = "pushbutton"
            else:
                ftype = "checkbox"
        elif str(ft) == "/Ch":
            if flags & (1 << 17):
                ftype = "dropdown"
            else:
                ftype = "listbox"

        value = field.get("/V", "")
        if hasattr(value, "get_object"):
            try:
                value = value.get_object()
            except Exception:
                pass
        value = str(value) if value is not None else ""

        options = None
        if str(ft) == "/Ch":
            opt_list = field.get("/Opt")
            if opt_list:
                options = [str(o) for o in opt_list]

        readonly = bool(flags & 1)
        required = bool(flags & 2)

        max_length = None
        if str(ft) == "/Tx":
            ml = field.get("/MaxLen")
            if ml is not None:
                max_length = _safe_int(ml)

        entry: dict[str, Any] = {
            "name": str(name),
            "type": ftype,
            "value": value,
            "readonly": readonly,
            "required": required,
        }
        if options is not None:
            entry["options"] = options
        if max_length is not None:
            entry["max_length"] = max_length
        fields_out.append(entry)

    return {
        "path": path,
        "fields": fields_out,
        "total_fields": len(fields_out),
    }


# ─── Operation: render-from-markdown ───────────────────────────────────────


def _do_render_from_markdown(payload: dict[str, Any]) -> dict[str, Any]:
    output_path = _require_str(payload, "output_path")
    _validate_pdf_extension(output_path)
    overwrite = bool(payload.get("overwrite", False))

    markdown_text = payload.get("markdown")
    markdown_path_str = payload.get("markdown_path")
    if not (markdown_text or markdown_path_str):
        raise _PdfError(
            "MISSING_FIELD",
            "Un seul de 'markdown' ou 'markdown_path' requis",
            details={"field": "markdown"},
        )
    if markdown_text and markdown_path_str:
        raise _PdfError(
            "INVALID_DATA",
            "Fournir SOIT 'markdown' SOIT 'markdown_path', pas les deux",
            details={"field": "markdown"},
        )

    if markdown_path_str:
        md_path = Path(markdown_path_str)
        if not md_path.exists():
            raise FileNotFoundError(f"fichier .md introuvable : {markdown_path_str}")
        markdown_text = md_path.read_text(encoding="utf-8")

    page_size = payload.get("page_size", "A4")
    if page_size not in ALLOWED_PAGE_SIZES:
        raise _PdfError(
            "INVALID_TYPE",
            f"page_size invalide : {page_size!r}. Valeurs : {list(ALLOWED_PAGE_SIZES)}",
            details={"field": "page_size"},
        )

    orientation = payload.get("orientation", "portrait")
    if orientation not in ALLOWED_ORIENTATIONS:
        raise _PdfError(
            "INVALID_TYPE",
            f"orientation invalide : {orientation!r}",
            details={"field": "orientation"},
        )

    margins_cm = payload.get("margins_cm") or {}
    if not isinstance(margins_cm, dict):
        raise _PdfError("INVALID_TYPE", "margins_cm doit être un objet", details={"field": "margins_cm"})

    out = Path(output_path).expanduser().resolve()
    if out.exists() and not overwrite:
        raise _PdfError(
            "OUTPUT_EXISTS",
            f"Fichier de sortie existe (overwrite=false) : {out}",
            details={"output_path": str(out)},
        )
    out.parent.mkdir(parents=True, exist_ok=True)

    # Markdown → HTML
    try:
        import markdown as md_lib
    except ImportError as exc:
        raise _PdfError("EXECUTION_FAILED", f"markdown lib non disponible : {exc}") from exc

    try:
        # xhtml output : balises auto-fermantes (<hr />, <br />) — compatibles
        # avec xml.etree.ElementTree (parser XML strict).
        html = md_lib.markdown(
            markdown_text or "",
            extensions=["tables", "fenced_code"],
            output_format="xhtml",
        )
    except Exception as exc:
        raise _PdfError("MARKDOWN_PARSE_ERROR", f"Markdown invalide : {exc}", details={"reason": str(exc)}) from exc

    # Wrap in root for ElementTree parsing
    root_xml = f"<root>{html}</root>"
    try:
        root = ET.fromstring(root_xml)
    except ET.ParseError as exc:
        raise _PdfError("MARKDOWN_PARSE_ERROR", f"HTML produit par markdown invalide : {exc}", details={"reason": str(exc)}) from exc

    # Build PDF
    pages_count = _build_pdf_from_html(
        root=root,
        output_path=str(out),
        page_size=page_size,
        orientation=orientation,
        margins_cm=margins_cm,
        title=payload.get("title"),
        author=payload.get("author"),
        subject=payload.get("subject"),
    )

    return {
        "output_path": str(out),
        "pages_count": pages_count,
        "file_size_bytes": out.stat().st_size,
    }


def _build_pdf_from_html(
    root: ET.Element,
    output_path: str,
    page_size: str,
    orientation: str,
    margins_cm: dict[str, Any],
    title: str | None,
    author: str | None,
    subject: str | None,
) -> int:
    from reportlab.lib.pagesizes import A4, A3, A5, letter, legal, landscape
    from reportlab.lib.units import cm
    from reportlab.lib.styles import ParagraphStyle
    from reportlab.lib.enums import TA_JUSTIFY
    from reportlab.lib import colors
    from reportlab.platypus import (
        SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle,
        ListFlowable, ListItem, Preformatted, PageBreak,
    )
    from reportlab.platypus.flowables import HRFlowable

    size_map = {"A4": A4, "Letter": letter, "Legal": legal, "A3": A3, "A5": A5}
    page_dims = size_map[page_size]
    if orientation == "landscape":
        page_dims = landscape(page_dims)

    top_cm = float(margins_cm.get("top", 2.5))
    bottom_cm = float(margins_cm.get("bottom", 2.5))
    left_cm = float(margins_cm.get("left", 2.5))
    right_cm = float(margins_cm.get("right", 2.5))

    doc_kwargs: dict[str, Any] = {
        "pagesize": page_dims,
        "topMargin": top_cm * cm,
        "bottomMargin": bottom_cm * cm,
        "leftMargin": left_cm * cm,
        "rightMargin": right_cm * cm,
    }
    if title:
        doc_kwargs["title"] = title
    if author:
        doc_kwargs["author"] = author
    if subject:
        doc_kwargs["subject"] = subject

    doc = SimpleDocTemplate(output_path, **doc_kwargs)
    styles = _build_styles()

    flowables: list[Any] = []
    for elem in root:
        flowables.extend(_element_to_flowables(elem, styles))

    try:
        doc.build(flowables)
    except Exception as exc:
        raise _PdfError("RENDER_ERROR", f"reportlab a échoué : {exc}", details={"reason": str(exc)}) from exc

    # Read back the PDF to count pages
    from pypdf import PdfReader

    reader = PdfReader(output_path)
    return len(reader.pages)


def _build_styles() -> dict[str, Any]:
    from reportlab.lib.styles import ParagraphStyle
    from reportlab.lib.enums import TA_JUSTIFY, TA_LEFT
    from reportlab.lib import colors

    body = ParagraphStyle(
        "body", fontName="Helvetica", fontSize=11, leading=14,
        alignment=TA_JUSTIFY, spaceAfter=6,
    )
    return {
        "body": body,
        "h1": ParagraphStyle("h1", parent=body, fontName="Helvetica-Bold", fontSize=18, leading=22, spaceBefore=12, spaceAfter=8, alignment=TA_LEFT),
        "h2": ParagraphStyle("h2", parent=body, fontName="Helvetica-Bold", fontSize=16, leading=20, spaceBefore=10, spaceAfter=6, alignment=TA_LEFT),
        "h3": ParagraphStyle("h3", parent=body, fontName="Helvetica-Bold", fontSize=14, leading=18, spaceBefore=8, spaceAfter=4, alignment=TA_LEFT),
        "h4": ParagraphStyle("h4", parent=body, fontName="Helvetica-BoldOblique", fontSize=12, leading=16, spaceBefore=6, spaceAfter=4, alignment=TA_LEFT),
        "h5": ParagraphStyle("h5", parent=body, fontName="Helvetica-Bold", fontSize=11, leading=15, spaceBefore=6, spaceAfter=4, alignment=TA_LEFT),
        "h6": ParagraphStyle("h6", parent=body, fontName="Helvetica-Oblique", fontSize=11, leading=15, spaceBefore=4, spaceAfter=4, alignment=TA_LEFT),
        "code": ParagraphStyle(
            "code", fontName="Courier", fontSize=9, leading=11,
            leftIndent=12, backColor=colors.HexColor("#F5F5F5"),
            borderPadding=6, borderColor=colors.HexColor("#D0D0D0"), borderWidth=0.5,
            spaceAfter=8, spaceBefore=4,
        ),
        "quote": ParagraphStyle(
            "quote", parent=body,
            fontName="Helvetica-Oblique", fontSize=11, leading=14,
            leftIndent=24, rightIndent=24, spaceAfter=6,
            textColor=colors.HexColor("#555555"),
        ),
    }


def _element_to_flowables(elem: ET.Element, styles: dict[str, Any]) -> list[Any]:
    from reportlab.platypus import Paragraph, Preformatted
    from reportlab.platypus.flowables import HRFlowable
    from reportlab.lib import colors

    tag = elem.tag.lower()

    if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
        text = _inline_html_to_reportlab(elem)
        return [Paragraph(text, styles[tag])]

    if tag == "p":
        text = _inline_html_to_reportlab(elem)
        if not text.strip():
            return []
        return [Paragraph(text, styles["body"])]

    if tag == "ul":
        return [_list_to_flowable(elem, styles, ordered=False)]
    if tag == "ol":
        return [_list_to_flowable(elem, styles, ordered=True)]

    if tag == "table":
        return [_table_to_flowable(elem, styles)]

    if tag == "pre":
        code_text = "".join(elem.itertext())
        # Use Preformatted to keep newlines
        return [Preformatted(code_text.rstrip(), styles["code"])]

    if tag == "blockquote":
        out: list[Any] = []
        for child in elem:
            # Use quote style for paragraphs inside
            child_tag = child.tag.lower()
            if child_tag == "p":
                text = _inline_html_to_reportlab(child)
                out.append(Paragraph(text, styles["quote"]))
            else:
                out.extend(_element_to_flowables(child, styles))
        return out

    if tag == "hr":
        return [HRFlowable(
            width="100%", thickness=0.5,
            color=colors.HexColor("#CCCCCC"),
            spaceBefore=6, spaceAfter=6,
        )]

    # Fallback : treat unknown tag as paragraph
    text = _inline_html_to_reportlab(elem)
    if text.strip():
        return [Paragraph(text, styles["body"])]
    return []


def _inline_html_to_reportlab(elem: ET.Element) -> str:
    parts: list[str] = []
    if elem.text:
        parts.append(_escape_pdf_text(elem.text))
    for child in elem:
        tag = child.tag.lower()
        inner = _inline_html_to_reportlab(child)
        if tag in ("strong", "b"):
            parts.append(f"<b>{inner}</b>")
        elif tag in ("em", "i"):
            parts.append(f"<i>{inner}</i>")
        elif tag == "code":
            parts.append(f'<font name="Courier" size="9">{inner}</font>')
        elif tag == "a":
            href = child.get("href", "")
            safe_href = href.replace('"', "&quot;")
            parts.append(f'<a href="{safe_href}" color="blue"><u>{inner}</u></a>')
        elif tag == "br":
            parts.append("<br/>")
        elif tag in ("del", "s"):
            parts.append(f"<strike>{inner}</strike>")
        elif tag == "u":
            parts.append(f"<u>{inner}</u>")
        else:
            parts.append(inner)
        if child.tail:
            parts.append(_escape_pdf_text(child.tail))
    return "".join(parts)


def _escape_pdf_text(s: str) -> str:
    """Escape & < > for reportlab Paragraph markup parser."""
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _list_to_flowable(elem: ET.Element, styles: dict[str, Any], ordered: bool) -> Any:
    from reportlab.platypus import ListFlowable, ListItem, Paragraph

    items: list[Any] = []
    for li in elem.findall("li"):
        # Inline content of <li>
        li_inline_parts: list[str] = []
        if li.text:
            li_inline_parts.append(_escape_pdf_text(li.text))
        nested: list[Any] = []
        for child in li:
            child_tag = child.tag.lower()
            if child_tag in ("ul", "ol"):
                nested.append(_list_to_flowable(child, styles, ordered=(child_tag == "ol")))
            elif child_tag == "p":
                # Inline <p> inside <li> — flatten
                li_inline_parts.append(_inline_html_to_reportlab(child))
                if child.tail:
                    li_inline_parts.append(_escape_pdf_text(child.tail))
            else:
                # Inline child
                tag = child_tag
                inner = _inline_html_to_reportlab(child)
                if tag in ("strong", "b"):
                    li_inline_parts.append(f"<b>{inner}</b>")
                elif tag in ("em", "i"):
                    li_inline_parts.append(f"<i>{inner}</i>")
                elif tag == "code":
                    li_inline_parts.append(f'<font name="Courier" size="9">{inner}</font>')
                elif tag == "a":
                    href = child.get("href", "")
                    li_inline_parts.append(f'<a href="{href}" color="blue"><u>{inner}</u></a>')
                else:
                    li_inline_parts.append(inner)
                if child.tail:
                    li_inline_parts.append(_escape_pdf_text(child.tail))
        text = "".join(li_inline_parts).strip()
        item_flowables: list[Any] = [Paragraph(text or " ", styles["body"])]
        item_flowables.extend(nested)
        items.append(ListItem(item_flowables, leftIndent=20))

    bullet_type = "1" if ordered else "bullet"
    return ListFlowable(items, bulletType=bullet_type, leftIndent=20)


def _table_to_flowable(elem: ET.Element, styles: dict[str, Any]) -> Any:
    from reportlab.platypus import Table, TableStyle, Paragraph
    from reportlab.lib import colors

    rows_data: list[list[Any]] = []
    has_header = False

    for section in elem:
        tag = section.tag.lower()
        if tag == "thead":
            has_header = True
        for tr in section.findall("tr") if tag in ("thead", "tbody", "tfoot") else [section] if tag == "tr" else []:
            row: list[Any] = []
            for cell in tr:
                if cell.tag.lower() in ("th", "td"):
                    text = _inline_html_to_reportlab(cell)
                    row.append(Paragraph(text, styles["body"]))
            rows_data.append(row)

    if not rows_data:
        return Paragraph("(empty table)", styles["body"])

    table = Table(rows_data, repeatRows=1 if has_header else 0)
    table_style_cmds = [
        ("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#BFBFBF")),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 6),
        ("RIGHTPADDING", (0, 0), (-1, -1), 6),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
    ]
    if has_header:
        table_style_cmds.insert(0, ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#DDEBF7")))
        table_style_cmds.insert(1, ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"))
    table.setStyle(TableStyle(table_style_cmds))
    return table


# ─── Operation: merge ──────────────────────────────────────────────────────


def _do_merge(payload: dict[str, Any]) -> dict[str, Any]:
    paths = payload.get("paths")
    if not isinstance(paths, list):
        raise _PdfError("MISSING_FIELD", "Champ 'paths' requis (liste)", details={"field": "paths"})
    if len(paths) < 2:
        raise _PdfError(
            "INVALID_DATA",
            f"paths doit contenir au moins 2 PDFs (reçu : {len(paths)})",
            details={"got": len(paths)},
        )

    output_path = _require_str(payload, "output_path")
    _validate_pdf_extension(output_path)
    overwrite = bool(payload.get("overwrite", False))
    metadata_from = int(payload.get("metadata_from", 0))

    if metadata_from < 0 or metadata_from >= len(paths):
        raise _PdfError(
            "INVALID_DATA",
            f"metadata_from={metadata_from} hors plage [0, {len(paths) - 1}]",
            details={"field": "metadata_from"},
        )

    out = Path(output_path).expanduser().resolve()
    if out.exists() and not overwrite:
        raise _PdfError(
            "OUTPUT_EXISTS",
            f"Output existe (overwrite=false) : {out}",
            details={"output_path": str(out)},
        )
    out.parent.mkdir(parents=True, exist_ok=True)

    # Validate all paths
    for i, p in enumerate(paths):
        if not isinstance(p, str) or not p:
            raise _PdfError("INVALID_DATA", f"paths[{i}] doit être une string non vide", details={"index": i})
        _validate_pdf_path(p, must_exist=True)

    from pypdf import PdfReader, PdfWriter

    writer = PdfWriter()
    total_pages = 0

    for p in paths:
        try:
            reader = PdfReader(p)
        except Exception as exc:
            raise _PdfError("PARSE_ERROR", f"Impossible de lire {p} : {exc}") from exc
        if reader.is_encrypted:
            raise _PdfError("ENCRYPTED_PDF", f"PDF chiffré : {p}", details={"path": p})
        for page in reader.pages:
            writer.add_page(page)
        total_pages += len(reader.pages)

    # Copy metadata from designated source
    try:
        src_reader = PdfReader(paths[metadata_from])
        if src_reader.metadata:
            md_dict = {k: str(v) for k, v in src_reader.metadata.items()}
            writer.add_metadata(md_dict)
    except Exception:
        # Best-effort — metadata copy is not critical
        pass

    try:
        with open(out, "wb") as f:
            writer.write(f)
    except Exception as exc:
        raise _PdfError("EXECUTION_FAILED", f"Écriture impossible : {exc}") from exc

    return {
        "output_path": str(out),
        "merged_count": len(paths),
        "total_pages": total_pages,
        "file_size_bytes": out.stat().st_size,
    }


# ─── Operation: split ──────────────────────────────────────────────────────


def _do_split(payload: dict[str, Any]) -> dict[str, Any]:
    path = _require_str(payload, "path")
    _validate_pdf_path(path, must_exist=True)
    output_dir = _require_str(payload, "output_dir")
    ranges_spec = payload.get("ranges")
    overwrite = bool(payload.get("overwrite", False))

    out_dir = Path(output_dir).expanduser().resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    from pypdf import PdfReader, PdfWriter

    try:
        reader = PdfReader(path)
    except Exception as exc:
        raise _PdfError("PARSE_ERROR", f"Impossible de lire le .pdf : {exc}") from exc
    if reader.is_encrypted:
        raise _PdfError("ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path})

    total_pages = len(reader.pages)
    splits: list[dict[str, Any]] = []

    if ranges_spec is not None:
        if not isinstance(ranges_spec, list) or not ranges_spec:
            raise _PdfError("INVALID_DATA", "ranges doit être une liste non vide", details={"field": "ranges"})

        for i, range_obj in enumerate(ranges_spec):
            if not isinstance(range_obj, dict):
                raise _PdfError("INVALID_DATA", f"ranges[{i}] doit être un objet", details={"index": i})
            name = range_obj.get("name")
            if not isinstance(name, str) or not name:
                raise _PdfError("INVALID_DATA", f"ranges[{i}].name requis (string non vide)", details={"index": i})
            pages_spec = range_obj.get("pages")
            if not isinstance(pages_spec, str) or not pages_spec:
                raise _PdfError("INVALID_DATA", f"ranges[{i}].pages requis (string non vide)", details={"index": i})

            indices = _parse_page_range(pages_spec, total_pages)
            if not indices:
                raise _PdfError("INVALID_DATA", f"ranges[{i}] vide après parsing", details={"index": i})

            out_path = out_dir / f"{_safe_filename(name)}.pdf"
            if out_path.exists() and not overwrite:
                raise _PdfError(
                    "OUTPUT_EXISTS",
                    f"Output existe (overwrite=false) : {out_path}",
                    details={"output_path": str(out_path)},
                )

            writer = PdfWriter()
            for idx in indices:
                writer.add_page(reader.pages[idx])
            with open(out_path, "wb") as f:
                writer.write(f)

            splits.append({
                "output_path": str(out_path),
                "pages_count": len(indices),
                "file_size_bytes": out_path.stat().st_size,
                "page_range": pages_spec,
            })
    else:
        # Page-by-page
        width = max(3, len(str(total_pages)))
        for i in range(total_pages):
            out_path = out_dir / f"page-{i + 1:0{width}d}.pdf"
            if out_path.exists() and not overwrite:
                raise _PdfError(
                    "OUTPUT_EXISTS",
                    f"Output existe (overwrite=false) : {out_path}",
                    details={"output_path": str(out_path)},
                )
            writer = PdfWriter()
            writer.add_page(reader.pages[i])
            with open(out_path, "wb") as f:
                writer.write(f)
            splits.append({
                "output_path": str(out_path),
                "pages_count": 1,
                "file_size_bytes": out_path.stat().st_size,
                "page_range": str(i + 1),
            })

    return {
        "output_dir": str(out_dir),
        "splits": splits,
        "total_splits": len(splits),
    }


# ─── Helpers ───────────────────────────────────────────────────────────────


def _require_str(payload: dict[str, Any], field: str) -> str:
    value = payload.get(field)
    if not isinstance(value, str) or not value:
        raise _PdfError(
            "MISSING_FIELD",
            f"Champ '{field}' requis (string non vide)",
            details={"field": field},
        )
    return value


def _validate_pdf_extension(path: str) -> None:
    if not path.lower().endswith(".pdf"):
        raise _PdfError(
            "UNSUPPORTED_FORMAT",
            f"Extension non reconnue : {path!r}. .pdf requis.",
            details={"path": path},
        )


def _validate_pdf_path(path: str, must_exist: bool = False) -> None:
    _validate_pdf_extension(path)
    if must_exist:
        p = Path(path)
        if not p.exists():
            raise FileNotFoundError(f"fichier introuvable : {path}")
        size = p.stat().st_size
        if size > MAX_FILE_BYTES:
            raise _PdfError(
                "TOO_LARGE",
                f"Fichier trop volumineux ({size} > {MAX_FILE_BYTES} bytes)",
                details={"size_bytes": size, "limit_bytes": MAX_FILE_BYTES},
            )


def _parse_page_range(spec: str, total_pages: int) -> list[int]:
    """Parse '1-5,7,10-12' → list of 0-based page indices (validated)."""
    if not isinstance(spec, str) or not spec.strip():
        raise _PdfError("INVALID_PAGE_RANGE", "page_range vide", details={"field": "page_range"})

    indices: list[int] = []
    seen: set[int] = set()
    parts = [p.strip() for p in spec.split(",") if p.strip()]
    if not parts:
        raise _PdfError("INVALID_PAGE_RANGE", f"page_range vide après split : {spec!r}", details={"field": "page_range"})

    for part in parts:
        if "-" in part:
            tokens = part.split("-")
            if len(tokens) != 2:
                raise _PdfError("INVALID_PAGE_RANGE", f"Token invalide : {part!r}", details={"token": part})
            try:
                start_i = int(tokens[0])
                end_i = int(tokens[1])
            except ValueError as exc:
                raise _PdfError("INVALID_PAGE_RANGE", f"Range non numérique : {part!r}", details={"token": part}) from exc
            if start_i < 1 or end_i < start_i:
                raise _PdfError(
                    "INVALID_PAGE_RANGE",
                    f"Range invalide : {part!r} (attendu 1-based croissant)",
                    details={"token": part},
                )
            if end_i > total_pages:
                raise _PdfError(
                    "PAGE_OUT_OF_RANGE",
                    f"Range {part!r} dépasse total_pages={total_pages}",
                    details={"token": part, "total_pages": total_pages},
                )
            for p in range(start_i, end_i + 1):
                if p not in seen:
                    indices.append(p - 1)
                    seen.add(p)
        else:
            try:
                p = int(part)
            except ValueError as exc:
                raise _PdfError("INVALID_PAGE_RANGE", f"Token non numérique : {part!r}", details={"token": part}) from exc
            if p < 1:
                raise _PdfError("INVALID_PAGE_RANGE", f"Page invalide : {p} (1-based)", details={"token": part})
            if p > total_pages:
                raise _PdfError(
                    "PAGE_OUT_OF_RANGE",
                    f"Page {p} dépasse total_pages={total_pages}",
                    details={"page": p, "total_pages": total_pages},
                )
            if p not in seen:
                indices.append(p - 1)
                seen.add(p)

    return indices


def _safe_filename(name: str) -> str:
    """Sanitize a filename : replace dangerous chars, cap length."""
    sanitized = re.sub(r"[^\w\-.]", "_", name)
    return sanitized[:128] or "file"


def _safe_int(v: Any) -> int:
    try:
        return int(v)
    except (TypeError, ValueError):
        return 0


def _json_default(o: Any) -> Any:
    if isinstance(o, (datetime.datetime, datetime.date)):
        return o.isoformat()
    raise TypeError(f"Object of type {type(o).__name__} is not JSON serializable")


# Module-level — requis par le loader runtime (crates/apollia-aip/src/loader.rs).
agent = PdfWorker()
