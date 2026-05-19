# Changelog — xlsx-worker

Toutes les modifications notables de ce worker sont documentées ici.

Format inspiré de [Keep a Changelog](https://keepachangelog.com/), versioning [SemVer](https://semver.org/).

## [0.1.0] — 2026-05-18

### Added
- Release initiale.
- Skill `xlsx.read` : lecture cellules brutes avec types préservés (int/float/str/bool/datetime/formula), merged ranges, freeze panes, mode `values` ou `formulas`, cap configurable `max_rows × max_cols`, option `include_types`.
- Skill `xlsx.read-as-dataframe` : lecture via pandas avec header detection, skip/nrows pagination, `dtypes_hint` pour forcer les types, `na_values` configurable. Sortie en `records` JSON sérialisable.
- Skill `xlsx.write` : création multi-sheet avec styles avancés :
  - `named_styles` réutilisables (font, fill, border, alignment, number_format)
  - `column_widths`, `row_heights`, `freeze`, `auto_filter` par feuille
  - `styles_apply` pour appliquer un style à une range ou cellule
  - `conditional_formatting` avec 10 règles supportées (comparatives + texte)
- Skill `xlsx.append-rows` : ajout de lignes préservant le reste (formules, styles, merged cells).
- Skill `xlsx.update-cells` : patch ciblé par A1 notation avec rapport d'erreurs par cellule.
- Eval suite avec 17 cas (happy paths × 5 skills, erreurs typées, safety checks, styles avancés).

### Notes
- Worker **déterministe pur** — n'utilise pas `ctx.llm` ni de boucle ReAct.
- Stateless — aucune mémoire entre appels.
- Dépendances : `openpyxl==3.1.5` + `pandas==2.2.3`. `pandas` est justifié par le saut de capacité qu'il apporte à `xlsx.read-as-dataframe` (dtype inference, header detection, slicing — non triviaux à reproduire en pur openpyxl).
- Schemas via `TypedDict` stdlib uniquement — pas de Pydantic runtime (`schemas.py` documentaire).

### Limitations connues
- Formats : `.xlsx` uniquement. `.xls`/`.xlsm`/`.xlsb` non supportés (`UNSUPPORTED_FORMAT`).
- Conditional formatting : règles comparatives + texte uniquement (pas de gradients, databars, icon sets, formules dynamiques en v0.1.0).
- Merged cells : signalées en `read`, **non créables en `write`** en v0.1.0.
- Charts, pivot tables, images, commentaires, data validation : conservés à l'ouverture mais ni exposés ni créables.
- Fichiers chiffrés : non supportés (`PARSE_ERROR`).
- Limite fichier en lecture : 100 MB.

## [0.1.1] — 2026-05-19

### Changed
- **Breaking** : remplacé le workaround champ `op` payload par la lecture de `task.skill_id` (propagé par le runtime A2A depuis 2026-05-19). Le worker dispatche désormais sur le full skill_id via `extract_skill_id(task)`.
- Nouveaux codes d'erreur : `MISSING_SKILL_ID`, `UNKNOWN_SKILL_ID`. Les codes `MISSING_FIELD`/`INVALID_TYPE` (anciennement renvoyés pour `op` manquant/invalide) ne sont plus émis pour ce vecteur.
- Eval cases : champ `skill_id` top-level ajouté à chaque case ; champ `op` retiré des payloads.

### Removed
- Convention `op` (read|read-as-dataframe|write|append-rows|update-cells) dans le payload — caller invoque maintenant `ctx.a2a_invoke("<skill>", {...})` sans champ `op`.
