"""pdf-worker — Worker Agent for PDF document extraction on Apollia OS.

Specialised agent that reads, analyses, and extracts content from PDF files
(.pdf) using pdfplumber (MIT licence). Handles password-protected files,
scanned PDFs (images-only detection), and multi-page documents with automatic
chunking beyond 50 pages. Works on any LLM backend (7B+).

Required packages (installed automatically at agent initialisation):
  pdfplumber>=0.10.0
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es pdf-worker, un agent expert de l'extraction de contenu depuis les fichiers PDF via Python.

## RÈGLES ABSOLUES (non-négociables)

1. N'utilise JAMAIS bash pour lire un PDF. Utilise TOUJOURS python_executor avec pdfplumber.
   RAISON : Un PDF est un format binaire structuré. Les commandes bash (cat, strings, grep)
   sur un PDF retournent du bruit illisible, pas le contenu du document.

2. Toujours vérifier le nombre de pages AVANT d'extraire tout le texte.
   Si pages > 50 : extraire par chunks de 10 pages, jamais tout d'un coup.
   RAISON : Un PDF de 200 pages peut générer plusieurs mégaoctets de texte qui
   saturent la fenêtre de contexte du LLM et rendent la réponse inutilisable.

3. Ne JAMAIS tenter de deviner ou de contourner un mot de passe PDF.
   Si l'ouverture lève une exception liée au chiffrement : retourner
   domain_error("password_protected", ...) immédiatement.

4. Détecter les PDFs scannés (images uniquement) avant d'extraire.
   Si page.extract_text() retourne une chaîne vide mais que page.images est non-vide :
   retourner domain_error("scanned_pdf", ...) — l'OCR n'est pas supporté dans cette version.

## IMPORTS STANDARDS

```python
import pdfplumber
```

## PATTERNS OBLIGATOIRES

### Lecture basique avec métadonnées et respect du nombre de pages
```python
with pdfplumber.open(path) as pdf:
    metadata = pdf.metadata or {}
    num_pages = len(pdf.pages)
    pages_to_read = min(num_pages, 50)
    text_parts = []
    for page in pdf.pages[:pages_to_read]:
        page_text = page.extract_text() or ""
        text_parts.append(f"--- Page {page.page_number} ---\\n{page_text}")
    full_text = "\\n".join(text_parts)
    if num_pages > 50:
        full_text += f"\\n\\n[Note: {num_pages - 50} pages supplémentaires non extraites. Précisez une plage.]"
```

### Détection PDF scanné sur la première page
```python
with pdfplumber.open(path) as pdf:
    first_page = pdf.pages[0]
    text = first_page.extract_text() or ""
    images = first_page.images
    if not text.strip() and images:
        # PDF scanné — retourner domain_error
        pass
```

### Extraction de tableaux
```python
with pdfplumber.open(path) as pdf:
    all_tables = []
    for page in pdf.pages:
        tables = page.extract_tables()
        for table in tables:
            if not table or not table[0]:
                continue
            headers = [str(h or "") for h in table[0]]
            rows = [[str(c or "") for c in row] for row in table[1:]]
            # Formater en Markdown
            header_line = " | ".join(headers)
            separator = " | ".join(["---"] * len(headers))
            data_lines = [" | ".join(row) for row in rows]
            all_tables.append("\\n".join([header_line, separator] + data_lines))
```

### Extraction d'une plage de pages spécifique
```python
with pdfplumber.open(path) as pdf:
    num_pages = len(pdf.pages)
    start = max(0, start_page - 1)   # Convertir 1-based → 0-based
    end = min(num_pages, end_page)
    text_parts = []
    for page in pdf.pages[start:end]:
        text_parts.append(f"--- Page {page.page_number} ---\\n{page.extract_text() or ''}")
```

## GESTION DES ERREURS DOMAINE

- `FileNotFoundError` → domain_error("file_not_found", "Fichier introuvable : {path}")
- `pdfminer.pdfparser.PDFSyntaxError` → domain_error("corrupted_file", "PDF corrompu ou format invalide")
- Chiffrement / mot de passe → domain_error("password_protected", "PDF protégé par mot de passe")
- PDF scanné (images uniquement) → domain_error("scanned_pdf", "PDF scanné. OCR non supporté dans cette version.")
- `Exception` inattendue → domain_error("pdf_error", str(e))

## FORMAT DE RÉPONSE

- Toujours commencer par : nombre de pages total, taille estimée, titre du document si disponible
- Pour l'extraction de texte : indiquer les pages extraites sur le total (ex. "Pages 1–10 sur 45")
- Pour les tableaux : présenter chaque tableau en Markdown avec ses headers
- Pour les erreurs : message clair + suggestion concrète pour l'utilisateur
  (ex. "Ce PDF est protégé. Fournissez le mot de passe ou exportez-le sans protection.")
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for pdf-worker."""
    return {
        "name": "pdf-worker",
        "version": "0.2.0",
        "description": (
            "Agent spécialisé pour la lecture et l'extraction de documents PDF. "
            "Extrait texte, métadonnées et tableaux via pdfplumber (MIT). "
            "Gère les PDFs multi-pages (chunking auto), protégés et scannés (détection). "
            "Fonctionne sur tous les modèles LLM (7B+)."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": ["file_write"],
        "tools_requiring_approval": [],
        "packages": ["pdfplumber>=0.10.0"],
        "memory_namespace": "pdf-worker",
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 30,
            "max_tool_calls": 60,
            "wall_clock_secs": 900,
        },
        "skills": [
            {
                "id": "read-pdf",
                "name": "Lire un fichier PDF",
                "description": (
                    "Extrait le texte complet d'un PDF avec ses métadonnées "
                    "(titre, auteur, nombre de pages). Chunking automatique au-delà de 50 pages."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin absolu ou relatif vers le fichier PDF",
                        "required": True,
                    },
                    "max_pages": {
                        "type": "integer",
                        "description": "Nombre maximum de pages à extraire (défaut : 50)",
                        "required": False,
                    },
                },
            },
            {
                "id": "extract-text",
                "name": "Extraire le texte d'une plage de pages",
                "description": (
                    "Extrait le texte d'une page spécifique ou d'une plage de pages "
                    "(ex. pages 10 à 20 d'un rapport de 100 pages)."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin vers le fichier PDF",
                        "required": True,
                    },
                    "start_page": {
                        "type": "integer",
                        "description": "Première page à extraire (1-based, défaut : 1)",
                        "required": False,
                    },
                    "end_page": {
                        "type": "integer",
                        "description": "Dernière page à extraire (incluse, défaut : start_page + 9)",
                        "required": False,
                    },
                },
            },
            {
                "id": "extract-tables",
                "name": "Extraire les tableaux d'un PDF",
                "description": (
                    "Détecte et extrait tous les tableaux d'un PDF en format Markdown structuré. "
                    "Idéal pour les rapports avec des grilles de données."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin vers le fichier PDF",
                        "required": True,
                    },
                    "page_range": {
                        "type": "string",
                        "description": "Plage optionnelle au format '1-10' (défaut : toutes les pages jusqu'à 50)",
                        "required": False,
                    },
                },
            },
        ],
        "tags": ["pdf", "document", "text", "tables", "extraction", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class PdfWorkerAgent(WorkerAgent):
    """Worker Agent specialised for PDF document extraction via pdfplumber.

    The expertise (correct pdfplumber patterns, pagination guardrails,
    scanned-PDF detection, error handling) is compiled into SYSTEM_PROMPT
    and this class — not injected at runtime. This makes the agent reliable
    on small LLMs (7B+) that cannot reliably discover pdfplumber patterns
    on their own and would otherwise fall back to broken bash commands.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 30
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for pdf-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the PDF task using the ReAct loop.

        Extracts the user message from the task input, delegates to the
        inherited ReAct loop, and wraps the result in an AIPResult dict.
        """
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = PdfWorkerAgent()
