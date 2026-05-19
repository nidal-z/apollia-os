# Changelog — pdf-worker

Toutes les modifications notables de ce worker sont documentées ici.

Format inspiré de [Keep a Changelog](https://keepachangelog.com/), versioning [SemVer](https://semver.org/).

## [0.1.0] — 2026-05-18

### Added
- Release initiale.
- Skill `pdf.read-text` : extraction texte par page avec metadata (title/author/subject/creator/producer/dates), page range syntax 1-based (`"1-5,7,10-12"`), cap configurable `max_chars_per_page` (défaut 100k).
- Skill `pdf.read-tables` : extraction tables via pdfplumber avec `table_settings` forwardé (vertical_strategy, horizontal_strategy, tolérances). Headers détectés sur 1ère ligne (option `include_headers`).
- Skill `pdf.read-forms` : extraction des champs AcroForm avec typing fine (text/checkbox/radio/pushbutton/dropdown/listbox/signature), valeurs, options, flags readonly/required, max_length. Refus XFA forms (Adobe LiveCycle).
- Skill `pdf.render-from-markdown` : Markdown → PDF via markdown lib → HTML → reportlab Flowables. Sous-ensemble riche :
  - Headings h1-h6 (tailles hardcodées 18/16/14/12/11/11 pt)
  - Paragraphes Helvetica 11pt justifié
  - Bold, italic, inline code (Courier), strikethrough
  - Listes ordered/unordered imbriquées (3 niveaux max)
  - Tables (extension `tables`) avec header row en gras + fond bleu
  - Code blocks fenced (extension `fenced_code`) Courier 9pt fond gris
  - Blockquotes italique gris indenté
  - Horizontal rules (trait gris fin)
  - Links cliquables bleus soulignés
  - Page size (A4/Letter/Legal/A3/A5), orientation, margins_cm
  - PDF metadata (title/author/subject)
- Skill `pdf.merge` : concaténation N PDFs (≥2). Préservation bookmarks/TOC en best-effort. `metadata_from` (index 0-based) pour choisir la source des metadata.
- Skill `pdf.split` : découpe par plages nommées (`{name, pages: "1-5"}`) ou page-par-page si absent. Nommage zero-padded `page-001.pdf` ou `<name>.pdf` (sanitization filename).
- Eval suite avec 22+ cas couvrant les 6 skills + erreurs typées + page range edge cases + Markdown rendering + safety checks.

### Notes
- Worker **déterministe pur** — n'utilise pas `ctx.llm` ni de boucle ReAct.
- Stateless — aucune mémoire entre appels.
- 4 dépendances Python pinnées : `pypdf==5.1.0`, `pdfplumber==0.11.4` (+ pypdfium2, pdfminer.six), `reportlab==4.2.5` (+ pillow), `markdown==3.7`. venv ~35 MB.
- Schemas via `TypedDict` stdlib uniquement.
- Page range parser strict : 1-based, validation des bornes, déduplication silencieuse des doublons.

### Limitations connues
- **OCR** : non. PDFs scannés → texte vide. Composer avec image-worker.ocr (futur).
- **PDFs chiffrés** : refusés en read (`ENCRYPTED_PDF`). Password v0.2.0.
- **Markdown** : sous-ensemble — pas d'images inline, CSS, math, footnotes complexes, definition lists, custom HTML.
- **Annotations** (highlights, comments, stamps) : non exposées ni créables.
- **Bookmarks/TOC** : pas de création. Préservation merge en best-effort.
- **Form filling** : read-only en v0.1.0 (pas d'écriture de valeurs).
- **XFA forms** : refusés.
- **PDF/A, PDF/X** : non garantis (output reportlab standard).
- **Compression/optimisation post-process** : non.
- **Taille fichier en read** : ≤ 100 MB.

## [0.1.1] — 2026-05-19

### Changed
- **Breaking** : remplacé le workaround champ `op` payload par la lecture de `task.skill_id` (propagé par le runtime A2A depuis 2026-05-19). Le worker dispatche désormais sur le full skill_id via `extract_skill_id(task)`.
- Nouveaux codes d'erreur : `MISSING_SKILL_ID`, `UNKNOWN_SKILL_ID`. Les codes `MISSING_FIELD`/`INVALID_TYPE` (anciennement renvoyés pour `op` manquant/invalide) ne sont plus émis pour ce vecteur.
- Eval cases : champ `skill_id` top-level ajouté à chaque case ; champ `op` retiré des payloads.

### Removed
- Convention `op` (read-text|read-tables|read-forms|render-from-markdown|merge|split) dans le payload — caller invoque maintenant `ctx.a2a_invoke("<skill>", {...})` sans champ `op`.
