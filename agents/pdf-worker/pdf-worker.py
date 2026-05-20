"""pdf-worker — Manipulation de fichiers PDF.

Worker Apollia OS standalone, 6 skills A2A déterministes :

* ``pdf.read_text``            — texte par page + metadata (pypdf)
* ``pdf.read_tables``          — tables (pdfplumber)
* ``pdf.read_forms``           — champs AcroForm (pypdf)
* ``pdf.render_from_markdown`` — Markdown → PDF (markdown + reportlab)
* ``pdf.merge``                — concaténer N PDFs (pypdf)
* ``pdf.split``                — découper par plages ou page-par-page

Page range syntax : ``"1-5,7,10-12"`` — 1-based.
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Annotated, Any

from apollia import DomainError, agent, skill
from apollia.types import Ctx

# Resolve ``schemas`` from the worker dir even when another agent has
# previously populated ``sys.modules['schemas']`` from its own folder.
import sys
from pathlib import Path as _PathForBootstrap
_WORKER_DIR = str(_PathForBootstrap(__file__).resolve().parent)
if _WORKER_DIR in sys.path:
    sys.path.remove(_WORKER_DIR)
sys.path.insert(0, _WORKER_DIR)
sys.modules.pop("schemas", None)

# Canonical input TypedDicts — structural input_schema for mid-market LLMs.
from schemas import (  # type: ignore[import-not-found]  # noqa: E402
    MarginsCm,
    SplitRangeSpec,
)


MAX_FILE_BYTES: int = 100 * 1024 * 1024
ALLOWED_PAGE_SIZES: tuple[str, ...] = ("A4", "Letter", "Legal", "A3", "A5")
ALLOWED_ORIENTATIONS: tuple[str, ...] = ("portrait", "landscape")


@agent(
    name="pdf-worker",
    version="0.1.0",
    description=(
        "Manipulation de fichiers PDF : extraction (texte/tables/forms), "
        "génération (Markdown → PDF), merge et split par plages."
    ),
    tags=("pdf", "pypdf", "pdfplumber", "reportlab", "markdown", "worker"),
    agent_type="worker",
    step_budget={"max_steps": 1, "max_tool_calls": 5, "wall_clock_secs": 300},
)
class PdfWorker:
    """Worker déterministe — 6 skills PDF."""

    @skill(
        "pdf.read_text",
        description=(
            "Extract text from a PDF page-by-page with optional metadata. "
            "Page numbers are 1-based; text is capped per page."
        ),
        examples=[
            {
                "path": "/path/to/document.pdf",
                "page_range": "1-3,5",
            },
        ],
    )
    async def read_text(
        self,
        path: Annotated[str, "Filesystem path to the source .pdf file."],
        page_range: Annotated[
            str | None,
            "1-based page selection, e.g. '1-5,7,10-12'. Omit to read all pages.",
        ] = None,
        max_chars_per_page: Annotated[
            int,
            "Maximum characters returned per page (truncates and sets truncated=true).",
        ] = 100_000,
        include_metadata: Annotated[
            bool,
            "Include PDF metadata (title, author, producer, ...) in the response.",
        ] = True,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Texte PDF, page par page."""
        _validate_pdf_path(path, must_exist=True)
        from pypdf import PdfReader

        try:
            reader = PdfReader(path)
        except Exception as exc:
            raise DomainError("PARSE_ERROR", f"Impossible de lire le .pdf : {exc}") from exc

        if reader.is_encrypted:
            raise DomainError(
                "ENCRYPTED_PDF",
                f"PDF chiffré, non supporté : {path}",
                details={"path": path},
            )

        total_pages = len(reader.pages)
        indices = (
            _parse_page_range(page_range, total_pages)
            if page_range
            else list(range(total_pages))
        )

        pages_out: list[dict[str, Any]] = []
        for idx in indices:
            try:
                raw_text = reader.pages[idx].extract_text() or ""
            except Exception as exc:
                raise DomainError(
                    "PARSE_ERROR",
                    f"Extraction texte échouée page {idx + 1} : {exc}",
                ) from exc

            truncated = len(raw_text) > max_chars_per_page
            text = raw_text[:max_chars_per_page] if truncated else raw_text
            pages_out.append(
                {
                    "page_num": idx + 1,
                    "text": text,
                    "char_count": len(text),
                    "truncated": truncated,
                }
            )

        result: dict[str, Any] = {
            "path": path,
            "total_pages": total_pages,
            "pages": pages_out,
        }
        if include_metadata:
            result["metadata"] = _extract_pdf_metadata(reader)
        return result

    @skill(
        "pdf.read_tables",
        description=(
            "Extract tables from a PDF via pdfplumber. Table detection "
            "strategies are configurable via table_settings."
        ),
        examples=[
            {
                "path": "/path/to/report.pdf",
                "page_range": "2-4",
                "table_settings": {
                    "vertical_strategy": "lines",
                    "horizontal_strategy": "lines",
                },
            },
        ],
    )
    async def read_tables(
        self,
        path: Annotated[str, "Filesystem path to the source .pdf file."],
        page_range: Annotated[
            str | None,
            "1-based page selection, e.g. '1-5,7'. Omit to scan every page.",
        ] = None,
        table_settings: Annotated[
            dict[str, Any] | None,
            (
                "pdfplumber detection options. Common keys: "
                "vertical_strategy ('lines' | 'text'), horizontal_strategy ('lines' | 'text'), "
                "snap_tolerance (number), edge_min_length (number)."
            ),
        ] = None,
        include_headers: Annotated[
            bool,
            "When true, exposes the first row of each table as 'headers' for convenience.",
        ] = True,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Tables d'un PDF via pdfplumber."""
        _validate_pdf_path(path, must_exist=True)

        from pypdf import PdfReader

        try:
            pre_reader = PdfReader(path)
        except Exception as exc:
            raise DomainError(
                "PARSE_ERROR", f"Impossible de lire le .pdf : {exc}"
            ) from exc
        if pre_reader.is_encrypted:
            raise DomainError(
                "ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path}
            )

        import pdfplumber

        try:
            pdf = pdfplumber.open(path)
        except Exception as exc:
            raise DomainError("PARSE_ERROR", f"pdfplumber a échoué : {exc}") from exc

        try:
            total_pages = len(pdf.pages)
            indices = (
                _parse_page_range(page_range, total_pages)
                if page_range
                else list(range(total_pages))
            )

            tables_out: list[dict[str, Any]] = []
            for page_idx in indices:
                page = pdf.pages[page_idx]
                try:
                    extracted = (
                        page.extract_tables(table_settings=table_settings)
                        if table_settings
                        else page.extract_tables()
                    )
                except Exception as exc:
                    raise DomainError(
                        "PARSE_ERROR",
                        f"Extraction tables échouée page {page_idx + 1} : {exc}",
                        details={"page": page_idx + 1},
                    ) from exc

                for t_idx, table in enumerate(extracted or []):
                    if not table:
                        continue
                    rows = table
                    entry: dict[str, Any] = {
                        "page_num": page_idx + 1,
                        "table_index": t_idx,
                        "rows": rows,
                        "rows_count": len(rows),
                        "cols_count": len(rows[0]) if rows else 0,
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

    @skill(
        "pdf.read_forms",
        description=(
            "Extract AcroForm fields (text, checkbox, radio, dropdown, "
            "listbox, signature). XFA forms are rejected."
        ),
        examples=[
            {"path": "/path/to/form.pdf"},
        ],
    )
    async def read_forms(
        self,
        path: Annotated[str, "Filesystem path to the source .pdf file."],
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Lit les champs AcroForm d'un PDF."""
        _validate_pdf_path(path, must_exist=True)

        from pypdf import PdfReader

        try:
            reader = PdfReader(path)
        except Exception as exc:
            raise DomainError(
                "PARSE_ERROR", f"Impossible de lire le .pdf : {exc}"
            ) from exc
        if reader.is_encrypted:
            raise DomainError(
                "ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path}
            )

        # XFA detection
        try:
            root = reader.trailer.get("/Root")
            if root:
                acro = root.get_object().get("/AcroForm")
                if acro and acro.get_object().get("/XFA"):
                    raise DomainError(
                        "UNSUPPORTED_FEATURE",
                        "XFA forms (Adobe LiveCycle) non supportés",
                        details={"feature": "XFA"},
                    )
        except DomainError:
            raise
        except Exception:
            pass

        fields_out: list[dict[str, Any]] = []
        for name, field in (reader.get_fields() or {}).items():
            ft = field.get("/FT", "")
            ftype_map = {"/Tx": "text", "/Ch": "choice", "/Btn": "button", "/Sig": "signature"}
            ftype = ftype_map.get(str(ft), "unknown")
            flags = _safe_int(field.get("/Ff", 0))

            if str(ft) == "/Btn":
                if flags & (1 << 15):
                    ftype = "radio"
                elif flags & (1 << 16):
                    ftype = "pushbutton"
                else:
                    ftype = "checkbox"
            elif str(ft) == "/Ch":
                ftype = "dropdown" if (flags & (1 << 17)) else "listbox"

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

            entry: dict[str, Any] = {
                "name": str(name),
                "type": ftype,
                "value": value,
                "readonly": bool(flags & 1),
                "required": bool(flags & 2),
            }
            if options is not None:
                entry["options"] = options
            if str(ft) == "/Tx":
                ml = field.get("/MaxLen")
                if ml is not None:
                    entry["max_length"] = _safe_int(ml)
            fields_out.append(entry)

        return {"path": path, "fields": fields_out, "total_fields": len(fields_out)}

    @skill(
        "pdf.render_from_markdown",
        description=(
            "Render a Markdown source into a PDF (Markdown → HTML → reportlab). "
            "Supports page size, orientation, custom margins, and PDF metadata."
        ),
        examples=[
            {
                "output_path": "/tmp/report.pdf",
                "markdown": "# Report\n\nFirst paragraph.\n\n- bullet 1\n- bullet 2\n",
                "page_size": "A4",
                "title": "Quarterly Report",
            },
        ],
    )
    async def render_from_markdown(
        self,
        output_path: Annotated[str, "Filesystem path of the .pdf to create."],
        markdown: Annotated[
            str | None,
            "Inline Markdown source. Provide exactly one of markdown or markdown_path.",
        ] = None,
        markdown_path: Annotated[
            str | None,
            "Path to a .md file. Provide exactly one of markdown or markdown_path.",
        ] = None,
        page_size: Annotated[
            str,
            "Page size: 'A4' (default) | 'Letter' | 'Legal' | 'A3' | 'A5'.",
        ] = "A4",
        orientation: Annotated[
            str,
            "Page orientation: 'portrait' (default) | 'landscape'.",
        ] = "portrait",
        margins_cm: MarginsCm | None = None,
        title: Annotated[str | None, "PDF metadata title."] = None,
        author: Annotated[str | None, "PDF metadata author."] = None,
        subject: Annotated[str | None, "PDF metadata subject."] = None,
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Markdown → PDF."""
        _validate_pdf_extension(output_path)
        if not (markdown or markdown_path):
            raise DomainError(
                "MISSING_FIELD",
                "Un seul de 'markdown' ou 'markdown_path' requis",
                details={"field": "markdown"},
            )
        if markdown and markdown_path:
            raise DomainError(
                "INVALID_DATA",
                "Fournir SOIT 'markdown' SOIT 'markdown_path', pas les deux",
                details={"field": "markdown"},
            )
        if markdown_path:
            md_path = Path(markdown_path)
            if not md_path.exists():
                raise DomainError(
                    "FILE_NOT_FOUND", f"fichier .md introuvable : {markdown_path}"
                )
            markdown = md_path.read_text(encoding="utf-8")

        if page_size not in ALLOWED_PAGE_SIZES:
            raise DomainError(
                "INVALID_TYPE",
                f"page_size invalide : {page_size!r}",
                details={"field": "page_size", "allowed": list(ALLOWED_PAGE_SIZES)},
            )
        if orientation not in ALLOWED_ORIENTATIONS:
            raise DomainError(
                "INVALID_TYPE",
                f"orientation invalide : {orientation!r}",
                details={"field": "orientation"},
            )
        margins_cm = margins_cm or {}
        if not isinstance(margins_cm, dict):
            raise DomainError(
                "INVALID_TYPE", "margins_cm doit être un objet", details={"field": "margins_cm"}
            )

        out = Path(output_path).expanduser().resolve()
        if out.exists() and not overwrite:
            raise DomainError(
                "OUTPUT_EXISTS",
                f"Output existe (overwrite=false) : {out}",
                details={"output_path": str(out)},
            )
        out.parent.mkdir(parents=True, exist_ok=True)

        try:
            import markdown as md_lib
        except ImportError as exc:
            raise DomainError(
                "EXECUTION_FAILED", f"markdown lib non disponible : {exc}"
            ) from exc

        try:
            html = md_lib.markdown(
                markdown or "",
                extensions=["tables", "fenced_code"],
                output_format="xhtml",
            )
        except Exception as exc:
            raise DomainError(
                "MARKDOWN_PARSE_ERROR",
                f"Markdown invalide : {exc}",
                details={"reason": str(exc)},
            ) from exc

        try:
            root = ET.fromstring(f"<root>{html}</root>")
        except ET.ParseError as exc:
            raise DomainError(
                "MARKDOWN_PARSE_ERROR",
                f"HTML invalide : {exc}",
                details={"reason": str(exc)},
            ) from exc

        pages_count = _build_pdf_from_html(
            root=root,
            output_path=str(out),
            page_size=page_size,
            orientation=orientation,
            margins_cm=margins_cm,
            title=title,
            author=author,
            subject=subject,
        )
        return {
            "output_path": str(out),
            "pages_count": pages_count,
            "file_size_bytes": out.stat().st_size,
        }

    @skill(
        "pdf.merge",
        description=(
            "Concatenate ≥2 PDF files into one. Metadata is copied from the "
            "PDF designated by metadata_from (index into paths)."
        ),
        examples=[
            {
                "paths": ["/tmp/part1.pdf", "/tmp/part2.pdf"],
                "output_path": "/tmp/merged.pdf",
                "metadata_from": 0,
            },
        ],
    )
    async def merge(
        self,
        paths: Annotated[
            list[str],
            "Ordered list of PDF paths to concatenate (length >= 2).",
        ],
        output_path: Annotated[str, "Filesystem path of the merged .pdf to create."],
        overwrite: Annotated[bool, "Allow overwriting an existing output file."] = False,
        metadata_from: Annotated[
            int,
            "0-based index into paths whose metadata is preserved in the merged PDF.",
        ] = 0,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Concatène N PDFs."""
        if len(paths) < 2:
            raise DomainError(
                "INVALID_DATA",
                f"paths doit contenir au moins 2 PDFs (reçu : {len(paths)})",
                details={"got": len(paths)},
            )
        _validate_pdf_extension(output_path)
        if metadata_from < 0 or metadata_from >= len(paths):
            raise DomainError(
                "INVALID_DATA",
                f"metadata_from={metadata_from} hors plage [0, {len(paths) - 1}]",
                details={"field": "metadata_from"},
            )
        out = Path(output_path).expanduser().resolve()
        if out.exists() and not overwrite:
            raise DomainError(
                "OUTPUT_EXISTS",
                f"Output existe (overwrite=false) : {out}",
                details={"output_path": str(out)},
            )
        out.parent.mkdir(parents=True, exist_ok=True)

        for i, p in enumerate(paths):
            if not isinstance(p, str) or not p:
                raise DomainError(
                    "INVALID_DATA",
                    f"paths[{i}] doit être une string non vide",
                    details={"index": i},
                )
            _validate_pdf_path(p, must_exist=True)

        from pypdf import PdfReader, PdfWriter

        writer = PdfWriter()
        total_pages = 0
        for p in paths:
            try:
                reader = PdfReader(p)
            except Exception as exc:
                raise DomainError(
                    "PARSE_ERROR", f"Impossible de lire {p} : {exc}"
                ) from exc
            if reader.is_encrypted:
                raise DomainError(
                    "ENCRYPTED_PDF", f"PDF chiffré : {p}", details={"path": p}
                )
            for page in reader.pages:
                writer.add_page(page)
            total_pages += len(reader.pages)

        try:
            src_reader = PdfReader(paths[metadata_from])
            if src_reader.metadata:
                writer.add_metadata({k: str(v) for k, v in src_reader.metadata.items()})
        except Exception:
            pass

        try:
            with open(out, "wb") as f:
                writer.write(f)
        except Exception as exc:
            raise DomainError(
                "EXECUTION_FAILED", f"Écriture impossible : {exc}"
            ) from exc

        return {
            "output_path": str(out),
            "merged_count": len(paths),
            "total_pages": total_pages,
            "file_size_bytes": out.stat().st_size,
        }

    @skill(
        "pdf.split",
        description=(
            "Split a PDF into multiple files. Omit 'ranges' to split "
            "page-by-page (zero-padded naming); otherwise pass named ranges."
        ),
        examples=[
            {
                "path": "/tmp/full.pdf",
                "output_dir": "/tmp/parts",
                "ranges": [
                    {"name": "intro", "pages": "1-3"},
                    {"name": "body", "pages": "4-10"},
                    {"name": "appendix", "pages": "11-12"},
                ],
            },
        ],
    )
    async def split(
        self,
        path: Annotated[str, "Filesystem path to the source .pdf file."],
        output_dir: Annotated[str, "Directory where the split parts are written (created if missing)."],
        ranges: list[SplitRangeSpec] | None = None,
        overwrite: Annotated[bool, "Allow overwriting existing output files."] = False,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Découpe un PDF."""
        _validate_pdf_path(path, must_exist=True)
        out_dir = Path(output_dir).expanduser().resolve()
        out_dir.mkdir(parents=True, exist_ok=True)

        from pypdf import PdfReader, PdfWriter

        try:
            reader = PdfReader(path)
        except Exception as exc:
            raise DomainError(
                "PARSE_ERROR", f"Impossible de lire le .pdf : {exc}"
            ) from exc
        if reader.is_encrypted:
            raise DomainError(
                "ENCRYPTED_PDF", f"PDF chiffré : {path}", details={"path": path}
            )

        total_pages = len(reader.pages)
        splits: list[dict[str, Any]] = []

        if ranges is not None:
            if not isinstance(ranges, list) or not ranges:
                raise DomainError(
                    "INVALID_DATA",
                    "ranges doit être une liste non vide",
                    details={"field": "ranges"},
                )
            for i, r in enumerate(ranges):
                if not isinstance(r, dict):
                    raise DomainError(
                        "INVALID_DATA",
                        f"ranges[{i}] doit être un objet",
                        details={"index": i},
                    )
                name = r.get("name")
                if not isinstance(name, str) or not name:
                    raise DomainError(
                        "INVALID_DATA",
                        f"ranges[{i}].name requis (string non vide)",
                        details={"index": i},
                    )
                pages_spec = r.get("pages")
                if not isinstance(pages_spec, str) or not pages_spec:
                    raise DomainError(
                        "INVALID_DATA",
                        f"ranges[{i}].pages requis (string non vide)",
                        details={"index": i},
                    )
                indices = _parse_page_range(pages_spec, total_pages)
                if not indices:
                    raise DomainError(
                        "INVALID_DATA",
                        f"ranges[{i}] vide après parsing",
                        details={"index": i},
                    )
                out_path = out_dir / f"{_safe_filename(name)}.pdf"
                if out_path.exists() and not overwrite:
                    raise DomainError(
                        "OUTPUT_EXISTS",
                        f"Output existe (overwrite=false) : {out_path}",
                        details={"output_path": str(out_path)},
                    )
                writer = PdfWriter()
                for idx in indices:
                    writer.add_page(reader.pages[idx])
                with open(out_path, "wb") as f:
                    writer.write(f)
                splits.append(
                    {
                        "output_path": str(out_path),
                        "pages_count": len(indices),
                        "file_size_bytes": out_path.stat().st_size,
                        "page_range": pages_spec,
                    }
                )
        else:
            width = max(3, len(str(total_pages)))
            for i in range(total_pages):
                out_path = out_dir / f"page-{i + 1:0{width}d}.pdf"
                if out_path.exists() and not overwrite:
                    raise DomainError(
                        "OUTPUT_EXISTS",
                        f"Output existe (overwrite=false) : {out_path}",
                        details={"output_path": str(out_path)},
                    )
                writer = PdfWriter()
                writer.add_page(reader.pages[i])
                with open(out_path, "wb") as f:
                    writer.write(f)
                splits.append(
                    {
                        "output_path": str(out_path),
                        "pages_count": 1,
                        "file_size_bytes": out_path.stat().st_size,
                        "page_range": str(i + 1),
                    }
                )

        return {
            "output_dir": str(out_dir),
            "splits": splits,
            "total_splits": len(splits),
        }


# ─── Helpers (déterministes, factorisés) ─────────────────────────────────


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


def _validate_pdf_extension(path: str) -> None:
    if not path.lower().endswith(".pdf"):
        raise DomainError(
            "UNSUPPORTED_FORMAT",
            f"Extension non reconnue : {path!r}. .pdf requis.",
            details={"path": path},
        )


def _validate_pdf_path(path: str, must_exist: bool = False) -> None:
    _validate_pdf_extension(path)
    if must_exist:
        p = Path(path)
        if not p.exists():
            raise DomainError("FILE_NOT_FOUND", f"fichier introuvable : {path}")
        size = p.stat().st_size
        if size > MAX_FILE_BYTES:
            raise DomainError(
                "TOO_LARGE",
                f"Fichier trop volumineux ({size} > {MAX_FILE_BYTES} bytes)",
                details={"size_bytes": size, "limit_bytes": MAX_FILE_BYTES},
            )


def _parse_page_range(spec: str, total_pages: int) -> list[int]:
    """Parse '1-5,7,10-12' → list of 0-based page indices."""
    if not isinstance(spec, str) or not spec.strip():
        raise DomainError(
            "INVALID_PAGE_RANGE", "page_range vide", details={"field": "page_range"}
        )
    indices: list[int] = []
    seen: set[int] = set()
    parts = [p.strip() for p in spec.split(",") if p.strip()]
    if not parts:
        raise DomainError(
            "INVALID_PAGE_RANGE",
            f"page_range vide après split : {spec!r}",
            details={"field": "page_range"},
        )
    for part in parts:
        if "-" in part:
            tokens = part.split("-")
            if len(tokens) != 2:
                raise DomainError(
                    "INVALID_PAGE_RANGE",
                    f"Token invalide : {part!r}",
                    details={"token": part},
                )
            try:
                start_i = int(tokens[0])
                end_i = int(tokens[1])
            except ValueError as exc:
                raise DomainError(
                    "INVALID_PAGE_RANGE",
                    f"Range non numérique : {part!r}",
                    details={"token": part},
                ) from exc
            if start_i < 1 or end_i < start_i:
                raise DomainError(
                    "INVALID_PAGE_RANGE",
                    f"Range invalide : {part!r}",
                    details={"token": part},
                )
            if end_i > total_pages:
                raise DomainError(
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
                raise DomainError(
                    "INVALID_PAGE_RANGE",
                    f"Token non numérique : {part!r}",
                    details={"token": part},
                ) from exc
            if p < 1:
                raise DomainError(
                    "INVALID_PAGE_RANGE",
                    f"Page invalide : {p} (1-based)",
                    details={"token": part},
                )
            if p > total_pages:
                raise DomainError(
                    "PAGE_OUT_OF_RANGE",
                    f"Page {p} dépasse total_pages={total_pages}",
                    details={"page": p, "total_pages": total_pages},
                )
            if p not in seen:
                indices.append(p - 1)
                seen.add(p)
    return indices


def _safe_filename(name: str) -> str:
    sanitized = re.sub(r"[^\w\-.]", "_", name)
    return sanitized[:128] or "file"


def _safe_int(v: Any) -> int:
    try:
        return int(v)
    except (TypeError, ValueError):
        return 0


# ─── Markdown → PDF flowables (reportlab) ────────────────────────────────


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
    from reportlab.platypus import SimpleDocTemplate

    size_map = {"A4": A4, "Letter": letter, "Legal": legal, "A3": A3, "A5": A5}
    page_dims = size_map[page_size]
    if orientation == "landscape":
        page_dims = landscape(page_dims)

    doc_kwargs: dict[str, Any] = {
        "pagesize": page_dims,
        "topMargin": float(margins_cm.get("top", 2.5)) * cm,
        "bottomMargin": float(margins_cm.get("bottom", 2.5)) * cm,
        "leftMargin": float(margins_cm.get("left", 2.5)) * cm,
        "rightMargin": float(margins_cm.get("right", 2.5)) * cm,
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
        raise DomainError(
            "RENDER_ERROR", f"reportlab a échoué : {exc}", details={"reason": str(exc)}
        ) from exc

    from pypdf import PdfReader

    return len(PdfReader(output_path).pages)


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
        return [Paragraph(_inline_html_to_reportlab(elem), styles[tag])]
    if tag == "p":
        text = _inline_html_to_reportlab(elem)
        return [Paragraph(text, styles["body"])] if text.strip() else []
    if tag == "ul":
        return [_list_to_flowable(elem, styles, ordered=False)]
    if tag == "ol":
        return [_list_to_flowable(elem, styles, ordered=True)]
    if tag == "table":
        return [_table_to_flowable(elem, styles)]
    if tag == "pre":
        return [Preformatted("".join(elem.itertext()).rstrip(), styles["code"])]
    if tag == "blockquote":
        out: list[Any] = []
        for child in elem:
            if child.tag.lower() == "p":
                out.append(Paragraph(_inline_html_to_reportlab(child), styles["quote"]))
            else:
                out.extend(_element_to_flowables(child, styles))
        return out
    if tag == "hr":
        return [HRFlowable(
            width="100%", thickness=0.5,
            color=colors.HexColor("#CCCCCC"),
            spaceBefore=6, spaceAfter=6,
        )]
    text = _inline_html_to_reportlab(elem)
    return [Paragraph(text, styles["body"])] if text.strip() else []


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
            href = child.get("href", "").replace('"', "&quot;")
            parts.append(f'<a href="{href}" color="blue"><u>{inner}</u></a>')
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
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _list_to_flowable(elem: ET.Element, styles: dict[str, Any], ordered: bool) -> Any:
    from reportlab.platypus import ListFlowable, ListItem, Paragraph

    items: list[Any] = []
    for li in elem.findall("li"):
        li_parts: list[str] = []
        if li.text:
            li_parts.append(_escape_pdf_text(li.text))
        nested: list[Any] = []
        for child in li:
            ct = child.tag.lower()
            if ct in ("ul", "ol"):
                nested.append(_list_to_flowable(child, styles, ordered=(ct == "ol")))
            elif ct == "p":
                li_parts.append(_inline_html_to_reportlab(child))
                if child.tail:
                    li_parts.append(_escape_pdf_text(child.tail))
            else:
                inner = _inline_html_to_reportlab(child)
                if ct in ("strong", "b"):
                    li_parts.append(f"<b>{inner}</b>")
                elif ct in ("em", "i"):
                    li_parts.append(f"<i>{inner}</i>")
                elif ct == "code":
                    li_parts.append(f'<font name="Courier" size="9">{inner}</font>')
                elif ct == "a":
                    href = child.get("href", "")
                    li_parts.append(f'<a href="{href}" color="blue"><u>{inner}</u></a>')
                else:
                    li_parts.append(inner)
                if child.tail:
                    li_parts.append(_escape_pdf_text(child.tail))
        text = "".join(li_parts).strip() or " "
        item_flowables: list[Any] = [Paragraph(text, styles["body"])]
        item_flowables.extend(nested)
        items.append(ListItem(item_flowables, leftIndent=20))
    return ListFlowable(items, bulletType="1" if ordered else "bullet", leftIndent=20)


def _table_to_flowable(elem: ET.Element, styles: dict[str, Any]) -> Any:
    from reportlab.platypus import Table, TableStyle, Paragraph
    from reportlab.lib import colors

    rows_data: list[list[Any]] = []
    has_header = False
    for section in elem:
        tag = section.tag.lower()
        if tag == "thead":
            has_header = True
        children = (
            section.findall("tr") if tag in ("thead", "tbody", "tfoot")
            else [section] if tag == "tr"
            else []
        )
        for tr in children:
            row: list[Any] = []
            for cell in tr:
                if cell.tag.lower() in ("th", "td"):
                    row.append(Paragraph(_inline_html_to_reportlab(cell), styles["body"]))
            rows_data.append(row)
    if not rows_data:
        return Paragraph("(empty table)", styles["body"])
    table = Table(rows_data, repeatRows=1 if has_header else 0)
    cmds = [
        ("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#BFBFBF")),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 6),
        ("RIGHTPADDING", (0, 0), (-1, -1), 6),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
    ]
    if has_header:
        cmds.insert(0, ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#DDEBF7")))
        cmds.insert(1, ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"))
    table.setStyle(TableStyle(cmds))
    return table
