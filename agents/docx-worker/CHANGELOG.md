# Changelog — docx-worker

Toutes les modifications notables de ce worker sont documentées ici.

Format inspiré de [Keep a Changelog](https://keepachangelog.com/), versioning [SemVer](https://semver.org/).

## [0.1.0] — 2026-05-18

### Added
- Release initiale.
- Skill `docx.read` : lecture structurée — blocs ordonnés (paragraph/heading/table/list/page_break), métadonnées de section (orientation, marges cm, paper_size détecté), hyperlinks, styles utilisés. Options `include_runs` (font détaillé par run) et `include_styles`.
- Skill `docx.write` : création multi-section avec :
  - `named_styles` réutilisables : font (name/size/bold/italic/underline/strike/color/highlight) + paragraph (alignment/spacing/indentation) + table (borders sur 6 côtés / cell_shading / alignment / autofit)
  - 7 types de blocs : paragraph, heading (level 1-9), table (avec cell merge horizontal), list (bullet/number flat), image (PNG/JPEG/GIF/TIFF/BMP), page_break, section_break
  - `document_setup` : orientation, margins_cm, paper_size (A4/Letter/Legal/A3/A5)
  - `sections` avec headers/footers : 10 placeholders (`{page}`, `{total_pages}`, `{date}`, `{time}`, `{datetime}`, `{author}`, `{title}`, `{subject}`, `{filename}`, `{section_num}`) avec format strftime (`{date:%d/%m/%Y}`)
- Skill `docx.append-section` : ajout de blocs en fin de document. Option `section_break_before` + `new_section_setup` pour démarrer une nouvelle section (potentiellement avec orientation différente, ex : annexe en landscape).
- Skill `docx.extract-tables` : extraction ciblée des tables (light alternative à `read`). Option `include_headers` pour détecter la 1ère ligne.
- Skill `docx.render-from-template` : rendu Jinja2 via docxtpl avec mode `strict_undefined` (défaut `true`). Détecte les variables non fournies dans le context et fail fast. Mode `false` disponible pour laisser les placeholders littéraux.
- Eval suite avec 22 cas couvrant les 5 skills + erreurs typées + safety checks + placeholders dynamiques + styles avancés.

### Notes
- Worker **déterministe pur** — n'utilise pas `ctx.llm` ni de boucle ReAct.
- Stateless — aucune mémoire entre appels.
- Dépendances : `python-docx==1.1.2` + `docxtpl==0.18.0` (~15 MB venv). docxtpl justifié par le saut de capacité qu'il apporte au rendu de templates business (analogue à pandas pour xlsx-worker).
- Schemas via `TypedDict` stdlib uniquement — pas de Pydantic runtime (`schemas.py` documentaire).
- Sécurité Jinja2 : `SandboxedEnvironment` par défaut — pas d'exécution de code arbitraire.

### Limitations connues
- Formats : `.docx` uniquement. `.doc`/`.docm`/`.rtf` non supportés (`UNSUPPORTED_FORMAT`).
- `docx.read` v0.1.0 : tous les blocs sont rattachés à la **première section** (mapping bloc → section requiert parsing XML w:sectPr — extension v0.2.0).
- Listes : niveaux simples (flat) uniquement. Nesting représenté par indentation textuelle.
- `section_break` block en `docx.write` : traité comme `page_break`. Pour de vraies sections multiples, utiliser la liste `sections` au top-level.
- Multi-level numbered lists complexes, footnotes/endnotes, commentaires, track changes, bookmarks, cross-references, table of contents, form fields, content controls : non exposés ni créables.
- Embedded objects (Excel, PowerPoint, OLE) : non supportés.
- Détection précise des merged cells en lecture : best-effort.
- Streaming pour fichiers > 100 MB : non optimisé.

## [0.1.1] — 2026-05-19

### Changed
- **Breaking** : remplacé le workaround champ `op` payload par la lecture de `task.skill_id` (propagé par le runtime A2A depuis 2026-05-19). Le worker dispatche désormais sur le full skill_id via `extract_skill_id(task)`.
- Nouveaux codes d'erreur : `MISSING_SKILL_ID`, `UNKNOWN_SKILL_ID`. Les codes `MISSING_FIELD`/`INVALID_TYPE` (anciennement renvoyés pour `op` manquant/invalide) ne sont plus émis pour ce vecteur.
- Eval cases : champ `skill_id` top-level ajouté à chaque case ; champ `op` retiré des payloads.

### Removed
- Convention `op` (read|write|append-section|extract-tables|render-from-template) dans le payload — caller invoque maintenant `ctx.a2a_invoke("<skill>", {...})` sans champ `op`.
