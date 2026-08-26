// ─── Memory entries and search ───

/** Entrée mémoire unifiée (episodic | semantic | procedural). */
export interface MemoryEntry {
  id: string;
  entry_type: "episodic" | "semantic" | "procedural";
  key: string;
  value: string;
  created_at: string;
  expires_at: string | null;
  score: number | null;
}

/** Résultat de recherche FTS5 mémoire. */
export interface MemorySearchResult {
  id: string;
  entry_type: "episodic" | "semantic";
  content: string;
  score: number;
  relevance: number | null;
  created_at: string;
}
