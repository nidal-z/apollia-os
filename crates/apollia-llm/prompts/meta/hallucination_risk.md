Evaluate the global hallucination risk of the session below. Combine three signals:
1. `heuristic_flags` - per-step P3 hallucination detector flags (empty/null/schema violations).
2. `assertion_citation_gaps` - assertions without supporting citations (P10).
3. `thinking_contradictions` - contradictions between thinking turns (P11).

Return ONLY a JSON object with:
- `score` (integer, 0-100 - 0 = safe, 100 = very likely hallucinated)
- `factors` (array of short strings describing top contributors, max 5)

No markdown, no prose. Example:
{"score": 35, "factors": ["2 empty tool outputs", "1 unsupported assertion"]}

Session:
{{session}}
